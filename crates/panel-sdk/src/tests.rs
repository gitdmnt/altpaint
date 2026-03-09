//! `panel-sdk` 公開 API の回帰テストを保持する。

use serde_json::json;

use crate::{command, commands, host, state};
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
        .string("tool", "brush")
        .bool("pinned", true)
        .value("weight", json!(1))
        .build();

    assert_eq!(descriptor.name, "tool.set_active");
    assert_eq!(descriptor.payload.get("tool"), Some(&json!("brush")));
    assert_eq!(descriptor.payload.get("pinned"), Some(&json!(true)));
    assert_eq!(descriptor.payload.get("weight"), Some(&json!(1)));
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
fn typed_tool_commands_hide_payload_keys() {
    let tool = commands::tool::set_active(commands::Tool::Eraser);
    let color = commands::tool::set_color_rgb(commands::RgbColor::new(0x0c, 0x22, 0x38));

    assert_eq!(tool.payload.get("tool"), Some(&json!("eraser")));
    assert_eq!(color.payload.get("color"), Some(&json!("#0C2238")));
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
    assert!(!host::tool::is_active(commands::Tool::Brush));
    assert_eq!(host::tool::pen_name(), "");
    assert_eq!(host::color::active_hex(), "");
    assert_eq!(host::jobs::status(), "");
    assert_eq!(host::snapshot::storage_status(), "");
}

#[test]
fn macro_annotated_functions_remain_directly_callable() {
    init_for_macro_test();
    save_for_macro_test();
    sync_host_for_macro_test();
    slider_for_macro_test(42);
}
