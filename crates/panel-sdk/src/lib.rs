pub use panel_macros::{panel_handler, panel_init, panel_sync_host};
pub use panel_schema::{
    CommandDescriptor, Diagnostic, DiagnosticLevel, HandlerResult, PanelEventRequest,
    PanelInitRequest, PanelInitResponse, StatePatch, StatePatchOp,
};

use serde_json::Value;

pub fn command(name: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        descriptor: CommandDescriptor::new(name),
    }
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    descriptor: CommandDescriptor,
}

impl CommandBuilder {
    pub fn string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::String(value.into()));
        self
    }

    pub fn bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::Bool(value));
        self
    }

    pub fn color(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.string(key, value)
    }

    pub fn value(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.descriptor.payload.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> CommandDescriptor {
        self.descriptor
    }
}

pub fn handler_result() -> HandlerResult {
    HandlerResult::default()
}

pub mod commands {
    use super::CommandDescriptor;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Tool {
        Brush,
        Pen,
        Eraser,
    }

    impl Tool {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Brush => "brush",
                Self::Pen => "pen",
                Self::Eraser => "eraser",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RgbColor {
        pub red: u8,
        pub green: u8,
        pub blue: u8,
    }

    impl RgbColor {
        pub const fn new(red: u8, green: u8, blue: u8) -> Self {
            Self { red, green, blue }
        }

        pub fn to_hex_string(self) -> String {
            format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
        }
    }

    pub mod project {
        use super::CommandDescriptor;
        use serde_json::json;

        pub fn new_document() -> CommandDescriptor {
            CommandDescriptor::new("project.new")
        }

        pub fn new_sized(width: usize, height: usize) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("project.new_sized");
            descriptor
                .payload
                .insert("size".to_string(), json!(format!("{width}x{height}")));
            descriptor
        }

        pub fn save() -> CommandDescriptor {
            CommandDescriptor::new("project.save")
        }

        pub fn save_as() -> CommandDescriptor {
            CommandDescriptor::new("project.save_as")
        }

        pub fn save_as_path(path: impl Into<String>) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("project.save_as_path");
            descriptor
                .payload
                .insert("path".to_string(), json!(path.into()));
            descriptor
        }

        pub fn load() -> CommandDescriptor {
            CommandDescriptor::new("project.load")
        }

        pub fn load_path(path: impl Into<String>) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("project.load_path");
            descriptor
                .payload
                .insert("path".to_string(), json!(path.into()));
            descriptor
        }
    }

    pub mod tool {
        use super::{CommandDescriptor, RgbColor, Tool};
        use serde_json::json;

        pub fn set_active(tool: Tool) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("tool.set_active");
            descriptor
                .payload
                .insert("tool".to_string(), json!(tool.as_str()));
            descriptor
        }

        pub fn set_color_hex(color: impl Into<String>) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("tool.set_color");
            descriptor
                .payload
                .insert("color".to_string(), json!(color.into()));
            descriptor
        }

        pub fn set_color_rgb(color: RgbColor) -> CommandDescriptor {
            set_color_hex(color.to_hex_string())
        }

        pub fn set_size(size: u32) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("tool.set_size");
            descriptor.payload.insert("size".to_string(), json!(size));
            descriptor
        }

        pub fn select_next_pen() -> CommandDescriptor {
            CommandDescriptor::new("tool.pen_next")
        }

        pub fn select_previous_pen() -> CommandDescriptor {
            CommandDescriptor::new("tool.pen_prev")
        }

        pub fn reload_pen_presets() -> CommandDescriptor {
            CommandDescriptor::new("tool.reload_pen_presets")
        }
    }

    pub mod view {
        use super::CommandDescriptor;
        use serde_json::json;

        pub fn zoom(zoom: f32) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("view.zoom");
            descriptor.payload.insert("zoom".to_string(), json!(zoom));
            descriptor
        }

        pub fn pan(delta_x: f32, delta_y: f32) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("view.pan");
            descriptor
                .payload
                .insert("delta_x".to_string(), json!(delta_x));
            descriptor
                .payload
                .insert("delta_y".to_string(), json!(delta_y));
            descriptor
        }

        pub fn reset() -> CommandDescriptor {
            CommandDescriptor::new("view.reset")
        }
    }

    pub mod layer {
        use super::CommandDescriptor;
        use serde_json::json;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum BlendMode {
            Normal,
            Multiply,
            Screen,
            Add,
        }

        impl BlendMode {
            pub fn as_str(self) -> &'static str {
                match self {
                    Self::Normal => "normal",
                    Self::Multiply => "multiply",
                    Self::Screen => "screen",
                    Self::Add => "add",
                }
            }
        }

        pub fn add() -> CommandDescriptor {
            CommandDescriptor::new("layer.add")
        }

        pub fn remove() -> CommandDescriptor {
            CommandDescriptor::new("layer.remove")
        }

        pub fn select(index: usize) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("layer.select");
            descriptor.payload.insert("index".to_string(), json!(index));
            descriptor
        }

        pub fn rename_active(name: impl Into<String>) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("layer.rename_active");
            descriptor
                .payload
                .insert("name".to_string(), json!(name.into()));
            descriptor
        }

        pub fn move_to(from_index: usize, to_index: usize) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("layer.move");
            descriptor
                .payload
                .insert("from_index".to_string(), json!(from_index));
            descriptor
                .payload
                .insert("to_index".to_string(), json!(to_index));
            descriptor
        }

        pub fn select_next() -> CommandDescriptor {
            CommandDescriptor::new("layer.select_next")
        }

        pub fn cycle_blend_mode() -> CommandDescriptor {
            CommandDescriptor::new("layer.cycle_blend_mode")
        }

        pub fn set_blend_mode(mode: impl Into<String>) -> CommandDescriptor {
            let mut descriptor = CommandDescriptor::new("layer.set_blend_mode");
            descriptor
                .payload
                .insert("mode".to_string(), json!(mode.into()));
            descriptor
        }

        pub fn set_blend_mode_enum(mode: BlendMode) -> CommandDescriptor {
            set_blend_mode(mode.as_str())
        }

        pub fn toggle_visibility() -> CommandDescriptor {
            CommandDescriptor::new("layer.toggle_visibility")
        }

        pub fn toggle_mask() -> CommandDescriptor {
            CommandDescriptor::new("layer.toggle_mask")
        }
    }
}

pub mod state {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BoolKey(&'static str);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IntKey(&'static str);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StringKey(&'static str);

    impl BoolKey {
        pub const fn new(path: &'static str) -> Self {
            Self(path)
        }
    }

    impl IntKey {
        pub const fn new(path: &'static str) -> Self {
            Self(path)
        }
    }

    impl StringKey {
        pub const fn new(path: &'static str) -> Self {
            Self(path)
        }
    }

    impl AsRef<str> for BoolKey {
        fn as_ref(&self) -> &str {
            self.0
        }
    }

    impl AsRef<str> for IntKey {
        fn as_ref(&self) -> &str {
            self.0
        }
    }

    impl AsRef<str> for StringKey {
        fn as_ref(&self) -> &str {
            self.0
        }
    }

    pub const fn bool(path: &'static str) -> BoolKey {
        BoolKey::new(path)
    }

    pub const fn int(path: &'static str) -> IntKey {
        IntKey::new(path)
    }

    pub const fn string(path: &'static str) -> StringKey {
        StringKey::new(path)
    }
}

pub mod runtime {
    use crate::CommandDescriptor;
    #[cfg(target_arch = "wasm32")]
    use serde_json::Value;

    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "host")]
    unsafe extern "C" {
        fn state_toggle(ptr: i32, len: i32);
        fn state_set_bool(ptr: i32, len: i32, value: i32);
        fn state_set_i32(ptr: i32, len: i32, value: i32);
        fn state_set_string(path_ptr: i32, path_len: i32, value_ptr: i32, value_len: i32);
        fn state_get_bool(ptr: i32, len: i32) -> i32;
        fn state_get_i32(ptr: i32, len: i32) -> i32;
        fn state_get_string_len(ptr: i32, len: i32) -> i32;
        fn state_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
        fn event_get_string_len(ptr: i32, len: i32) -> i32;
        fn event_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
        fn host_get_bool(ptr: i32, len: i32) -> i32;
        fn host_get_i32(ptr: i32, len: i32) -> i32;
        fn host_get_string_len(ptr: i32, len: i32) -> i32;
        fn host_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
        fn command(ptr: i32, len: i32);
        fn command_string(
            name_ptr: i32,
            name_len: i32,
            key_ptr: i32,
            key_len: i32,
            value_ptr: i32,
            value_len: i32,
        );
        fn diagnostic(level: i32, ptr: i32, len: i32);
    }

    #[cfg(target_arch = "wasm32")]
    fn with_bytes<T>(value: &str, f: impl FnOnce(i32, i32) -> T) -> T {
        f(value.as_ptr() as i32, value.len() as i32)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn toggle_state(path: impl AsRef<str>) {
        with_bytes(path.as_ref(), |ptr, len| unsafe { state_toggle(ptr, len) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn toggle_state(_path: impl AsRef<str>) {}

    #[cfg(target_arch = "wasm32")]
    pub fn set_state_bool(path: impl AsRef<str>, value: bool) {
        with_bytes(path.as_ref(), |ptr, len| unsafe {
            state_set_bool(ptr, len, i32::from(value))
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_state_bool(_path: impl AsRef<str>, _value: bool) {}

    #[cfg(target_arch = "wasm32")]
    pub fn set_state_i32(path: impl AsRef<str>, value: i32) {
        with_bytes(path.as_ref(), |ptr, len| unsafe { state_set_i32(ptr, len, value) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_state_i32(_path: impl AsRef<str>, _value: i32) {}

    #[cfg(target_arch = "wasm32")]
    pub fn set_state_string(path: impl AsRef<str>, value: impl AsRef<str>) {
        with_bytes(path.as_ref(), |path_ptr, path_len| {
            with_bytes(value.as_ref(), |value_ptr, value_len| unsafe {
                state_set_string(path_ptr, path_len, value_ptr, value_len)
            })
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_state_string(_path: impl AsRef<str>, _value: impl AsRef<str>) {}

    #[cfg(target_arch = "wasm32")]
    pub fn state_bool(path: impl AsRef<str>) -> bool {
        with_bytes(path.as_ref(), |ptr, len| unsafe { state_get_bool(ptr, len) != 0 })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn state_bool(_path: impl AsRef<str>) -> bool {
        false
    }

    #[cfg(target_arch = "wasm32")]
    pub fn state_i32(path: impl AsRef<str>) -> i32 {
        with_bytes(path.as_ref(), |ptr, len| unsafe { state_get_i32(ptr, len) })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn state_i32(_path: impl AsRef<str>) -> i32 {
        0
    }

    #[cfg(target_arch = "wasm32")]
    pub fn state_string(path: impl AsRef<str>) -> String {
        let path = path.as_ref();
        let length = with_bytes(path, |ptr, len| unsafe { state_get_string_len(ptr, len) });
        if length <= 0 {
            return String::new();
        }

        let mut buffer = vec![0u8; length as usize];
        with_bytes(path, |path_ptr, path_len| unsafe {
            state_get_string_copy(
                path_ptr,
                path_len,
                buffer.as_mut_ptr() as i32,
                buffer.len() as i32,
            )
        });
        String::from_utf8(buffer).unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn state_string(_path: impl AsRef<str>) -> String {
        String::new()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn event_string(path: impl AsRef<str>) -> String {
        let path = path.as_ref();
        let length = with_bytes(path, |ptr, len| unsafe { event_get_string_len(ptr, len) });
        if length <= 0 {
            return String::new();
        }

        let mut buffer = vec![0u8; length as usize];
        with_bytes(path, |path_ptr, path_len| unsafe {
            event_get_string_copy(
                path_ptr,
                path_len,
                buffer.as_mut_ptr() as i32,
                buffer.len() as i32,
            )
        });
        String::from_utf8(buffer).unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn event_string(_path: impl AsRef<str>) -> String {
        String::new()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn host_bool(path: impl AsRef<str>) -> bool {
        with_bytes(path.as_ref(), |ptr, len| unsafe { host_get_bool(ptr, len) != 0 })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn host_bool(_path: impl AsRef<str>) -> bool {
        false
    }

    #[cfg(target_arch = "wasm32")]
    pub fn host_i32(path: impl AsRef<str>) -> i32 {
        with_bytes(path.as_ref(), |ptr, len| unsafe { host_get_i32(ptr, len) })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn host_i32(_path: impl AsRef<str>) -> i32 {
        0
    }

    #[cfg(target_arch = "wasm32")]
    pub fn host_string(path: impl AsRef<str>) -> String {
        let path = path.as_ref();
        let length = with_bytes(path, |ptr, len| unsafe { host_get_string_len(ptr, len) });
        if length <= 0 {
            return String::new();
        }

        let mut buffer = vec![0u8; length as usize];
        with_bytes(path, |path_ptr, path_len| unsafe {
            host_get_string_copy(
                path_ptr,
                path_len,
                buffer.as_mut_ptr() as i32,
                buffer.len() as i32,
            )
        });
        String::from_utf8(buffer).unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn host_string(_path: impl AsRef<str>) -> String {
        String::new()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn emit_command_descriptor(descriptor: &CommandDescriptor) {
        match descriptor.payload.len() {
            0 => with_bytes(&descriptor.name, |ptr, len| unsafe { command(ptr, len) }),
            1 => {
                let (key, value) = descriptor.payload.iter().next().expect("payload exists");
                let value = match value {
                    Value::String(value) => value.clone(),
                    Value::Bool(value) => value.to_string(),
                    Value::Number(value) => value.to_string(),
                    _ => {
                        error("unsupported command payload type in panel-sdk runtime");
                        return;
                    }
                };
                with_bytes(&descriptor.name, |name_ptr, name_len| {
                    with_bytes(key, |key_ptr, key_len| {
                        with_bytes(&value, |value_ptr, value_len| unsafe {
                            command_string(
                                name_ptr, name_len, key_ptr, key_len, value_ptr, value_len,
                            )
                        })
                    })
                });
            }
            _ => error("unsupported multi-field command payload in panel-sdk runtime"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn emit_command_descriptor(_descriptor: &CommandDescriptor) {}

    pub fn emit_command(descriptor: &CommandDescriptor) {
        emit_command_descriptor(descriptor);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn info(message: &str) {
        with_bytes(message, |ptr, len| unsafe { diagnostic(0, ptr, len) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn info(_message: &str) {}

    #[cfg(target_arch = "wasm32")]
    pub fn warn(message: &str) {
        with_bytes(message, |ptr, len| unsafe { diagnostic(1, ptr, len) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn warn(_message: &str) {}

    #[cfg(target_arch = "wasm32")]
    pub fn error(message: &str) {
        with_bytes(message, |ptr, len| unsafe { diagnostic(2, ptr, len) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn error(_message: &str) {}
}

pub mod host {
    use crate::{
        commands::Tool,
        runtime::{host_bool, host_i32, host_string},
    };

    pub mod document {
        use super::{host_bool, host_i32, host_string};

        pub fn title() -> String {
            host_string("document.title")
        }

        pub fn page_count() -> i32 {
            host_i32("document.page_count")
        }

        pub fn panel_count() -> i32 {
            host_i32("document.panel_count")
        }

        pub fn layer_count() -> i32 {
            host_i32("document.layer_count")
        }

        pub fn active_layer_name() -> String {
            host_string("document.active_layer_name")
        }

        pub fn active_layer_index() -> i32 {
            host_i32("document.active_layer_index")
        }

        pub fn active_layer_blend_mode() -> String {
            host_string("document.active_layer_blend_mode")
        }

        pub fn active_layer_visible() -> bool {
            host_bool("document.active_layer_visible")
        }

        pub fn active_layer_masked() -> bool {
            host_bool("document.active_layer_masked")
        }

        pub fn layers_json() -> String {
            host_string("document.layers_json")
        }
    }

    pub mod tool {
        use super::{host_i32, host_string, Tool};

        pub fn active_name() -> String {
            host_string("tool.active")
        }

        pub fn is_active(tool: Tool) -> bool {
            active_name().eq_ignore_ascii_case(tool.as_str())
        }

        pub fn pen_name() -> String {
            host_string("tool.pen_name")
        }

        pub fn pen_id() -> String {
            host_string("tool.pen_id")
        }

        pub fn pen_index() -> i32 {
            host_i32("tool.pen_index")
        }

        pub fn pen_count() -> i32 {
            host_i32("tool.pen_count")
        }

        pub fn pen_size() -> i32 {
            host_i32("tool.pen_size")
        }
    }

    pub mod color {
        use super::{host_i32, host_string};

        pub fn active_hex() -> String {
            host_string("color.active")
        }

        pub fn red() -> i32 {
            host_i32("color.red")
        }

        pub fn green() -> i32 {
            host_i32("color.green")
        }

        pub fn blue() -> i32 {
            host_i32("color.blue")
        }
    }

    pub mod jobs {
        use super::{host_i32, host_string};

        pub fn active() -> i32 {
            host_i32("jobs.active")
        }

        pub fn queued() -> i32 {
            host_i32("jobs.queued")
        }

        pub fn status() -> String {
            host_string("jobs.status")
        }
    }

    pub mod snapshot {
        use super::host_string;

        pub fn storage_status() -> String {
            host_string("snapshot.storage_status")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
