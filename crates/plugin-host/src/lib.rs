use std::path::{Path, PathBuf};

use panel_schema::{
    CommandDescriptor, Diagnostic, DiagnosticLevel, HandlerResult, PanelEventRequest,
    PanelInitRequest, PanelInitResponse, StatePatch,
};
use serde_json::{Map, Value};
use thiserror::Error;
use wasmtime::{Caller, Engine, Extern, Func, Instance, Linker, Memory, Module, Store};

#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error("failed to load runtime module at {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("failed to instantiate runtime module at {path}: {message}")]
    Instantiate { path: PathBuf, message: String },
    #[error("runtime handler failed: {0}")]
    Runtime(String),
}

#[derive(Debug, Default)]
struct RuntimeCollector {
    result: HandlerResult,
    current_request: Option<PanelEventRequest>,
}

impl RuntimeCollector {
    fn clear(&mut self) {
        self.result = HandlerResult::default();
        self.current_request = None;
    }
}

pub struct WasmPanelRuntime {
    path: PathBuf,
    store: Store<RuntimeCollector>,
    instance: Instance,
}

impl WasmPanelRuntime {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PluginHostError> {
        let path = path.as_ref().to_path_buf();
        let engine = Engine::default();
        let module = Module::from_file(&engine, &path).map_err(|error| PluginHostError::Load {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "host",
                "state_toggle",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| {
                    if let Some(path) = read_utf8(&mut caller, ptr, len) {
                        caller
                            .data_mut()
                            .result
                            .state_patch
                            .push(StatePatch::toggle(path));
                    } else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read state path for toggle"));
                    }
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_set_bool",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32, value: i32| {
                    if let Some(path) = read_utf8(&mut caller, ptr, len) {
                        caller
                            .data_mut()
                            .result
                            .state_patch
                            .push(StatePatch::set(path, value != 0));
                    } else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read state path for bool set"));
                    }
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_set_i32",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32, value: i32| {
                    if let Some(path) = read_utf8(&mut caller, ptr, len) {
                        caller
                            .data_mut()
                            .result
                            .state_patch
                            .push(StatePatch::set(path, value));
                    } else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read state path for i32 set"));
                    }
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_set_string",
                |mut caller: Caller<'_, RuntimeCollector>,
                 path_ptr: i32,
                 path_len: i32,
                 value_ptr: i32,
                 value_len: i32| {
                    let Some(path) = read_utf8(&mut caller, path_ptr, path_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read state path for string set",
                        ));
                        return;
                    };
                    let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read string value for state set",
                        ));
                        return;
                    };
                    caller
                        .data_mut()
                        .result
                        .state_patch
                        .push(StatePatch::set(path, value));
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_get_bool",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read state path for bool get"));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.state_snapshot, &path))
                        .and_then(Value::as_bool)
                        .map(i32::from)
                        .unwrap_or_default()
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_get_i32",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read state path for i32 get"));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.state_snapshot, &path))
                        .and_then(Value::as_i64)
                        .unwrap_or_default() as i32
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_get_string_len",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read state path for string len",
                        ));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.state_snapshot, &path))
                        .and_then(Value::as_str)
                        .map(|value| value.len() as i32)
                        .unwrap_or_default()
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "state_get_string_copy",
                |mut caller: Caller<'_, RuntimeCollector>,
                 path_ptr: i32,
                 path_len: i32,
                 buffer_ptr: i32,
                 buffer_len: i32| {
                    let Some(path) = read_utf8(&mut caller, path_ptr, path_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read state path for string copy",
                        ));
                        return;
                    };
                    let Some(value) = caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.state_snapshot, &path))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                    else {
                        return;
                    };
                    let Some(memory) = current_memory(&mut caller) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("missing wasm memory for string copy"));
                        return;
                    };
                    if buffer_ptr < 0 || buffer_len < 0 {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("invalid buffer range for string copy"));
                        return;
                    }
                    let data = memory.data_mut(&mut caller);
                    let start = buffer_ptr as usize;
                    let end = start.saturating_add(buffer_len as usize).min(data.len());
                    let Some(target) = data.get_mut(start..end) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "buffer range out of bounds for string copy",
                        ));
                        return;
                    };
                    let bytes = value.as_bytes();
                    let count = bytes.len().min(target.len());
                    target[..count].copy_from_slice(&bytes[..count]);
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "host_get_bool",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read host path for bool get"));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.host_snapshot, &path))
                        .and_then(Value::as_bool)
                        .map(i32::from)
                        .unwrap_or_default()
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "host_get_i32",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read host path for i32 get"));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.host_snapshot, &path))
                        .and_then(Value::as_i64)
                        .unwrap_or_default() as i32
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "host_get_string_len",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read host path for string len",
                        ));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.host_snapshot, &path))
                        .and_then(Value::as_str)
                        .map(|value| value.len() as i32)
                        .unwrap_or_default()
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "host_get_string_copy",
                |mut caller: Caller<'_, RuntimeCollector>,
                 path_ptr: i32,
                 path_len: i32,
                 buffer_ptr: i32,
                 buffer_len: i32| {
                    let Some(path) = read_utf8(&mut caller, path_ptr, path_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read host path for string copy",
                        ));
                        return;
                    };
                    let Some(value) = caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.host_snapshot, &path))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                    else {
                        return;
                    };
                    let Some(memory) = current_memory(&mut caller) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("missing wasm memory for host string copy"));
                        return;
                    };
                    if buffer_ptr < 0 || buffer_len < 0 {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("invalid buffer range for host string copy"));
                        return;
                    }
                    let data = memory.data_mut(&mut caller);
                    let start = buffer_ptr as usize;
                    let end = start.saturating_add(buffer_len as usize).min(data.len());
                    let Some(target) = data.get_mut(start..end) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "buffer range out of bounds for host string copy",
                        ));
                        return;
                    };
                    let bytes = value.as_bytes();
                    let count = bytes.len().min(target.len());
                    target[..count].copy_from_slice(&bytes[..count]);
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "event_get_string_len",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| -> i32 {
                    let Some(path) = read_utf8(&mut caller, ptr, len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read event path for string len",
                        ));
                        return 0;
                    };
                    caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.event_payload, &path))
                        .and_then(Value::as_str)
                        .map(|value| value.len() as i32)
                        .unwrap_or_default()
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "event_get_string_copy",
                |mut caller: Caller<'_, RuntimeCollector>,
                 path_ptr: i32,
                 path_len: i32,
                 buffer_ptr: i32,
                 buffer_len: i32| {
                    let Some(path) = read_utf8(&mut caller, path_ptr, path_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read event path for string copy",
                        ));
                        return;
                    };
                    let Some(value) = caller
                        .data()
                        .current_request
                        .as_ref()
                        .and_then(|request| lookup_json_path(&request.event_payload, &path))
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                    else {
                        return;
                    };
                    let Some(memory) = current_memory(&mut caller) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "missing wasm memory for event string copy",
                        ));
                        return;
                    };
                    if buffer_ptr < 0 || buffer_len < 0 {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "invalid buffer range for event string copy",
                        ));
                        return;
                    }
                    let data = memory.data_mut(&mut caller);
                    let start = buffer_ptr as usize;
                    let end = start.saturating_add(buffer_len as usize).min(data.len());
                    let Some(target) = data.get_mut(start..end) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "buffer range out of bounds for event string copy",
                        ));
                        return;
                    };
                    let bytes = value.as_bytes();
                    let count = bytes.len().min(target.len());
                    target[..count].copy_from_slice(&bytes[..count]);
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "command",
                |mut caller: Caller<'_, RuntimeCollector>, ptr: i32, len: i32| {
                    if let Some(name) = read_utf8(&mut caller, ptr, len) {
                        caller
                            .data_mut()
                            .result
                            .commands
                            .push(CommandDescriptor::new(name));
                    } else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read command name"));
                    }
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "command_string",
                |mut caller: Caller<'_, RuntimeCollector>,
                 name_ptr: i32,
                 name_len: i32,
                 key_ptr: i32,
                 key_len: i32,
                 value_ptr: i32,
                 value_len: i32| {
                    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
                        caller.data_mut().result.diagnostics.push(Diagnostic::error(
                            "failed to read command name for string payload",
                        ));
                        return;
                    };
                    let Some(key) = read_utf8(&mut caller, key_ptr, key_len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read command payload key"));
                        return;
                    };
                    let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
                        caller
                            .data_mut()
                            .result
                            .diagnostics
                            .push(Diagnostic::error("failed to read command payload value"));
                        return;
                    };

                    let mut descriptor = CommandDescriptor::new(name);
                    descriptor.payload.insert(key, Value::String(value));
                    caller.data_mut().result.commands.push(descriptor);
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;
        linker
            .func_wrap(
                "host",
                "diagnostic",
                |mut caller: Caller<'_, RuntimeCollector>, level: i32, ptr: i32, len: i32| {
                    let diagnostic = read_utf8(&mut caller, ptr, len)
                        .map(|message| Diagnostic {
                            level: match level {
                                0 => DiagnosticLevel::Info,
                                1 => DiagnosticLevel::Warning,
                                _ => DiagnosticLevel::Error,
                            },
                            message,
                        })
                        .unwrap_or_else(|| {
                            Diagnostic::error("failed to read diagnostic message from runtime")
                        });
                    caller.data_mut().result.diagnostics.push(diagnostic);
                },
            )
            .map_err(|error| PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            })?;

        let mut store = Store::new(&engine, RuntimeCollector::default());
        let instance = linker.instantiate(&mut store, &module).map_err(|error| {
            PluginHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            path,
            store,
            instance,
        })
    }

    pub fn initialize(
        &mut self,
        request: &PanelInitRequest,
    ) -> Result<PanelInitResponse, PluginHostError> {
        self.store.data_mut().clear();
        if let Some(init) = self.instance.get_func(&mut self.store, "panel_init") {
            call_export(&mut self.store, init, None).map_err(PluginHostError::Runtime)?;
        }

        let mut state = request.initial_state.clone();
        apply_state_patches(&mut state, &self.store.data().result.state_patch);
        Ok(PanelInitResponse {
            state,
            diagnostics: self.store.data().result.diagnostics.clone(),
        })
    }

    pub fn handle_event(
        &mut self,
        request: &PanelEventRequest,
    ) -> Result<HandlerResult, PluginHostError> {
        self.store.data_mut().clear();
        self.store.data_mut().current_request = Some(request.clone());
        let export_name = format!(
            "panel_handle_{}",
            sanitize_handler_name(&request.handler_name)
        );
        let handler = self
            .instance
            .get_func(&mut self.store, &export_name)
            .ok_or_else(|| {
                PluginHostError::Runtime(format!("missing handler export: {export_name}"))
            })?;
        let numeric_value = request
            .event_payload
            .get("value")
            .and_then(Value::as_i64)
            .unwrap_or_default() as i32;
        let payload = request.event_payload.get("value").map(|_| numeric_value);
        call_export(&mut self.store, handler, payload).map_err(PluginHostError::Runtime)?;
        Ok(self.store.data().result.clone())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn has_handler(&mut self, handler_name: &str) -> bool {
        let export_name = format!("panel_handle_{}", sanitize_handler_name(handler_name));
        self.instance
            .get_func(&mut self.store, &export_name)
            .is_some()
    }
}

fn call_export(
    store: &mut Store<RuntimeCollector>,
    func: Func,
    payload: Option<i32>,
) -> Result<(), String> {
    if let Ok(typed) = func.typed::<(), ()>(&mut *store) {
        typed.call(store, ()).map_err(|error| error.to_string())
    } else if let Ok(typed) = func.typed::<i32, ()>(&mut *store) {
        typed
            .call(store, payload.unwrap_or_default())
            .map_err(|error| error.to_string())
    } else {
        Err("unsupported handler signature; expected () or (i32)".to_string())
    }
}

fn read_utf8(caller: &mut Caller<'_, RuntimeCollector>, ptr: i32, len: i32) -> Option<String> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let memory = current_memory(caller)?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start.checked_add(len as usize)?;
    let bytes = data.get(start..end)?;
    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn current_memory(caller: &mut Caller<'_, RuntimeCollector>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(memory)) => Some(memory),
        _ => None,
    }
}

fn apply_state_patches(state: &mut Value, patches: &[StatePatch]) {
    if !state.is_object() {
        *state = Value::Object(Map::new());
    }
    for patch in patches {
        apply_state_patch(state, patch);
    }
}

fn apply_state_patch(state: &mut Value, patch: &StatePatch) {
    let mut current = state;
    let mut segments = patch.path.split('.').peekable();
    while let Some(segment) = segments.next() {
        let is_last = segments.peek().is_none();
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let object = current.as_object_mut().expect("object ensured");
        if is_last {
            match patch.op {
                panel_schema::StatePatchOp::Set | panel_schema::StatePatchOp::Replace => {
                    object.insert(
                        segment.to_string(),
                        patch.value.clone().unwrap_or(Value::Null),
                    );
                }
                panel_schema::StatePatchOp::Toggle => {
                    let next = !object
                        .get(segment)
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    object.insert(segment.to_string(), Value::Bool(next));
                }
            }
            return;
        }
        current = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
}

fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn sanitize_handler_name(handler_name: &str) -> String {
    handler_name
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => character,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_WAT: &str = r#"(module
  (import "host" "state_toggle" (func $state_toggle (param i32 i32)))
  (import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
    (import "host" "state_get_string_len" (func $state_get_string_len (param i32 i32) (result i32)))
    (import "host" "state_get_string_copy" (func $state_get_string_copy (param i32 i32 i32 i32)))
  (import "host" "command" (func $command (param i32 i32)))
  (import "host" "command_string" (func $command_string (param i32 i32 i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "expanded")
  (data (i32.const 16) "project.save")
  (data (i32.const 32) "tool.set_active")
  (data (i32.const 64) "tool")
  (data (i32.const 80) "brush")
    (data (i32.const 96) "save_path")
  (func (export "panel_init")
    i32.const 0
    i32.const 8
    i32.const 0
    call $state_set_bool)
  (func (export "panel_handle_toggle_expanded")
    i32.const 0
    i32.const 8
    call $state_toggle)
  (func (export "panel_handle_save_project")
    i32.const 16
    i32.const 12
    call $command)
  (func (export "panel_handle_activate_brush")
    i32.const 32
    i32.const 15
    i32.const 64
    i32.const 4
    i32.const 80
    i32.const 5
        call $command_string)
    (func (export "panel_handle_save_path_len")
        i32.const 96
        i32.const 9
        call $state_get_string_len
        drop))"#;

        const HOST_SYNC_WAT: &str = r#"(module
    (import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
    (import "host" "state_set_i32" (func $state_set_i32 (param i32 i32 i32)))
    (import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
    (import "host" "host_get_bool" (func $host_get_bool (param i32 i32) (result i32)))
    (import "host" "host_get_i32" (func $host_get_i32 (param i32 i32) (result i32)))
    (import "host" "host_get_string_len" (func $host_get_string_len (param i32 i32) (result i32)))
    (import "host" "host_get_string_copy" (func $host_get_string_copy (param i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "title")
    (data (i32.const 16) "visible")
    (data (i32.const 32) "count")
    (data (i32.const 48) "document.title")
    (data (i32.const 80) "document.active_layer_visible")
    (data (i32.const 128) "document.page_count")
    (func (export "panel_handle_sync_host")
        (local $title_len i32)
        (local $buffer_ptr i32)
        i32.const 48
        i32.const 14
        call $host_get_string_len
        local.set $title_len
        i32.const 192
        local.set $buffer_ptr
        i32.const 48
        i32.const 14
        local.get $buffer_ptr
        local.get $title_len
        call $host_get_string_copy
        i32.const 0
        i32.const 5
        local.get $buffer_ptr
        local.get $title_len
        call $state_set_string
        i32.const 16
        i32.const 7
        i32.const 80
        i32.const 29
        call $host_get_bool
        call $state_set_bool
        i32.const 32
        i32.const 5
        i32.const 128
        i32.const 19
        call $host_get_i32
        call $state_set_i32))"#;

    #[test]
    fn runtime_initializes_state_and_emits_commands() {
        let wasm_path = write_temp_wat(SAMPLE_WAT);
        let mut runtime = WasmPanelRuntime::load(&wasm_path).expect("runtime loads");

        let init = runtime
            .initialize(&PanelInitRequest {
                initial_state: json!({}),
                host_snapshot: json!({}),
            })
            .expect("runtime initializes");
        assert_eq!(init.state, json!({"expanded": false}));

        let toggled = runtime
            .handle_event(&PanelEventRequest {
                handler_name: "toggle-expanded".to_string(),
                event_kind: "change".to_string(),
                event_payload: json!({}),
                state_snapshot: init.state.clone(),
                host_snapshot: json!({}),
            })
            .expect("toggle handler runs");
        assert_eq!(toggled.state_patch, vec![StatePatch::toggle("expanded")]);

        let saved = runtime
            .handle_event(&PanelEventRequest {
                handler_name: "save_project".to_string(),
                event_kind: "click".to_string(),
                event_payload: json!({}),
                state_snapshot: init.state.clone(),
                host_snapshot: json!({}),
            })
            .expect("save handler runs");
        assert_eq!(saved.commands, vec![CommandDescriptor::new("project.save")]);

        let brush = runtime
            .handle_event(&PanelEventRequest {
                handler_name: "activate_brush".to_string(),
                event_kind: "click".to_string(),
                event_payload: json!({}),
                state_snapshot: init.state,
                host_snapshot: json!({}),
            })
            .expect("tool handler runs");
        let mut expected = CommandDescriptor::new("tool.set_active");
        expected
            .payload
            .insert("tool".to_string(), Value::String("brush".to_string()));
        assert_eq!(brush.commands, vec![expected]);

        let string_len = runtime
            .handle_event(&PanelEventRequest {
                handler_name: "save_path_len".to_string(),
                event_kind: "click".to_string(),
                event_payload: json!({}),
                state_snapshot: json!({"save_path": "project.altp.json"}),
                host_snapshot: json!({}),
            })
            .expect("string state handler runs");
        assert!(string_len.diagnostics.is_empty());
    }

    #[test]
    fn runtime_reads_host_snapshot_through_host_imports() {
        let wasm_path = write_temp_wat(HOST_SYNC_WAT);
        let mut runtime = WasmPanelRuntime::load(&wasm_path).expect("runtime loads");

        let synced = runtime
            .handle_event(&PanelEventRequest {
                handler_name: "sync_host".to_string(),
                event_kind: "sync_host".to_string(),
                event_payload: json!({}),
                state_snapshot: json!({}),
                host_snapshot: json!({
                    "document": {
                        "title": "Runtime Title",
                        "active_layer_visible": true,
                        "page_count": 7,
                    }
                }),
            })
            .expect("host sync handler runs");

        assert_eq!(
            synced.state_patch,
            vec![
                StatePatch::set("title", "Runtime Title"),
                StatePatch::set("visible", true),
                StatePatch::set("count", 7),
            ]
        );
        assert!(synced.diagnostics.is_empty());
    }

    fn write_temp_wat(contents: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("altpaint-plugin-host-{suffix}"));
        fs::create_dir_all(&directory).expect("temp directory created");
        let path = directory.join("sample.wasm");
        fs::write(&path, contents).expect("wat file written");
        path
    }
}
