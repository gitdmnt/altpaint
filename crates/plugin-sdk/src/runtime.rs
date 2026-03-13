//! Wasm パネルから host ABI を呼び出すランタイム関数群を提供する。

use panel_schema::CommandDescriptor;
use panel_schema::StatePatch;
#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "host")]
unsafe extern "C" {
    /// 状態 切替 に必要な処理を行う。
    fn state_toggle(ptr: i32, len: i32);
    /// 状態 設定 bool に必要な処理を行う。
    fn state_set_bool(ptr: i32, len: i32, value: i32);
    /// 状態 設定 i32 に必要な処理を行う。
    fn state_set_i32(ptr: i32, len: i32, value: i32);
    /// 状態 設定 string に必要な処理を行う。
    fn state_set_string(path_ptr: i32, path_len: i32, value_ptr: i32, value_len: i32);
    /// 現在の 状態 適用 JSON を返す。
    fn state_apply_json(ptr: i32, len: i32);
    /// 状態 get bool を計算して返す。
    fn state_get_bool(ptr: i32, len: i32) -> i32;
    /// 状態 get i32 を計算して返す。
    fn state_get_i32(ptr: i32, len: i32) -> i32;
    /// 状態 get string len を計算して返す。
    fn state_get_string_len(ptr: i32, len: i32) -> i32;
    /// 状態 get string copy に必要な処理を行う。
    fn state_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
    /// イベント get string len を計算して返す。
    fn event_get_string_len(ptr: i32, len: i32) -> i32;
    /// イベント get string copy に必要な処理を行う。
    fn event_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
    /// ホスト get bool を計算して返す。
    fn host_get_bool(ptr: i32, len: i32) -> i32;
    /// ホスト get i32 を計算して返す。
    fn host_get_i32(ptr: i32, len: i32) -> i32;
    /// ホスト get string len を計算して返す。
    fn host_get_string_len(ptr: i32, len: i32) -> i32;
    /// ホスト get string copy に必要な処理を行う。
    fn host_get_string_copy(path_ptr: i32, path_len: i32, buffer_ptr: i32, buffer_len: i32);
    /// コマンド に必要な処理を行う。
    fn command(ptr: i32, len: i32);
    /// コマンド string に必要な処理を行う。
    fn command_string(
        name_ptr: i32,
        name_len: i32,
        key_ptr: i32,
        key_len: i32,
        value_ptr: i32,
        value_len: i32,
    );
    /// 現在の コマンド JSON を返す。
    fn command_json(name_ptr: i32, name_len: i32, json_ptr: i32, json_len: i32);
    /// diagnostic に必要な処理を行う。
    fn diagnostic(level: i32, ptr: i32, len: i32);
}

/// with bytes を計算して返す。
#[cfg(target_arch = "wasm32")]
fn with_bytes<T>(value: &str, f: impl FnOnce(i32, i32) -> T) -> T {
    f(value.as_ptr() as i32, value.len() as i32)
}

/// 状態上の 状態 を切り替える。
#[cfg(target_arch = "wasm32")]
pub fn toggle_state(path: impl AsRef<str>) {
    with_bytes(path.as_ref(), |ptr, len| unsafe { state_toggle(ptr, len) });
}

/// 状態上の 状態 を切り替える。
#[cfg(not(target_arch = "wasm32"))]
pub fn toggle_state(_path: impl AsRef<str>) {}

/// 状態上の 状態 bool を更新する。
#[cfg(target_arch = "wasm32")]
pub fn set_state_bool(path: impl AsRef<str>, value: bool) {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        state_set_bool(ptr, len, i32::from(value))
    });
}

/// 状態上の 状態 bool を更新する。
#[cfg(not(target_arch = "wasm32"))]
pub fn set_state_bool(_path: impl AsRef<str>, _value: bool) {}

/// 状態上の 状態 i32 を更新する。
#[cfg(target_arch = "wasm32")]
pub fn set_state_i32(path: impl AsRef<str>, value: i32) {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        state_set_i32(ptr, len, value)
    });
}

/// 状態上の 状態 i32 を更新する。
#[cfg(not(target_arch = "wasm32"))]
pub fn set_state_i32(_path: impl AsRef<str>, _value: i32) {}

/// 状態上の 状態 string を更新する。
#[cfg(target_arch = "wasm32")]
pub fn set_state_string(path: impl AsRef<str>, value: impl AsRef<str>) {
    with_bytes(path.as_ref(), |path_ptr, path_len| {
        with_bytes(value.as_ref(), |value_ptr, value_len| unsafe {
            state_set_string(path_ptr, path_len, value_ptr, value_len)
        })
    });
}

/// 状態上の 状態 string を更新する。
#[cfg(not(target_arch = "wasm32"))]
pub fn set_state_string(_path: impl AsRef<str>, _value: impl AsRef<str>) {}

/// 状態 JSON を設定する。
pub fn set_state_json(path: impl Into<String>, value: impl Into<serde_json::Value>) {
    apply_state_patches(&[StatePatch::set(path.into(), value.into())]);
}

/// 状態 JSON を置き換える。
pub fn replace_state_json(path: impl Into<String>, value: impl Into<serde_json::Value>) {
    apply_state_patches(&[StatePatch::replace(path.into(), value.into())]);
}

/// 現在の値を 状態 patches へ変換する。
#[cfg(target_arch = "wasm32")]
pub fn apply_state_patches(patches: &[StatePatch]) {
    let Ok(serialized) = serde_json::to_string(patches) else {
        error("failed to serialize state patch batch in plugin-sdk runtime");
        return;
    };
    with_bytes(&serialized, |ptr, len| unsafe {
        state_apply_json(ptr, len)
    });
}

/// 状態 patches を現在の状態へ適用する。
#[cfg(not(target_arch = "wasm32"))]
pub fn apply_state_patches(_patches: &[StatePatch]) {}

/// まとめて適用する state patch バッファ。
#[derive(Debug, Default, Clone)]
pub struct StatePatchBuffer {
    patches: Vec<StatePatch>,
}

impl StatePatchBuffer {
    /// 既定値を使って新しいインスタンスを生成する。
    pub fn new() -> Self {
        Self::default()
    }

    /// Is empty かどうかを返す。
    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    /// push に必要な処理を行う。
    pub fn push(&mut self, patch: StatePatch) {
        self.patches.push(patch);
    }

    /// Bool を設定する。
    pub fn set_bool(&mut self, path: impl Into<String>, value: bool) {
        self.push(StatePatch::set(path.into(), value));
    }

    /// I32 を設定する。
    pub fn set_i32(&mut self, path: impl Into<String>, value: i32) {
        self.push(StatePatch::set(path.into(), value));
    }

    /// String を設定する。
    pub fn set_string(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    /// JSON を設定する。
    pub fn set_json(&mut self, path: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.push(StatePatch::set(path.into(), value.into()));
    }

    /// JSON を置き換える。
    pub fn replace_json(&mut self, path: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.push(StatePatch::replace(path.into(), value.into()));
    }

    /// 切替 に必要な処理を行う。
    pub fn toggle(&mut self, path: impl Into<String>) {
        self.push(StatePatch::toggle(path.into()));
    }

    /// 適用 に必要な処理を行う。
    pub fn apply(&self) {
        apply_state_patches(&self.patches);
    }

    /// 現在の値を vec 形式へ変換する。
    pub fn into_vec(self) -> Vec<StatePatch> {
        self.patches
    }
}

/// 状態 bool を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn state_bool(path: impl AsRef<str>) -> bool {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        state_get_bool(ptr, len) != 0
    })
}

/// 状態 bool を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn state_bool(_path: impl AsRef<str>) -> bool {
    false
}

/// 状態 i32 を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn state_i32(path: impl AsRef<str>) -> i32 {
    with_bytes(path.as_ref(), |ptr, len| unsafe { state_get_i32(ptr, len) })
}

/// 状態 i32 を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn state_i32(_path: impl AsRef<str>) -> i32 {
    0
}

/// 状態 string を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn state_string(path: impl AsRef<str>) -> String {
    read_string(path.as_ref(), state_get_string_len, state_get_string_copy)
}

/// 状態 string を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn state_string(_path: impl AsRef<str>) -> String {
    String::new()
}

/// イベント string を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn event_string(path: impl AsRef<str>) -> String {
    read_string(path.as_ref(), event_get_string_len, event_get_string_copy)
}

/// イベント string を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn event_string(_path: impl AsRef<str>) -> String {
    String::new()
}

/// ホスト bool を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn host_bool(path: impl AsRef<str>) -> bool {
    with_bytes(path.as_ref(), |ptr, len| unsafe {
        host_get_bool(ptr, len) != 0
    })
}

/// ホスト bool を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn host_bool(_path: impl AsRef<str>) -> bool {
    false
}

/// ホスト i32 を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn host_i32(path: impl AsRef<str>) -> i32 {
    with_bytes(path.as_ref(), |ptr, len| unsafe { host_get_i32(ptr, len) })
}

/// ホスト i32 を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn host_i32(_path: impl AsRef<str>) -> i32 {
    0
}

/// ホスト string を計算して返す。
#[cfg(target_arch = "wasm32")]
pub fn host_string(path: impl AsRef<str>) -> String {
    read_string(path.as_ref(), host_get_string_len, host_get_string_copy)
}

/// ホスト string を計算して返す。
#[cfg(not(target_arch = "wasm32"))]
pub fn host_string(_path: impl AsRef<str>) -> String {
    String::new()
}

/// 入力内容を判別し、必要な状態更新とサービス呼び出しへ振り分ける。
///
/// 内部でコマンドを発行します。
#[cfg(target_arch = "wasm32")]
pub fn emit_command_descriptor(descriptor: &CommandDescriptor) {
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

/// 現在の値を コマンド payload JSON へ変換する。
///
/// 内部でコマンドを発行します。
#[cfg(target_arch = "wasm32")]
fn emit_command_payload_json(descriptor: &CommandDescriptor, payload: &Value) {
    let Ok(json) = serde_json::to_string(payload) else {
        error("failed to serialize command payload in plugin-sdk runtime");
        return;
    };

    with_bytes(&descriptor.name, |name_ptr, name_len| {
        with_bytes(&json, |json_ptr, json_len| unsafe {
            command_json(name_ptr, name_len, json_ptr, json_len)
        })
    });
}

/// emit コマンド 記述子 に必要な処理を行う。
///
/// 内部でコマンドを発行します。
#[cfg(not(target_arch = "wasm32"))]
pub fn emit_command_descriptor(_descriptor: &CommandDescriptor) {}

/// emit サービス 記述子 に必要な処理を行う。
///
/// 内部でサービス要求を発行します。
pub fn emit_service_descriptor(descriptor: &CommandDescriptor) {
    emit_command_descriptor(descriptor);
}

/// emit コマンド に必要な処理を行う。
///
/// 内部でコマンドを発行します。
pub fn emit_command(descriptor: &CommandDescriptor) {
    emit_command_descriptor(descriptor);
}

/// emit サービス に必要な処理を行う。
///
/// 内部でサービス要求を発行します。
pub fn emit_service(descriptor: &CommandDescriptor) {
    emit_service_descriptor(descriptor);
}

/// info に必要な処理を行う。
#[cfg(target_arch = "wasm32")]
pub fn info(message: &str) {
    with_bytes(message, |ptr, len| unsafe { diagnostic(0, ptr, len) });
}

/// info に必要な処理を行う。
#[cfg(not(target_arch = "wasm32"))]
pub fn info(_message: &str) {}

/// warn に必要な処理を行う。
#[cfg(target_arch = "wasm32")]
pub fn warn(message: &str) {
    with_bytes(message, |ptr, len| unsafe { diagnostic(1, ptr, len) });
}

/// warn に必要な処理を行う。
#[cfg(not(target_arch = "wasm32"))]
pub fn warn(_message: &str) {}

/// エラー に必要な処理を行う。
#[cfg(target_arch = "wasm32")]
pub fn error(message: &str) {
    with_bytes(message, |ptr, len| unsafe { diagnostic(2, ptr, len) });
}

/// エラー に必要な処理を行う。
#[cfg(not(target_arch = "wasm32"))]
pub fn error(_message: &str) {}

/// String を読み込み、必要に応じて整形して返す。
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
