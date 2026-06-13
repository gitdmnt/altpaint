//! Wasm パネルから host ABI を呼び出すランタイム関数群を提供する。

use panel_protocol::RequestDescriptor;
use panel_protocol::StatePatch;
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    fn state_toggle(ptr: i32, len: i32);
    fn state_set_bool(ptr: i32, len: i32, value: i32);
    fn state_set_i32(ptr: i32, len: i32, value: i32);
    fn state_set_string(path_ptr: i32, path_len: i32, value_ptr: i32, value_len: i32);
    fn state_apply_json(ptr: i32, len: i32);
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
    fn command_json(name_ptr: i32, name_len: i32, json_ptr: i32, json_len: i32);
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
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        state_set_i32(ptr, len, value)
    });
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
pub fn apply_state_patches(patches: &[StatePatch]) {
    let Ok(serialized) = serde_json::to_string(patches) else {
        error("failed to serialize state patch batch in panel-sdk runtime");
        return;
    };
    with_bytes(&serialized, |ptr, len| unsafe {
        state_apply_json(ptr, len)
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_state_patches(_patches: &[StatePatch]) {}

/// まとめて適用する state patch バッファ。
#[derive(Debug, Default, Clone)]
pub struct StatePatchBuffer {
    patches: Vec<StatePatch>,
}

impl StatePatchBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn push(&mut self, patch: StatePatch) {
        self.patches.push(patch);
    }

    pub fn set_bool(&mut self, path: impl Into<String>, value: bool) {
        self.push(StatePatch::set(path.into(), value));
    }

    pub fn set_i32(&mut self, path: impl Into<String>, value: i32) {
        self.push(StatePatch::set(path.into(), value));
    }

    pub fn set_string(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    pub fn set_json(&mut self, path: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    pub fn toggle(&mut self, path: impl Into<String>) {
        self.push(StatePatch::toggle(path.into()));
    }

    pub fn apply(&self) {
        apply_state_patches(&self.patches);
    }

    pub fn into_vec(self) -> Vec<StatePatch> {
        self.patches
    }
}

#[cfg(target_arch = "wasm32")]
pub fn state_bool(path: impl AsRef<str>) -> bool {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        state_get_bool(ptr, len) != 0
    })
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
    read_string(path.as_ref(), state_get_string_len, state_get_string_copy)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn state_string(_path: impl AsRef<str>) -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
pub fn event_string(path: impl AsRef<str>) -> String {
    read_string(path.as_ref(), event_get_string_len, event_get_string_copy)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn event_string(_path: impl AsRef<str>) -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
pub fn host_bool(path: impl AsRef<str>) -> bool {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        host_get_bool(ptr, len) != 0
    })
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
    read_string(path.as_ref(), host_get_string_len, host_get_string_copy)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn host_string(_path: impl AsRef<str>) -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
pub fn emit_command_descriptor(descriptor: &RequestDescriptor) {
    match descriptor.payload.len() {
        0 => with_bytes(&descriptor.name, |ptr, len| unsafe { command(ptr, len) }),
        1 => {
            let (key, value) = descriptor.payload.iter().next().expect("payload exists");
            match value {
                Value::String(value) => with_bytes(&descriptor.name, |name_ptr, name_len| {
                    with_bytes(key, |key_ptr, key_len| {
                        with_bytes(value, |value_ptr, value_len| unsafe {
                            command_string(
                                name_ptr, name_len, key_ptr, key_len, value_ptr, value_len,
                            )
                        })
                    })
                }),
                Value::Bool(value) => {
                    emit_command_payload_json(descriptor, &serde_json::json!({ key: value }))
                }
                Value::Number(value) => {
                    emit_command_payload_json(descriptor, &serde_json::json!({ key: value }))
                }
                _ => emit_command_payload_json(
                    descriptor,
                    &Value::Object(descriptor.payload.clone()),
                ),
            }
        }
        _ => emit_command_payload_json(descriptor, &Value::Object(descriptor.payload.clone())),
    }
}

#[cfg(target_arch = "wasm32")]
fn emit_command_payload_json(descriptor: &RequestDescriptor, payload: &Value) {
    let Ok(json) = serde_json::to_string(payload) else {
        error("failed to serialize command payload in panel-sdk runtime");
        return;
    };

    with_bytes(&descriptor.name, |name_ptr, name_len| {
        with_bytes(&json, |json_ptr, json_len| unsafe {
            command_json(name_ptr, name_len, json_ptr, json_len)
        })
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn emit_command_descriptor(_descriptor: &RequestDescriptor) {}

pub fn emit_service_descriptor(descriptor: &RequestDescriptor) {
    emit_command_descriptor(descriptor);
}

pub fn emit_command(descriptor: &RequestDescriptor) {
    emit_command_descriptor(descriptor);
}

pub fn emit_service(descriptor: &RequestDescriptor) {
    emit_service_descriptor(descriptor);
}

#[cfg(target_arch = "wasm32")]
pub fn info(message: &str) {
    let level = panel_protocol::DiagnosticLevel::Info.to_abi();
    with_bytes(message, |ptr, len| unsafe { diagnostic(level, ptr, len) });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn info(_message: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn warn(message: &str) {
    let level = panel_protocol::DiagnosticLevel::Warning.to_abi();
    with_bytes(message, |ptr, len| unsafe { diagnostic(level, ptr, len) });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn warn(_message: &str) {}

#[cfg(target_arch = "wasm32")]
pub fn error(message: &str) {
    let level = panel_protocol::DiagnosticLevel::Error.to_abi();
    with_bytes(message, |ptr, len| unsafe { diagnostic(level, ptr, len) });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn error(_message: &str) {}

#[cfg(target_arch = "wasm32")]
fn read_string(
    path: &str,
    length_fn: unsafe extern "C" fn(i32, i32) -> i32,
    copy_fn: unsafe extern "C" fn(i32, i32, i32, i32),
) -> String {
    let length = with_bytes(path, |ptr, len| unsafe { length_fn(ptr, len) });
    if length <= 0 {
        return String::new();
    }

    let mut buffer = vec![0u8; length as usize];
    with_bytes(path, |path_ptr, path_len| unsafe {
        copy_fn(
            path_ptr,
            path_len,
            buffer.as_mut_ptr() as i32,
            buffer.len() as i32,
        )
    });
    String::from_utf8(buffer).unwrap_or_default()
}
