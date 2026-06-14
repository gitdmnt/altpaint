//! UI イベント payload の取得 ABI (BL-147)。
//!
//! handler が typed payload を受ける規約 (BL-141) の取り出し口。`event_payload`
//! が正準経路で、`event_string` / `event_payload_json` は下位の生取得口。

#[cfg(target_arch = "wasm32")]
use super::abi::read_string;
use super::abi::wasm_or_native;

wasm_or_native! {
    /// UI イベント payload の指定キーを文字列で読む (低水準。typed payload 経路を推奨)。
    pub fn event_string(path: impl AsRef<str>) -> String {
        wasm: { read_string(path.as_ref(), super::abi::event_get_string_len, super::abi::event_get_string_copy) }
        native: { String::new() }
    }
}

/// UI イベントの `event_payload` 全体を JSON 文字列で 1 回取得する (BL-141)。
#[cfg(target_arch = "wasm32")]
pub fn event_payload_json() -> String {
    let length = unsafe { super::abi::event_get_payload_json_len() };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u8; length as usize];
    unsafe {
        super::abi::event_get_payload_json_copy(buffer.as_mut_ptr() as i32, buffer.len() as i32);
    }
    String::from_utf8(buffer).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn event_payload_json() -> String {
    String::new()
}

/// `event_payload` を typed payload (serde `Deserialize` 構造体) へ落とす (BL-141)。
///
/// panel_handler マクロが typed payload 引数の handler に対し生成する取り出し口。
/// 空 payload や deserialize 失敗時は `T::default()` を返す
/// (handler は常に値を 1 つ受け取る規約; payload 欠落は既定値として扱う)。
pub fn event_payload<T>() -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    let json = event_payload_json();
    if json.is_empty() {
        return T::default();
    }
    serde_json::from_str(&json).unwrap_or_default()
}
