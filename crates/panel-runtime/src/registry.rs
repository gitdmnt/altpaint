use crate::builtin_plugin::BuiltinPanelPlugin;
use crate::config::{collect_persistent_panel_configs, restore_persistent_panel_configs};
use crate::host_sync::EMPTY_WORKSPACE_PANELS_JSON;
use app_core::Document;
use panel_api::{HostAction, PanelEvent, PanelPlugin};
use panel_html_experiment::{vello, wgpu, HtmlPanelEngine, PanelSizeConstraints, RenderedPanelHit};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// パネル毎の GPU 描画結果をまとめて返す。
pub struct PanelGpuFrame<'a> {
    pub panel_id: String,
    pub texture: &'a wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub hit_regions: Vec<RenderedPanelHit>,
    pub rendered_this_frame: bool,
}

/// 共有 wgpu リソース + 集約 vello::Renderer。`install_gpu_context` で初期化。
struct PanelGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    renderer: vello::Renderer,
    scene_scratch: vello::Scene,
}

/// パネルから可変 `HtmlPanelEngine` を取り出すための共通アクセサ。
/// `BuiltinPanelPlugin` のみが GPU 描画 (HtmlPanelEngine) を持つので downcast する。
fn panel_engine_mut(panel: &mut Box<dyn PanelPlugin>) -> Option<&mut HtmlPanelEngine> {
    let any = panel.as_any_mut()?;
    any.downcast_mut::<BuiltinPanelPlugin>()
        .map(|p| p.engine_mut())
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimeDispatchResult {
    pub actions: Vec<HostAction>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimeKeyboardResult {
    pub handled: bool,
    pub actions: Vec<HostAction>,
    pub changed_panel_ids: BTreeSet<String>,
    pub config_changed: bool,
}

/// パネル runtime と registry を保持する。
pub struct PanelRuntime {
    panels: Vec<Box<dyn PanelPlugin>>,
    persistent_panel_configs: BTreeMap<String, Value>,
    /// イベント駆動再描画のための dirty パネル集合。
    dirty_panels: BTreeSet<String>,
    /// GPU コンテキスト（device/queue/renderer/scene scratch）。
    gpu_ctx: Option<PanelGpuContext>,
    /// `workspace_layout` の登録パネル一覧 (id / title / visible) を表現する JSON。
    /// `sync_document_subset` の前に各 `BuiltinPanelPlugin` へ注入され、
    /// host snapshot の `workspace.panels_json` フィールドに反映される。
    workspace_panels_json: String,
}

impl Default for PanelRuntime {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new()
    }
}

impl PanelRuntime {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            persistent_panel_configs: BTreeMap::new(),
            dirty_panels: BTreeSet::new(),
            gpu_ctx: None,
            workspace_panels_json: EMPTY_WORKSPACE_PANELS_JSON.to_string(),
        }
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
    /// 9E-4: ステータスバーなど panel-runtime 外部の `HtmlPanelEngine` 利用者が
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

    /// GPU 直描画対応パネル (BuiltinPanelPlugin) の ID 一覧。
    pub fn panel_ids_with_gpu(&mut self) -> Vec<String> {
        let mut ids = Vec::new();
        for panel in &mut self.panels {
            let panel_id = panel.id().to_string();
            if let Some(any) = panel.as_any_mut() {
                if any.downcast_mut::<BuiltinPanelPlugin>().is_some() {
                    ids.push(panel_id);
                }
            }
        }
        ids
    }

    /// パネル毎の現在の権威サイズを返す。
    /// 戻り値: `Vec<(panel_id, width, height)>`。
    pub fn panel_measured_sizes(&mut self) -> Vec<(String, u32, u32)> {
        let mut out = Vec::new();
        for panel in &mut self.panels {
            let panel_id = panel.id().to_string();
            if let Some(engine) = panel_engine_mut(panel) {
                let (w, h) = engine.measured_size();
                out.push((panel_id, w, h));
            }
        }
        out
    }

    /// 指定パネル (DSL/HTML) の measured_size を返す。該当無しの場合は `(1, 1)`。
    pub fn measured_size(&mut self, panel_id: &str) -> (u32, u32) {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            if let Some(engine) = panel_engine_mut(panel) {
                return engine.measured_size();
            }
        }
        (1, 1)
    }

    /// 指定パネルに UI 入力イベントを転送する。`:hover` / `<details>` 開閉等の動的レイアウトを動かす。
    /// 戻り値: 該当パネルが見つかった場合 true。
    pub fn forward_panel_input(
        &mut self,
        panel_id: &str,
        event: panel_html_experiment::blitz_traits::events::UiEvent,
    ) -> bool {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            if let Some(engine) = panel_engine_mut(panel) {
                engine.on_input(event);
                return true;
            }
            return false;
        }
        false
    }

    /// Phase 11: 指定パネル root 要素の CSS `min/max-width/height` 制約を返す。
    /// リサイズ時のクランプ値として `compute_resized_rect` で使う。
    /// GPU パネルでない or 未登録の場合は `None` (= 制約なし)。
    pub fn panel_size_constraints(&mut self, panel_id: &str) -> Option<PanelSizeConstraints> {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            if let Some(engine) = panel_engine_mut(panel) {
                return Some(engine.root_size_constraints());
            }
            return None;
        }
        None
    }

    /// 指定パネルの meta.json `default_size` を返す。`None` は GPU パネルでないか未登録。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入する用途。
    pub fn panel_default_size(&mut self, panel_id: &str) -> Option<(u32, u32)> {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            let any = panel.as_any_mut()?;
            return any
                .downcast_ref::<BuiltinPanelPlugin>()
                .map(|p| p.default_size());
        }
        None
    }

    /// 起動時 restore 用：指定 panel_id に永続化された measured_size を流し込む。
    /// 戻り値: 該当パネルが見つかった場合 true。
    pub fn restore_panel_size(&mut self, panel_id: &str, size: (u32, u32)) -> bool {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            if let Some(engine) = panel_engine_mut(panel) {
                engine.on_load(size);
                return true;
            }
            return false;
        }
        false
    }

    /// 指定された (panel_id, width, height) リストの GPU パネルを描画する。
    /// `chrome_height` > 0 ならパネル上端にホスト描画タイトルバーを重ねる。
    /// `install_gpu_context` 未呼び出しなら空 Vec。
    pub fn render_panels(
        &mut self,
        sized: &[(String, u32, u32)],
        scale: f32,
        chrome_height: u32,
    ) -> Vec<PanelGpuFrame<'_>> {
        let Some(gpu_ctx) = self.gpu_ctx.as_mut() else {
            return Vec::new();
        };
        // ループ内で self.panels を可変借用するため、まず ID → 描画情報 のメタを集める
        type FrameTuple = (String, *const wgpu::Texture, u32, u32, Vec<RenderedPanelHit>, bool);
        let mut frames: Vec<FrameTuple> = Vec::new();
        for (panel_id, width, height) in sized {
            // 該当パネルを mutable で取得
            let Some(panel) = self.panels.iter_mut().find(|p| p.id() == panel_id.as_str()) else {
                continue;
            };
            let Some(engine) = panel_engine_mut(panel) else {
                continue;
            };
            let (rendered, texture_ptr, tw, th) = {
                let outcome = engine.on_render(
                    &gpu_ctx.device,
                    &gpu_ctx.queue,
                    &mut gpu_ctx.renderer,
                    &mut gpu_ctx.scene_scratch,
                    (*width, *height),
                    scale,
                    chrome_height,
                );
                let rendered = outcome.is_rendered();
                let target = outcome.target();
                let ptr: *const wgpu::Texture = &target.texture;
                (rendered, ptr, target.width, target.height)
            };
            let hits = engine.collect_action_rects();
            frames.push((panel_id.clone(), texture_ptr, tw, th, hits, rendered));
        }
        // SAFETY: 各 *const wgpu::Texture は self.panels 内の Box<dyn PanelPlugin> 内
        // engine が保持するテクスチャを指す。Box は heap に固定されており、戻り値の
        // PanelGpuFrame は &mut self に紐付くので、戻り値存在中は self.panels が
        // 不変に保たれる。テクスチャの寿命も同期する。
        frames
            .into_iter()
            .map(|(panel_id, ptr, w, h, hits, rendered)| PanelGpuFrame {
                panel_id,
                texture: unsafe { &*ptr },
                width: w,
                height: h,
                hit_regions: hits,
                rendered_this_frame: rendered,
            })
            .collect()
    }

    /// 現在の値を パネル へ変換する。
    pub fn register_panel(&mut self, mut panel: Box<dyn PanelPlugin>) {
        if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
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

    /// 現在の パネル 件数 を返す。
    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// 既存データを走査して パネル debug summaries を組み立てる。
    pub fn panel_debug_summaries(&self) -> Vec<(&'static str, &'static str, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id(), panel.title(), panel.debug_summary()))
            .collect()
    }

    /// 登録されたパネル ID / title の対 (登録順) を返す。
    /// builtin.workspace-layout が host snapshot 用に title を引くのに使う。
    pub fn panel_id_titles(&self) -> Vec<(String, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id().to_string(), panel.title().to_string()))
            .collect()
    }

    /// 登録されたパネル ID (登録順、`&'static str`) を返す。
    /// reconcile_workspace_layout 用。
    pub fn panel_static_ids(&self) -> Vec<&'static str> {
        self.panels.iter().map(|panel| panel.id()).collect()
    }

    /// persistent パネル configs を計算して返す。
    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        collect_persistent_panel_configs(&self.panels)
    }

    /// Persistent パネル configs を置き換える。
    pub fn replace_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        restore_persistent_panel_configs(&mut self.panels, &self.persistent_panel_configs);
    }

    /// 現在の値を イベント へ変換する。
    ///
    /// ADR 014 以降、HTML パネル経路では GPU 側 `render_dirty` が真の dirty 判定を持つため、
    /// runtime 側ではイベントを受けたパネルを無条件で `changed_panel_ids` に入れる。
    pub fn dispatch_event(&mut self, event: &PanelEvent) -> RuntimeDispatchResult {
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == event_panel_id(event))
        else {
            return RuntimeDispatchResult::default();
        };

        let actions = panel.handle_event(event);
        let mut changed_panel_ids = BTreeSet::new();
        changed_panel_ids.insert(panel.id().to_string());
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
        RuntimeDispatchResult {
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
    ) -> RuntimeKeyboardResult {
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
        RuntimeKeyboardResult {
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
            // BuiltinPanelPlugin にはホストスナップショット組立用の workspace 情報を注入する。
            if let Some(any) = panel.as_any_mut()
                && let Some(builtin) = any.downcast_mut::<BuiltinPanelPlugin>()
            {
                builtin.set_workspace_panels_json(workspace_json.clone());
            }
            panel.update(document, can_undo, can_redo, active_jobs, snapshot_count);
            changed_panels.insert(panel.id().to_string());
        }
        changed_panels
    }
}

/// イベント パネル ID を計算して返す。
fn event_panel_id(event: &PanelEvent) -> &str {
    match event {
        PanelEvent::Activate { panel_id, .. }
        | PanelEvent::SetValue { panel_id, .. }
        | PanelEvent::DragValue { panel_id, .. }
        | PanelEvent::SetText { panel_id, .. }
        | PanelEvent::Keyboard { panel_id, .. } => panel_id,
    }
}
