use super::dsl::command_from_descriptor;
use super::workspace::WORKSPACE_PANEL_ID;
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
struct TestPanel { updates: usize }
impl PanelPlugin for TestPanel {
    fn id(&self) -> &'static str { "test.panel" }
    fn title(&self) -> &'static str { "Test Panel" }
    fn update(&mut self, _document: &Document) { self.updates += 1; }
}

struct TestLayerListPanel;
impl PanelPlugin for TestLayerListPanel {
    fn id(&self) -> &'static str { "test.layer-list" }
    fn title(&self) -> &'static str { "Layer List" }
    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(), title: self.title(), children: vec![PanelNode::LayerList {
                id: "layers.list".to_string(), label: "Layers".to_string(), selected_index: 0, action: HostAction::DispatchCommand(Command::Noop), items: vec![
                    LayerListItem { label: "Layer 1".to_string(), detail: "blend: normal / visible / mask: false".to_string() },
                    LayerListItem { label: "Layer 2".to_string(), detail: "blend: multiply / visible / mask: false".to_string() },
                    LayerListItem { label: "Layer 3".to_string(), detail: "blend: screen / hidden / mask: true".to_string() },
                ],
            }],
        }
    }
}

struct TestDropdownPanel;
impl PanelPlugin for TestDropdownPanel {
    fn id(&self) -> &'static str { "test.dropdown" }
    fn title(&self) -> &'static str { "Dropdown" }
    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(), title: self.title(), children: vec![PanelNode::Dropdown {
                id: "blend.mode".to_string(), label: "Blend Mode".to_string(), value: "normal".to_string(), action: HostAction::DispatchCommand(Command::Noop), options: vec![
                    DropdownOption { label: "Normal".to_string(), value: "normal".to_string() },
                    DropdownOption { label: "Multiply".to_string(), value: "multiply".to_string() },
                ],
            }],
        }
    }
}

#[test]
fn registering_panel_increases_panel_count() {
    let mut shell = UiShell::new();
    let initial_count = shell.panel_count();
    shell.register_panel(Box::new(TestPanel { updates: 0 }));
    assert_eq!(shell.panel_count(), initial_count + 1);
}

#[test]
fn update_dispatches_to_registered_panels() {
    let mut shell = UiShell::new();
    let initial_count = shell.panel_count();
    shell.register_panel(Box::new(TestPanel { updates: 0 }));
    shell.update(&Document::default());
    assert_eq!(shell.panel_count(), initial_count + 1);
}

#[test]
fn default_shell_registers_builtin_layers_panel() { let shell = shell_with_builtin_panels(); let panels = shell.panel_trees(); let layers_panel = panels.iter().find(|panel| panel.id == "builtin.layers-panel").expect("layers panel exists"); assert!(tree_contains_text(&layers_panel.children, "Layer 1")); }
#[test]
fn default_shell_registers_builtin_tool_palette() { let shell = shell_with_builtin_panels(); let panels = shell.panel_trees(); let tool_panel = panels.iter().find(|panel| panel.id == "builtin.tool-palette").expect("tool panel exists"); assert!(tree_contains_button_label(&tool_panel.children, "Brush", true)); }
#[test]
fn shell_exposes_panel_tree_buttons() { let shell = shell_with_builtin_panels(); let panels = shell.panel_trees(); let tool_panel = panels.iter().find(|panel| panel.id == "builtin.tool-palette").expect("tool panel exists"); fn has_brush_button(items: &[PanelNode]) -> bool { items.iter().any(|item| match item { PanelNode::Button { label, .. } => label == "Brush", PanelNode::Column { children, .. } | PanelNode::Row { children, .. } | PanelNode::Section { children, .. } => has_brush_button(children), PanelNode::Text { .. } | PanelNode::ColorPreview { .. } | PanelNode::Slider { .. } | PanelNode::TextInput { .. } | PanelNode::Dropdown { .. } | PanelNode::LayerList { .. } => false, }) } assert!(has_brush_button(&tool_panel.children)); }
#[test]
fn panel_event_returns_command_action() { let mut shell = shell_with_builtin_panels(); let actions = shell.handle_panel_event(&PanelEvent::Activate { panel_id: "builtin.tool-palette".to_string(), node_id: "tool.eraser".to_string(), }); assert_eq!(actions, vec![HostAction::DispatchCommand(Command::SetActiveTool { tool: ToolKind::Eraser })]); }
#[test]
fn default_shell_registers_builtin_color_palette() { let shell = shell_with_builtin_panels(); let panels = shell.panel_trees(); let color_panel = panels.iter().find(|panel| panel.id == "builtin.color-palette").expect("color panel exists"); assert!(tree_contains_text(&color_panel.children, "#000000")); }
#[test]
fn color_palette_slider_event_returns_color_command_action() { let mut shell = shell_with_builtin_panels(); let actions = shell.handle_panel_event(&PanelEvent::SetValue { panel_id: "builtin.color-palette".to_string(), node_id: "color.slider.red".to_string(), value: 128, }); assert_eq!(actions, vec![HostAction::DispatchCommand(Command::SetActiveColor { color: app_core::ColorRgba8::new(128, 0x00, 0x00, 0xff) })]); }
#[test]
fn color_palette_tree_contains_live_preview() { let shell = shell_with_builtin_panels(); let panels = shell.panel_trees(); let color_panel = panels.iter().find(|panel| panel.id == "builtin.color-palette").expect("color panel exists"); fn has_preview(items: &[PanelNode]) -> bool { items.iter().any(|item| match item { PanelNode::ColorPreview { .. } => true, PanelNode::Column { children, .. } | PanelNode::Row { children, .. } | PanelNode::Section { children, .. } => has_preview(children), PanelNode::Text { .. } | PanelNode::Button { .. } | PanelNode::Slider { .. } | PanelNode::TextInput { .. } | PanelNode::Dropdown { .. } | PanelNode::LayerList { .. } => false, }) } assert!(has_preview(&color_panel.children)); }
#[test]
fn rendered_panel_surface_maps_slider_region_to_value_event() { let mut shell = shell_with_builtin_panels(); let surface = shell.render_panel_surface(280, 800); let mut found = None; 'outer: for y in 0..surface.height { for x in 0..surface.width { if let Some(PanelEvent::SetValue { panel_id, node_id, value }) = surface.hit_test(x, y) && panel_id == "builtin.color-palette" && node_id == "color.slider.red" { found = Some(value); break 'outer; } } } assert!(found.is_some()); }
#[test]
fn rendered_panel_surface_contains_clickable_button_region() { let mut shell = shell_with_builtin_panels(); let surface = shell.render_panel_surface(280, 3200); let mut found = None; 'outer: for y in 0..surface.height { for x in 0..surface.width { if let Some(PanelEvent::Activate { panel_id, node_id }) = surface.hit_test(x, y) && panel_id == "builtin.tool-palette" && node_id == "tool.brush" { found = Some((x, y)); break 'outer; } } } assert!(found.is_some()); }
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
                } else if value == 2 {
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
fn dropdown_expands_and_option_hit_sets_text_event() { let mut shell = UiShell::new(); shell.register_panel(Box::new(TestDropdownPanel)); shell.update(&Document::default()); let collapsed = shell.render_panel_surface(280, 200); let (root_x, root_y) = (0..collapsed.height).find_map(|y| { (0..collapsed.width).find_map(|x| match collapsed.hit_test(x, y) { Some(PanelEvent::Activate { panel_id, node_id }) if panel_id == "test.dropdown" && node_id == "blend.mode" => Some((x, y)), _ => None, }) }).expect("dropdown root exists"); assert!(shell.handle_panel_event(&PanelEvent::Activate { panel_id: "test.dropdown".to_string(), node_id: "blend.mode".to_string(), }).is_empty()); let expanded = shell.render_panel_surface(280, 240); let option_event = (0..expanded.height).find_map(|y| { (0..expanded.width).find_map(|x| match expanded.hit_test(x, y) { Some(PanelEvent::SetText { panel_id, node_id, value }) if panel_id == "test.dropdown" && node_id == "blend.mode" && value == "multiply" => Some((x, y)), _ => None, }) }).expect("dropdown option exists"); assert!(root_x < expanded.width && root_y < expanded.height); assert!(option_event.0 < expanded.width && option_event.1 < expanded.height); }
#[test]
fn focus_navigation_can_activate_focused_button() { let mut shell = shell_with_builtin_panels(); assert!(shell.focus_next()); assert_eq!(shell.focused_target(), Some(("builtin.workspace-layout", "workspace.move-up.builtin.app-actions"))); assert!(shell.focus_panel_node("builtin.app-actions", "app.save")); assert_eq!(shell.activate_focused(), vec![HostAction::DispatchCommand(Command::SaveProject)]); }
#[test]
fn app_actions_panel_exposes_inline_new_document_inputs() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let app_panel = shell.panel_trees().into_iter().find(|panel| panel.id == "builtin.input-test").expect("app panel exists"); assert!(tree_contains_text_input(&app_panel.children, "input.width", "64")); }
#[test]
fn focused_text_input_updates_bound_state() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); assert!(shell.focus_panel_node("builtin.input-test", "input.width")); assert!(shell.backspace_focused_input()); assert!(shell.backspace_focused_input()); assert!(shell.insert_text_into_focused_input("320")); let app_panel = shell.panel_trees().into_iter().find(|panel| panel.id == "builtin.input-test").expect("app panel exists"); assert!(tree_contains_text_input(&app_panel.children, "input.width", "320")); }
#[test]
fn focused_text_input_supports_cursor_movement_and_space() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("input.altp-panel"), SAMPLE_TEXT_INPUT_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); assert!(shell.focus_panel_node("builtin.text-input-test", "input.text")); assert!(shell.insert_text_into_focused_input(" c")); assert!(shell.move_focused_input_cursor(-1)); assert!(shell.backspace_focused_input()); assert!(shell.insert_text_into_focused_input("d")); let app_panel = shell.panel_trees().into_iter().find(|panel| panel.id == "builtin.text-input-test").expect("app panel exists"); assert!(tree_contains_text_input(&app_panel.children, "input.text", "abdc")); }
#[test]
fn focused_text_input_tracks_preedit_text() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("input.altp-panel"), SAMPLE_INPUT_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("input_test.wasm"), SAMPLE_INPUT_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); assert!(shell.focus_panel_node("builtin.input-test", "input.width")); assert!(shell.set_focused_input_preedit(Some("12".to_string()))); assert!(shell.set_focused_input_preedit(None)); }
#[test]
fn workspace_manager_panel_can_emit_reorder_action() { let mut shell = shell_with_builtin_panels(); let actions = shell.handle_panel_event(&PanelEvent::Activate { panel_id: WORKSPACE_PANEL_ID.to_string(), node_id: "workspace.move-down.builtin.app-actions".to_string(), }); assert_eq!(actions, vec![HostAction::MovePanel { panel_id: "builtin.app-actions".to_string(), direction: PanelMoveDirection::Down, }]); }
#[test]
fn workspace_layout_hides_panel_from_rendered_tree() { let mut shell = shell_with_builtin_panels(); assert!(shell.set_panel_visibility("builtin.tool-palette", false)); assert!(shell.panel_trees().iter().all(|panel| panel.id != "builtin.tool-palette")); }
#[test]
fn workspace_layout_reorders_visible_panels() { let mut shell = shell_with_builtin_panels(); let before_ids = shell.panel_trees().iter().map(|panel| panel.id).collect::<Vec<_>>(); let before_index = before_ids.iter().position(|panel_id| *panel_id == "builtin.layers-panel").expect("layers panel visible"); assert!(shell.move_panel("builtin.layers-panel", PanelMoveDirection::Up)); assert!(shell.move_panel("builtin.layers-panel", PanelMoveDirection::Up)); let visible_ids = shell.panel_trees().iter().map(|panel| panel.id).collect::<Vec<_>>(); let layers_index = visible_ids.iter().position(|panel_id| *panel_id == "builtin.layers-panel").expect("layers panel visible"); assert!(layers_index < before_index); }
#[test]
fn scrolling_panels_updates_scroll_offset() { let mut shell = shell_with_builtin_panels(); let _ = shell.render_panel_surface(280, 96); assert!(shell.scroll_panels(6, 96)); assert!(shell.panel_scroll_offset() > 0); }
#[test]
fn scrolling_panels_keeps_cached_panel_content() { let mut shell = shell_with_builtin_panels(); let _ = shell.render_panel_surface(280, 96); assert!(!shell.panel_content_dirty); assert!(shell.scroll_panels(6, 96)); assert!(!shell.panel_content_dirty); }
#[test]
fn focus_change_invalidates_cached_panel_content() { let mut shell = shell_with_builtin_panels(); let _ = shell.render_panel_surface(280, 96); assert!(!shell.panel_content_dirty); assert!(shell.focus_next()); assert!(shell.panel_content_dirty); }
#[test]
fn loading_panel_directory_registers_dsl_panel() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); let diagnostics = shell.load_panel_directory(&temp_dir); assert!(diagnostics.is_empty()); let panels = shell.panel_trees(); let dsl_panel = panels.iter().find(|panel| panel.id == "builtin.dsl-test").expect("dsl panel exists"); assert!(matches!(&dsl_panel.children[0], PanelNode::Column { children, .. } if matches!(&children[0], PanelNode::Section { title, .. } if title == "Runtime"))); assert_eq!(shell.handle_panel_event(&PanelEvent::Activate { panel_id: "builtin.dsl-test".to_string(), node_id: "dsl.save".to_string(), }), vec![HostAction::DispatchCommand(Command::SaveProject)]); }
#[test]
fn runtime_backed_dsl_panel_applies_state_patch_and_host_snapshot() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let before = shell.panel_trees().into_iter().find(|panel| panel.id == "builtin.dsl-test").expect("dsl panel exists"); assert!(!tree_contains_text(&before.children, "Untitled")); let _ = shell.handle_panel_event(&PanelEvent::Activate { panel_id: "builtin.dsl-test".to_string(), node_id: "dsl.expanded".to_string(), }); let after = shell.panel_trees().into_iter().find(|panel| panel.id == "builtin.dsl-test").expect("dsl panel exists"); assert!(tree_contains_text(&after.children, "Untitled")); }
#[test]
fn runtime_backed_dsl_panel_converts_command_descriptor_to_command() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written"); fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let actions = shell.handle_panel_event(&PanelEvent::Activate { panel_id: "builtin.dsl-test".to_string(), node_id: "dsl.brush".to_string(), }); assert_eq!(actions, vec![HostAction::DispatchCommand(Command::SetActiveTool { tool: ToolKind::Brush })]); }
#[test]
fn reloading_panel_directory_replaces_previous_dsl_panel() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written"); fs::write(temp_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("first dsl panel written"); let mut shell = UiShell::new(); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let updated_panel = SAMPLE_DSL_PANEL.replace("Phase 6 Test", "DSL Reloaded"); fs::write(temp_dir.join("sample.altp-panel"), updated_panel).expect("updated dsl panel written"); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let panels = shell.panel_trees(); let matching = panels.iter().filter(|panel| panel.id == "builtin.dsl-test").collect::<Vec<_>>(); assert_eq!(matching.len(), 1); assert_eq!(matching[0].title, "DSL Reloaded"); }
#[test]
fn loading_dsl_panel_replaces_builtin_panel_with_same_id() { let temp_dir = unique_test_dir(); fs::create_dir_all(&temp_dir).expect("temp dir created"); fs::write(temp_dir.join("builtin-app-actions.altp-panel"), BUILTIN_APP_ACTIONS_PANEL).expect("app actions panel written"); fs::write(temp_dir.join("builtin-app-actions.wasm"), BUILTIN_APP_ACTIONS_WAT).expect("app actions wasm written"); let mut shell = shell_with_builtin_panels(); assert!(shell.load_panel_directory(&temp_dir).is_empty()); let panels = shell.panel_trees(); let matching = panels.iter().filter(|panel| panel.id == "builtin.app-actions").collect::<Vec<_>>(); assert_eq!(matching.len(), 1); assert!(tree_contains_text(&matching[0].children, "Hosted via DSL + Wasm")); assert_eq!(shell.handle_panel_event(&PanelEvent::Activate { panel_id: "builtin.app-actions".to_string(), node_id: "app.save".to_string(), }), vec![HostAction::DispatchCommand(Command::SaveProject)]); }
#[test]
fn migrated_builtin_dsl_panels_use_host_snapshot_data() { let mut shell = UiShell::new(); let mut document = Document::default(); document.set_active_tool(ToolKind::Eraser); assert!(shell.load_panel_directory(default_builtin_panel_dir()).is_empty()); shell.update(&document); let panels = shell.panel_trees(); let tool_panel = panels.iter().find(|panel| panel.id == "builtin.tool-palette").expect("tool panel exists"); let layers_panel = panels.iter().find(|panel| panel.id == "builtin.layers-panel").expect("layers panel exists"); assert!(tree_contains_button_label(&tool_panel.children, "Eraser", true)); assert!(tree_contains_text(&layers_panel.children, "Untitled")); assert!(tree_contains_text(&layers_panel.children, "pages: 1 / panels: 1 / layers: 1")); assert!(tree_contains_text(&layers_panel.children, "Layer 1")); }
#[test]
fn migrated_builtin_dsl_panels_render_interpolated_mixed_text() { let mut shell = UiShell::new(); assert!(shell.load_panel_directory(default_builtin_panel_dir()).is_empty()); shell.update(&Document::default()); let panels = shell.panel_trees(); let tool_panel = panels.iter().find(|panel| panel.id == "builtin.tool-palette").expect("tool panel exists"); let layers_panel = panels.iter().find(|panel| panel.id == "builtin.layers-panel").expect("layers panel exists"); let pen_panel = panels.iter().find(|panel| panel.id == "builtin.pen-settings").expect("pen panel exists"); assert!(tree_contains_text(&tool_panel.children, "Preset: Round Pen")); assert!(tree_contains_text(&tool_panel.children, "Size: 4px / presets: 1")); assert!(tree_contains_text(&layers_panel.children, "index: 0")); assert!(tree_contains_text(&layers_panel.children, "blend: normal / visible / mask: false")); assert!(tree_contains_text(&layers_panel.children, "visible: true")); assert!(tree_contains_text(&layers_panel.children, "mask: false")); assert!(tree_contains_text(&pen_panel.children, "4px")); }
#[test]
fn command_descriptor_accepts_numeric_payload_encoded_as_string() { let mut descriptor = CommandDescriptor::new("tool.set_size"); descriptor.payload.insert("size".to_string(), Value::String("12".to_string())); assert_eq!(command_from_descriptor(&descriptor), Ok(Command::SetActivePenSize { size: 12 })); }
#[test]
fn command_descriptor_maps_layer_rename_active() { let mut descriptor = CommandDescriptor::new("layer.rename_active"); descriptor.payload.insert("name".to_string(), Value::String("Ink".to_string())); assert_eq!(command_from_descriptor(&descriptor), Ok(Command::RenameActiveLayer { name: "Ink".to_string() })); }
#[test]
fn load_panel_directory_discovers_nested_panel_files() { let temp_dir = unique_test_dir(); let nested_dir = temp_dir.join("nested").join("plugin"); fs::create_dir_all(&nested_dir).expect("nested temp dir created"); fs::write(nested_dir.join("sample.altp-panel"), SAMPLE_DSL_PANEL).expect("dsl panel written"); fs::write(nested_dir.join("sample_test.wasm"), SAMPLE_DSL_WAT).expect("wasm sample written"); let mut shell = UiShell::new(); shell.update(&Document::default()); assert!(shell.load_panel_directory(&temp_dir).is_empty()); assert!(shell.panel_trees().iter().any(|panel| panel.id == "builtin.dsl-test")); }

fn tree_contains_text(nodes: &[PanelNode], target: &str) -> bool { nodes.iter().any(|node| match node { PanelNode::Text { text, .. } => text == target, PanelNode::Column { children, .. } | PanelNode::Row { children, .. } | PanelNode::Section { children, .. } => tree_contains_text(children, target), PanelNode::Dropdown { label, value, options, .. } => label == target || value == target || options.iter().any(|option| option.label == target || option.value == target), PanelNode::LayerList { label, items, .. } => label == target || items.iter().any(|item| item.label == target || item.detail == target), PanelNode::ColorPreview { .. } | PanelNode::Button { .. } | PanelNode::Slider { .. } | PanelNode::TextInput { .. } => false, }) }
fn tree_contains_text_input(nodes: &[PanelNode], target_id: &str, target_value: &str) -> bool { nodes.iter().any(|node| match node { PanelNode::TextInput { id, value, .. } => id == target_id && value == target_value, PanelNode::Column { children, .. } | PanelNode::Row { children, .. } | PanelNode::Section { children, .. } => tree_contains_text_input(children, target_id, target_value), PanelNode::ColorPreview { .. } | PanelNode::Button { .. } | PanelNode::Slider { .. } | PanelNode::Text { .. } | PanelNode::Dropdown { .. } | PanelNode::LayerList { .. } => false, }) }
fn tree_contains_button_label(nodes: &[PanelNode], target: &str, active: bool) -> bool { nodes.iter().any(|node| match node { PanelNode::Button { label, active: is_active, .. } => label == target && *is_active == active, PanelNode::Column { children, .. } | PanelNode::Row { children, .. } | PanelNode::Section { children, .. } => tree_contains_button_label(children, target, active), PanelNode::ColorPreview { .. } | PanelNode::Slider { .. } | PanelNode::Text { .. } | PanelNode::TextInput { .. } | PanelNode::Dropdown { .. } | PanelNode::LayerList { .. } => false, }) }
#[test]
fn text_renderer_draws_visible_pixels() { let mut pixels = vec![0; 160 * 40 * 4]; draw_text_rgba(&mut pixels, 160, 40, 4, 4, "Aa", [0xff, 0xff, 0xff, 0xff]); assert!(pixels.chunks_exact(4).any(|pixel| pixel != [0, 0, 0, 0])); if text_backend_name() == "system" { assert!(pixels.chunks_exact(4).any(|pixel| pixel[0] != 0 && pixel[0] != 0xff && pixel[0] == pixel[1] && pixel[1] == pixel[2])); } }
#[test]
fn wrap_text_lines_preserves_long_words() { let lines = wrap_text_lines("antidisestablishmentarianism", 24); assert!(lines.len() > 1); assert_eq!(lines.concat(), "antidisestablishmentarianism"); }
fn unique_test_dir() -> std::path::PathBuf { let suffix = SystemTime::now().duration_since(UNIX_EPOCH).expect("system time available").as_nanos(); std::env::temp_dir().join(format!("altpaint-ui-shell-{suffix}")) }
fn default_builtin_panel_dir() -> std::path::PathBuf { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("plugins") }
fn shell_with_builtin_panels() -> UiShell { let mut shell = UiShell::new(); let diagnostics = shell.load_panel_directory(default_builtin_panel_dir()); assert!(diagnostics.is_empty(), "expected builtin panels to load: {diagnostics:?}"); shell.update(&Document::default()); shell }
