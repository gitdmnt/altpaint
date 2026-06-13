//! パネルが自身の state を変更するための host function 群。
//!
//! いずれも [`StatePatch`] を `HandlerEffects::state_patch` に積むだけで、適用は
//! panel-runtime 側 (`panel_protocol::apply_patches`) が行う。

use crate::HostCallContext;
use crate::memory::{push_error, read_utf8};
use panel_protocol::StatePatch;
use panel_protocol::abi::HOST_IMPORT_MODULE;
use wasmtime::{Caller, Linker};

/// `state_toggle` / `state_set_*` / `state_apply_json` を登録する。
pub(crate) fn register_state_writers(
    linker: &mut Linker<HostCallContext>,
) -> wasmtime::Result<()> {
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "state_toggle",
        |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| {
            let Some(path) = read_utf8(&mut caller, ptr, len) else {
                push_error(&mut caller, "failed to read state path for toggle");
                return;
            };
            push_patch(&mut caller, StatePatch::toggle(path));
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "state_set_bool",
        |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32, value: i32| {
            let Some(path) = read_utf8(&mut caller, ptr, len) else {
                push_error(&mut caller, "failed to read state path for bool set");
                return;
            };
            push_patch(&mut caller, StatePatch::set(path, value != 0));
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "state_set_i32",
        |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32, value: i32| {
            let Some(path) = read_utf8(&mut caller, ptr, len) else {
                push_error(&mut caller, "failed to read state path for i32 set");
                return;
            };
            push_patch(&mut caller, StatePatch::set(path, value));
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "state_set_string",
        |mut caller: Caller<'_, HostCallContext>,
         path_ptr: i32,
         path_len: i32,
         value_ptr: i32,
         value_len: i32| {
            let Some(path) = read_utf8(&mut caller, path_ptr, path_len) else {
                push_error(&mut caller, "failed to read state path for string set");
                return;
            };
            let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
                push_error(&mut caller, "failed to read string value for state set");
                return;
            };
            push_patch(&mut caller, StatePatch::set(path, value));
        },
    )?;
    linker.func_wrap(
        HOST_IMPORT_MODULE,
        "state_apply_json",
        |mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32| {
            let Some(payload_text) = read_utf8(&mut caller, ptr, len) else {
                push_error(&mut caller, "failed to read state patch batch json");
                return;
            };
            let Ok(patches) = serde_json::from_str::<Vec<StatePatch>>(&payload_text) else {
                push_error(&mut caller, "failed to parse state patch batch json");
                return;
            };
            caller.data_mut().result.state_patch.extend(patches);
        },
    )?;
    Ok(())
}

fn push_patch(caller: &mut Caller<'_, HostCallContext>, patch: StatePatch) {
    caller.data_mut().result.state_patch.push(patch);
}
