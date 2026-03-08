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

pub mod runtime {
    use crate::CommandDescriptor;
    #[cfg(target_arch = "wasm32")]
    use serde_json::Value;

    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "host")]
    unsafe extern "C" {
        fn state_toggle(ptr: i32, len: i32);
        fn state_set_bool(ptr: i32, len: i32, value: i32);
        fn state_get_i32(ptr: i32, len: i32) -> i32;
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
    pub fn toggle_state(path: &str) {
        with_bytes(path, |ptr, len| unsafe { state_toggle(ptr, len) });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn toggle_state(_path: &str) {}

    #[cfg(target_arch = "wasm32")]
    pub fn set_state_bool(path: &str, value: bool) {
        with_bytes(path, |ptr, len| unsafe {
            state_set_bool(ptr, len, i32::from(value))
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_state_bool(_path: &str, _value: bool) {}

    #[cfg(target_arch = "wasm32")]
    pub fn state_i32(path: &str) -> i32 {
        with_bytes(path, |ptr, len| unsafe { state_get_i32(ptr, len) })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn state_i32(_path: &str) -> i32 {
        0
    }

    #[cfg(target_arch = "wasm32")]
    pub fn emit_command_descriptor(descriptor: &CommandDescriptor) {
        match descriptor.payload.len() {
            0 => with_bytes(&descriptor.name, |ptr, len| unsafe { command(ptr, len) }),
            1 => {
                let (key, value) = descriptor.payload.iter().next().expect("payload exists");
                let Value::String(value) = value else {
                    error("unsupported command payload type in panel-sdk runtime");
                    return;
                };
                with_bytes(&descriptor.name, |name_ptr, name_len| {
                    with_bytes(key, |key_ptr, key_len| {
                        with_bytes(value, |value_ptr, value_len| unsafe {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
