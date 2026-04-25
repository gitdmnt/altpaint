use crate::config::{collect_persistent_panel_configs, restore_persistent_panel_configs};
use crate::dsl_loader::collect_panel_files_recursive;
use crate::dsl_panel::DslPanelPlugin;
#[cfg(feature = "html-panel")]
use crate::html_panel::HtmlPanelPlugin;
use app_core::Document;
use panel_api::{HostAction, PanelEvent, PanelPlugin, PanelTree, PanelView};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[cfg(feature = "html-panel")]
use std::sync::Arc;
#[cfg(feature = "html-panel")]
use panel_html_experiment::{vello, wgpu, RenderedPanelHit};

/// `html-panel` feature 時、HTML パネル毎の GPU 描画結果をまとめて返す。
#[cfg(feature = "html-panel")]
pub struct HtmlPanelGpuFrame<'a> {
    pub panel_id: String,
    pub texture: &'a wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub hit_regions: Vec<RenderedPanelHit>,
    pub rendered_this_frame: bool,
}

/// 共有 wgpu リソース + 集約 vello::Renderer。`install_gpu_context` で初期化。
#[cfg(feature = "html-panel")]
struct PanelGpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    renderer: vello::Renderer,
    scene_scratch: vello::Scene,
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

/// DSL/Wasm panel runtime と registry を保持する。
pub struct PanelRuntime {
    panels: Vec<Box<dyn PanelPlugin>>,
    loaded_panel_ids: Vec<String>,
    panel_tree_cache: BTreeMap<String, PanelTree>,
    persistent_panel_configs: BTreeMap<String, Value>,
    /// イベント駆動再描画のための dirty パネル集合。
    dirty_panels: BTreeSet<String>,
    /// html-panel feature 時の GPU コンテキスト（device/queue/renderer/scene scratch）。
    #[cfg(feature = "html-panel")]
    gpu_ctx: Option<PanelGpuContext>,
    /// 直近 `render_html_panels` で measured_size が変化した panel の (id, (w, h)) 列。
    /// `take_html_size_changes` で吸い取って永続化に流す。
    #[cfg(feature = "html-panel")]
    pending_html_size_changes: Vec<(String, (u32, u32))>,
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
            loaded_panel_ids: Vec::new(),
            panel_tree_cache: BTreeMap::new(),
            persistent_panel_configs: BTreeMap::new(),
            dirty_panels: BTreeSet::new(),
            #[cfg(feature = "html-panel")]
            gpu_ctx: None,
            #[cfg(feature = "html-panel")]
            pending_html_size_changes: Vec::new(),
        }
    }

    /// 共有 wgpu Device/Queue を受け取り、vello::Renderer を集約構築する。
    /// 失敗時は `gpu_ctx = None` を維持し、HTML パネルは描画スキップにフォールバック。
    #[cfg(feature = "html-panel")]
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

    /// HTML パネル ID 一覧（dyn 起こし downcast 経由で識別）。
    #[cfg(feature = "html-panel")]
    pub fn html_panel_ids(&mut self) -> Vec<String> {
        let mut ids = Vec::new();
        for panel in &mut self.panels {
            if let Some(any) = panel.as_any_mut()
                && any.downcast_mut::<HtmlPanelPlugin>().is_some()
            {
                ids.push(panel.id().to_string());
            }
        }
        ids
    }

    /// HTML パネル毎の現在の権威サイズを返す。
    /// 戻り値: `Vec<(panel_id, width, height)>`。
    #[cfg(feature = "html-panel")]
    pub fn html_measured_sizes(&mut self) -> Vec<(String, u32, u32)> {
        let mut out = Vec::new();
        for panel in &mut self.panels {
            let panel_id = panel.id().to_string();
            let Some(any) = panel.as_any_mut() else { continue };
            let Some(html_plugin) = any.downcast_mut::<HtmlPanelPlugin>() else {
                continue;
            };
            let (w, h) = html_plugin.measured_size();
            out.push((panel_id, w, h));
        }
        out
    }

    /// 指定 HTML パネルに UI 入力イベントを転送する。`:hover` / `<details>` 開閉等の動的レイアウトを動かす。
    /// 戻り値: 該当パネルが見つかった場合 true。
    #[cfg(feature = "html-panel")]
    pub fn forward_html_input(
        &mut self,
        panel_id: &str,
        event: panel_html_experiment::blitz_traits::events::UiEvent,
    ) -> bool {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            let Some(any) = panel.as_any_mut() else { return false };
            let Some(html_plugin) = any.downcast_mut::<HtmlPanelPlugin>() else {
                return false;
            };
            html_plugin.forward_input(event);
            return true;
        }
        false
    }

    /// 起動時 restore 用：指定 panel_id に永続化された measured_size を流し込む。
    /// 戻り値: 該当 HTML パネルが見つかった場合 true。
    #[cfg(feature = "html-panel")]
    pub fn restore_html_panel_size(&mut self, panel_id: &str, size: (u32, u32)) -> bool {
        for panel in &mut self.panels {
            if panel.id() != panel_id {
                continue;
            }
            let Some(any) = panel.as_any_mut() else { return false };
            let Some(html_plugin) = any.downcast_mut::<HtmlPanelPlugin>() else {
                return false;
            };
            html_plugin.restore_size(size);
            return true;
        }
        false
    }

    /// `render_html_panels` 内で発生した measured_size 変化を吸い取る（take セマンティクス）。
    /// 戻り値: `(panel_id, (new_w, new_h))` の列。二回目の呼び出しは空。
    #[cfg(feature = "html-panel")]
    pub fn take_html_size_changes(&mut self) -> Vec<(String, (u32, u32))> {
        std::mem::take(&mut self.pending_html_size_changes)
    }

    /// テスト専用: size 変化を強制注入する。本番経路では `render_html_panels` 内で記録される。
    #[cfg(all(feature = "html-panel", test))]
    pub fn test_mark_html_size_changed(&mut self, panel_id: &str, size: (u32, u32)) {
        self.pending_html_size_changes
            .push((panel_id.to_string(), size));
    }

    /// 指定された (panel_id, width, height) リストの HTML パネルを GPU 描画する。
    /// `chrome_height` > 0 ならパネル上端にホスト描画タイトルバーを重ねる。
    /// `install_gpu_context` 未呼び出しなら空 Vec。
    #[cfg(feature = "html-panel")]
    pub fn render_html_panels(
        &mut self,
        sized: &[(String, u32, u32)],
        scale: f32,
        chrome_height: u32,
    ) -> Vec<HtmlPanelGpuFrame<'_>> {
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
            let Some(any) = panel.as_any_mut() else {
                continue;
            };
            let Some(html_plugin) = any.downcast_mut::<HtmlPanelPlugin>() else {
                continue;
            };
            let (rendered, texture_ptr, tw, th) = {
                let outcome = html_plugin.render_gpu(
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
            // size 変化があれば pending リストへ。take_html_size_changes で吸い取られる。
            if let Some(new_size) = html_plugin.take_size_change() {
                self.pending_html_size_changes
                    .push((panel_id.clone(), new_size));
            }
            let hits = html_plugin.collect_action_rects();
            frames.push((panel_id.clone(), texture_ptr, tw, th, hits, rendered));
        }
        // SAFETY: 各 *const wgpu::Texture は self.panels 内の Box<HtmlPanelPlugin> 内テクスチャを指す。
        // 戻り値の HtmlPanelGpuFrame の借用は &mut self に紐付くので、戻り値存在中は self.panels が
        // 不変に保たれる。テクスチャの寿命も同期する。
        frames
            .into_iter()
            .map(|(panel_id, ptr, w, h, hits, rendered)| HtmlPanelGpuFrame {
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
        self.panel_tree_cache.remove(panel.id());
        self.dirty_panels.insert(panel.id().to_string());
        self.panels.push(panel);
        if let Some(panel) = self.panels.last() {
            self.panel_tree_cache
                .insert(panel.id().to_string(), panel.panel_tree());
        }
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

    /// 入力や種別に応じて処理を振り分ける。
    pub fn load_panel_directory(&mut self, directory: impl AsRef<Path>) -> Vec<String> {
        let directory = directory.as_ref();
        self.remove_loaded_panels();

        let mut panel_files = Vec::new();
        if collect_panel_files_recursive(directory, &mut panel_files).is_err() {
            return Vec::new();
        }
        panel_files.sort();

        let mut diagnostics = Vec::new();

        // DSL パネル: 既存と同じロジック
        for path in &panel_files {
            match panel_dsl::load_panel_file(path) {
                Ok(definition) => {
                    let panel_id = definition.manifest.id.clone();
                    match DslPanelPlugin::from_definition(definition) {
                        Ok(panel) => {
                            self.loaded_panel_ids.push(panel_id);
                            self.register_panel(Box::new(panel));
                        }
                        Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
                    }
                }
                Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
            }
        }

        // html-panel feature: 各パネルディレクトリに panel.html + panel.meta.json があれば
        // 並列して HTML パネルも登録する（ID が異なれば DSL 版と共存できる）。
        #[cfg(feature = "html-panel")]
        {
            let mut seen_dirs: std::collections::BTreeSet<std::path::PathBuf> =
                std::collections::BTreeSet::new();
            for path in &panel_files {
                let Some(parent) = path.parent() else { continue };
                if !seen_dirs.insert(parent.to_path_buf()) {
                    continue;
                }
                if parent.join("panel.html").exists() && parent.join("panel.meta.json").exists() {
                    // restored_size は Phase 5 で workspace_layout から引いて渡す。
                    // 現状はロード時 intrinsic 測定で初期化する。
                    match HtmlPanelPlugin::load(parent, None) {
                        Ok(panel) => {
                            let panel_id = panel.id().to_string();
                            self.loaded_panel_ids.push(panel_id);
                            self.register_panel(Box::new(panel));
                        }
                        Err(error) => {
                            diagnostics.push(format!("{}: {error}", parent.display()));
                        }
                    }
                }
            }
        }

        diagnostics
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

    /// 既存データを走査して パネル views を組み立てる。
    pub fn panel_views(&self) -> Vec<PanelView> {
        self.panels.iter().map(|panel| panel.view()).collect()
    }

    /// 既存データを走査して パネル trees を組み立てる。
    pub fn panel_trees(&self) -> Vec<PanelTree> {
        self.panels
            .iter()
            .map(|panel| {
                self.panel_tree_cache
                    .get(panel.id())
                    .cloned()
                    .unwrap_or_else(|| panel.panel_tree())
            })
            .collect()
    }

    /// persistent パネル configs を計算して返す。
    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        collect_persistent_panel_configs(&self.panels)
    }

    /// Persistent パネル configs を置き換える。
    pub fn replace_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        restore_persistent_panel_configs(&mut self.panels, &self.persistent_panel_configs);
        self.rebuild_tree_cache();
    }

    /// 現在の値を イベント へ変換する。
    pub fn dispatch_event(&mut self, event: &PanelEvent) -> RuntimeDispatchResult {
        let previous_configs = collect_persistent_panel_configs(&self.panels);
        let Some(panel) = self
            .panels
            .iter_mut()
            .find(|panel| panel.id() == event_panel_id(event))
        else {
            return RuntimeDispatchResult::default();
        };

        let previous_tree = self
            .panel_tree_cache
            .get(panel.id())
            .cloned()
            .unwrap_or_else(|| panel.panel_tree());
        let actions = panel.handle_event(event);
        let next_tree = panel.panel_tree();
        self.panel_tree_cache
            .insert(panel.id().to_string(), next_tree.clone());
        let mut changed_panel_ids = BTreeSet::new();
        if next_tree != previous_tree {
            changed_panel_ids.insert(panel.id().to_string());
        }
        let config_changed = collect_persistent_panel_configs(&self.panels) != previous_configs;
        RuntimeDispatchResult {
            actions,
            changed_panel_ids,
            config_changed,
        }
    }

    /// 現在の値を キーボード へ変換する。
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
            let previous_tree = self
                .panel_tree_cache
                .get(panel.id())
                .cloned()
                .unwrap_or_else(|| panel.panel_tree());
            let previous_config = panel.persistent_config();
            let panel_actions = panel.handle_event(&PanelEvent::Keyboard {
                panel_id: panel.id().to_string(),
                shortcut: shortcut.to_string(),
                key: key.to_string(),
                repeat,
            });
            let next_tree = panel.panel_tree();
            self.panel_tree_cache
                .insert(panel.id().to_string(), next_tree.clone());
            let keyboard_handled = !panel_actions.is_empty()
                || next_tree != previous_tree
                || panel.persistent_config() != previous_config;
            if next_tree != previous_tree {
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
    fn sync_document_subset(
        &mut self,
        document: &Document,
        panel_ids: Option<&BTreeSet<String>>,
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) -> BTreeSet<String> {
        let mut changed_panels = BTreeSet::new();
        for panel in &mut self.panels {
            if panel_ids.is_some_and(|panel_ids| !panel_ids.contains(panel.id())) {
                continue;
            }
            let previous_tree = self
                .panel_tree_cache
                .get(panel.id())
                .cloned()
                .unwrap_or_else(|| panel.panel_tree());
            panel.update(document, can_undo, can_redo, active_jobs, snapshot_count);
            let next_tree = panel.panel_tree();
            self.panel_tree_cache
                .insert(panel.id().to_string(), next_tree.clone());
            if next_tree != previous_tree {
                changed_panels.insert(panel.id().to_string());
            }
        }
        changed_panels
    }

    /// Loaded panels を削除する。
    fn remove_loaded_panels(&mut self) {
        if self.loaded_panel_ids.is_empty() {
            return;
        }
        self.panels.retain(|panel| {
            !self
                .loaded_panel_ids
                .iter()
                .any(|loaded_id| loaded_id == panel.id())
        });
        self.loaded_panel_ids.clear();
        self.rebuild_tree_cache();
    }

    /// Tree cache を再構築する。
    fn rebuild_tree_cache(&mut self) {
        self.panel_tree_cache.clear();
        for panel in &self.panels {
            self.panel_tree_cache
                .insert(panel.id().to_string(), panel.panel_tree());
        }
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
