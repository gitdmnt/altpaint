//! パネルがホスト状態を読むための host function 群。
//!
//! `state_get_*` / `host_get_*` / `event_get_*` はいずれも「現在の `HostCallInput` の
//! ある JSON ソースをドット区切り path で引く」という同一処理であり、ソースだけが異なる。
//! [`StateSource`] でソースを選び、[`register_source_readers`] が ABI 名 prefix ごとに
//! 一括登録する。
//!
//! 各ソースが提供する ABI:
//! - `<prefix>_get_bool` / `<prefix>_get_i32` / `<prefix>_get_string_len` / `<prefix>_get_string_copy`
//!
//! event ソースは `get_bool` / `get_i32` も登録されるが、現状 SDK は string 系のみ使用する
//! (未使用 ABI を追加で公開しないよう、登録対象は呼び出し側 [`register_state_readers`] が選ぶ)。

use crate::HostCallContext;
use crate::memory::{push_error, read_utf8, write_str_to_buffer};
use panel_protocol::abi::HOST_IMPORT_MODULE;
use serde_json::Value;
use wasmtime::{Caller, Linker};

/// 読み取り対象となる `HostCallInput` の JSON ソース。
#[derive(Clone, Copy)]
pub(crate) enum StateSource {
    /// パネル自身の永続/一時 state (`state`)。
    PanelState,
    /// ホストが配るドキュメント等の状態 (`host_state`)。
    HostState,
    /// UI イベントの payload (`event_payload`)。
    Event,
}

impl StateSource {
    /// このソースに対応する `HostCallInput` の JSON 値を返す。
    fn value(self, ctx: &HostCallContext) -> Option<&Value> {
        let input = ctx.current_input.as_ref()?;
        Some(match self {
            StateSource::PanelState => &input.state,
            StateSource::HostState => &input.host_state,
            StateSource::Event => &input.event_payload,
        })
    }

    /// 診断メッセージに使う人間可読なソース名。
    fn label(self) -> &'static str {
        match self {
            StateSource::PanelState => "state",
            StateSource::HostState => "host",
            StateSource::Event => "event",
        }
    }
}

/// `state_get_*` と `host_get_*` の全アクセサ + `event_get_string_*` を登録する。
pub(crate) fn register_state_readers(
    linker: &mut Linker<HostCallContext>,
) -> wasmtime::Result<()> {
    register_source_readers(linker, "state", StateSource::PanelState)?;
    register_source_readers(linker, "host", StateSource::HostState)?;
    register_event_string_readers(linker, "event", StateSource::Event)?;
    // host state セクション JSON を 1 回で取得する ABI (BL-142)。
    register_section_json_readers(linker)?;
    // event payload 全体を JSON で取得する ABI (BL-141 typed handler payload)。
    register_event_payload_json_readers(linker)?;
    Ok(())
}

/// `event_get_payload_json_len` / `event_get_payload_json_copy` を登録する (BL-141)。
///
/// typed handler payload のため、UI イベントの `event_payload` 全体を JSON 文字列で
/// 1 回取得し、パネル側で serde `Deserialize` 構造体へ落とす。引数付きパスは取らない
/// (常に payload ルートをシリアライズする)。
fn register_event_payload_json_readers(
    linker: &mut Linker<HostCallContext>,
) -> wasmtime::Result<()> {
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "event_get_payload_json_len",
        |caller: Caller<'_, HostCallContext>| -> i32 {
            event_payload_json(&caller)
                .map(|json| json.len() as i32)
                .unwrap_or_default()
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "event_get_payload_json_copy",
        |mut caller: Caller<'_, HostCallContext>, buffer_ptr: i32, buffer_len: i32| {
            let Some(json) = event_payload_json(&caller) else {
                return;
            };
            write_str_to_buffer(
                &mut caller,
                &json,
                buffer_ptr,
                buffer_len,
                "event payload json copy",
            );
        },
    )?;
    Ok(())
}

/// 現在の event_payload 全体を JSON 文字列へシリアライズする。
fn event_payload_json(caller: &Caller<'_, HostCallContext>) -> Option<String> {
    let payload = StateSource::Event.value(caller.data())?;
    serde_json::to_string(payload).ok()
}

/// `host_get_section_json_len` / `host_get_section_json_copy` を登録する (BL-142)。
///
/// 個別 path getter (`host_get_string` 等) を値の数だけ呼ぶ代わりに、host state の
/// トップレベルセクション (例 `"document"`) を JSON 文字列として 1 回で取得し、
/// パネル側で型付き DTO ([`panel_protocol::host_state`]) へ serde デシリアライズする。
fn register_section_json_readers(linker: &mut Linker<HostCallContext>) -> wasmtime::Result<()> {
    let source = StateSource::HostState;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "host_get_section_json_len",
        move |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| -> i32 {
            let Some(key) = read_path(&mut caller, ptr, len, source, "section json len") else {
                return 0;
            };
            section_json(&caller, &key)
                .map(|json| json.len() as i32)
                .unwrap_or_default()
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "host_get_section_json_copy",
        move |mut caller: Caller<'_, HostCallContext>,
              key_ptr: i32,
              key_len: i32,
              buffer_ptr: i32,
              buffer_len: i32| {
            let Some(key) = read_path(&mut caller, key_ptr, key_len, source, "section json copy")
            else {
                return;
            };
            let Some(json) = section_json(&caller, &key) else {
                return;
            };
            write_str_to_buffer(
                &mut caller,
                &json,
                buffer_ptr,
                buffer_len,
                "host state section json copy",
            );
        },
    )?;
    Ok(())
}

/// host state の トップレベルセクション (`key`) を JSON 文字列へシリアライズする。
fn section_json(caller: &Caller<'_, HostCallContext>, key: &str) -> Option<String> {
    let section = StateSource::HostState
        .value(caller.data())
        .and_then(|value| value.get(key))?;
    serde_json::to_string(section).ok()
}

/// `<prefix>_get_bool` / `_get_i32` / `_get_string_len` / `_get_string_copy` を登録する。
fn register_source_readers(
    linker: &mut Linker<HostCallContext>,
    prefix: &str,
    source: StateSource,
) -> wasmtime::Result<()> {
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        &format!("{prefix}_get_bool"),
        move |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| -> i32 {
            let Some(path) = read_path(&mut caller, ptr, len, source, "bool get") else {
                return 0;
            };
            lookup(&caller, source, &path)
                .and_then(Value::as_bool)
                .map(i32::from)
                .unwrap_or_default()
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        &format!("{prefix}_get_i32"),
        move |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| -> i32 {
            let Some(path) = read_path(&mut caller, ptr, len, source, "i32 get") else {
                return 0;
            };
            lookup(&caller, source, &path)
                .and_then(Value::as_i64)
                .unwrap_or_default() as i32
        },
    )?;
    register_event_string_readers(linker, prefix, source)?;
    Ok(())
}

/// `<prefix>_get_string_len` / `<prefix>_get_string_copy` を登録する。
fn register_event_string_readers(
    linker: &mut Linker<HostCallContext>,
    prefix: &str,
    source: StateSource,
) -> wasmtime::Result<()> {
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        &format!("{prefix}_get_string_len"),
        move |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| -> i32 {
            let Some(path) = read_path(&mut caller, ptr, len, source, "string len") else {
                return 0;
            };
            lookup(&caller, source, &path)
                .and_then(Value::as_str)
                .map(|value| value.len() as i32)
                .unwrap_or_default()
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        &format!("{prefix}_get_string_copy"),
        move |mut caller: Caller<'_, HostCallContext>,
              path_ptr: i32,
              path_len: i32,
              buffer_ptr: i32,
              buffer_len: i32| {
            let Some(path) = read_path(&mut caller, path_ptr, path_len, source, "string copy")
            else {
                return;
            };
            let Some(value) = lookup(&caller, source, &path)
                .and_then(Value::as_str)
                .map(ToString::to_string)
            else {
                return;
            };
            write_str_to_buffer(
                &mut caller,
                &value,
                buffer_ptr,
                buffer_len,
                &format!("{} string copy", source.label()),
            );
        },
    )?;
    Ok(())
}

/// path を読み、失敗時は診断を追加して `None` を返す。
fn read_path(
    caller: &mut Caller<'_, HostCallContext>,
    ptr: i32,
    len: i32,
    source: StateSource,
    op: &str,
) -> Option<String> {
    match read_utf8(caller, ptr, len) {
        Some(path) => Some(path),
        None => {
            push_error(
                caller,
                format!("failed to read {} path for {op}", source.label()),
            );
            None
        }
    }
}

/// 現在の request の対象ソースから path を引く。
fn lookup<'a>(
    caller: &'a Caller<'_, HostCallContext>,
    source: StateSource,
    path: &str,
) -> Option<&'a Value> {
    source.value(caller.data()).and_then(|value| lookup_json_path(value, path))
}

/// ドット区切り path で JSON をネスト探索する。
fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}
