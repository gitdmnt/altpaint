//! `panel-sdk` surface の回帰テストを保持する。

use serde_json::json;

use crate::{RequestDescriptor, commands, host_state, names, runtime, services, state};
use crate::{panel_handler, panel_init, panel_on_host_change};

#[panel_init]
fn init_for_macro_test() {}

#[panel_handler]
fn save_for_macro_test() {}

#[panel_on_host_change]
fn on_host_change_for_macro_test() {}

#[panel_handler]
fn slider_for_macro_test(value: i32) {
    assert_eq!(value, 42);
}

/// typed payload (serde Deserialize + Default) を受ける handler の macro 展開を検証する。
#[derive(Debug, Default, serde::Deserialize, PartialEq)]
struct MoveLayerPayload {
    from_index: i64,
    to_index: i64,
}

#[panel_handler]
fn typed_payload_for_macro_test(payload: MoveLayerPayload) {
    // native では event_payload が空 → Default。
    assert_eq!(payload, MoveLayerPayload::default());
}

#[test]
fn typed_service_requests_hide_service_names() {
    let save = services::project_io::save_current();
    let preset = services::workspace_io::save_preset("review", "Review");
    let zoom = services::view::set_zoom(1.25);
    let select_koma = services::koma_nav::select(2);

    assert_eq!(save.name, names::project_io::SAVE_CURRENT);
    assert_eq!(preset.payload.get("preset_id"), Some(&json!("review")));
    assert_eq!(zoom.payload.get("zoom"), Some(&json!(1.25)));
    assert_eq!(select_koma.payload.get("index"), Some(&json!(2)));
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
    assert_eq!(commands::Tool::KomaRect.as_str(), "koma_rect");
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
    assert_eq!(commands::tool::select_next_pen().name, names::tool::PEN_NEXT);
    assert_eq!(
        commands::tool::select_previous_pen().name,
        names::tool::PEN_PREV
    );
    assert_eq!(
        commands::tool::reload_pen_presets().name,
        names::tool::RELOAD_PEN_PRESETS
    );
}

#[test]
fn typed_layer_commands_hide_payload_keys() {
    // BL-148: move は安定 id 間で指定する。
    let move_descriptor = commands::layer::move_to(20, 10);
    let blend_descriptor = commands::layer::set_blend_mode_enum(commands::layer::BlendMode::Screen);
    let rename_descriptor = commands::layer::rename_active("Ink");

    assert_eq!(commands::layer::remove().name, names::layer::REMOVE);
    assert_eq!(move_descriptor.payload.get("from_id"), Some(&json!(20)));
    assert_eq!(move_descriptor.payload.get("to_id"), Some(&json!(10)));
    assert_eq!(blend_descriptor.payload.get("mode"), Some(&json!("screen")));
    assert_eq!(rename_descriptor.name, names::layer::RENAME_ACTIVE);
    assert_eq!(rename_descriptor.payload.get("name"), Some(&json!("Ink")));
}

#[test]
fn typed_layer_commands_cover_remaining_variants() {
    assert_eq!(commands::layer::BlendMode::Normal.as_str(), "normal");
    assert_eq!(commands::layer::BlendMode::Multiply.as_str(), "multiply");
    assert_eq!(commands::layer::BlendMode::Add.as_str(), "add");
    assert_eq!(commands::layer::add().name, names::layer::ADD);
    // BL-148: select は安定 id (`RasterLayer.id`) を運ぶ。
    assert_eq!(
        commands::layer::select(3).payload.get("id"),
        Some(&json!(3))
    );
    assert_eq!(commands::layer::select_next().name, names::layer::SELECT_NEXT);
    assert_eq!(
        commands::layer::cycle_blend_mode().name,
        names::layer::CYCLE_BLEND_MODE
    );
    assert_eq!(
        commands::layer::toggle_visibility().name,
        names::layer::TOGGLE_VISIBILITY
    );
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
    // P27: emit_request が単一発行 API (旧 emit_command/emit_service/*_descriptor は撤去済み)。
    runtime::emit_request(&services::project_io::save_current());
    runtime::emit_request(&RequestDescriptor::new("project.save"));
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
    // host state セクション JSON 取得 (BL-142) は native では空 → None。
    assert_eq!(runtime::host_section_json(host_state::section::DOCUMENT), "");
    assert!(runtime::host_section::<host_state::DocumentState>(host_state::section::DOCUMENT).is_none());
}

#[test]
fn state_patch_buffer_collects_expected_patch_sequence() {
    let mut batch = runtime::StatePatchBuffer::new();
    batch.set_bool("show", true);
    batch.set_i32("count", 7);
    batch.set_string("name", "demo");
    batch.set_json("config", json!({"mode": "advanced"}));
    batch.toggle("expanded");

    assert_eq!(
        batch.into_vec(),
        vec![
            panel_protocol::StatePatch::set("show", true),
            panel_protocol::StatePatch::set("count", 7),
            panel_protocol::StatePatch::set("name", "demo"),
            panel_protocol::StatePatch::set("config", json!({"mode": "advanced"})),
            panel_protocol::StatePatch::toggle("expanded"),
        ]
    );
}

#[test]
fn macro_annotated_functions_remain_directly_callable() {
    init_for_macro_test();
    save_for_macro_test();
    on_host_change_for_macro_test();
    slider_for_macro_test(42);
    typed_payload_for_macro_test(MoveLayerPayload::default());
}

// BL-150: `assert_entrypoints!` が no-arg / i32 / typed payload いずれの handler 呼び
// 出し式も受け取り、native スモークテストを生成できることを検証する。各パネルは
// この 1 行で従来のコピペ `entrypoints_callable_on_native` を置換する。
crate::assert_entrypoints!(generated_entrypoints_smoke => {
    init_for_macro_test(),
    save_for_macro_test(),
    on_host_change_for_macro_test(),
    slider_for_macro_test(42),
    typed_payload_for_macro_test(MoveLayerPayload::default()),
});
