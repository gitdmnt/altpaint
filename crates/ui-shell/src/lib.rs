//! `ui-shell` は panel runtime と panel presentation の統合境界を提供する。
//!
//! runtime・workspace 管理・focus/text input・software rendering を module 単位へ分割し、
//! `UiShell` 本体は公開 API と状態所有に集中させる。

mod dsl;
mod focus;
mod presentation;
mod surface_render;
mod text;
mod tree_query;
mod workspace;

#[cfg(test)]
mod tests;

pub use text::{
    draw_text_rgba, line_height as text_line_height, measure_text_width, text_backend_name,
    wrap_text_lines,
};

use app_core::{ColorRgba8, Command, Document, ToolKind, WorkspaceLayout, WorkspacePanelState};
use panel_dsl::{AttrValue as DslAttrValue, PanelDefinition, StateField, ViewElement, ViewNode};
use panel_schema::{CommandDescriptor, Diagnostic, PanelEventRequest, PanelInitRequest, StatePatch};
use plugin_api::{
    DropdownOption, HostAction, LayerListItem, PanelEvent, PanelMoveDirection, PanelNode,
    PanelPlugin, PanelTree, PanelView, TextInputMode,
};
use plugin_host::{PluginHostError, WasmPanelRuntime};
pub use presentation::PanelSurface;
use presentation::{FocusTarget, PanelRenderState, TextInputEditorState};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use surface_render::PANEL_SCROLL_PIXELS_PER_LINE;
use workspace::{event_panel_id, workspace_panel_actions, WORKSPACE_PANEL_ID};

use crate::presentation::{PanelHitKind, PanelHitRegion};

/// パネルホストとして振る舞う UI シェル。
pub struct UiShell {
    /// 登録済みの panel plugin 一覧。
    panels: Vec<Box<dyn PanelPlugin>>,
    /// 最新の document snapshot。
    latest_document: Option<Document>,
    /// DSL 読込由来 panel id 群。
    loaded_panel_ids: Vec<String>,
    /// panel 並び順と表示状態。
    workspace_layout: WorkspaceLayout,
    /// スクロール前 content surface のキャッシュ。
    panel_content_cache: Option<PanelSurface>,
    /// panel content を再構築すべきかのフラグ。
    panel_content_dirty: bool,
    /// 現在の縦スクロール量。
    panel_scroll_offset: usize,
    /// content 全体の高さ。
    panel_content_height: usize,
    /// 現在 focus 中の node。
    focused_target: Option<FocusTarget>,
    /// 展開中 dropdown。
    expanded_dropdown: Option<FocusTarget>,
    /// text input ごとの editor state。
    text_input_states: BTreeMap<(String, String), TextInputEditorState>,
    /// panel ごとの persistent config。
    persistent_panel_configs: BTreeMap<String, Value>,
}

impl UiShell {
    /// 空の UI シェルを作成する。
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            latest_document: None,
            loaded_panel_ids: Vec::new(),
            workspace_layout: WorkspaceLayout::default(),
            panel_content_cache: None,
            panel_content_dirty: true,
            panel_scroll_offset: 0,
            panel_content_height: 0,
            focused_target: None,
            expanded_dropdown: None,
            text_input_states: BTreeMap::new(),
            persistent_panel_configs: BTreeMap::new(),
        }
    }

    /// panel plugin を 1 つ登録する。
    pub fn register_panel(&mut self, mut panel: Box<dyn PanelPlugin>) {
        if let Some(document) = self.latest_document.as_ref() {
            panel.update(document);
        }
        if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
        self.ensure_workspace_panel_entry(panel.id());
        self.panels.retain(|registered| registered.id() != panel.id());
        self.panels.push(panel);
        self.reconcile_workspace_layout();
        self.panel_content_dirty = true;
    }

    /// ディレクトリ以下の DSL panel を再帰ロードする。
    pub fn load_panel_directory(&mut self, directory: impl AsRef<Path>) -> Vec<String> {
        let directory = directory.as_ref();
        self.remove_loaded_panels();

        let mut panel_files = Vec::new();
        if dsl::collect_panel_files_recursive(directory, &mut panel_files).is_err() {
            return Vec::new();
        }
        panel_files.sort();

        let mut diagnostics = Vec::new();
        for path in panel_files {
            match panel_dsl::load_panel_file(&path) {
                Ok(definition) => {
                    let panel_id = definition.manifest.id.clone();
                    match dsl::DslPanelPlugin::from_definition(definition) {
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

        diagnostics
    }

    /// 最新 document を panel 群へ配送する。
    pub fn update(&mut self, document: &Document) {
        self.latest_document = Some(document.clone());
        for panel in &mut self.panels {
            panel.update(document);
        }
        self.panel_content_dirty = true;
    }

    /// 登録済み panel 数を返す。
    pub fn panel_count(&self) -> usize { self.panels.len() }

    /// 登録済み panel の最小デバッグ情報を返す。
    pub fn panel_debug_summaries(&self) -> Vec<(&'static str, &'static str, String)> {
        self.panels.iter().map(|panel| (panel.id(), panel.title(), panel.debug_summary())).collect()
    }

    /// 登録済み panel の `PanelView` 一覧を返す。
    pub fn panel_views(&self) -> Vec<PanelView> {
        self.panels.iter().map(|panel| panel.view()).collect()
    }

    /// workspace 管理 panel を含む `PanelTree` 一覧を返す。
    pub fn panel_trees(&self) -> Vec<PanelTree> {
        let mut trees = vec![self.workspace_manager_tree()];
        trees.extend(self.visible_panels_in_order().map(|panel| panel.panel_tree()));
        trees
    }

    /// 現在の workspace layout を返す。
    pub fn workspace_layout(&self) -> WorkspaceLayout { self.workspace_layout.clone() }

    /// workspace layout を置き換える。
    pub fn set_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.reconcile_workspace_layout();
        self.panel_content_dirty = true;
    }

    /// 現在 focus 中の `(panel_id, node_id)` を返す。
    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target.as_ref().map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }

    /// 現在の panel スクロール量を返す。
    pub fn panel_scroll_offset(&self) -> usize { self.panel_scroll_offset }

    /// マウスホイール相当のスクロールを適用する。
    pub fn scroll_panels(&mut self, delta_lines: i32, viewport_height: usize) -> bool {
        let delta_pixels = delta_lines.saturating_mul(PANEL_SCROLL_PIXELS_PER_LINE);
        let max_offset = self.max_panel_scroll_offset(viewport_height) as i32;
        let next_offset = (self.panel_scroll_offset as i32 + delta_pixels).clamp(0, max_offset) as usize;
        if next_offset == self.panel_scroll_offset {
            return false;
        }
        self.panel_scroll_offset = next_offset;
        true
    }

    /// panel event を適切な panel または workspace manager へ配送する。
    pub fn handle_panel_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        if let PanelEvent::Activate { panel_id, node_id } = event {
            let _ = self.focus_panel_node(panel_id, node_id);
            if self.is_dropdown_target(panel_id, node_id) {
                let dropdown = FocusTarget { panel_id: panel_id.clone(), node_id: node_id.clone() };
                self.expanded_dropdown = if self.expanded_dropdown.as_ref() == Some(&dropdown) { None } else { Some(dropdown) };
                self.panel_content_dirty = true;
                return Vec::new();
            }
        }
        if let PanelEvent::SetText { panel_id, node_id, .. } = event
            && self.is_dropdown_target(panel_id, node_id)
        {
            self.expanded_dropdown = None;
        }
        if event_panel_id(event) == WORKSPACE_PANEL_ID {
            let actions = workspace_panel_actions(self.workspace_manager_tree().children.as_slice(), event);
            self.panel_content_dirty = true;
            return actions;
        }
        let actions = self.panels.iter_mut().flat_map(|panel| panel.handle_event(event)).collect();
        self.panel_content_dirty = true;
        actions
    }

    /// keyboard event を keyboard handler を持つ panel へ配送する。
    pub fn handle_keyboard_event(&mut self, shortcut: &str, key: &str, repeat: bool) -> (bool, Vec<HostAction>) {
        let mut handled = false;
        let mut actions = Vec::new();
        for panel in &mut self.panels {
            if !panel.handles_keyboard_event() {
                continue;
            }
            let previous_tree = panel.panel_tree();
            let previous_config = panel.persistent_config();
            let panel_actions = panel.handle_event(&PanelEvent::Keyboard {
                panel_id: panel.id().to_string(),
                shortcut: shortcut.to_string(),
                key: key.to_string(),
                repeat,
            });
            let keyboard_handled = !panel_actions.is_empty()
                || panel.panel_tree() != previous_tree
                || panel.persistent_config() != previous_config;
            handled |= keyboard_handled;
            actions.extend(panel_actions);
        }
        if handled {
            self.panel_content_dirty = true;
        }
        (handled, actions)
    }

    /// 永続化対象 panel config 群を抽出する。
    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        self.panels
            .iter()
            .filter_map(|panel| panel.persistent_config().map(|config| (panel.id().to_string(), config)))
            .collect()
    }

    /// 永続化済み panel config 群を復元する。
    pub fn set_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        for panel in &mut self.panels {
            if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
                panel.restore_persistent_config(config);
            }
        }
        self.panel_content_dirty = true;
    }
}

impl Default for UiShell {
    fn default() -> Self { Self::new() }
}
