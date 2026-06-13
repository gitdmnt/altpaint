//! パネルがホストへ要求 (command / service) と診断を送るための host function 群。
//!
//! `command*` は [`RequestDescriptor`] を組み立てて `HandlerEffects::commands` に積む。
//! `diagnostic` はレベル付きメッセージを `HandlerEffects::diagnostics` に積む。

use crate::HostCallContext;
use crate::memory::{push_error, read_utf8};
use panel_protocol::abi::HOST_IMPORT_MODULE;
use panel_protocol::{Diagnostic, DiagnosticLevel, RequestDescriptor};
use serde_json::Value;
use wasmtime::{Caller, Linker};

/// `command` / `command_string` / `command_json` / `diagnostic` を登録する。
pub(crate) fn register_request_emitters(
    linker: &mut Linker<HostCallContext>,
) -> wasmtime::Result<()> {
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "command",
        |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| {
            let Some(name) = read_utf8(&mut caller, ptr, len) else {
                push_error(&mut caller, "failed to read command name");
                return;
            };
            push_command(&mut caller, RequestDescriptor::new(name));
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "command_string",
        |mut caller: Caller<'_, HostCallContext>,
         name_ptr: i32,
         name_len: i32,
         key_ptr: i32,
         key_len: i32,
         value_ptr: i32,
         value_len: i32| {
            let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
                push_error(&mut caller, "failed to read command name for string payload");
                return;
            };
            let Some(key) = read_utf8(&mut caller, key_ptr, key_len) else {
                push_error(&mut caller, "failed to read command payload key");
                return;
            };
            let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
                push_error(&mut caller, "failed to read command payload value");
                return;
            };
            let mut descriptor = RequestDescriptor::new(name);
            descriptor.payload.insert(key, Value::String(value));
            push_command(&mut caller, descriptor);
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "command_json",
        |mut caller: Caller<'_, HostCallContext>,
         name_ptr: i32,
         name_len: i32,
         json_ptr: i32,
         json_len: i32| {
            let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
                push_error(&mut caller, "failed to read command name for json payload");
                return;
            };
            let Some(payload_text) = read_utf8(&mut caller, json_ptr, json_len) else {
                push_error(&mut caller, "failed to read command payload json");
                return;
            };
            let Ok(Value::Object(payload)) = serde_json::from_str::<Value>(&payload_text) else {
                push_error(&mut caller, "failed to parse command payload json object");
                return;
            };
            let mut descriptor = RequestDescriptor::new(name);
            descriptor.payload = payload;
            push_command(&mut caller, descriptor);
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "diagnostic",
        |mut caller: Caller<'_, HostCallContext>, level: i32, ptr: i32, len: i32| {
            let diagnostic = read_utf8(&mut caller, ptr, len)
                .map(|message| Diagnostic {
                    level: DiagnosticLevel::from_abi(level),
                    message,
                })
                .unwrap_or_else(|| {
                    Diagnostic::error("failed to read diagnostic message from runtime")
                });
            caller.data_mut().result.diagnostics.push(diagnostic);
        },
    )?;
    Ok(())
}

fn push_command(caller: &mut Caller<'_, HostCallContext>, descriptor: RequestDescriptor) {
    caller.data_mut().result.commands.push(descriptor);
}
