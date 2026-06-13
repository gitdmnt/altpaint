use crate::html_wasm_panel::HtmlWasmPanel;
use crate::persistent_config::{collect_persistent_panel_configs, restore_persistent_panel_configs};
use crate::request_translation::register_default_translators;
use crate::translator_registry::TranslatorRegistry;
use crate::host_state::{
    EMPTY_WORKSPACE_PANELS_JSON, HostState, HostStateContext, HostStateRegistry,
};
use crate::panel_input::PanelPointerInput;
use document_model::Document;
use crate::host_request::{HostRequest, PanelEvent};
use panel_html::{vello, wgpu, ChromeStyle, PanelSizeConstraints, ActionRect};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// HTML パネル上端のホスト描画タイトルバー (chrome) の塗り色 (RGBA, sRGB)。
///
/// BL-099: panel-html はテーマ色を知らないため、chrome の見た目は panel-runtime
/// (パネル基盤) が所有する。render_panels がこの色で [`ChromeStyle`] を組み立てて
/// view へ注入する。
pub const PANEL_CHROME_FILL_RGBA: [u8; 4] = [40, 60, 90, 255];

/// パネル毎の GPU 描画結果をまとめて返す。
///
/// `texture` は `Arc<wgpu::Texture>` で所有渡しする。`wgpu::Texture` は内部的に
/// refcount されたハンドルなので複製は安価で、present 経路へ raw pointer + unsafe
/// なしで受け渡せる (BL-092)。
///
/// hit 矩形は GPU 描画から分離済み (`collect_panel_hits`)。
pub struct RenderedPanelTexture {
    pub panel_id: String,
    pub texture: Arc<wgpu::Texture>,
    pub width: u32,
    pub height: u32,
}

/// 共有 wgpu リソース + 集約 vello::Renderer。`install_gpu_context` で初期化。
struct PanelGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    renderer: vello::Renderer,
    scene_scratch: vello::Scene,
}

/// panel-runtime 外部 (ステータスバー等) が共有 GPU コンテキストを使って
/// `HtmlPanelView::on_render` を呼ぶための借用ハンドル (BL-092)。
///
/// 旧 `gpu_context_parts` の 4 連 tuple を置換し、device/queue/renderer/scene を
/// 1 つの型でまとめて貸し出す。`on_render` が要求する引数順をそのまま提供する。
pub struct HtmlSurfaceRenderer<'a> {
    pub device: &'a Arc<wgpu::Device>,
    pub queue: &'a Arc<wgpu::Queue>,
    pub renderer: &'a mut vello::Renderer,
    pub scene_scratch: &'a mut vello::Scene,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PanelDispatchResult {
    pub actions: Vec<HostRequest>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PanelKeyboardResult {
    pub handled: bool,
    pub actions: Vec<HostRequest>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

/// パネル runtime と registry を保持する。
pub struct PanelRuntime {
    /// 登録済みパネル。実装は `HtmlWasmPanel` 1 種のみなので具象保持する (P1)。
    panels: Vec<HtmlWasmPanel>,
    persistent_panel_configs: BTreeMap<String, Value>,
    /// イベント駆動再描画のための dirty パネル集合。
    dirty_panels: BTreeSet<String>,
    /// GPU コンテキスト（device/queue/renderer/scene scratch）。
    gpu_ctx: Option<PanelGpuContext>,
    /// `workspace_layout` の登録パネル一覧 (id / title / visible) を表現する JSON。
    /// host state の `workspace` セクションへ供給され、`workspace.panels_json`
    /// フィールドに反映される。builtin.workspace-layout 用。
    workspace_panels_json: String,
    /// host state の section registry + revision キャッシュ (BL-093)。
    /// `sync_document_subset` が 1 回だけ全セクションを合成し、変化したセクションを
    /// 各パネルの購読判定に使う。
    host_state_registry: HostStateRegistry,
    /// `RequestDescriptor` → host 経路の翻訳に使う共有 registry (BL-061)。
    /// 登録は一箇所 (`PanelRuntime::new`) で行い、登録した各パネルへ注入する。
    translator_registry: Arc<TranslatorRegistry>,
}

impl Default for PanelRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelRuntime {
    pub fn new() -> Self {
        let mut translator_registry = TranslatorRegistry::new();
        register_default_translators(&mut translator_registry);
        Self {
            panels: Vec::new(),
            persistent_panel_configs: BTreeMap::new(),
            dirty_panels: BTreeSet::new(),
            gpu_ctx: None,
            workspace_panels_json: EMPTY_WORKSPACE_PANELS_JSON.to_string(),
            host_state_registry: HostStateRegistry::default(),
            translator_registry: Arc::new(translator_registry),
        }
    }

    /// 共有 translator registry への参照を返す (起動時 assert / 診断用)。
    pub fn translator_registry(&self) -> &Arc<TranslatorRegistry> {
        &self.translator_registry
    }

    /// host state registry に登録されたセクションキー一覧 (起動時 assert / 診断用)。
    pub fn host_state_section_keys(&self) -> Vec<&'static str> {
        self.host_state_registry.section_keys()
    }

    /// ワークスペース登録パネル一覧 JSON を更新する。
    ///
    /// 値が変化した場合は `builtin.workspace-layout` を dirty 扱いにし、
    /// 次回 `sync_dirty_panels` で workspace-layout の DOM が再構築される。
    pub fn set_workspace_panels_json(&mut self, json: String) -> bool {
        if self.workspace_panels_json == json {
            return false;
        }
        self.workspace_panels_json = json;
        if self
            .panels
            .iter()
            .any(|panel| panel.id() == "builtin.workspace-layout")
        {
            self.dirty_panels
                .insert("builtin.workspace-layout".to_string());
        }
        true
    }

    /// 共有 GPU コンテキスト (device/queue/renderer/scene scratch) を
    /// [`HtmlSurfaceRenderer`] ハンドルとして貸し出す。
    /// `install_gpu_context` 未呼び出しなら `None`。
    /// 9E-4: ステータスバーなど panel-runtime 外部の `HtmlPanelView` 利用者が
    /// 共有 GPU コンテキストを再利用するために公開する (BL-092 で 4 連 tuple を置換)。
    pub fn html_surface_renderer(&mut self) -> Option<HtmlSurfaceRenderer<'_>> {
        let ctx = self.gpu_ctx.as_mut()?;
        Some(HtmlSurfaceRenderer {
            device: &ctx.device,
            queue: &ctx.queue,
            renderer: &mut ctx.renderer,
            scene_scratch: &mut ctx.scene_scratch,
        })
    }

    /// 共有 wgpu Device/Queue を受け取り、vello::Renderer を集約構築する。
    /// 失敗時は `gpu_ctx = None` を維持し、HTML パネルは描画スキップにフォールバック。
    pub fn install_gpu_context(
        &mut self,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) {
        match vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                num_init_threads: None,
                antialiasing_support: vello::AaSupport::area_only(),
                pipeline_cache: None,
            },
        ) {
            Ok(renderer) => {
                self.gpu_ctx = Some(PanelGpuContext {
                    device,
                    queue,
                    renderer,
                    scene_scratch: vello::Scene::new(),
                });
            }
            Err(err) => {
                eprintln!(
                    "panel-runtime: vello renderer init failed: {err:?}; HTML panels disabled"
                );
                self.gpu_ctx = None;
            }
        }
    }

    /// GPU 直描画対応パネル (HtmlWasmPanel) の ID 一覧。
    ///
    /// 実装は `HtmlWasmPanel` 1 種のみなので、全登録パネルが対象 (P1)。
    pub fn panel_ids_with_gpu(&mut self) -> Vec<String> {
        self.panels.iter().map(|panel| panel.id().to_string()).collect()
    }

    /// パネル毎の現在の権威サイズを返す。
    /// 戻り値: `Vec<(panel_id, width, height)>`。
    pub fn panel_sizes(&mut self) -> Vec<(String, u32, u32)> {
        self.panels
            .iter_mut()
            .map(|panel| {
                let panel_id = panel.id().to_string();
                let (w, h) = panel.view_mut().panel_size();
                (panel_id, w, h)
            })
            .collect()
    }

    /// 指定パネルの panel_size を返す。該当無しの場合は `(1, 1)`。
    pub fn panel_size(&mut self, panel_id: &str) -> (u32, u32) {
        match self.panels.iter_mut().find(|panel| panel.id() == panel_id) {
            Some(panel) => panel.view_mut().panel_size(),
            None => (1, 1),
        }
    }

    /// 指定パネルにポインタ入力を転送する。`:hover` / `<details>` 開閉等の動的レイアウトを動かす。
    /// 戻り値: 該当パネルが見つかった場合 true。
    ///
    /// 入力は panel-runtime 定義の [`PanelPointerInput`] で受け取り、blitz `UiEvent` への
    /// 変換は本クレート内部で行う (BL-091: ホスト側の blitz 直接構築を撤去)。
    pub fn forward_panel_input(&mut self, panel_id: &str, input: PanelPointerInput) -> bool {
        match self.panels.iter_mut().find(|panel| panel.id() == panel_id) {
            Some(panel) => {
                panel.view_mut().on_input(input.into_ui_event());
                true
            }
            None => false,
        }
    }

    /// Phase 11: 指定パネル root 要素の CSS `min/max-width/height` 制約を返す。
    /// リサイズ時のクランプ値として `compute_resized_rect` で使う。
    /// 未登録の場合は `None` (= 制約なし)。
    pub fn panel_size_constraints(&mut self, panel_id: &str) -> Option<PanelSizeConstraints> {
        self.panels
            .iter_mut()
            .find(|panel| panel.id() == panel_id)
            .map(|panel| panel.view_mut().root_size_constraints())
    }

    /// 指定パネルの meta.json `default_size` を返す。`None` は未登録。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入する用途。
    pub fn panel_default_size(&mut self, panel_id: &str) -> Option<(u32, u32)> {
        self.panels
            .iter()
            .find(|panel| panel.id() == panel_id)
            .map(|panel| panel.default_size())
    }

    /// 起動時 restore 用：指定 panel_id に永続化された panel_size を流し込む。
    /// 戻り値: 該当パネルが見つかった場合 true。
    pub fn restore_panel_size(&mut self, panel_id: &str, size: (u32, u32)) -> bool {
        match self.panels.iter_mut().find(|panel| panel.id() == panel_id) {
            Some(panel) => {
                panel.view_mut().set_panel_size(size);
                true
            }
            None => false,
        }
    }

    /// 指定された (panel_id, width, height) リストの GPU パネルを描画する。
    /// `chrome_height` > 0 ならパネル上端にホスト描画タイトルバーを重ねる。
    /// `install_gpu_context` 未呼び出しなら空 Vec。
    pub fn render_panels(
        &mut self,
        sized: &[(String, u32, u32)],
        scale: f32,
        chrome_height: u32,
    ) -> Vec<RenderedPanelTexture> {
        let Some(gpu_ctx) = self.gpu_ctx.as_mut() else {
            return Vec::new();
        };
        // chrome 高さ > 0 のときだけ panel-runtime 所有のテーマ色で chrome を重ねる。
        let chrome = (chrome_height > 0).then_some(ChromeStyle {
            height: chrome_height,
            fill_rgba: PANEL_CHROME_FILL_RGBA,
        });
        // 各パネルを描画し、所有テクスチャハンドル (`Arc<wgpu::Texture>`) を集める。
        // `texture_handle()` の複製は refcount ハンドルなので安価で、戻り値が
        // self.panels の借用と独立するため raw pointer + unsafe は不要 (BL-092)。
        let mut textures = Vec::with_capacity(sized.len());
        for (panel_id, width, height) in sized {
            let Some(panel) = self.panels.iter_mut().find(|p| p.id() == panel_id.as_str()) else {
                continue;
            };
            let view = panel.view_mut();
            let outcome = view.on_render(
                &gpu_ctx.device,
                &gpu_ctx.queue,
                &mut gpu_ctx.renderer,
                &mut gpu_ctx.scene_scratch,
                (*width, *height),
                scale,
                chrome,
            );
            let target = outcome.target();
            textures.push(RenderedPanelTexture {
                panel_id: panel_id.clone(),
                texture: target.texture_handle(),
                width: target.width,
                height: target.height,
            });
        }
        textures
    }

    /// 指定された (panel_id, viewport_w, viewport_h) リストのパネルについて、
    /// GPU コンテキスト不要でレイアウトを解決し `data-action` hit 矩形を収集する。
    ///
    /// `render_panels` (GPU 描画) と同一のクランプ規則で resolve するため、
    /// hit 矩形と実描画は常に一致する。headless 環境 (テスト等) でも動作する。
    pub fn collect_panel_hits(
        &mut self,
        sized: &[(String, u32, u32)],
        scale: f32,
        chrome_height: u32,
    ) -> Vec<(String, Vec<ActionRect>)> {
        let mut out = Vec::with_capacity(sized.len());
        for (panel_id, width, height) in sized {
            let Some(panel) = self.panels.iter_mut().find(|p| p.id() == panel_id.as_str()) else {
                continue;
            };
            let hits = panel
                .view_mut()
                .resolve_action_rects((*width, *height), scale, chrome_height);
            out.push((panel_id.clone(), hits));
        }
        out
    }

    pub fn register_panel(&mut self, mut panel: HtmlWasmPanel) {
        if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
        // 共有 translator registry を注入する (BL-061)。
        panel.set_translator_registry(Arc::clone(&self.translator_registry));
        self.panels
            .retain(|registered| registered.id() != panel.id());
        self.dirty_panels.insert(panel.id().to_string());
        self.panels.push(panel);
    }

    /// 指定パネルを dirty としてマークする。
    ///
    /// `sync_dirty_panels` が呼ばれるまで再描画をスキップする。
    pub fn mark_dirty(&mut self, panel_id: &str) {
        if self.panels.iter().any(|p| p.id() == panel_id) {
            self.dirty_panels.insert(panel_id.to_string());
        }
    }

    /// 全パネルを dirty としてマークする。
    pub fn mark_all_dirty(&mut self) {
        for panel in &self.panels {
            self.dirty_panels.insert(panel.id().to_string());
        }
    }

    /// dirty なパネルが1つ以上あるかどうかを返す。
    pub fn has_dirty_panels(&self) -> bool {
        !self.dirty_panels.is_empty()
    }

    /// dirty パネルの件数を返す。
    pub fn dirty_panel_count(&self) -> usize {
        self.dirty_panels.len()
    }

    /// dirty パネルのみ `update` を呼び、変更したパネル ID の集合を返す。
    ///
    /// 呼び出し後、dirty 集合はクリアされる。
    pub fn sync_dirty_panels(
        &mut self,
        document: &Document,
        host_state: HostState,
    ) -> BTreeSet<String> {
        if self.dirty_panels.is_empty() {
            return BTreeSet::new();
        }
        let dirty = std::mem::take(&mut self.dirty_panels);
        self.sync_document_subset(document, Some(&dirty), host_state)
    }

    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// 登録されたパネル ID / title の対 (登録順) を返す。
    /// builtin.workspace-layout が host state 用に title を引くのに使う。
    pub fn panel_id_titles(&self) -> Vec<(String, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id().to_string(), panel.title().to_string()))
            .collect()
    }

    /// 登録されたパネル ID (登録順) を借用で返す。
    /// reconcile_panels 用 (BL-098: `&'static str` 要求を撤去)。
    pub fn panel_ids(&self) -> Vec<&str> {
        self.panels.iter().map(|panel| panel.id()).collect()
    }

    /// 各パネルの meta.json で宣言された既定ワークスペース配置 (BL-095)。
    ///
    /// `(panel_id, &PanelLayoutMeta)` を登録順で返す。desktop が panel-workspace へ
    /// 配置既定値を bridge するために使う (panel-workspace はビルトイン ID を持たない)。
    pub fn panel_layout_metas(&self) -> Vec<(&str, &crate::meta::PanelLayoutMeta)> {
        self.panels
            .iter()
            .map(|panel| (panel.id(), panel.layout()))
            .collect()
    }

    /// 同梱 default-floating プリセット配置を宣言したパネルの `(id, &PanelPresetMeta)`
    /// を登録順で返す (BL-095)。`default_workspace_preset_catalog` のハードコードを
    /// 置換し、desktop が meta からプリセットカタログを構築するために使う。
    pub fn panel_preset_metas(&self) -> Vec<(&str, &crate::meta::PanelPresetMeta)> {
        self.panels
            .iter()
            .filter_map(|panel| panel.preset().map(|preset| (panel.id(), preset)))
            .collect()
    }

    /// 指定 host state セクションを**明示的に**購読しているパネル ID 一覧 (BL-095)。
    ///
    /// desktop が「特定トピック (tool/color/view 等) が変わったら購読パネルだけ
    /// dirty にする」解決に使い、ビルトイン ID リストのハードコードを廃止する。
    /// `subscribes` が空のパネル (全セクション購読) は対象外 — それらは全面同期
    /// (`mark_all_dirty`) 経路で扱われる。
    pub fn panel_ids_subscribing(&self, section: &str) -> Vec<String> {
        self.panels
            .iter()
            .filter(|panel| panel.subscribes().iter().any(|s| s == section))
            .map(|panel| panel.id().to_string())
            .collect()
    }

    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        collect_persistent_panel_configs(&self.panels)
    }

    pub fn replace_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        restore_persistent_panel_configs(&mut self.panels, &self.persistent_panel_configs);
    }

    /// 現在の値を イベント へ変換する。
    ///
    /// ADR 014 以降、HTML パネル経路では GPU 側 `render_dirty` が真の dirty 判定を持つため、
    /// runtime 側ではイベントを受けたパネルを無条件で `changed_panel_ids` に入れる。
    pub fn dispatch_event(&mut self, event: &PanelEvent) -> PanelDispatchResult {
        let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == event_panel_id(event))
        else {
            return PanelDispatchResult::default();
        };

        // BL-097: config 変化検知は対象パネル単体の persistent_config 比較に一本化する
        // (全パネル map の二重 collect を廃止)。イベントを受けるのは対象パネル 1 枚だけなので、
        // 他パネルの config は変化しえない。
        let previous_config = panel.persistent_config();
        let actions = panel.handle_event(event);
        let config_changed = panel.persistent_config() != previous_config;
        let mut changed_panel_ids = BTreeSet::new();
        changed_panel_ids.insert(panel.id().to_string());
        PanelDispatchResult {
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// 現在の値を キーボード へ変換する。
    ///
    /// `handled` は「対象パネルが actions を発行した」または「persistent_config が変化した」で決まる。
    /// PanelTree 比較は ADR 014 で撤去済み。
    pub fn dispatch_keyboard(
        &mut self,
        shortcut: &str,
        key: &str,
        repeat: bool,
    ) -> PanelKeyboardResult {
        let mut handled = false;
        let mut actions = Vec::new();
        let mut changed_panel_ids = BTreeSet::new();
        // BL-097: config 変化検知はパネル単体の before/after 比較に一本化し、
        // その結果を `config_changed` へ畳み込む (全パネル map の二重 collect を廃止)。
        let mut config_changed = false;
        for panel in &mut self.panels {
            if !panel.handles_keyboard_event() {
                continue;
            }
            let previous_config = panel.persistent_config();
            let panel_actions = panel.handle_event(&PanelEvent::Keyboard {
                panel_id: panel.id().to_string(),
                shortcut: shortcut.to_string(),
                key: key.to_string(),
                repeat,
            });
            let panel_config_changed = panel.persistent_config() != previous_config;
            config_changed |= panel_config_changed;
            let keyboard_handled = !panel_actions.is_empty() || panel_config_changed;
            if keyboard_handled {
                changed_panel_ids.insert(panel.id().to_string());
            }
            handled |= keyboard_handled;
            actions.extend(panel_actions);
        }
        PanelKeyboardResult {
            handled,
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// dirty パネルへ合成済み host state を配り、購読セクションが変化したパネルのみ
    /// 再 render する (BL-093)。
    ///
    /// host state は section registry が 1 回だけ全セクションを合成する
    /// (revision キャッシュにより変化したセクションのみ再シリアライズ)。各パネルは
    /// `subscribes` (meta.json) で宣言した購読セクションの revision が変化した
    /// 時のみ DOM を再 render する。
    ///
    /// `changed_panels` には「実際に DOM を再 render したパネル」のみを入れる。
    fn sync_document_subset(
        &mut self,
        document: &Document,
        panel_ids: Option<&BTreeSet<String>>,
        host_state: HostState,
    ) -> BTreeSet<String> {
        let build = self.host_state_registry.build(&HostStateContext {
            document,
            host_state,
            workspace_panels_json: &self.workspace_panels_json,
        });
        let mut changed_panels = BTreeSet::new();
        for panel in &mut self.panels {
            if panel_ids.is_some_and(|panel_ids| !panel_ids.contains(panel.id())) {
                continue;
            }
            if panel.update(&build) {
                changed_panels.insert(panel.id().to_string());
            }
        }
        changed_panels
    }
}

fn event_panel_id(event: &PanelEvent) -> &str {
    match event {
        PanelEvent::Activate { panel_id, .. }
        | PanelEvent::SetValue { panel_id, .. }
        | PanelEvent::DragValue { panel_id, .. }
        | PanelEvent::SetText { panel_id, .. }
        | PanelEvent::Keyboard { panel_id, .. } => panel_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html_wasm_panel::test_fixture::{
        KEYBOARD_WAT, NO_KEYBOARD_WAT, write_panel_fixture,
    };
    use serde_json::json;

    fn runtime_with_panel(name: &str, wat: &str) -> PanelRuntime {
        let dir = write_panel_fixture(name, wat);
        let panel = HtmlWasmPanel::load(&dir, "panel.wasm", None).expect("panel loads");
        let mut runtime = PanelRuntime::new();
        runtime.register_panel(panel);
        runtime
    }

    /// collect_panel_hits は GPU コンテキストなし (headless) でも hit 矩形を返す。
    #[test]
    fn collect_panel_hits_works_without_gpu_context() {
        let mut runtime = runtime_with_panel("registry-hits", NO_KEYBOARD_WAT);

        let hits = runtime.collect_panel_hits(
            &[("builtin.test-kb".to_string(), 1280, 720)],
            1.0,
            24,
        );

        assert_eq!(hits.len(), 1);
        let (panel_id, panel_hits) = &hits[0];
        assert_eq!(panel_id, "builtin.test-kb");
        assert!(
            panel_hits
                .iter()
                .any(|h| h.element_id.as_deref() == Some("kb.test")),
            "expected data-action button hit, got {panel_hits:?}"
        );
    }

    /// dispatch_keyboard が Wasm keyboard handler へ届き、config 変化が報告される。
    #[test]
    fn dispatch_keyboard_reaches_wasm_handler_and_reports_config_change() {
        let mut runtime = runtime_with_panel("registry-kb", KEYBOARD_WAT);

        let result = runtime.dispatch_keyboard("Ctrl+K", "K", false);

        assert!(result.handled);
        assert!(result.config_changed);
        assert_eq!(
            runtime.persistent_panel_configs().get("builtin.test-kb"),
            Some(&json!({ "last_shortcut": "Ctrl+K" }))
        );
    }

    /// keyboard handler を持たないパネルは dispatch_keyboard でスキップされる。
    #[test]
    fn dispatch_keyboard_skips_panels_without_keyboard_handler() {
        let mut runtime = runtime_with_panel("registry-no-kb", NO_KEYBOARD_WAT);

        let result = runtime.dispatch_keyboard("Ctrl+K", "K", false);

        assert!(!result.handled);
        assert!(!result.config_changed);
    }

    /// BL-097: dispatch_event の config 変化検知は対象パネル単体比較に一本化されており、
    /// config を変えないイベントでは `config_changed` が false になる。
    #[test]
    fn dispatch_event_reports_no_config_change_when_handler_keeps_config() {
        let mut runtime = runtime_with_panel("registry-activate", NO_KEYBOARD_WAT);

        let result = runtime.dispatch_event(&PanelEvent::Activate {
            panel_id: "builtin.test-kb".to_string(),
            node_id: "kb.test".to_string(),
        });

        // 対象パネルは config を変えないので config_changed は立たない。
        assert!(!result.config_changed);
        // 対象パネルは changed_panel_ids に入る (イベントは届いている)。
        assert!(result.changed_panel_ids.contains("builtin.test-kb"));
    }

    /// BL-097: 未登録パネル宛イベントは config_changed を立てず default を返す。
    #[test]
    fn dispatch_event_for_unknown_panel_reports_no_config_change() {
        let mut runtime = runtime_with_panel("registry-unknown", NO_KEYBOARD_WAT);

        let result = runtime.dispatch_event(&PanelEvent::Activate {
            panel_id: "builtin.missing".to_string(),
            node_id: "x".to_string(),
        });

        assert!(!result.config_changed);
        assert!(result.changed_panel_ids.is_empty());
    }

    /// register_panel が runtime 共有の translator registry を各パネルへ注入する (BL-061)。
    #[test]
    fn register_panel_injects_shared_translator_registry() {
        let runtime = runtime_with_panel("registry-share", NO_KEYBOARD_WAT);
        let runtime_ptr = Arc::as_ptr(runtime.translator_registry());
        let panel = runtime
            .panels
            .first()
            .expect("registered HtmlWasmPanel");
        assert_eq!(
            panel.translator_registry_ptr(),
            runtime_ptr,
            "panel should share the runtime translator registry instance"
        );
    }
}
