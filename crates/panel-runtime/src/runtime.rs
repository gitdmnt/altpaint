use crate::html_wasm_panel::HtmlWasmPanel;
use crate::persistent_config::{collect_persistent_panel_configs, restore_persistent_panel_configs};
use crate::request_translation::register_default_translators;
use crate::translator_registry::TranslatorRegistry;
use crate::host_state::EMPTY_WORKSPACE_PANELS_JSON;
use document_model::Document;
use panel_api::{HostAction, PanelEvent};
use panel_html::{vello, wgpu, PanelSizeConstraints, ActionRect};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// パネル毎の GPU 描画結果をまとめて返す。
///
/// hit 矩形は GPU 描画から分離済み (`collect_panel_hits`)。
pub struct RenderedPanelTexture<'a> {
    pub panel_id: String,
    pub texture: &'a wgpu::Texture,
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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PanelDispatchResult {
    pub actions: Vec<HostAction>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PanelKeyboardResult {
    pub handled: bool,
    pub actions: Vec<HostAction>,
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
    /// `sync_document_subset` の前に各 `HtmlWasmPanel` へ注入され、
    /// host state の `workspace.panels_json` フィールドに反映される。
    workspace_panels_json: String,
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
            translator_registry: Arc::new(translator_registry),
        }
    }

    /// 共有 translator registry への参照を返す (起動時 assert / 診断用)。
    pub fn translator_registry(&self) -> &Arc<TranslatorRegistry> {
        &self.translator_registry
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

    /// 集約 vello::Renderer / scene scratch / device / queue への可変アクセスを提供する。
    /// `install_gpu_context` 未呼び出しなら `None`。
    /// 9E-4: ステータスバーなど panel-runtime 外部の `HtmlPanelView` 利用者が
    /// 共有 GPU コンテキストを再利用するために公開する。
    pub fn gpu_context_parts(
        &mut self,
    ) -> Option<(&Arc<wgpu::Device>, &Arc<wgpu::Queue>, &mut vello::Renderer, &mut vello::Scene)>
    {
        let ctx = self.gpu_ctx.as_mut()?;
        Some((&ctx.device, &ctx.queue, &mut ctx.renderer, &mut ctx.scene_scratch))
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

    /// 指定パネルに UI 入力イベントを転送する。`:hover` / `<details>` 開閉等の動的レイアウトを動かす。
    /// 戻り値: 該当パネルが見つかった場合 true。
    pub fn forward_panel_input(
        &mut self,
        panel_id: &str,
        event: panel_html::blitz_traits::events::UiEvent,
    ) -> bool {
        match self.panels.iter_mut().find(|panel| panel.id() == panel_id) {
            Some(panel) => {
                panel.view_mut().on_input(event);
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
    ) -> Vec<RenderedPanelTexture<'_>> {
        let Some(gpu_ctx) = self.gpu_ctx.as_mut() else {
            return Vec::new();
        };
        // ループ内で self.panels を可変借用するため、まず ID → 描画情報 のメタを集める
        type TextureTuple = (String, *const wgpu::Texture, u32, u32);
        let mut textures: Vec<TextureTuple> = Vec::new();
        for (panel_id, width, height) in sized {
            // 該当パネルを mutable で取得
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
                chrome_height,
            );
            let target = outcome.target();
            let ptr: *const wgpu::Texture = &target.texture;
            textures.push((panel_id.clone(), ptr, target.width, target.height));
        }
        // SAFETY: 各 *const wgpu::Texture は self.panels 内の Box<dyn PanelPlugin> 内
        // view が保持するテクスチャを指す。Box は heap に固定されており、戻り値の
        // RenderedPanelTexture は &mut self に紐付くので、戻り値存在中は self.panels が
        // 不変に保たれる。テクスチャの寿命も同期する。
        textures
            .into_iter()
            .map(|(panel_id, ptr, w, h)| RenderedPanelTexture {
                panel_id,
                texture: unsafe { &*ptr },
                width: w,
                height: h,
            })
            .collect()
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
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) -> BTreeSet<String> {
        if self.dirty_panels.is_empty() {
            return BTreeSet::new();
        }
        let dirty = std::mem::take(&mut self.dirty_panels);
        self.sync_document_subset(document, Some(&dirty), can_undo, can_redo, active_jobs, snapshot_count)
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

    /// 登録されたパネル ID (登録順、`&'static str`) を返す。
    /// reconcile_panels 用。
    pub fn panel_static_ids(&self) -> Vec<&'static str> {
        self.panels.iter().map(|panel| panel.id()).collect()
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
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == event_panel_id(event))
        else {
            return PanelDispatchResult::default();
        };

        let actions = panel.handle_event(event);
        let mut changed_panel_ids = BTreeSet::new();
        changed_panel_ids.insert(panel.id().to_string());
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
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
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let mut handled = false;
        let mut actions = Vec::new();
        let mut changed_panel_ids = BTreeSet::new();
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
            let keyboard_handled =
                !panel_actions.is_empty() || panel.persistent_config() != previous_config;
            if keyboard_handled {
                changed_panel_ids.insert(panel.id().to_string());
            }
            handled |= keyboard_handled;
            actions.extend(panel_actions);
        }
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
        PanelKeyboardResult {
            handled,
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// 現在の値を ドキュメント subset へ変換する。
    ///
    /// ADR 014 以降、HTML パネル経路では GPU 側 `render_dirty` が真の dirty 判定を持つため、
    /// `update` が呼ばれたパネルは無条件で `changed_panels` に入れる。
    fn sync_document_subset(
        &mut self,
        document: &Document,
        panel_ids: Option<&BTreeSet<String>>,
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) -> BTreeSet<String> {
        let workspace_json = self.workspace_panels_json.clone();
        let mut changed_panels = BTreeSet::new();
        for panel in &mut self.panels {
            if panel_ids.is_some_and(|panel_ids| !panel_ids.contains(panel.id())) {
                continue;
            }
            // host state 組立用の workspace 情報を注入する。
            panel.set_workspace_panels_json(workspace_json.clone());
            panel.update(document, can_undo, can_redo, active_jobs, snapshot_count);
            changed_panels.insert(panel.id().to_string());
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

    /// register_panel が runtime 共有の translator registry を各パネルへ注入する (BL-061)。
    #[test]
    fn register_panel_injects_shared_translator_registry() {
        let mut runtime = runtime_with_panel("registry-share", NO_KEYBOARD_WAT);
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
