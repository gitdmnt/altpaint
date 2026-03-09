//! `panel-sdk` 公開 API の回帰テストを保持する。

use serde_json::json;

use crate::{command, commands, handler_result, host, runtime, state};
use crate::{panel_handler, panel_init, panel_sync_host};

#[panel_init]
fn init_for_macro_test() {}

#[panel_handler]
fn save_for_macro_test() {}

#[panel_sync_host]
fn sync_host_for_macro_test() {}

#[panel_handler]
fn slider_for_macro_test(value: i32) {
    assert_eq!(value, 42);
}

#[test]
fn command_builder_collects_payload_fields() {
    let descriptor = command("tool.set_active")
        .string("tool", "pen")
        .bool("pinned", true)
        .value("weight", json!(1))
        .build();

    assert_eq!(descriptor.name, "tool.set_active");
    assert_eq!(descriptor.payload.get("tool"), Some(&json!("pen")));
    assert_eq!(descriptor.payload.get("pinned"), Some(&json!(true)));
    assert_eq!(descriptor.payload.get("weight"), Some(&json!(1)));
}

#[test]
fn command_builder_color_aliases_string_payload_and_handler_result_defaults() {
    let descriptor = command("tool.set_color").color("color", "#112233").build();

    assert_eq!(descriptor.name, "tool.set_color");
    assert_eq!(descriptor.payload.get("color"), Some(&json!("#112233")));
    assert_eq!(handler_result(), crate::HandlerResult::default());
}

#[test]
fn typed_project_commands_hide_command_strings() {
    let descriptor = commands::project::new_sized(320, 240);

    assert_eq!(descriptor.name, "project.new_sized");
    assert_eq!(descriptor.payload.get("size"), Some(&json!("320x240")));
    assert_eq!(commands::project::save().name, "project.save");
    assert_eq!(commands::project::load().name, "project.load");
}

#[test]
fn typed_project_commands_cover_path_variants() {
    assert_eq!(commands::project::new_document().name, "project.new");
    assert_eq!(commands::project::save_as().name, "project.save_as");
    assert_eq!(
        commands::project::save_as_path("demo.altp")
            .payload
            .get("path"),
        Some(&json!("demo.altp"))
    );
    assert_eq!(
        commands::project::load_path("demo.altp")
            .payload
            .get("path"),
        Some(&json!("demo.altp"))
    );
}

#[test]
fn typed_tool_commands_hide_payload_keys() {
    let tool = commands::tool::set_active(commands::Tool::Eraser);
    let color = commands::tool::set_color_rgb(commands::RgbColor::new(0x0c, 0x22, 0x38));

    assert_eq!(tool.payload.get("tool"), Some(&json!("eraser")));
    assert_eq!(color.payload.get("color"), Some(&json!("#0C2238")));
}

#[test]
fn typed_tool_commands_cover_remaining_variants() {
    assert_eq!(commands::Tool::Pen.as_str(), "pen");
    assert_eq!(
        commands::tool::set_color_hex("#ABCDEF")
            .payload
            .get("color"),
        Some(&json!("#ABCDEF"))
    );
    assert_eq!(
        commands::tool::set_size(24).payload.get("size"),
        Some(&json!(24))
    );
    assert_eq!(commands::tool::select_next_pen().name, "tool.pen_next");
    assert_eq!(commands::tool::select_previous_pen().name, "tool.pen_prev");
    assert_eq!(
        commands::tool::reload_pen_presets().name,
        "tool.reload_pen_presets"
    );
}

#[test]
fn typed_view_commands_hide_payload_keys() {
    let zoom = commands::view::zoom(1.5);
    let pan = commands::view::pan(4.0, -2.0);
    let set_pan = commands::view::set_pan(12.0, -6.0);
    let rotate = commands::view::rotate(-1);
    let set_rotation = commands::view::set_rotation_degrees(270.0);

    assert_eq!(zoom.name, "view.zoom");
    assert_eq!(zoom.payload.get("zoom"), Some(&json!(1.5)));
    assert_eq!(pan.payload.get("delta_x"), Some(&json!(4.0)));
    assert_eq!(pan.payload.get("delta_y"), Some(&json!(-2.0)));
    assert_eq!(set_pan.payload.get("pan_x"), Some(&json!(12.0)));
    assert_eq!(set_pan.payload.get("pan_y"), Some(&json!(-6.0)));
    assert_eq!(rotate.payload.get("quarter_turns"), Some(&json!(-1)));
    assert_eq!(
        set_rotation.payload.get("rotation_degrees"),
        Some(&json!(270.0))
    );
    assert_eq!(commands::view::flip_horizontal().name, "view.flip_horizontal");
    assert_eq!(commands::view::flip_vertical().name, "view.flip_vertical");
    assert_eq!(commands::view::reset().name, "view.reset");
}

#[test]
fn typed_layer_commands_hide_payload_keys() {
    let move_descriptor = commands::layer::move_to(2, 0);
    let blend_descriptor = commands::layer::set_blend_mode_enum(commands::layer::BlendMode::Screen);
    let rename_descriptor = commands::layer::rename_active("Ink");

    assert_eq!(commands::layer::remove().name, "layer.remove");
    assert_eq!(move_descriptor.payload.get("from_index"), Some(&json!(2)));
    assert_eq!(move_descriptor.payload.get("to_index"), Some(&json!(0)));
    assert_eq!(blend_descriptor.payload.get("mode"), Some(&json!("screen")));
    assert_eq!(rename_descriptor.name, "layer.rename_active");
    assert_eq!(rename_descriptor.payload.get("name"), Some(&json!("Ink")));
}

#[test]
fn typed_layer_commands_cover_remaining_variants() {
    assert_eq!(commands::layer::BlendMode::Normal.as_str(), "normal");
    assert_eq!(commands::layer::BlendMode::Multiply.as_str(), "multiply");
    assert_eq!(commands::layer::BlendMode::Add.as_str(), "add");
    assert_eq!(commands::layer::add().name, "layer.add");
    assert_eq!(
        commands::layer::select(3).payload.get("index"),
        Some(&json!(3))
    );
    assert_eq!(commands::layer::select_next().name, "layer.select_next");
    assert_eq!(
        commands::layer::cycle_blend_mode().name,
        "layer.cycle_blend_mode"
    );
    assert_eq!(
        commands::layer::toggle_visibility().name,
        "layer.toggle_visibility"
    );
    assert_eq!(commands::layer::toggle_mask().name, "layer.toggle_mask");
}

#[test]
fn typed_state_keys_can_be_declared_once() {
    const SHOW_NEW: state::BoolKey = state::bool("show_new");
    const RED: state::IntKey = state::int("red");
    const NAME: state::StringKey = state::string("name");

    assert_eq!(SHOW_NEW.as_ref(), "show_new");
    assert_eq!(RED.as_ref(), "red");
    assert_eq!(NAME.as_ref(), "name");
}

#[test]
fn typed_host_helpers_are_callable_on_native_targets() {
    assert_eq!(host::document::title(), "");
    assert_eq!(host::document::page_count(), 0);
    assert_eq!(host::document::panel_count(), 0);
    assert_eq!(host::document::layer_count(), 0);
    assert_eq!(host::document::active_layer_name(), "");
    assert_eq!(host::document::active_layer_index(), 0);
    assert_eq!(host::document::active_layer_blend_mode(), "");
    assert!(!host::document::active_layer_visible());
    assert!(!host::document::active_layer_masked());
    assert_eq!(host::document::layers_json(), "");
    assert!(!host::tool::is_active(commands::Tool::Pen));
    assert_eq!(host::tool::active_name(), "");
    assert_eq!(host::tool::pen_name(), "");
    assert_eq!(host::tool::pen_id(), "");
    assert_eq!(host::tool::pen_index(), 0);
    assert_eq!(host::tool::pen_count(), 0);
    assert_eq!(host::tool::pen_size(), 0);
    assert_eq!(host::color::active_hex(), "");
    assert_eq!(host::color::red(), 0);
    assert_eq!(host::color::green(), 0);
    assert_eq!(host::color::blue(), 0);
    assert_eq!(host::view::zoom_milli(), 0);
    assert_eq!(host::view::pan_x(), 0);
    assert_eq!(host::view::pan_y(), 0);
    assert_eq!(host::view::quarter_turns(), 0);
    assert!(!host::view::flipped_x());
    assert!(!host::view::flipped_y());
    assert_eq!(host::jobs::active(), 0);
    assert_eq!(host::jobs::queued(), 0);
    assert_eq!(host::jobs::status(), "");
    assert_eq!(host::snapshot::storage_status(), "");
}

#[test]
fn native_runtime_helpers_are_safe_noops() {
    let mut batch = runtime::StatePatchBuffer::new();
    batch.set_bool("flag", true);
    batch.set_i32("count", 3);
    batch.set_string("name", "demo");
    batch.set_json("config", json!({"enabled": true}));
    batch.toggle("expanded");
    batch.apply();

    runtime::toggle_state("flag");
    runtime::set_state_bool("flag", true);
    runtime::set_state_i32("count", 3);
    runtime::set_state_string("name", "demo");
    runtime::set_state_json("config", json!({"enabled": true}));
    runtime::replace_state_json("config", json!({"enabled": false}));
    runtime::emit_command(&command("project.save").build());
    runtime::emit_command_descriptor(&command("project.load").build());
    runtime::info("info");
    runtime::warn("warn");
    runtime::error("error");

    assert!(!runtime::state_bool("flag"));
    assert_eq!(runtime::state_i32("count"), 0);
    assert_eq!(runtime::state_string("name"), "");
    assert_eq!(runtime::event_string("value"), "");
    assert!(!runtime::host_bool("host.bool"));
    assert_eq!(runtime::host_i32("host.int"), 0);
    assert_eq!(runtime::host_string("host.string"), "");
}

#[test]
fn state_patch_buffer_collects_expected_patch_sequence() {
    let mut batch = runtime::StatePatchBuffer::new();
    batch.set_bool("show", true);
    batch.set_i32("count", 7);
    batch.set_string("name", "demo");
    batch.replace_json("config", json!({"mode": "advanced"}));
    batch.toggle("expanded");

    assert_eq!(
        batch.into_vec(),
        vec![
            panel_schema::StatePatch::set("show", true),
            panel_schema::StatePatch::set("count", 7),
            panel_schema::StatePatch::set("name", "demo"),
            panel_schema::StatePatch::replace("config", json!({"mode": "advanced"})),
            panel_schema::StatePatch::toggle("expanded"),
        ]
    );
}

#[test]
fn macro_annotated_functions_remain_directly_callable() {
    init_for_macro_test();
    save_for_macro_test();
    sync_host_for_macro_test();
    slider_for_macro_test(42);
}
