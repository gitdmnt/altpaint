//! `ui-shell` はアプリケーションウィンドウ上でパネルをホストする最小UI層。
//!
//! フェーズ0では、個々のパネル機能そのものは持たず、
//! `PanelPlugin` 群を束ねる薄い境界として機能する。

mod presentation;
mod text;

pub use text::{
    draw_text_rgba, line_height as text_line_height, measure_text_width, text_backend_name,
    wrap_text_lines,
};

use app_core::{ColorRgba8, Command, Document, ToolKind, WorkspaceLayout, WorkspacePanelState};
use panel_dsl::{AttrValue as DslAttrValue, PanelDefinition, StateField, ViewElement, ViewNode};
use panel_schema::{
    CommandDescriptor, Diagnostic, PanelEventRequest, PanelInitRequest, StatePatch,
};
use plugin_api::{
    DropdownOption, HostAction, LayerListItem, PanelEvent, PanelMoveDirection, PanelNode,
    PanelPlugin, PanelTree, PanelView, TextInputMode,
};
use plugin_host::{PluginHostError, WasmPanelRuntime};
pub use presentation::PanelSurface;
use presentation::{FocusTarget, PanelHitKind, PanelHitRegion, PanelRenderState, TextInputEditorState};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const SIDEBAR_BACKGROUND: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
const PANEL_BACKGROUND: [u8; 4] = [0x1f, 0x1f, 0x1f, 0xff];
const PANEL_BORDER: [u8; 4] = [0x3f, 0x3f, 0x3f, 0xff];
const PANEL_TITLE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
const SECTION_TITLE: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
const BODY_TEXT: [u8; 4] = [0xd8, 0xd8, 0xd8, 0xff];
const BUTTON_FILL: [u8; 4] = [0x32, 0x32, 0x32, 0xff];
const BUTTON_ACTIVE_FILL: [u8; 4] = [0x44, 0x5f, 0xb0, 0xff];
const BUTTON_BORDER: [u8; 4] = [0x56, 0x56, 0x56, 0xff];
const BUTTON_ACTIVE_BORDER: [u8; 4] = [0xc6, 0xd4, 0xff, 0xff];
const BUTTON_FOCUS_BORDER: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
const BUTTON_TEXT: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const BUTTON_TEXT_DARK: [u8; 4] = [0x14, 0x14, 0x14, 0xff];
const SLIDER_TRACK_BACKGROUND: [u8; 4] = [0x2c, 0x2c, 0x2c, 0xff];
const SLIDER_TRACK_BORDER: [u8; 4] = [0x5f, 0x5f, 0x5f, 0xff];
const SLIDER_KNOB: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const PREVIEW_SWATCH_BORDER: [u8; 4] = [0x74, 0x74, 0x74, 0xff];
const PANEL_OUTER_PADDING: usize = 8;
const PANEL_INNER_PADDING: usize = 8;
const NODE_GAP: usize = 6;
const SECTION_GAP: usize = 4;
const SECTION_INDENT: usize = 10;
const BUTTON_HEIGHT: usize = 24;
const COLOR_PREVIEW_HEIGHT: usize = 52;
const INPUT_BOX_HEIGHT: usize = 24;
const SLIDER_HEIGHT: usize = 32;
const SLIDER_TRACK_HEIGHT: usize = 8;
const SLIDER_TRACK_TOP: usize = 20;
const SLIDER_KNOB_WIDTH: usize = 8;
const DROPDOWN_HEIGHT: usize = 24;
const LAYER_LIST_ITEM_HEIGHT: usize = 38;
const LAYER_LIST_DETAIL_OFFSET: usize = 18;
const LAYER_LIST_DRAG_HANDLE_WIDTH: usize = 14;
const INPUT_BACKGROUND: [u8; 4] = [0x15, 0x15, 0x15, 0xff];
const INPUT_BORDER: [u8; 4] = [0x56, 0x56, 0x56, 0xff];
const INPUT_PLACEHOLDER: [u8; 4] = [0x88, 0x88, 0x88, 0xff];
const WORKSPACE_PANEL_ID: &str = "builtin.workspace-layout";
const MAX_DOCUMENT_DIMENSION: usize = 8192;
const MAX_DOCUMENT_PIXELS: usize = 16_777_216;
const PANEL_SCROLL_PIXELS_PER_LINE: i32 = 48;

/// パネルホストとして振る舞う最小UIシェル。
pub struct UiShell {
    /// 登録済みのパネルプラグイン一覧。
    panels: Vec<Box<dyn PanelPlugin>>,
    latest_document: Option<Document>,
    loaded_panel_ids: Vec<String>,
    workspace_layout: WorkspaceLayout,
    panel_content_cache: Option<PanelSurface>,
    panel_content_dirty: bool,
    panel_scroll_offset: usize,
    panel_content_height: usize,
    focused_target: Option<FocusTarget>,
    expanded_dropdown: Option<FocusTarget>,
    text_input_states: BTreeMap<(String, String), TextInputEditorState>,
    persistent_panel_configs: BTreeMap<String, Value>,
}

impl UiShell {
    /// 空のUIシェルを作成する。
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

    /// パネルプラグインを1つ登録する。
    pub fn register_panel(&mut self, mut panel: Box<dyn PanelPlugin>) {
        if let Some(document) = self.latest_document.as_ref() {
            panel.update(document);
        }
        if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
        self.ensure_workspace_panel_entry(panel.id());
        self.panels
            .retain(|registered| registered.id() != panel.id());
        self.panels.push(panel);
        self.reconcile_workspace_layout();
        self.panel_content_dirty = true;
    }

    pub fn load_panel_directory(&mut self, directory: impl AsRef<Path>) -> Vec<String> {
        let directory = directory.as_ref();
        self.remove_loaded_panels();

        let mut panel_files = Vec::new();
        if collect_panel_files_recursive(directory, &mut panel_files).is_err() {
            return Vec::new();
        }
        panel_files.sort();

        let mut diagnostics = Vec::new();
        for path in panel_files {
            match panel_dsl::load_panel_file(&path) {
                Ok(definition) => {
                    let panel_id = definition.manifest.id.clone();
                    match DslPanelPlugin::from_definition(definition) {
                        Ok(panel) => {
                            self.loaded_panel_ids.push(panel_id);
                            self.register_panel(Box::new(panel));
                        }
                        Err(error) => {
                            diagnostics.push(format!("{}: {error}", path.display()));
                        }
                    }
                }
                Err(error) => diagnostics.push(format!("{}: {error}", path.display())),
            }
        }

        diagnostics
    }

    /// ドキュメント更新をレンダラと各パネルへ配送する。
    pub fn update(&mut self, document: &Document) {
        self.latest_document = Some(document.clone());
        for panel in &mut self.panels {
            panel.update(document);
        }
        self.panel_content_dirty = true;
    }

    /// 現在登録されているパネル数を返す。
    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// 現在登録されているパネルの最小デバッグ情報を返す。
    pub fn panel_debug_summaries(&self) -> Vec<(&'static str, &'static str, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id(), panel.title(), panel.debug_summary()))
            .collect()
    }

    pub fn panel_views(&self) -> Vec<PanelView> {
        self.panels.iter().map(|panel| panel.view()).collect()
    }

    pub fn panel_trees(&self) -> Vec<PanelTree> {
        let mut trees = vec![self.workspace_manager_tree()];
        trees.extend(
            self.visible_panels_in_order()
                .map(|panel| panel.panel_tree()),
        );
        trees
    }

    pub fn workspace_layout(&self) -> WorkspaceLayout {
        self.workspace_layout.clone()
    }

    pub fn set_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.reconcile_workspace_layout();
        self.panel_content_dirty = true;
    }

    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }

    pub fn panel_scroll_offset(&self) -> usize {
        self.panel_scroll_offset
    }

    pub fn focus_panel_node(&mut self, panel_id: &str, node_id: &str) -> bool {
        let exists = self
            .focusable_targets()
            .iter()
            .any(|target| target.panel_id == panel_id && target.node_id == node_id);
        if !exists {
            return false;
        }

        let next = FocusTarget {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        };
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        if let Some(focused) = self.focused_target.clone()
            && let Some((value, _)) = self.text_input_state_for_target(&focused)
        {
            self.ensure_text_input_editor_state(&focused, &value);
        }
        self.panel_content_dirty = true;
        true
    }

    pub fn focus_next(&mut self) -> bool {
        self.move_focus(1)
    }

    pub fn focus_previous(&mut self) -> bool {
        self.move_focus(-1)
    }

    pub fn activate_focused(&mut self) -> Vec<HostAction> {
        let Some(target) = self.focused_target.clone() else {
            return Vec::new();
        };

        self.handle_panel_event(&PanelEvent::Activate {
            panel_id: target.panel_id,
            node_id: target.node_id,
        })
    }

    pub fn scroll_panels(&mut self, delta_lines: i32, viewport_height: usize) -> bool {
        let delta_pixels = delta_lines.saturating_mul(PANEL_SCROLL_PIXELS_PER_LINE);
        let max_offset = self.max_panel_scroll_offset(viewport_height) as i32;
        let next_offset = (self.panel_scroll_offset as i32 + delta_pixels).clamp(0, max_offset);
        let next_offset = next_offset as usize;
        if next_offset == self.panel_scroll_offset {
            return false;
        }

        self.panel_scroll_offset = next_offset;
        true
    }

    pub fn handle_panel_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        if let PanelEvent::Activate { panel_id, node_id } = event {
            let _ = self.focus_panel_node(panel_id, node_id);
            if self.is_dropdown_target(panel_id, node_id) {
                let dropdown = FocusTarget {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                };
                self.expanded_dropdown = if self.expanded_dropdown.as_ref() == Some(&dropdown) {
                    None
                } else {
                    Some(dropdown)
                };
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
            let actions =
                workspace_panel_actions(self.workspace_manager_tree().children.as_slice(), event);
            self.panel_content_dirty = true;
            return actions;
        }
        let actions = self
            .panels
            .iter_mut()
            .flat_map(|panel| panel.handle_event(event))
            .collect();
        self.panel_content_dirty = true;
        actions
    }

    pub fn handle_keyboard_event(
        &mut self,
        shortcut: &str,
        key: &str,
        repeat: bool,
    ) -> (bool, Vec<HostAction>) {
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

    pub fn persistent_panel_configs(&self) -> BTreeMap<String, Value> {
        self.panels
            .iter()
            .filter_map(|panel| {
                panel
                    .persistent_config()
                    .map(|config| (panel.id().to_string(), config))
            })
            .collect()
    }

    pub fn set_persistent_panel_configs(&mut self, configs: BTreeMap<String, Value>) {
        self.persistent_panel_configs = configs;
        for panel in &mut self.panels {
            if let Some(config) = self.persistent_panel_configs.get(panel.id()) {
                panel.restore_persistent_config(config);
            }
        }
        self.panel_content_dirty = true;
    }

    pub fn has_focused_text_input(&self) -> bool {
        self.focused_target
            .as_ref()
            .and_then(|target| self.text_input_state_for_target(target))
            .is_some()
    }

    pub fn insert_text_into_focused_input(&mut self, text: &str) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, input_mode)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let filtered = filter_text_input(text, input_mode);
        if filtered.is_empty() {
            return false;
        }
        let mut editor_state = self.editor_state_for_target(&target, &current);
        let next_value = insert_text_at_char_index(&current, editor_state.cursor_chars, &filtered);
        editor_state.cursor_chars += text_char_len(&filtered);
        editor_state.preedit = None;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.handle_panel_event(&PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: next_value,
        });
        true
    }

    pub fn backspace_focused_input(&mut self) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((mut current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit.take().is_some() {
            self.text_input_states
                .insert(text_input_state_key(&target), editor_state);
            self.panel_content_dirty = true;
            return true;
        }
        if current.is_empty() || editor_state.cursor_chars == 0 {
            return false;
        }
        current = remove_char_before_char_index(&current, editor_state.cursor_chars);
        editor_state.cursor_chars -= 1;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.handle_panel_event(&PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: current,
        });
        true
    }

    pub fn delete_focused_input(&mut self) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit.take().is_some() {
            self.text_input_states
                .insert(text_input_state_key(&target), editor_state);
            self.panel_content_dirty = true;
            return true;
        }
        if editor_state.cursor_chars >= text_char_len(&current) {
            return false;
        }
        let next_value = remove_char_at_char_index(&current, editor_state.cursor_chars);
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.handle_panel_event(&PanelEvent::SetText {
            panel_id: target.panel_id,
            node_id: target.node_id,
            value: next_value,
        });
        true
    }

    pub fn move_focused_input_cursor(&mut self, delta_chars: isize) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        editor_state.preedit = None;
        let max_chars = text_char_len(&current) as isize;
        let next_cursor = (editor_state.cursor_chars as isize + delta_chars).clamp(0, max_chars);
        let next_cursor = next_cursor as usize;
        if next_cursor == editor_state.cursor_chars {
            return false;
        }
        editor_state.cursor_chars = next_cursor;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.panel_content_dirty = true;
        true
    }

    pub fn move_focused_input_cursor_to_start(&mut self) -> bool {
        self.set_focused_input_cursor(0)
    }

    pub fn move_focused_input_cursor_to_end(&mut self) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        self.set_focused_input_cursor(text_char_len(&current))
    }

    pub fn set_focused_input_preedit(&mut self, preedit: Option<String>) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        if editor_state.preedit == preedit {
            return false;
        }
        editor_state.preedit = preedit.filter(|value| !value.is_empty());
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.panel_content_dirty = true;
        true
    }

    pub fn move_panel(&mut self, panel_id: &str, direction: PanelMoveDirection) -> bool {
        let Some(index) = self
            .workspace_layout
            .panels
            .iter()
            .position(|entry| entry.id == panel_id)
        else {
            return false;
        };

        let target_index = match direction {
            PanelMoveDirection::Up if index > 0 => index - 1,
            PanelMoveDirection::Down if index + 1 < self.workspace_layout.panels.len() => index + 1,
            _ => return false,
        };

        self.workspace_layout.panels.swap(index, target_index);
        self.panel_content_dirty = true;
        true
    }

    pub fn set_panel_visibility(&mut self, panel_id: &str, visible: bool) -> bool {
        if panel_id == WORKSPACE_PANEL_ID {
            return false;
        }

        let Some(entry) = self
            .workspace_layout
            .panels
            .iter_mut()
            .find(|entry| entry.id == panel_id)
        else {
            return false;
        };

        if entry.visible == visible {
            return false;
        }

        entry.visible = visible;
        if !visible
            && self
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id)
        {
            self.focused_target = None;
        }
        self.panel_content_dirty = true;
        true
    }

    pub fn render_panel_surface(&mut self, width: usize, height: usize) -> PanelSurface {
        let width = width.max(1);
        let height = height.max(1);
        let panel_width = width.saturating_sub(PANEL_OUTER_PADDING * 2);
        let needs_rebuild = self.panel_content_dirty
            || self
                .panel_content_cache
                .as_ref()
                .is_none_or(|content| content.width != width);
        if needs_rebuild {
            self.panel_content_cache = Some(self.build_panel_content_surface(width, panel_width));
            self.panel_content_dirty = false;
        }

        self.panel_content_height = self
            .panel_content_cache
            .as_ref()
            .map(|content| content.height)
            .unwrap_or(0);
        self.panel_scroll_offset = self
            .panel_scroll_offset
            .min(self.max_panel_scroll_offset(height));

        viewport_panel_surface(
            self.panel_content_cache
                .as_ref()
                .expect("panel content cache exists"),
            height,
            self.panel_scroll_offset,
        )
    }

    fn build_panel_content_surface(&mut self, width: usize, panel_width: usize) -> PanelSurface {
        let trees = self.panel_trees();
        let focused_target = self.focused_target.clone();
        let expanded_dropdown = self.expanded_dropdown.clone();
        let text_input_states = self.text_input_states.clone();
        let render_state = PanelRenderState {
            focused_target: focused_target.as_ref(),
            expanded_dropdown: expanded_dropdown.as_ref(),
            text_input_states: &text_input_states,
        };
        self.panel_content_height = measure_panel_content_height(&trees, panel_width, render_state);

        let content_height = self.panel_content_height.max(1);
        let mut content = PanelSurface {
            width,
            height: content_height,
            pixels: vec![0; width * content_height * 4],
            hit_regions: Vec::new(),
        };
        fill_rect(
            &mut content,
            0,
            0,
            width,
            content_height,
            SIDEBAR_BACKGROUND,
        );

        let mut cursor_y = PANEL_OUTER_PADDING;
        for tree in trees {
            let panel_height = measure_panel_tree(&tree, panel_width, render_state);
            fill_rect(
                &mut content,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                panel_height,
                PANEL_BACKGROUND,
            );
            stroke_rect(
                &mut content,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                panel_height,
                PANEL_BORDER,
            );
            draw_panel_tree(
                &mut content,
                &tree,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                render_state,
            );
            cursor_y += panel_height + PANEL_OUTER_PADDING;
        }

        content
    }

    fn move_focus(&mut self, step: isize) -> bool {
        let targets = self.focusable_targets();
        if targets.is_empty() {
            return false;
        }

        let current_index = self.focused_target.as_ref().and_then(|current| {
            targets.iter().position(|target| {
                target.panel_id == current.panel_id && target.node_id == current.node_id
            })
        });
        let next_index = match current_index {
            Some(index) => (index as isize + step).rem_euclid(targets.len() as isize) as usize,
            None if step >= 0 => 0,
            None => targets.len() - 1,
        };
        let next = targets[next_index].clone();
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        self.panel_content_dirty = true;
        true
    }

    fn focusable_targets(&self) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for tree in self.panel_trees() {
            collect_focus_targets(tree.id, &tree.children, &mut targets);
        }
        targets
    }

    fn is_dropdown_target(&self, panel_id: &str, node_id: &str) -> bool {
        self.panel_trees()
            .into_iter()
            .find(|tree| tree.id == panel_id)
            .map(|tree| find_dropdown_node(&tree.children, node_id).is_some())
            .unwrap_or(false)
    }

    fn max_panel_scroll_offset(&self, viewport_height: usize) -> usize {
        self.panel_content_height.saturating_sub(viewport_height)
    }

    fn text_input_state_for_target(&self, target: &FocusTarget) -> Option<(String, TextInputMode)> {
        self.panel_trees()
            .into_iter()
            .find(|tree| tree.id == target.panel_id)
            .and_then(|tree| find_text_input_value(&tree.children, &target.node_id))
    }

    fn ensure_text_input_editor_state(&mut self, target: &FocusTarget, current_value: &str) {
        let max_chars = text_char_len(current_value);
        self.text_input_states
            .entry(text_input_state_key(target))
            .and_modify(|state| {
                state.cursor_chars = state.cursor_chars.min(max_chars);
            })
            .or_insert(TextInputEditorState {
                cursor_chars: max_chars,
                preedit: None,
            });
    }

    fn editor_state_for_target(
        &self,
        target: &FocusTarget,
        current_value: &str,
    ) -> TextInputEditorState {
        let mut state = self
            .text_input_states
            .get(&text_input_state_key(target))
            .cloned()
            .unwrap_or(TextInputEditorState {
                cursor_chars: text_char_len(current_value),
                preedit: None,
            });
        state.cursor_chars = state.cursor_chars.min(text_char_len(current_value));
        state
    }

    fn set_focused_input_cursor(&mut self, cursor_chars: usize) -> bool {
        let Some(target) = self.focused_target.clone() else {
            return false;
        };
        let Some((current, _)) = self.text_input_state_for_target(&target) else {
            return false;
        };
        let mut editor_state = self.editor_state_for_target(&target, &current);
        let next_cursor = cursor_chars.min(text_char_len(&current));
        if next_cursor == editor_state.cursor_chars && editor_state.preedit.is_none() {
            return false;
        }
        editor_state.cursor_chars = next_cursor;
        editor_state.preedit = None;
        self.text_input_states
            .insert(text_input_state_key(&target), editor_state);
        self.panel_content_dirty = true;
        true
    }

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
        self.panel_content_dirty = true;
    }

    fn ensure_workspace_panel_entry(&mut self, panel_id: &str) {
        if panel_id == WORKSPACE_PANEL_ID
            || self
                .workspace_layout
                .panels
                .iter()
                .any(|entry| entry.id == panel_id)
        {
            return;
        }

        self.workspace_layout.panels.push(WorkspacePanelState {
            id: panel_id.to_string(),
            visible: true,
        });
    }

    fn reconcile_workspace_layout(&mut self) {
        let panel_ids = self
            .panels
            .iter()
            .map(|panel| panel.id())
            .collect::<Vec<_>>();
        for panel_id in panel_ids {
            self.ensure_workspace_panel_entry(panel_id);
        }

        if self
            .focused_target
            .as_ref()
            .is_some_and(|target| !self.panel_is_visible(&target.panel_id))
        {
            self.focused_target = None;
        }
    }

    fn visible_panels_in_order(&self) -> impl Iterator<Item = &dyn PanelPlugin> {
        let ordered_ids = self
            .workspace_layout
            .panels
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();

        ordered_ids.into_iter().filter_map(|panel_id| {
            self.panels
                .iter()
                .find(|panel| panel.id() == panel_id && self.panel_is_visible(panel_id))
                .map(|panel| panel.as_ref())
        })
    }

    fn panel_is_visible(&self, panel_id: &str) -> bool {
        if panel_id == WORKSPACE_PANEL_ID {
            return true;
        }

        self.workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)
            .map(|entry| entry.visible)
            .unwrap_or(true)
    }

    fn workspace_manager_tree(&self) -> PanelTree {
        let panel_titles = self
            .panels
            .iter()
            .map(|panel| (panel.id(), panel.title()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let ordered_entries = self
            .workspace_layout
            .panels
            .iter()
            .filter(|entry| panel_titles.contains_key(entry.id.as_str()))
            .collect::<Vec<_>>();

        let rows = ordered_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let title = panel_titles
                    .get(entry.id.as_str())
                    .copied()
                    .unwrap_or(entry.id.as_str());
                PanelNode::Row {
                    id: format!("workspace.row.{}", entry.id),
                    children: vec![
                        PanelNode::Text {
                            id: format!("workspace.title.{}", entry.id),
                            text: title.to_string(),
                        },
                        PanelNode::Button {
                            id: format!("workspace.move-up.{}", entry.id),
                            label: "Up".to_string(),
                            action: HostAction::MovePanel {
                                panel_id: entry.id.clone(),
                                direction: PanelMoveDirection::Up,
                            },
                            active: index > 0,
                            fill_color: None,
                        },
                        PanelNode::Button {
                            id: format!("workspace.move-down.{}", entry.id),
                            label: "Down".to_string(),
                            action: HostAction::MovePanel {
                                panel_id: entry.id.clone(),
                                direction: PanelMoveDirection::Down,
                            },
                            active: index + 1 < ordered_entries.len(),
                            fill_color: None,
                        },
                        PanelNode::Button {
                            id: format!("workspace.visibility.{}", entry.id),
                            label: if entry.visible {
                                "Hide".to_string()
                            } else {
                                "Show".to_string()
                            },
                            action: HostAction::SetPanelVisibility {
                                panel_id: entry.id.clone(),
                                visible: !entry.visible,
                            },
                            active: !entry.visible,
                            fill_color: None,
                        },
                    ],
                }
            })
            .collect::<Vec<_>>();

        PanelTree {
            id: WORKSPACE_PANEL_ID,
            title: "Workspace",
            children: vec![PanelNode::Column {
                id: "workspace.root".to_string(),
                children: vec![PanelNode::Section {
                    id: "workspace.panels".to_string(),
                    title: "Panels".to_string(),
                    children: rows,
                }],
            }],
        }
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

fn workspace_panel_actions(nodes: &[PanelNode], event: &PanelEvent) -> Vec<HostAction> {
    let target_id = match event {
        PanelEvent::Activate { node_id, .. }
        | PanelEvent::SetValue { node_id, .. }
        | PanelEvent::DragValue { node_id, .. }
        | PanelEvent::SetText { node_id, .. } => node_id,
        PanelEvent::Keyboard { .. } => return Vec::new(),
    };
    find_actions_in_nodes_local(nodes, target_id)
}

fn find_actions_in_nodes_local(nodes: &[PanelNode], target_id: &str) -> Vec<HostAction> {
    for node in nodes {
        if let Some(actions) = find_actions_in_node_local(node, target_id) {
            return actions;
        }
    }
    Vec::new()
}

fn find_actions_in_node_local(node: &PanelNode, target_id: &str) -> Option<Vec<HostAction>> {
    match node {
        PanelNode::Column { children, .. }
        | PanelNode::Row { children, .. }
        | PanelNode::Section { children, .. } => children
            .iter()
            .find_map(|child| find_actions_in_node_local(child, target_id)),
        PanelNode::Text { .. } | PanelNode::ColorPreview { .. } => None,
        PanelNode::Button { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Button { .. } => None,
        PanelNode::Slider { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Slider { .. } => None,
        PanelNode::TextInput {
            id,
            action: Some(action),
            ..
        } if id == target_id => Some(vec![action.clone()]),
        PanelNode::TextInput { .. } => None,
        PanelNode::Dropdown { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Dropdown { .. } => None,
        PanelNode::LayerList { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::LayerList { .. } => None,
    }
}

fn collect_panel_files_recursive(
    directory: &Path,
    panel_files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_panel_files_recursive(&path, panel_files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("altp-panel") {
            panel_files.push(path);
        }
    }
    Ok(())
}

struct DslPanelPlugin {
    id: &'static str,
    title: &'static str,
    definition: PanelDefinition,
    runtime: WasmPanelRuntime,
    state: Value,
    host_snapshot: Value,
    diagnostics: Vec<Diagnostic>,
    has_keyboard_handler: bool,
    supports_sync_host: bool,
}

impl DslPanelPlugin {
    fn from_definition(definition: PanelDefinition) -> Result<Self, String> {
        let id = leak_string(definition.manifest.id.clone());
        let title = leak_string(definition.manifest.title.clone());
        let runtime_path = definition
            .source_path
            .parent()
            .map(|directory| directory.join(&definition.runtime.wasm))
            .unwrap_or_else(|| definition.source_path.clone());
        let mut runtime = WasmPanelRuntime::load(&runtime_path)
            .map_err(|error: PluginHostError| error.to_string())?;
        let has_keyboard_handler = runtime.has_handler("keyboard");
        let supports_sync_host = runtime.supports_sync_host();
        let initial_state = state_defaults_to_json(&definition.state);
        let init = runtime
            .initialize(&PanelInitRequest {
                initial_state,
                host_snapshot: json!({}),
            })
            .map_err(|error: PluginHostError| error.to_string())?;

        Ok(Self {
            id,
            title,
            definition,
            runtime,
            state: init.state,
            host_snapshot: json!({}),
            diagnostics: init.diagnostics,
            has_keyboard_handler,
            supports_sync_host,
        })
    }

    fn evaluate_tree(&self) -> PanelTree {
        let mut context = DslEvaluationContext {
            panel_id: self.id.to_string(),
            state: &self.state,
            generated_ids: 0,
        };
        let mut children = self
            .definition
            .view
            .iter()
            .flat_map(|node| convert_dsl_view_node(node, &mut context))
            .collect::<Vec<_>>();
        if !self.diagnostics.is_empty() {
            children.push(PanelNode::Section {
                id: "dsl.diagnostics".to_string(),
                title: "Diagnostics".to_string(),
                children: self
                    .diagnostics
                    .iter()
                    .enumerate()
                    .map(|(index, diagnostic)| PanelNode::Text {
                        id: format!("dsl.diagnostics.{index}"),
                        text: format!("{:?}: {}", diagnostic.level, diagnostic.message),
                    })
                    .collect(),
            });
        }

        PanelTree {
            id: self.id,
            title: self.title,
            children,
        }
    }

    fn resolve_handler_action(&self, event: &PanelEvent) -> Option<HostAction> {
        let tree = self.evaluate_tree();
        match event {
            PanelEvent::Activate { node_id, .. }
            | PanelEvent::SetValue { node_id, .. }
            | PanelEvent::DragValue { node_id, .. }
            | PanelEvent::SetText { node_id, .. } => find_panel_action(&tree.children, node_id),
            PanelEvent::Keyboard { .. } if self.has_keyboard_handler => {
                Some(HostAction::InvokePanelHandler {
                    panel_id: self.id.to_string(),
                    handler_name: "keyboard".to_string(),
                    event_kind: "keyboard".to_string(),
                })
            }
            PanelEvent::Keyboard { .. } => None,
        }
    }

    fn apply_text_input_event(&mut self, node_id: &str, value: &str) -> bool {
        let tree = self.evaluate_tree();
        let Some((binding, input_mode)) = find_text_input_binding(&tree.children, node_id) else {
            return false;
        };
        let _ = input_mode;
        let next_value = Value::String(value.to_string());
        apply_state_patch(&mut self.state, &StatePatch::set(binding, next_value));
        true
    }

    fn sync_host_state(&mut self) {
        if !self.supports_sync_host {
            return;
        }

        let result = match self
            .runtime
            .sync_host(&self.state, &self.host_snapshot)
        {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics = vec![Diagnostic::error(error.to_string())];
                return;
            }
        };

        apply_state_patches(&mut self.state, &result.state_patch);
        self.diagnostics = result.diagnostics;
    }
}

impl PanelPlugin for DslPanelPlugin {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn update(&mut self, document: &Document) {
        self.host_snapshot = build_host_snapshot(document);
        self.sync_host_state();
    }

    fn debug_summary(&self) -> String {
        format!(
            "dsl runtime={} handlers={} diagnostics={} state={}",
            self.runtime.path().display(),
            self.definition.handler_bindings.len(),
            self.diagnostics.len(),
            self.state
        )
    }

    fn view(&self) -> PanelView {
        let mut lines = vec![format!("runtime: {}", self.definition.runtime.wasm)];
        if !self.diagnostics.is_empty() {
            lines.push(format!("diagnostics: {}", self.diagnostics.len()));
        }
        PanelView {
            id: self.id,
            title: self.title,
            lines,
        }
    }

    fn panel_tree(&self) -> PanelTree {
        self.evaluate_tree()
    }

    fn handles_keyboard_event(&self) -> bool {
        self.has_keyboard_handler
    }

    fn persistent_config(&self) -> Option<Value> {
        lookup_json_path(&self.state, "config").cloned()
    }

    fn restore_persistent_config(&mut self, config: &Value) {
        apply_state_patch(
            &mut self.state,
            &StatePatch::replace("config", config.clone()),
        );
    }

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        let mut updated_text = false;
        if let PanelEvent::SetText { node_id, value, .. } = event {
            updated_text = self.apply_text_input_event(node_id, value);
        }
        let Some(HostAction::InvokePanelHandler {
            panel_id,
            handler_name,
            event_kind,
        }) = self.resolve_handler_action(event)
        else {
            if updated_text {
                self.diagnostics.clear();
            }
            return Vec::new();
        };
        if panel_id != self.id {
            return Vec::new();
        }

        let event_payload = match event {
            PanelEvent::Activate { .. } => json!({}),
            PanelEvent::SetValue { value, .. } => json!({ "value": value }),
            PanelEvent::DragValue { from, to, .. } => json!({
                "from": from.to_string(),
                "to": to.to_string(),
                "value": to,
            }),
            PanelEvent::SetText { value, .. } => json!({ "value": value }),
            PanelEvent::Keyboard {
                shortcut,
                key,
                repeat,
                ..
            } => json!({
                "shortcut": shortcut,
                "key": key,
                "repeat": repeat,
            }),
        };
        let result = match self.runtime.handle_event(&PanelEventRequest {
            handler_name,
            event_kind,
            event_payload,
            state_snapshot: self.state.clone(),
            host_snapshot: self.host_snapshot.clone(),
        }) {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics = vec![Diagnostic::error(error.to_string())];
                return Vec::new();
            }
        };

        apply_state_patches(&mut self.state, &result.state_patch);
        self.diagnostics = result.diagnostics.clone();
        command_descriptors_to_actions(result.commands, &mut self.diagnostics)
    }
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

struct DslEvaluationContext<'a> {
    panel_id: String,
    state: &'a Value,
    generated_ids: usize,
}

fn convert_dsl_view_node(
    node: &ViewNode,
    context: &mut DslEvaluationContext<'_>,
) -> Vec<PanelNode> {
    match node {
        ViewNode::Text(text) => {
            let text = evaluate_text_content(text, context);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![PanelNode::Text {
                    id: next_generated_node_id(context, "text"),
                    text,
                }]
            }
        }
        ViewNode::Element(element) => match element.tag.as_str() {
            "column" => vec![PanelNode::Column {
                id: node_id_for(element, context, "column"),
                children: convert_dsl_children(&element.children, context),
            }],
            "row" => vec![PanelNode::Row {
                id: node_id_for(element, context, "row"),
                children: convert_dsl_children(&element.children, context),
            }],
            "section" => vec![PanelNode::Section {
                id: node_id_for(element, context, "section"),
                title: attribute_string(&element.attributes, "title", context)
                    .unwrap_or_else(|| "Section".to_string()),
                children: convert_dsl_children(&element.children, context),
            }],
            "text" => vec![PanelNode::Text {
                id: node_id_for(element, context, "text"),
                text: collect_dsl_text(&element.children, context),
            }],
            "color-preview" => vec![PanelNode::ColorPreview {
                id: node_id_for(element, context, "color-preview"),
                label: attribute_string(&element.attributes, "label", context)
                    .unwrap_or_else(|| collect_dsl_text(&element.children, context)),
                color: attribute_string(&element.attributes, "color", context)
                    .and_then(|value| parse_hex_color(&value))
                    .unwrap_or_default(),
            }],
            "button" => vec![PanelNode::Button {
                id: node_id_for(element, context, "button"),
                label: collect_dsl_text(&element.children, context),
                action: element
                    .attributes
                    .get("on:click")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "click".to_string(),
                    })
                    .unwrap_or(HostAction::DispatchCommand(Command::Noop)),
                active: attribute_bool(&element.attributes, "active", context).unwrap_or(false),
                fill_color: None,
            }],
            "toggle" => {
                let checked =
                    attribute_bool(&element.attributes, "checked", context).unwrap_or(false);
                vec![PanelNode::Button {
                    id: node_id_for(element, context, "toggle"),
                    label: format!(
                        "[{}] {}",
                        if checked { "x" } else { " " },
                        collect_dsl_text(&element.children, context)
                    ),
                    action: element
                        .attributes
                        .get("on:change")
                        .and_then(DslAttrValue::as_string)
                        .map(|handler_name| HostAction::InvokePanelHandler {
                            panel_id: context.panel_id.clone(),
                            handler_name: handler_name.to_string(),
                            event_kind: "change".to_string(),
                        })
                        .unwrap_or(HostAction::DispatchCommand(Command::Noop)),
                    active: checked,
                    fill_color: None,
                }]
            }
            "slider" => vec![PanelNode::Slider {
                id: node_id_for(element, context, "slider"),
                label: attribute_string(&element.attributes, "label", context)
                    .unwrap_or_else(|| "Value".to_string()),
                action: element
                    .attributes
                    .get("on:change")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "change".to_string(),
                    })
                    .unwrap_or(HostAction::DispatchCommand(Command::Noop)),
                min: attribute_usize(&element.attributes, "min", context).unwrap_or(0),
                max: attribute_usize(&element.attributes, "max", context).unwrap_or(100),
                value: attribute_usize(&element.attributes, "value", context).unwrap_or(0),
                fill_color: attribute_string(&element.attributes, "fill", context)
                    .and_then(|value| parse_hex_color(&value)),
            }],
            "input" => vec![PanelNode::TextInput {
                id: node_id_for(element, context, "input"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                value: attribute_string(&element.attributes, "value", context).unwrap_or_default(),
                placeholder: attribute_string(&element.attributes, "placeholder", context)
                    .unwrap_or_default(),
                binding_path: attribute_string(&element.attributes, "bind", context)
                    .unwrap_or_default(),
                action: element
                    .attributes
                    .get("on:change")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "change".to_string(),
                    }),
                input_mode: attribute_string(&element.attributes, "mode", context)
                    .map(|mode| {
                        if mode.eq_ignore_ascii_case("numeric")
                            || mode.eq_ignore_ascii_case("number")
                        {
                            TextInputMode::Numeric
                        } else {
                            TextInputMode::Text
                        }
                    })
                    .unwrap_or(TextInputMode::Text),
            }],
            "dropdown" => vec![PanelNode::Dropdown {
                id: node_id_for(element, context, "dropdown"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                value: attribute_string(&element.attributes, "value", context).unwrap_or_default(),
                action: element
                    .attributes
                    .get("on:change")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "change".to_string(),
                    })
                    .unwrap_or(HostAction::DispatchCommand(Command::Noop)),
                options: attribute_dropdown_options(&element.attributes, "options", context),
            }],
            "layer-list" => vec![PanelNode::LayerList {
                id: node_id_for(element, context, "layer-list"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                selected_index: attribute_usize(&element.attributes, "selected", context)
                    .unwrap_or_default(),
                action: element
                    .attributes
                    .get("on:change")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "change".to_string(),
                    })
                    .unwrap_or(HostAction::DispatchCommand(Command::Noop)),
                items: attribute_layer_list_items(&element.attributes, "items", context),
            }],
            "separator" => vec![PanelNode::Text {
                id: node_id_for(element, context, "separator"),
                text: "────────".to_string(),
            }],
            "spacer" => vec![PanelNode::Text {
                id: node_id_for(element, context, "spacer"),
                text: String::new(),
            }],
            "when" => {
                if attribute_bool(&element.attributes, "test", context).unwrap_or(false) {
                    convert_dsl_children(&element.children, context)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        },
    }
}

fn convert_dsl_children(
    children: &[ViewNode],
    context: &mut DslEvaluationContext<'_>,
) -> Vec<PanelNode> {
    children
        .iter()
        .flat_map(|child| convert_dsl_view_node(child, context))
        .collect()
}

fn collect_dsl_text(children: &[ViewNode], context: &DslEvaluationContext<'_>) -> String {
    let text = children
        .iter()
        .filter_map(|child| match child {
            ViewNode::Text(text) => Some(evaluate_text_content(text, context)),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        String::from("Unnamed")
    } else {
        text
    }
}

fn evaluate_text_content(text: &str, context: &DslEvaluationContext<'_>) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            rendered.push_str(&rest[start..]);
            return rendered;
        };

        let expression = after_start[..end].trim();
        rendered.push_str(&expression_to_string(expression, context));
        rest = &after_start[end + 1..];
    }

    if rendered.is_empty() {
        text.to_string()
    } else {
        rendered.push_str(rest);
        rendered
    }
}

fn attribute_string(
    attributes: &std::collections::BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<String> {
    attributes
        .get(key)
        .map(|value| attr_value_to_string(value, context))
}

fn attribute_bool(
    attributes: &std::collections::BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<bool> {
    attributes
        .get(key)
        .map(|value| attr_value_to_bool(value, context))
}

fn attr_value_to_string(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> String {
    match value {
        DslAttrValue::String(text) => text.clone(),
        DslAttrValue::Integer(number) => number.to_string(),
        DslAttrValue::Float(number) => number.clone(),
        DslAttrValue::Bool(value) => value.to_string(),
        DslAttrValue::Expression(expression) => expression_to_string(expression, context),
    }
}

fn attr_value_to_bool(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> bool {
    match value {
        DslAttrValue::Bool(value) => *value,
        DslAttrValue::Expression(expression) => expression_to_bool(expression, context),
        DslAttrValue::String(text) => !text.is_empty(),
        DslAttrValue::Integer(number) => *number != 0,
        DslAttrValue::Float(number) => number != "0" && number != "0.0",
    }
}

fn attr_value_to_usize(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> Option<usize> {
    match value {
        DslAttrValue::Integer(number) => usize::try_from(*number).ok(),
        DslAttrValue::Expression(expression) => match evaluate_expression(expression, context) {
            Value::Number(number) => number
                .as_u64()
                .and_then(|value| usize::try_from(value).ok()),
            Value::String(text) => text.parse::<usize>().ok(),
            _ => None,
        },
        DslAttrValue::String(text) => text.parse::<usize>().ok(),
        DslAttrValue::Float(_) | DslAttrValue::Bool(_) => None,
    }
}

fn attribute_usize(
    attributes: &std::collections::BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<usize> {
    attributes
        .get(key)
        .and_then(|value| attr_value_to_usize(value, context))
}

fn attribute_dropdown_options(
    attributes: &std::collections::BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Vec<DropdownOption> {
    let Some(raw) = attribute_string(attributes, key, context) else {
        return Vec::new();
    };

    raw.split('|')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            let (value, label) = item.split_once(':').unwrap_or((item, item));
            Some(DropdownOption {
                label: label.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

fn attribute_layer_list_items(
    attributes: &std::collections::BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Vec<LayerListItem> {
    let Some(value) = attributes.get(key) else {
        return Vec::new();
    };

    layer_list_items_from_value(value, context)
}

fn layer_list_items_from_value(
    value: &DslAttrValue,
    context: &DslEvaluationContext<'_>,
) -> Vec<LayerListItem> {
    let json_value = match value {
        DslAttrValue::Expression(expression) => evaluate_expression(expression, context),
        DslAttrValue::String(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone())),
        DslAttrValue::Integer(number) => Value::Number((*number).into()),
        DslAttrValue::Float(number) => number
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        DslAttrValue::Bool(value) => Value::Bool(*value),
    };

    layer_list_items_from_json(&json_value)
}

fn layer_list_items_from_json(value: &Value) -> Vec<LayerListItem> {
    match value {
        Value::Array(items) => items.iter().filter_map(layer_list_item_from_json).collect(),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .map(|parsed| layer_list_items_from_json(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn layer_list_item_from_json(value: &Value) -> Option<LayerListItem> {
    let object = value.as_object()?;
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| object.get("name").and_then(Value::as_str))?
        .to_string();
    let detail = object
        .get("detail")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            let blend_mode = object
                .get("blend_mode")
                .and_then(Value::as_str)
                .unwrap_or("normal");
            let visible = object
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let masked = object
                .get("masked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            format!(
                "blend: {blend_mode} / {} / mask: {}",
                if visible { "visible" } else { "hidden" },
                masked
            )
        });

    Some(LayerListItem { label, detail })
}

fn expression_to_string(expression: &str, context: &DslEvaluationContext<'_>) -> String {
    match evaluate_expression(expression, context) {
        Value::String(text) => text,
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn expression_to_bool(expression: &str, context: &DslEvaluationContext<'_>) -> bool {
    match evaluate_expression(expression, context) {
        Value::Bool(value) => value,
        Value::String(text) => !text.is_empty() && text != "false",
        Value::Number(number) => number.as_i64().unwrap_or_default() != 0,
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

fn evaluate_expression(expression: &str, context: &DslEvaluationContext<'_>) -> Value {
    let expression = expression.trim();
    if let Some(inner) = expression.strip_prefix('!') {
        return Value::Bool(!expression_to_bool(inner, context));
    }
    if let Some((left, right)) = expression.split_once("!=") {
        return Value::Bool(
            evaluate_expression(left.trim(), context) != evaluate_expression(right.trim(), context),
        );
    }
    if let Some((left, right)) = expression.split_once("==") {
        return Value::Bool(
            evaluate_expression(left.trim(), context) == evaluate_expression(right.trim(), context),
        );
    }
    if expression.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if expression.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if expression.starts_with('"') && expression.ends_with('"') && expression.len() >= 2 {
        return Value::String(expression[1..expression.len() - 1].to_string());
    }
    if let Ok(number) = expression.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Some(path) = expression.strip_prefix("state.") {
        return lookup_json_path(context.state, path)
            .cloned()
            .unwrap_or(Value::Null);
    }

    Value::String(expression.to_string())
}

fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn node_id_for(
    element: &ViewElement,
    context: &mut DslEvaluationContext<'_>,
    prefix: &str,
) -> String {
    element
        .attributes
        .get("id")
        .map(|value| attr_value_to_string(value, context))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| next_generated_node_id(context, prefix))
}

fn next_generated_node_id(context: &mut DslEvaluationContext<'_>, prefix: &str) -> String {
    context.generated_ids += 1;
    format!("dsl.{prefix}.{}", context.generated_ids)
}

fn state_defaults_to_json(fields: &[StateField]) -> Value {
    let mut state = Value::Object(Map::new());
    for field in fields {
        apply_state_patch(
            &mut state,
            &StatePatch::set(
                field.name.clone(),
                default_attr_value_to_json(&field.default),
            ),
        );
    }
    state
}

fn default_attr_value_to_json(value: &DslAttrValue) -> Value {
    match value {
        DslAttrValue::String(text) => Value::String(text.clone()),
        DslAttrValue::Integer(number) => Value::Number((*number).into()),
        DslAttrValue::Float(number) => number
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        DslAttrValue::Bool(value) => Value::Bool(*value),
        DslAttrValue::Expression(_) => Value::Null,
    }
}

fn build_host_snapshot(document: &Document) -> Value {
    let active_panel = document
        .work
        .pages
        .first()
        .and_then(|page| page.panels.first());
    let layers = active_panel
        .map(|panel| {
            panel
                .layers
                .iter()
                .map(|layer| {
                    json!({
                        "name": layer.name,
                        "blend_mode": layer.blend_mode.as_str(),
                        "visible": layer.visible,
                        "masked": layer.mask.is_some(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![json!({
            "name": "Layer 1",
            "blend_mode": "normal",
            "visible": true,
            "masked": false,
        })]);
    let layers_json = serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());
    let layer_count = active_panel.map(|panel| panel.layers.len()).unwrap_or(1);
    let active_layer_index = active_panel
        .map(|panel| panel.active_layer_index)
        .unwrap_or(0);
    let active_layer = active_panel.and_then(|panel| panel.layers.get(panel.active_layer_index));
    let page_count = document.work.pages.len();
    let panel_count = document
        .work
        .pages
        .iter()
        .map(|page| page.panels.len())
        .sum::<usize>();
    let active_layer_name = active_layer
        .map(|layer| layer.name.clone())
        .unwrap_or_else(|| "<no layer>".to_string());
    let active_pen = document.active_pen_preset().cloned().unwrap_or_default();

    json!({
        "document": {
            "title": document.work.title,
            "page_count": page_count,
            "panel_count": panel_count,
            "active_layer_name": active_layer_name,
            "layer_count": layer_count,
            "active_layer_index": active_layer_index,
            "active_layer_blend_mode": active_layer
                .map(|layer| layer.blend_mode.as_str())
                .unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "layers": layers,
            "layers_json": layers_json,
        },
        "tool": {
            "active": match document.active_tool {
                ToolKind::Brush => "brush",
                ToolKind::Pen => "pen",
                ToolKind::Eraser => "eraser",
            },
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_index": document.active_pen_index(),
            "pen_count": document.pen_presets.len(),
            "pen_size": document.active_pen_size,
        },
        "color": {
            "active": document.active_color.hex_rgb(),
            "red": document.active_color.r,
            "green": document.active_color.g,
            "blue": document.active_color.b,
        },
        "jobs": {
            "active": 0,
            "queued": 0,
            "status": format!("idle / work={}", document.work.title),
        },
        "snapshot": {
            "storage_status": "pending",
        },
        "view": {
            "zoom": document.view_transform.zoom,
            "pan_x": document.view_transform.pan_x,
            "pan_y": document.view_transform.pan_y,
        },
    })
}

fn apply_state_patches(state: &mut Value, patches: &[StatePatch]) {
    if !state.is_object() {
        *state = Value::Object(Map::new());
    }
    for patch in patches {
        apply_state_patch(state, patch);
    }
}

fn apply_state_patch(state: &mut Value, patch: &StatePatch) {
    let mut current = state;
    let mut segments = patch.path.split('.').peekable();
    while let Some(segment) = segments.next() {
        let is_last = segments.peek().is_none();
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let object = current.as_object_mut().expect("object ensured");
        if is_last {
            match patch.op {
                panel_schema::StatePatchOp::Set | panel_schema::StatePatchOp::Replace => {
                    object.insert(
                        segment.to_string(),
                        patch.value.clone().unwrap_or(Value::Null),
                    );
                }
                panel_schema::StatePatchOp::Toggle => {
                    let next = !object
                        .get(segment)
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    object.insert(segment.to_string(), Value::Bool(next));
                }
            }
            return;
        }
        current = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

fn command_descriptors_to_actions(
    commands: Vec<CommandDescriptor>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<HostAction> {
    commands
        .into_iter()
        .filter_map(|descriptor| match command_from_descriptor(&descriptor) {
            Ok(command) => Some(HostAction::DispatchCommand(command)),
            Err(message) => {
                diagnostics.push(Diagnostic::warning(message));
                None
            }
        })
        .collect()
}

fn command_from_descriptor(descriptor: &CommandDescriptor) -> Result<Command, String> {
    match descriptor.name.as_str() {
        "project.new" => Ok(Command::NewDocument),
        "project.new_sized" => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.new_sized is missing payload.size".to_string())?;
            let (width, height) = parse_document_size(size)
                .ok_or_else(|| format!("invalid project.new_sized payload: {size}"))?;
            Ok(Command::NewDocumentSized { width, height })
        }
        "project.save" => Ok(Command::SaveProject),
        "project.save_as" => Ok(Command::SaveProjectAs),
        "project.save_as_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.save_as_path is missing payload.path".to_string())?;
            Ok(Command::SaveProjectToPath {
                path: path.to_string(),
            })
        }
        "project.load" => Ok(Command::LoadProject),
        "project.load_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.load_path is missing payload.path".to_string())?;
            Ok(Command::LoadProjectFromPath {
                path: path.to_string(),
            })
        }
        "tool.set_active" => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_active is missing payload.tool".to_string())?;
            let tool = match tool {
                "brush" => ToolKind::Brush,
                "pen" => ToolKind::Pen,
                "eraser" => ToolKind::Eraser,
                other => return Err(format!("unsupported tool kind: {other}")),
            };
            Ok(Command::SetActiveTool { tool })
        }
        "tool.set_size" => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| "tool.set_size is missing payload.size".to_string())?;
            Ok(Command::SetActivePenSize { size: size as u32 })
        }
        "tool.pen_next" => Ok(Command::SelectNextPenPreset),
        "tool.pen_prev" => Ok(Command::SelectPreviousPenPreset),
        "tool.reload_pen_presets" => Ok(Command::ReloadPenPresets),
        "tool.set_color" => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_color is missing payload.color".to_string())?;
            parse_hex_color(color)
                .map(|color| Command::SetActiveColor { color })
                .ok_or_else(|| format!("invalid color payload: {color}"))
        }
        "layer.add" => Ok(Command::AddRasterLayer),
        "layer.remove" => Ok(Command::RemoveActiveLayer),
        "layer.select" => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.select is missing payload.index".to_string())?;
            Ok(Command::SelectLayer {
                index: index as usize,
            })
        }
        "layer.rename_active" => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.rename_active is missing payload.name".to_string())?;
            Ok(Command::RenameActiveLayer {
                name: name.to_string(),
            })
        }
        "layer.move" => {
            let from_index = descriptor
                .payload
                .get("from_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.from_index".to_string())?;
            let to_index = descriptor
                .payload
                .get("to_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.to_index".to_string())?;
            Ok(Command::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            })
        }
        "layer.select_next" => Ok(Command::SelectNextLayer),
        "layer.cycle_blend_mode" => Ok(Command::CycleActiveLayerBlendMode),
        "layer.set_blend_mode" => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.set_blend_mode is missing payload.mode".to_string())?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(Command::SetActiveLayerBlendMode { mode })
        }
        "layer.toggle_visibility" => Ok(Command::ToggleActiveLayerVisibility),
        "layer.toggle_mask" => Ok(Command::ToggleActiveLayerMask),
        "view.reset" => Ok(Command::ResetView),
        "view.zoom" => {
            let zoom = descriptor
                .payload
                .get("zoom")
                .and_then(payload_f64)
                .ok_or_else(|| "view.zoom is missing payload.zoom".to_string())?;
            Ok(Command::SetViewZoom { zoom: zoom as f32 })
        }
        "view.pan" => {
            let delta_x = descriptor
                .payload
                .get("delta_x")
                .and_then(payload_f64)
                .ok_or_else(|| "view.pan is missing payload.delta_x".to_string())?;
            let delta_y = descriptor
                .payload
                .get("delta_y")
                .and_then(payload_f64)
                .ok_or_else(|| "view.pan is missing payload.delta_y".to_string())?;
            Ok(Command::PanView {
                delta_x: delta_x as f32,
                delta_y: delta_y as f32,
            })
        }
        other => Err(format!("unsupported command descriptor: {other}")),
    }
}

fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

fn payload_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

fn parse_hex_color(input: &str) -> Option<ColorRgba8> {
    let hex = input.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(ColorRgba8::new(r, g, b, 0xff))
}

fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }

    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_DOCUMENT_DIMENSION
        || height > MAX_DOCUMENT_DIMENSION
        || width.saturating_mul(height) > MAX_DOCUMENT_PIXELS
    {
        return None;
    }

    Some((width, height))
}

fn find_panel_action(nodes: &[PanelNode], target_id: &str) -> Option<HostAction> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(action) = find_panel_action(children, target_id) {
                    return Some(action);
                }
            }
            PanelNode::Button { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::Slider { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::TextInput {
                id,
                action: Some(action),
                ..
            } if id == target_id => return Some(action.clone()),
            PanelNode::Dropdown { id, action, .. } if id == target_id => {
                return Some(action.clone());
            }
            PanelNode::LayerList { id, action, .. } if id == target_id => {
                return Some(action.clone());
            }
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

fn find_dropdown_node<'a>(nodes: &'a [PanelNode], target_id: &str) -> Option<&'a PanelNode> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(found) = find_dropdown_node(children, target_id) {
                    return Some(found);
                }
            }
            PanelNode::Dropdown { id, .. } if id == target_id => return Some(node),
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

fn find_text_input_binding(
    nodes: &[PanelNode],
    target_id: &str,
) -> Option<(String, TextInputMode)> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(binding) = find_text_input_binding(children, target_id) {
                    return Some(binding);
                }
            }
            PanelNode::TextInput {
                id,
                binding_path,
                input_mode,
                ..
            } if id == target_id => {
                if binding_path.is_empty() {
                    return None;
                }
                return Some((binding_path.clone(), *input_mode));
            }
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

fn find_text_input_value(nodes: &[PanelNode], target_id: &str) -> Option<(String, TextInputMode)> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(value) = find_text_input_value(children, target_id) {
                    return Some(value);
                }
            }
            PanelNode::TextInput {
                id,
                value,
                input_mode,
                ..
            } if id == target_id => return Some((value.clone(), *input_mode)),
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

fn collect_focus_targets(panel_id: &str, nodes: &[PanelNode], targets: &mut Vec<FocusTarget>) {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                collect_focus_targets(panel_id, children, targets);
            }
            PanelNode::Button { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::TextInput { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::Dropdown { id, .. } | PanelNode::LayerList { id, .. } => {
                targets.push(FocusTarget {
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                })
            }
            PanelNode::Text { .. } | PanelNode::ColorPreview { .. } | PanelNode::Slider { .. } => {}
        }
    }
}

fn filter_text_input(text: &str, input_mode: TextInputMode) -> String {
    match input_mode {
        TextInputMode::Text => text
            .chars()
            .filter(|character| !character.is_control())
            .collect(),
        TextInputMode::Numeric => text
            .chars()
            .filter(|character| character.is_ascii_digit())
            .collect(),
    }
}

fn text_input_state_key(target: &FocusTarget) -> (String, String) {
    (target.panel_id.clone(), target.node_id.clone())
}

fn text_char_len(text: &str) -> usize {
    text.chars().count()
}

fn byte_index_for_char_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn insert_text_at_char_index(text: &str, char_index: usize, inserted: &str) -> String {
    let split_at = byte_index_for_char_index(text, char_index);
    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..split_at]);
    next.push_str(inserted);
    next.push_str(&text[split_at..]);
    next
}

fn remove_char_before_char_index(text: &str, char_index: usize) -> String {
    if char_index == 0 {
        return text.to_string();
    }
    let start = byte_index_for_char_index(text, char_index - 1);
    let end = byte_index_for_char_index(text, char_index);
    remove_byte_range(text, start, end)
}

fn remove_char_at_char_index(text: &str, char_index: usize) -> String {
    let start = byte_index_for_char_index(text, char_index);
    let end = byte_index_for_char_index(text, char_index + 1);
    if start >= end {
        return text.to_string();
    }
    remove_byte_range(text, start, end)
}

fn remove_byte_range(text: &str, start: usize, end: usize) -> String {
    let mut next = String::with_capacity(text.len().saturating_sub(end.saturating_sub(start)));
    next.push_str(&text[..start]);
    next.push_str(&text[end..]);
    next
}

fn prefix_for_char_count(text: &str, char_count: usize) -> String {
    text.chars().take(char_count).collect()
}

fn viewport_panel_surface(
    content: &PanelSurface,
    height: usize,
    scroll_offset: usize,
) -> PanelSurface {
    if scroll_offset == 0 && content.height == height {
        return content.clone();
    }

    let mut surface = PanelSurface {
        width: content.width,
        height,
        pixels: vec![0; content.width * height * 4],
        hit_regions: Vec::new(),
    };
    fill_rect(
        &mut surface,
        0,
        0,
        content.width,
        height,
        SIDEBAR_BACKGROUND,
    );

    let start_row = scroll_offset.min(content.height.saturating_sub(1));
    let visible_rows = height.min(content.height.saturating_sub(start_row));
    let row_bytes = content.width * 4;
    for row in 0..visible_rows {
        let src_start = (start_row + row) * row_bytes;
        let dst_start = row * row_bytes;
        surface.pixels[dst_start..dst_start + row_bytes]
            .copy_from_slice(&content.pixels[src_start..src_start + row_bytes]);
    }

    for region in &content.hit_regions {
        let region_bottom = region.y + region.height;
        if region_bottom <= scroll_offset || region.y >= scroll_offset + height {
            continue;
        }
        let top = region.y.saturating_sub(scroll_offset);
        let bottom = (region_bottom.saturating_sub(scroll_offset)).min(height);
        if bottom <= top {
            continue;
        }
        surface.hit_regions.push(PanelHitRegion {
            x: region.x,
            y: top,
            width: region.width,
            height: bottom - top,
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            kind: region.kind.clone(),
        });
    }

    surface
}

fn measure_panel_content_height(
    trees: &[PanelTree],
    width: usize,
    render_state: PanelRenderState<'_>,
) -> usize {
    if trees.is_empty() {
        return PANEL_OUTER_PADDING * 2;
    }

    let panels_height: usize = trees
        .iter()
        .map(|tree| measure_panel_tree(tree, width, render_state) + PANEL_OUTER_PADDING)
        .sum();
    PANEL_OUTER_PADDING + panels_height
}

fn measure_panel_tree(tree: &PanelTree, width: usize, render_state: PanelRenderState<'_>) -> usize {
    let title_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = measure_text(tree.title, title_width);
    let mut content_height = 0;
    for (index, child) in tree.children.iter().enumerate() {
        content_height += measure_node(child, tree.id, title_width, render_state);
        if index + 1 != tree.children.len() {
            content_height += NODE_GAP;
        }
    }

    PANEL_INNER_PADDING * 2 + title_height + 6 + content_height
}

fn measure_node(
    node: &PanelNode,
    panel_id: &str,
    available_width: usize,
    render_state: PanelRenderState<'_>,
) -> usize {
    match node {
        PanelNode::Column { children, .. } => children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                measure_node(child, panel_id, available_width, render_state)
                    + usize::from(index + 1 != children.len()) * NODE_GAP
            })
            .sum(),
        PanelNode::Row { children, .. } => {
            let width_per_child = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1))
                    / children.len()
            };
            children
                .iter()
                .map(|child| measure_node(child, panel_id, width_per_child, render_state))
                .max()
                .unwrap_or(0)
        }
        PanelNode::Section {
            children, title, ..
        } => {
            let title_height = measure_text(title, available_width);
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut children_height = 0;
            for (index, child) in children.iter().enumerate() {
                children_height += measure_node(child, panel_id, child_width, render_state);
                if index + 1 != children.len() {
                    children_height += SECTION_GAP;
                }
            }
            title_height + SECTION_GAP + children_height
        }
        PanelNode::Text { text, .. } => measure_text(text, available_width),
        PanelNode::ColorPreview { .. } => COLOR_PREVIEW_HEIGHT,
        PanelNode::Button { .. } => BUTTON_HEIGHT,
        PanelNode::Slider { .. } => SLIDER_HEIGHT,
        PanelNode::TextInput { label, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                measure_text(label, available_width) + 4
            };
            label_height + INPUT_BOX_HEIGHT
        }
        PanelNode::Dropdown { id, options, .. } => {
            let mut height = DROPDOWN_HEIGHT;
            if render_state.expanded_dropdown.is_some_and(|target| {
                target.panel_id == panel_id && target.node_id == id.as_str()
            }) {
                height += options.len() * DROPDOWN_HEIGHT;
            }
            height
        }
        PanelNode::LayerList { label, items, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                measure_text(label, available_width) + 4
            };
            label_height + items.len().max(1) * LAYER_LIST_ITEM_HEIGHT
        }
    }
}

fn draw_panel_tree(
    surface: &mut PanelSurface,
    tree: &PanelTree,
    x: usize,
    y: usize,
    width: usize,
    render_state: PanelRenderState<'_>,
) {
    let inner_x = x + PANEL_INNER_PADDING;
    let inner_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = draw_wrapped_text(
        surface,
        inner_x,
        y + PANEL_INNER_PADDING,
        tree.title,
        PANEL_TITLE,
        inner_width,
    );
    let mut cursor_y = y + PANEL_INNER_PADDING + title_height + 6;

    for child in &tree.children {
        let used = draw_node(
            surface,
            child,
            tree.id,
            inner_x,
            cursor_y,
            inner_width,
            render_state,
        );
        cursor_y += used + NODE_GAP;
    }
}

fn draw_node(
    surface: &mut PanelSurface,
    node: &PanelNode,
    panel_id: &str,
    x: usize,
    y: usize,
    available_width: usize,
    render_state: PanelRenderState<'_>,
) -> usize {
    match node {
        PanelNode::Column { children, .. } => {
            let mut cursor_y = y;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(
                    surface,
                    child,
                    panel_id,
                    x,
                    cursor_y,
                    available_width,
                    render_state,
                );
                if index + 1 != children.len() {
                    cursor_y += NODE_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Row { children, .. } => {
            let child_gap = NODE_GAP;
            let child_width = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(child_gap * children.len().saturating_sub(1))
                    / children.len()
            };
            let mut cursor_x = x;
            let mut max_height = 0;
            for child in children {
                let used = draw_node(
                    surface,
                    child,
                    panel_id,
                    cursor_x,
                    y,
                    child_width,
                    render_state,
                );
                max_height = max_height.max(used);
                cursor_x += child_width + child_gap;
            }
            max_height
        }
        PanelNode::Section {
            title, children, ..
        } => {
            let title_height =
                draw_wrapped_text(surface, x, y, title, SECTION_TITLE, available_width);
            let child_x = x + SECTION_INDENT;
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut cursor_y = y + title_height + SECTION_GAP;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(
                    surface,
                    child,
                    panel_id,
                    child_x,
                    cursor_y,
                    child_width,
                    render_state,
                );
                if index + 1 != children.len() {
                    cursor_y += SECTION_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Text { text, .. } => {
            draw_wrapped_text(surface, x, y, text, BODY_TEXT, available_width)
        }
        PanelNode::ColorPreview { label, color, .. } => {
            let label_height = draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width);
            let swatch_y = y + label_height + 4;
            let swatch_height = COLOR_PREVIEW_HEIGHT
                .saturating_sub(label_height + 4)
                .max(12);
            fill_rect(
                surface,
                x,
                swatch_y,
                available_width,
                swatch_height,
                color.to_rgba8(),
            );
            stroke_rect(
                surface,
                x,
                swatch_y,
                available_width,
                swatch_height,
                PREVIEW_SWATCH_BORDER,
            );
            COLOR_PREVIEW_HEIGHT
        }
        PanelNode::Button {
            id,
            label,
            active,
            fill_color,
            ..
        } => {
            let fill = fill_color.map_or(
                if *active {
                    BUTTON_ACTIVE_FILL
                } else {
                    BUTTON_FILL
                },
                ColorRgba8::to_rgba8,
            );
            let is_focused = render_state
                .focused_target
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            fill_rect(surface, x, y, available_width, BUTTON_HEIGHT, fill);
            stroke_rect(
                surface,
                x,
                y,
                available_width,
                BUTTON_HEIGHT,
                if *active {
                    BUTTON_ACTIVE_BORDER
                } else {
                    BUTTON_BORDER
                },
            );
            if is_focused && available_width > 2 && BUTTON_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    y + 1,
                    available_width - 2,
                    BUTTON_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            draw_wrapped_text(
                surface,
                x + 6,
                y + 7,
                label,
                button_text_color(*fill_color),
                available_width.saturating_sub(12),
            );
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: available_width,
                height: BUTTON_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Activate,
            });
            BUTTON_HEIGHT
        }
        PanelNode::Slider {
            id,
            label,
            min,
            max,
            value,
            fill_color,
            ..
        } => {
            let clamped_value = (*value).clamp(*min, *max);
            let accent = fill_color.unwrap_or(ColorRgba8::new(0x9f, 0xb7, 0xff, 0xff));
            let track_y = y + SLIDER_TRACK_TOP;
            let track_width = available_width.max(1);
            let track_inner_width = track_width.saturating_sub(2);
            let range = max.saturating_sub(*min).max(1);
            let progress = clamped_value.saturating_sub(*min);
            let fill_width = if track_inner_width == 0 {
                0
            } else {
                ((progress * track_inner_width) / range).max(1)
            };
            let knob_offset = if track_inner_width <= 1 {
                0
            } else {
                (progress * (track_inner_width - 1)) / range
            };
            let knob_x = (x + 1 + knob_offset)
                .saturating_sub(SLIDER_KNOB_WIDTH / 2)
                .min(x + track_width.saturating_sub(SLIDER_KNOB_WIDTH.min(track_width)));

            draw_wrapped_text(
                surface,
                x,
                y,
                &format!("{label}: {clamped_value}"),
                BODY_TEXT,
                available_width,
            );
            fill_rect(
                surface,
                x,
                track_y,
                track_width,
                SLIDER_TRACK_HEIGHT,
                SLIDER_TRACK_BACKGROUND,
            );
            stroke_rect(
                surface,
                x,
                track_y,
                track_width,
                SLIDER_TRACK_HEIGHT,
                SLIDER_TRACK_BORDER,
            );
            if fill_width > 0 {
                fill_rect(
                    surface,
                    x + 1,
                    track_y + 1,
                    fill_width.min(track_inner_width),
                    SLIDER_TRACK_HEIGHT.saturating_sub(2).max(1),
                    accent.to_rgba8(),
                );
            }
            fill_rect(
                surface,
                knob_x,
                track_y.saturating_sub(3),
                SLIDER_KNOB_WIDTH.min(track_width),
                SLIDER_TRACK_HEIGHT + 6,
                SLIDER_KNOB,
            );
            stroke_rect(
                surface,
                knob_x,
                track_y.saturating_sub(3),
                SLIDER_KNOB_WIDTH.min(track_width),
                SLIDER_TRACK_HEIGHT + 6,
                SLIDER_TRACK_BORDER,
            );
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: track_width,
                height: SLIDER_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Slider {
                    min: *min,
                    max: *max,
                },
            });
            SLIDER_HEIGHT
        }
        PanelNode::Dropdown {
            id,
            label,
            value,
            options,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let is_expanded = render_state
                .expanded_dropdown
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let selected_label = options
                .iter()
                .find(|option| option.value == *value)
                .map(|option| option.label.as_str())
                .unwrap_or(value.as_str());
            let button_label = if label.is_empty() {
                format!("{selected_label} ▾")
            } else {
                format!("{label}: {selected_label} ▾")
            };

            fill_rect(surface, x, y, available_width, DROPDOWN_HEIGHT, BUTTON_FILL);
            stroke_rect(
                surface,
                x,
                y,
                available_width,
                DROPDOWN_HEIGHT,
                if is_expanded {
                    BUTTON_ACTIVE_BORDER
                } else {
                    BUTTON_BORDER
                },
            );
            if is_focused && available_width > 2 && DROPDOWN_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    y + 1,
                    available_width - 2,
                    DROPDOWN_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            draw_wrapped_text(
                surface,
                x + 6,
                y + 7,
                &button_label,
                BUTTON_TEXT,
                available_width.saturating_sub(12),
            );
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: available_width,
                height: DROPDOWN_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Activate,
            });

            if !is_expanded {
                return DROPDOWN_HEIGHT;
            }

            let mut cursor_y = y + DROPDOWN_HEIGHT;
            for option in options {
                let active = option.value == *value;
                fill_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    DROPDOWN_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_FILL
                    } else {
                        PANEL_BACKGROUND
                    },
                );
                stroke_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    DROPDOWN_HEIGHT,
                    BUTTON_BORDER,
                );
                draw_wrapped_text(
                    surface,
                    x + 6,
                    cursor_y + 7,
                    &option.label,
                    if active { BUTTON_TEXT } else { BODY_TEXT },
                    available_width.saturating_sub(12),
                );
                surface.hit_regions.push(PanelHitRegion {
                    x,
                    y: cursor_y,
                    width: available_width,
                    height: DROPDOWN_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::DropdownOption {
                        value: option.value.clone(),
                    },
                });
                cursor_y += DROPDOWN_HEIGHT;
            }

            DROPDOWN_HEIGHT + options.len() * DROPDOWN_HEIGHT
        }
        PanelNode::LayerList {
            id,
            label,
            selected_index,
            items,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let mut cursor_y = y + label_height;
            let item_count = items.len().max(1);
            for index in 0..item_count {
                let item = items.get(index).cloned().unwrap_or(LayerListItem {
                    label: "<no layers>".to_string(),
                    detail: String::new(),
                });
                let active = *selected_index == index;
                fill_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    LAYER_LIST_ITEM_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_FILL
                    } else {
                        BUTTON_FILL
                    },
                );
                stroke_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    LAYER_LIST_ITEM_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_BORDER
                    } else {
                        BUTTON_BORDER
                    },
                );
                if is_focused && active && available_width > 2 && LAYER_LIST_ITEM_HEIGHT > 2 {
                    stroke_rect(
                        surface,
                        x + 1,
                        cursor_y + 1,
                        available_width - 2,
                        LAYER_LIST_ITEM_HEIGHT - 2,
                        BUTTON_FOCUS_BORDER,
                    );
                }
                draw_text_rgba(
                    &mut surface.pixels,
                    surface.width,
                    surface.height,
                    x + 6,
                    cursor_y + 6,
                    &item.label,
                    BUTTON_TEXT,
                );
                if !item.detail.is_empty() {
                    draw_text_rgba(
                        &mut surface.pixels,
                        surface.width,
                        surface.height,
                        x + 6,
                        cursor_y + LAYER_LIST_DETAIL_OFFSET,
                        &item.detail,
                        BODY_TEXT,
                    );
                }
                let grip_x = x + available_width.saturating_sub(LAYER_LIST_DRAG_HANDLE_WIDTH);
                for offset in [8usize, 14, 20] {
                    fill_rect(surface, grip_x, cursor_y + offset, 8, 1, BODY_TEXT);
                }
                surface.hit_regions.push(PanelHitRegion {
                    x,
                    y: cursor_y,
                    width: available_width,
                    height: LAYER_LIST_ITEM_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::LayerListItem { value: index },
                });
                cursor_y += LAYER_LIST_ITEM_HEIGHT;
            }

            label_height + item_count * LAYER_LIST_ITEM_HEIGHT
        }
        PanelNode::TextInput {
            id,
            label,
            value,
            placeholder,
            binding_path: _,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let editor_state = render_state
                .text_input_states
                .get(&(panel_id.to_string(), id.clone()))
                .cloned()
                .unwrap_or(TextInputEditorState {
                    cursor_chars: text_char_len(value),
                    preedit: None,
                });
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let box_y = y + label_height;
            fill_rect(
                surface,
                x,
                box_y,
                available_width,
                INPUT_BOX_HEIGHT,
                INPUT_BACKGROUND,
            );
            stroke_rect(
                surface,
                x,
                box_y,
                available_width,
                INPUT_BOX_HEIGHT,
                INPUT_BORDER,
            );
            if is_focused && available_width > 2 && INPUT_BOX_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    box_y + 1,
                    available_width - 2,
                    INPUT_BOX_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            let display_text = if let Some(preedit) = editor_state.preedit.as_deref() {
                insert_text_at_char_index(value, editor_state.cursor_chars, preedit)
            } else {
                value.clone()
            };
            let text_to_draw = if display_text.is_empty() {
                placeholder.clone()
            } else {
                display_text.clone()
            };
            draw_text_rgba(
                &mut surface.pixels,
                surface.width,
                surface.height,
                x + 6,
                box_y + 7,
                &text_to_draw,
                if display_text.is_empty() {
                    INPUT_PLACEHOLDER
                } else {
                    BUTTON_TEXT
                },
            );
            if is_focused {
                let caret_char_index = editor_state.cursor_chars
                    + editor_state
                        .preedit
                        .as_deref()
                        .map(text_char_len)
                        .unwrap_or(0);
                let caret_prefix = prefix_for_char_count(&display_text, caret_char_index);
                let caret_x = (x + 6 + measure_text_width(&caret_prefix))
                    .min(x + available_width.saturating_sub(3));
                fill_rect(
                    surface,
                    caret_x,
                    box_y + 4,
                    1,
                    INPUT_BOX_HEIGHT.saturating_sub(8).max(1),
                    BUTTON_FOCUS_BORDER,
                );
            }
            surface.hit_regions.push(PanelHitRegion {
                x,
                y: box_y,
                width: available_width,
                height: INPUT_BOX_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Activate,
            });
            label_height + INPUT_BOX_HEIGHT
        }
    }
}

fn button_text_color(fill_color: Option<ColorRgba8>) -> [u8; 4] {
    let Some(fill_color) = fill_color else {
        return BUTTON_TEXT;
    };
    let luminance = 0.2126 * f32::from(fill_color.r)
        + 0.7152 * f32::from(fill_color.g)
        + 0.0722 * f32::from(fill_color.b);
    if luminance >= 140.0 {
        BUTTON_TEXT_DARK
    } else {
        BUTTON_TEXT
    }
}

fn measure_text(text: &str, available_width: usize) -> usize {
    let lines = wrap_text(text, available_width);
    lines.len().max(1) * text_line_height()
}

fn draw_wrapped_text(
    surface: &mut PanelSurface,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
    available_width: usize,
) -> usize {
    let lines = wrap_text(text, available_width);
    for (index, line) in lines.iter().enumerate() {
        draw_text_line(surface, x, y + index * text_line_height(), line, color);
    }
    lines.len().max(1) * text_line_height()
}

fn wrap_text(text: &str, available_width: usize) -> Vec<String> {
    wrap_text_lines(text, available_width)
}

fn draw_text_line(surface: &mut PanelSurface, x: usize, y: usize, text: &str, color: [u8; 4]) {
    draw_text_rgba(
        surface.pixels.as_mut_slice(),
        surface.width,
        surface.height,
        x,
        y,
        text,
        color,
    );
}

fn fill_rect(
    surface: &mut PanelSurface,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    let max_x = (x + width).min(surface.width);
    let max_y = (y + height).min(surface.height);
    for yy in y..max_y {
        for xx in x..max_x {
            write_pixel(surface, xx, yy, color);
        }
    }
}

fn stroke_rect(
    surface: &mut PanelSurface,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    if width == 0 || height == 0 {
        return;
    }
    fill_rect(surface, x, y, width, 1, color);
    fill_rect(surface, x, y + height.saturating_sub(1), width, 1, color);
    fill_rect(surface, x, y, 1, height, color);
    fill_rect(surface, x + width.saturating_sub(1), y, 1, height, color);
}

fn write_pixel(surface: &mut PanelSurface, x: usize, y: usize, color: [u8; 4]) {
    if x >= surface.width || y >= surface.height {
        return;
    }
    let index = (y * surface.width + x) * 4;
    surface.pixels[index..index + 4].copy_from_slice(&color);
}

impl Default for UiShell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{draw_text_rgba, text_backend_name, wrap_text_lines};
    use app_core::{Command, ToolKind};
    use plugin_api::{DropdownOption, HostAction, LayerListItem, PanelPlugin};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_DSL_PANEL: &str = r#"
panel {
    id: "builtin.dsl-test"
    title: "Phase 6 Test"
    version: 1
}

permissions {
    read.document
    write.command
}

runtime {
    wasm: "sample_test.wasm"
}

state {
    expanded: bool = false
    active_tool: string = ""
    document_title: string = ""
}

view {
    <column gap=8 padding=8>
        <section title="Runtime">
                        <text tone="muted">Loaded from disk</text>
                        <button id="dsl.save" on:click="save_project">Save</button>
                        <button id="dsl.brush" on:click="activate_brush" active={state.active_tool == "brush"}>Brush</button>
                        <toggle id="dsl.expanded" checked={state.expanded} on:change="toggle_expanded">Expanded</toggle>
                        <when test={state.expanded}>
                                <text>{state.document_title}</text>
                        </when>
        </section>
    </column>
}
"#;

    const SAMPLE_DSL_WAT: &str = r#"(module
    (import "host" "state_toggle" (func $state_toggle (param i32 i32)))
    (import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
    (import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
    (import "host" "host_get_string_len" (func $host_get_string_len (param i32 i32) (result i32)))
    (import "host" "host_get_string_copy" (func $host_get_string_copy (param i32 i32 i32 i32)))
    (import "host" "command" (func $command (param i32 i32)))
    (import "host" "command_string" (func $command_string (param i32 i32 i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "expanded")
    (data (i32.const 16) "active_tool")
    (data (i32.const 32) "document_title")
    (data (i32.const 64) "tool.active")
    (data (i32.const 80) "document.title")
    (data (i32.const 96) "project.save")
    (data (i32.const 112) "tool.set_active")
    (data (i32.const 144) "tool")
    (data (i32.const 160) "brush")
    (func (export "panel_init")
        i32.const 0
        i32.const 8
        i32.const 0
        call $state_set_bool)
    (func (export "panel_sync_host")
        (local $len i32)
        i32.const 64
        i32.const 11
        call $host_get_string_len
        local.set $len
        i32.const 64
        i32.const 11
        i32.const 256
        local.get $len
        call $host_get_string_copy
        i32.const 16
        i32.const 11
        i32.const 256
        local.get $len
        call $state_set_string
        i32.const 80
        i32.const 14
        call $host_get_string_len
        local.set $len
        i32.const 80
        i32.const 14
        i32.const 320
        local.get $len
        call $host_get_string_copy
        i32.const 32
        i32.const 14
        i32.const 320
        local.get $len
        call $state_set_string)
    (func (export "panel_handle_toggle_expanded")
        i32.const 0
        i32.const 8
        call $state_toggle)
    (func (export "panel_handle_save_project")
        i32.const 96
        i32.const 12
        call $command)
    (func (export "panel_handle_activate_brush")
        i32.const 112
        i32.const 15
        i32.const 144
        i32.const 4
        i32.const 160
        i32.const 5
        call $command_string))"#;

    const BUILTIN_APP_ACTIONS_PANEL: &str = r#"
panel {
    id: "builtin.app-actions"
    title: "App"
    version: 1
}

permissions {
    read.document
    write.command
}

runtime {
    wasm: "builtin-app-actions.wasm"
}

state {
}

view {
    <column gap=8 padding=8>
        <section title="Project">
            <text tone="muted">Hosted via DSL + Wasm</text>
            <button id="app.new" on:click="new_project">New</button>
            <button id="app.save" on:click="save_project">Save</button>
            <button id="app.load" on:click="load_project">Load</button>
        </section>
    </column>
}
"#;

    const BUILTIN_APP_ACTIONS_WAT: &str = r#"(module
    (import "host" "command" (func $command (param i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "project.new")
    (data (i32.const 16) "project.save")
    (data (i32.const 32) "project.load")
    (func (export "panel_init"))
    (func (export "panel_handle_new_project")
        i32.const 0
        i32.const 11
        call $command)
    (func (export "panel_handle_save_project")
        i32.const 16
        i32.const 12
        call $command)
    (func (export "panel_handle_load_project")
        i32.const 32
        i32.const 12
        call $command))"#;

    const SAMPLE_INPUT_PANEL: &str = r#"
panel {
    id: "builtin.input-test"
    title: "Input Test"
    version: 1
}

permissions {
    read.document
}

runtime {
    wasm: "input_test.wasm"
}

state {
    width: string = "64"
}

view {
    <column gap=8 padding=8>
        <section title="Fields">
            <input id="input.width" label="Width" value={state.width} bind="width" mode="numeric" placeholder="64" />
        </section>
    </column>
}
"#;

    const SAMPLE_INPUT_WAT: &str = r#"(module
    (memory (export "memory") 1)
    (func (export "panel_init")))"#;

    const SAMPLE_TEXT_INPUT_PANEL: &str = r#"
panel {
    id: "builtin.text-input-test"
    title: "Text Input Test"
    version: 1
}

permissions {
    read.document
}

runtime {
    wasm: "input_test.wasm"
}

state {
    text: string = "ab"
}

view {
    <column gap=8 padding=8>
        <section title="Fields">
            <input id="input.text" label="Text" value={state.text} bind="text" placeholder="text" />
        </section>
    </column>
}
"#;

    /// `UiShell` の更新配送を確認するためのダミーパネル。
    struct TestPanel {
        updates: usize,
    }

    impl PanelPlugin for TestPanel {
        fn id(&self) -> &'static str {
            "test.panel"
        }

        fn title(&self) -> &'static str {
            "Test Panel"
        }

        fn update(&mut self, _document: &Document) {
            self.updates += 1;
        }
    }

    struct TestLayerListPanel;

    impl PanelPlugin for TestLayerListPanel {
        fn id(&self) -> &'static str {
            "test.layer-list"
        }

        fn title(&self) -> &'static str {
            "Layer List"
        }

        fn panel_tree(&self) -> PanelTree {
            PanelTree {
                id: self.id(),
                title: self.title(),
                children: vec![PanelNode::LayerList {
                    id: "layers.list".to_string(),
                    label: "Layers".to_string(),
                    selected_index: 0,
                    action: HostAction::DispatchCommand(Command::Noop),
                    items: vec![
                        LayerListItem {
                            label: "Layer 1".to_string(),
                            detail: "blend: normal / visible / mask: false".to_string(),
                        },
                        LayerListItem {
                            label: "Layer 2".to_string(),
                            detail: "blend: multiply / visible / mask: false".to_string(),
                        },
                        LayerListItem {
                            label: "Layer 3".to_string(),
                            detail: "blend: screen / hidden / mask: true".to_string(),
                        },
                    ],
                }],
            }
        }
    }

    struct TestDropdownPanel;

    impl PanelPlugin for TestDropdownPanel {
        fn id(&self) -> &'static str {
            "test.dropdown"
        }

        fn title(&self) -> &'static str {
            "Dropdown"
        }

        fn panel_tree(&self) -> PanelTree {
            PanelTree {
                id: self.id(),
                title: self.title(),
                children: vec![PanelNode::Dropdown {
                    id: "blend.mode".to_string(),
                    label: "Blend Mode".to_string(),
                    value: "normal".to_string(),
                    action: HostAction::DispatchCommand(Command::Noop),
                    options: vec![
                        DropdownOption {
                            label: "Normal".to_string(),
                            value: "normal".to_string(),
                        },
                        DropdownOption {
                            label: "Multiply".to_string(),
                            value: "multiply".to_string(),
                        },
                    ],
                }],
            }
        }
    }

    /// パネル登録がホスト状態に反映されることを確認する。
    #[test]
    fn registering_panel_increases_panel_count() {
        let mut shell = UiShell::new();
        let initial_count = shell.panel_count();
        shell.register_panel(Box::new(TestPanel { updates: 0 }));

        assert_eq!(shell.panel_count(), initial_count + 1);
    }

    /// `update` が登録済みパネルへ配送される経路を壊していないことを確認する。
    #[test]
    fn update_dispatches_to_registered_panels() {
        let mut shell = UiShell::new();
        let initial_count = shell.panel_count();
        shell.register_panel(Box::new(TestPanel { updates: 0 }));

        shell.update(&Document::default());

        assert_eq!(shell.panel_count(), initial_count + 1);
    }

    #[test]
    fn default_shell_registers_builtin_layers_panel() {
        let shell = shell_with_builtin_panels();
        let panels = shell.panel_trees();
        let layers_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.layers-panel")
            .expect("layers panel exists");

        assert!(tree_contains_text(&layers_panel.children, "Layer 1"));
    }

    #[test]
    fn default_shell_registers_builtin_tool_palette() {
        let shell = shell_with_builtin_panels();
        let panels = shell.panel_trees();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");

        assert!(tree_contains_button_label(
            &tool_panel.children,
            "Brush",
            true
        ));
    }

    #[test]
    fn shell_exposes_panel_tree_buttons() {
        let shell = shell_with_builtin_panels();

        let panels = shell.panel_trees();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");

        fn has_brush_button(items: &[PanelNode]) -> bool {
            items.iter().any(|item| match item {
                PanelNode::Button { label, .. } => label == "Brush",
                PanelNode::Column { children, .. }
                | PanelNode::Row { children, .. }
                | PanelNode::Section { children, .. } => has_brush_button(children),
                PanelNode::Text { .. }
                | PanelNode::ColorPreview { .. }
                | PanelNode::Slider { .. }
                | PanelNode::TextInput { .. }
                | PanelNode::Dropdown { .. }
                | PanelNode::LayerList { .. } => false,
            })
        }

        assert!(has_brush_button(&tool_panel.children));
    }

    #[test]
    fn panel_event_returns_command_action() {
        let mut shell = shell_with_builtin_panels();

        let actions = shell.handle_panel_event(&PanelEvent::Activate {
            panel_id: "builtin.tool-palette".to_string(),
            node_id: "tool.eraser".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveTool {
                tool: ToolKind::Eraser,
            })]
        );
    }

    #[test]
    fn default_shell_registers_builtin_color_palette() {
        let shell = shell_with_builtin_panels();
        let panels = shell.panel_trees();
        let color_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.color-palette")
            .expect("color panel exists");

        assert!(tree_contains_text(&color_panel.children, "#000000"));
    }

    #[test]
    fn color_palette_slider_event_returns_color_command_action() {
        let mut shell = shell_with_builtin_panels();

        let actions = shell.handle_panel_event(&PanelEvent::SetValue {
            panel_id: "builtin.color-palette".to_string(),
            node_id: "color.slider.red".to_string(),
            value: 128,
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveColor {
                color: app_core::ColorRgba8::new(128, 0x00, 0x00, 0xff),
            })]
        );
    }

    #[test]
    fn color_palette_tree_contains_live_preview() {
        let shell = shell_with_builtin_panels();

        let panels = shell.panel_trees();
        let color_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.color-palette")
            .expect("color panel exists");

        fn has_preview(items: &[PanelNode]) -> bool {
            items.iter().any(|item| match item {
                PanelNode::ColorPreview { .. } => true,
                PanelNode::Column { children, .. }
                | PanelNode::Row { children, .. }
                | PanelNode::Section { children, .. } => has_preview(children),
                PanelNode::Text { .. }
                | PanelNode::Button { .. }
                | PanelNode::Slider { .. }
                | PanelNode::TextInput { .. }
                | PanelNode::Dropdown { .. }
                | PanelNode::LayerList { .. } => false,
            })
        }

        assert!(has_preview(&color_panel.children));
    }

    #[test]
    fn rendered_panel_surface_maps_slider_region_to_value_event() {
        let mut shell = shell_with_builtin_panels();
        let surface = shell.render_panel_surface(280, 800);

        let mut found = None;
        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::SetValue {
                    panel_id,
                    node_id,
                    value,
                }) = surface.hit_test(x, y)
                    && panel_id == "builtin.color-palette"
                    && node_id == "color.slider.red"
                {
                    found = Some(value);
                    break 'outer;
                }
            }
        }

        assert!(found.is_some());
    }

    #[test]
    fn rendered_panel_surface_contains_clickable_button_region() {
        let mut shell = shell_with_builtin_panels();
        let surface = shell.render_panel_surface(280, 3200);

        let mut found = None;
        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::Activate { panel_id, node_id }) = surface.hit_test(x, y)
                    && panel_id == "builtin.tool-palette"
                    && node_id == "tool.brush"
                {
                    found = Some((x, y));
                    break 'outer;
                }
            }
        }

        assert!(found.is_some());
    }

    #[test]
    fn rendered_layer_list_drag_maps_to_drag_value_event() {
        let mut shell = UiShell::new();
        shell.register_panel(Box::new(TestLayerListPanel));
        shell.update(&Document::default());

        let surface = shell.render_panel_surface(280, 320);
        let mut source = None;
        let mut target = None;

        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::SetValue {
                    panel_id,
                    node_id,
                    value,
                }) = surface.hit_test(x, y)
                    && panel_id == "test.layer-list"
                    && node_id == "layers.list"
                {
                    if value == 0 && source.is_none() {
                        source = Some((x, y));
                    }
                    if value == 2 {
                        target = Some((x, y));
                        break 'outer;
                    }
                }
            }
        }

        let (target_x, target_y) = target.expect("target layer hit exists");
        let drag_event = surface.drag_event("test.layer-list", "layers.list", 0, target_x, target_y);

        assert_eq!(
            drag_event,
            Some(PanelEvent::DragValue {
                panel_id: "test.layer-list".to_string(),
                node_id: "layers.list".to_string(),
                from: 0,
                to: 2,
            })
        );
        assert!(source.is_some());
    }

    #[test]
    fn dropdown_expands_and_option_hit_sets_text_event() {
        let mut shell = UiShell::new();
        shell.register_panel(Box::new(TestDropdownPanel));
        shell.update(&Document::default());

        let collapsed = shell.render_panel_surface(280, 200);
        let (root_x, root_y) = (0..collapsed.height)
            .find_map(|y| {
                (0..collapsed.width).find_map(|x| {
                    match collapsed.hit_test(x, y) {
                        Some(PanelEvent::Activate { panel_id, node_id })
                            if panel_id == "test.dropdown" && node_id == "blend.mode" =>
                        {
                            Some((x, y))
                        }
                        _ => None,
                    }
                })
            })
            .expect("dropdown root exists");

        assert!(shell
            .handle_panel_event(&PanelEvent::Activate {
                panel_id: "test.dropdown".to_string(),
                node_id: "blend.mode".to_string(),
            })
            .is_empty());

        let expanded = shell.render_panel_surface(280, 240);
        let option_event = (0..expanded.height)
            .find_map(|y| {
                (0..expanded.width).find_map(|x| match expanded.hit_test(x, y) {
                    Some(PanelEvent::SetText {
                        panel_id,
                        node_id,
                        value,
                    }) if panel_id == "test.dropdown"
                        && node_id == "blend.mode"
                        && value == "multiply" => Some((x, y)),
                    _ => None,
                })
            })
            .expect("dropdown option exists");

        assert!(root_x < expanded.width && root_y < expanded.height);
        assert!(option_event.0 < expanded.width && option_event.1 < expanded.height);
    }

    #[test]
    fn focus_navigation_can_activate_focused_button() {
        let mut shell = shell_with_builtin_panels();

        assert!(shell.focus_next());
        assert_eq!(
            shell.focused_target(),
            Some((
                "builtin.workspace-layout",
                "workspace.move-up.builtin.app-actions"
            ))
        );

        assert!(shell.focus_panel_node("builtin.app-actions", "app.save"));
        assert_eq!(
            shell.activate_focused(),
            vec![HostAction::DispatchCommand(Command::SaveProject)]
        );
    }

    #[test]
    fn app_actions_panel_exposes_inline_new_document_inputs() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL)
            .expect("dsl panel written");
        fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let app_panel = shell
            .panel_trees()
            .into_iter()
            .find(|panel| panel.id == "builtin.input-test")
            .expect("app panel exists");
        assert!(tree_contains_text_input(
            &app_panel.children,
            "input.width",
            "64"
        ));
    }

    #[test]
    fn focused_text_input_updates_bound_state() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL)
            .expect("dsl panel written");
        fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        assert!(shell.focus_panel_node("builtin.input-test", "input.width"));
        assert!(shell.backspace_focused_input());
        assert!(shell.backspace_focused_input());
        assert!(shell.insert_text_into_focused_input("320"));

        let app_panel = shell
            .panel_trees()
            .into_iter()
            .find(|panel| panel.id == "builtin.input-test")
            .expect("app panel exists");
        assert!(tree_contains_text_input(
            &app_panel.children,
            "input.width",
            "320"
        ));
    }

    #[test]
    fn focused_text_input_supports_cursor_movement_and_space() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("input.altp-panel"), SAMPLE_TEXT_INPUT_PANEL)
            .expect("dsl panel written");
        fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        assert!(shell.focus_panel_node("builtin.text-input-test", "input.text"));
        assert!(shell.insert_text_into_focused_input(" c"));
        assert!(shell.move_focused_input_cursor(-1));
        assert!(shell.backspace_focused_input());
        assert!(shell.insert_text_into_focused_input("d"));

        let app_panel = shell
            .panel_trees()
            .into_iter()
            .find(|panel| panel.id == "builtin.text-input-test")
            .expect("app panel exists");
        assert!(tree_contains_text_input(
            &app_panel.children,
            "input.text",
            "abdc"
        ));
    }

    #[test]
    fn focused_text_input_tracks_preedit_text() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL)
            .expect("dsl panel written");
        fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        assert!(shell.focus_panel_node("builtin.input-test", "input.width"));
        assert!(shell.set_focused_input_preedit(Some("12".to_string())));
        assert!(shell.set_focused_input_preedit(None));
    }

    #[test]
    fn workspace_manager_panel_can_emit_reorder_action() {
        let mut shell = shell_with_builtin_panels();

        let actions = shell.handle_panel_event(&PanelEvent::Activate {
            panel_id: WORKSPACE_PANEL_ID.to_string(),
            node_id: "workspace.move-down.builtin.app-actions".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::MovePanel {
                panel_id: "builtin.app-actions".to_string(),
                direction: PanelMoveDirection::Down,
            }]
        );
    }

    #[test]
    fn workspace_layout_hides_panel_from_rendered_tree() {
        let mut shell = shell_with_builtin_panels();

        assert!(shell.set_panel_visibility("builtin.tool-palette", false));

        assert!(
            shell
                .panel_trees()
                .iter()
                .all(|panel| panel.id != "builtin.tool-palette")
        );
    }

    #[test]
    fn workspace_layout_reorders_visible_panels() {
        let mut shell = shell_with_builtin_panels();

        let before_ids = shell
            .panel_trees()
            .iter()
            .map(|panel| panel.id)
            .collect::<Vec<_>>();
        let before_index = before_ids
            .iter()
            .position(|panel_id| *panel_id == "builtin.layers-panel")
            .expect("layers panel visible");

        assert!(shell.move_panel("builtin.layers-panel", PanelMoveDirection::Up));
        assert!(shell.move_panel("builtin.layers-panel", PanelMoveDirection::Up));

        let visible_ids = shell
            .panel_trees()
            .iter()
            .map(|panel| panel.id)
            .collect::<Vec<_>>();
        let layers_index = visible_ids
            .iter()
            .position(|panel_id| *panel_id == "builtin.layers-panel")
            .expect("layers panel visible");
        assert!(layers_index < before_index);
    }

    #[test]
    fn scrolling_panels_updates_scroll_offset() {
        let mut shell = shell_with_builtin_panels();
        let _ = shell.render_panel_surface(280, 96);

        assert!(shell.scroll_panels(6, 96));
        assert!(shell.panel_scroll_offset() > 0);
    }

    #[test]
    fn scrolling_panels_keeps_cached_panel_content() {
        let mut shell = shell_with_builtin_panels();
        let _ = shell.render_panel_surface(280, 96);

        assert!(!shell.panel_content_dirty);
        assert!(shell.scroll_panels(6, 96));
        assert!(!shell.panel_content_dirty);
    }

    #[test]
    fn focus_change_invalidates_cached_panel_content() {
        let mut shell = shell_with_builtin_panels();
        let _ = shell.render_panel_surface(280, 96);

        assert!(!shell.panel_content_dirty);
        assert!(shell.focus_next());
        assert!(shell.panel_content_dirty);
    }

    #[test]
    fn loading_panel_directory_registers_dsl_panel() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written");
        fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        let diagnostics = shell.load_panel_directory(&temp_dir);

        assert!(diagnostics.is_empty());
        let panels = shell.panel_trees();
        let dsl_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.dsl-test")
            .expect("dsl panel exists");
        assert!(matches!(
            &dsl_panel.children[0],
            PanelNode::Column { children, .. }
                if matches!(
                    &children[0],
                    PanelNode::Section { title, .. } if title == "Runtime"
                )
        ));
        assert_eq!(
            shell.handle_panel_event(&PanelEvent::Activate {
                panel_id: "builtin.dsl-test".to_string(),
                node_id: "dsl.save".to_string(),
            }),
            vec![HostAction::DispatchCommand(Command::SaveProject)]
        );
    }

    #[test]
    fn runtime_backed_dsl_panel_applies_state_patch_and_host_snapshot() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written");
        fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let before = shell
            .panel_trees()
            .into_iter()
            .find(|panel| panel.id == "builtin.dsl-test")
            .expect("dsl panel exists");
        assert!(!tree_contains_text(&before.children, "Untitled"));

        let _ = shell.handle_panel_event(&PanelEvent::Activate {
            panel_id: "builtin.dsl-test".to_string(),
            node_id: "dsl.expanded".to_string(),
        });

        let after = shell
            .panel_trees()
            .into_iter()
            .find(|panel| panel.id == "builtin.dsl-test")
            .expect("dsl panel exists");
        assert!(tree_contains_text(&after.children, "Untitled"));
    }

    #[test]
    fn runtime_backed_dsl_panel_converts_command_descriptor_to_command() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written");
        fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let actions = shell.handle_panel_event(&PanelEvent::Activate {
            panel_id: "builtin.dsl-test".to_string(),
            node_id: "dsl.brush".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveTool {
                tool: ToolKind::Brush,
            })]
        );
    }

    #[test]
    fn reloading_panel_directory_replaces_previous_dsl_panel() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written");
        fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL)
            .expect("first dsl panel written");

        let mut shell = UiShell::new();
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let updated_panel = SAMPLE_DSL_PANEL.replace("Phase 6 Test", "DSL Reloaded");
        fs::write(temp_dir.join("sample.altp-panel"), updated_panel)
            .expect("updated dsl panel written");

        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let panels = shell.panel_trees();
        let matching = panels
            .iter()
            .filter(|panel| panel.id == "builtin.dsl-test")
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].title, "DSL Reloaded");
    }

    #[test]
    fn loading_dsl_panel_replaces_builtin_panel_with_same_id() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        fs::write(
            temp_dir.join("builtin-app-actions.altp-panel"),
            BUILTIN_APP_ACTIONS_PANEL,
        )
        .expect("app actions panel written");
        fs::write(
            temp_dir.join("builtin-app-actions.wasm"),
            BUILTIN_APP_ACTIONS_WAT,
        )
        .expect("app actions wasm written");

        let mut shell = shell_with_builtin_panels();
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        let panels = shell.panel_trees();
        let matching = panels
            .iter()
            .filter(|panel| panel.id == "builtin.app-actions")
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert!(tree_contains_text(
            &matching[0].children,
            "Hosted via DSL + Wasm"
        ));
        assert_eq!(
            shell.handle_panel_event(&PanelEvent::Activate {
                panel_id: "builtin.app-actions".to_string(),
                node_id: "app.save".to_string(),
            }),
            vec![HostAction::DispatchCommand(Command::SaveProject)]
        );
    }

    #[test]
    fn migrated_builtin_dsl_panels_use_host_snapshot_data() {
        let mut shell = UiShell::new();
        let mut document = Document::default();
        document.set_active_tool(ToolKind::Eraser);
        assert!(
            shell
                .load_panel_directory(default_builtin_panel_dir())
                .is_empty()
        );
        shell.update(&document);

        let panels = shell.panel_trees();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");
        let layers_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.layers-panel")
            .expect("layers panel exists");

        assert!(tree_contains_button_label(
            &tool_panel.children,
            "Eraser",
            true
        ));
        assert!(tree_contains_text(&layers_panel.children, "Untitled"));
        assert!(tree_contains_text(
            &layers_panel.children,
            "pages: 1 / panels: 1 / layers: 1"
        ));
        assert!(tree_contains_text(&layers_panel.children, "Layer 1"));
    }

    #[test]
    fn migrated_builtin_dsl_panels_render_interpolated_mixed_text() {
        let mut shell = UiShell::new();
        assert!(
            shell
                .load_panel_directory(default_builtin_panel_dir())
                .is_empty()
        );
        shell.update(&Document::default());

        let panels = shell.panel_trees();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");
        let layers_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.layers-panel")
            .expect("layers panel exists");
        let pen_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.pen-settings")
            .expect("pen panel exists");

        assert!(tree_contains_text(
            &tool_panel.children,
            "Preset: Round Pen"
        ));
        assert!(tree_contains_text(
            &tool_panel.children,
            "Size: 4px / presets: 1"
        ));
        assert!(tree_contains_text(&layers_panel.children, "index: 0"));
        assert!(tree_contains_text(
            &layers_panel.children,
            "blend: normal / visible / mask: false"
        ));
        assert!(tree_contains_text(&layers_panel.children, "visible: true"));
        assert!(tree_contains_text(&layers_panel.children, "mask: false"));
        assert!(tree_contains_text(&pen_panel.children, "4px"));
    }

    #[test]
    fn command_descriptor_accepts_numeric_payload_encoded_as_string() {
        let mut descriptor = CommandDescriptor::new("tool.set_size");
        descriptor
            .payload
            .insert("size".to_string(), Value::String("12".to_string()));

        assert_eq!(
            command_from_descriptor(&descriptor),
            Ok(Command::SetActivePenSize { size: 12 })
        );
    }

    #[test]
    fn command_descriptor_maps_layer_rename_active() {
        let mut descriptor = CommandDescriptor::new("layer.rename_active");
        descriptor
            .payload
            .insert("name".to_string(), Value::String("Ink".to_string()));

        assert_eq!(
            command_from_descriptor(&descriptor),
            Ok(Command::RenameActiveLayer {
                name: "Ink".to_string(),
            })
        );
    }

    #[test]
    fn load_panel_directory_discovers_nested_panel_files() {
        let temp_dir = unique_test_dir();
        let nested_dir = temp_dir.join("nested").join("plugin");
        fs::create_dir_all(&nested_dir).expect("nested temp dir created");
        fs::write(nested_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL)
            .expect("dsl panel written");
        fs::write(nested_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT)
            .expect("wasm sample written");

        let mut shell = UiShell::new();
        shell.update(&Document::default());
        assert!(shell.load_panel_directory(&temp_dir).is_empty());

        assert!(
            shell
                .panel_trees()
                .iter()
                .any(|panel| panel.id == "builtin.dsl-test")
        );
    }

    fn tree_contains_text(nodes: &[PanelNode], target: &str) -> bool {
        nodes.iter().any(|node| match node {
            PanelNode::Text { text, .. } => text == target,
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => tree_contains_text(children, target),
            PanelNode::Dropdown {
                label,
                value,
                options,
                ..
            } => {
                label == target
                    || value == target
                    || options.iter().any(|option| option.label == target || option.value == target)
            }
            PanelNode::LayerList { label, items, .. } => {
                label == target
                    || items
                        .iter()
                        .any(|item| item.label == target || item.detail == target)
            }
            PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. } => false,
        })
    }

    fn tree_contains_text_input(nodes: &[PanelNode], target_id: &str, target_value: &str) -> bool {
        nodes.iter().any(|node| match node {
            PanelNode::TextInput { id, value, .. } => id == target_id && value == target_value,
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                tree_contains_text_input(children, target_id, target_value)
            }
            PanelNode::ColorPreview { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::Text { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => false,
        })
    }

    fn tree_contains_button_label(nodes: &[PanelNode], target: &str, active: bool) -> bool {
        nodes.iter().any(|node| match node {
            PanelNode::Button {
                label,
                active: is_active,
                ..
            } => label == target && *is_active == active,
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                tree_contains_button_label(children, target, active)
            }
            PanelNode::ColorPreview { .. }
            | PanelNode::Slider { .. }
            | PanelNode::Text { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => false,
        })
    }

    #[test]
    fn text_renderer_draws_visible_pixels() {
        let mut pixels = vec![0; 160 * 40 * 4];

        draw_text_rgba(&mut pixels, 160, 40, 4, 4, "Aa", [0xff, 0xff, 0xff, 0xff]);

        assert!(pixels.chunks_exact(4).any(|pixel| pixel != [0, 0, 0, 0]));
        if text_backend_name() == "system" {
            assert!(pixels.chunks_exact(4).any(|pixel| {
                pixel[0] != 0 && pixel[0] != 0xff && pixel[0] == pixel[1] && pixel[1] == pixel[2]
            }));
        }
    }

    #[test]
    fn wrap_text_lines_preserves_long_words() {
        let lines = wrap_text_lines("antidisestablishmentarianism", 24);

        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), "antidisestablishmentarianism");
    }

    fn unique_test_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-ui-shell-{suffix}"))
    }

    fn default_builtin_panel_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins")
    }

    fn shell_with_builtin_panels() -> UiShell {
        let mut shell = UiShell::new();
        let diagnostics = shell.load_panel_directory(default_builtin_panel_dir());
        assert!(
            diagnostics.is_empty(),
            "expected builtin panels to load: {diagnostics:?}"
        );
        shell.update(&Document::default());
        shell
    }
}
