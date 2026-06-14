mod dom_api;
mod host_state_api;
mod memory;
mod request_api;
mod state_api;

use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;

use blitz_html::HtmlDocument;
use dom_api::DomCtx;
use panel_protocol::abi::{
    PANEL_INIT_EXPORT, PANEL_ON_HOST_CHANGE_EXPORT, PAYLOAD_VALUE_KEY, handler_export_name,
};
use panel_protocol::{HandlerEffects, HostCallInput};
use serde_json::Value;
use thiserror::Error;
use wasmtime::{Engine, Func, Instance, Linker, Module, Store};

static SHARED_ENGINE: OnceLock<Engine> = OnceLock::new();

fn shared_engine() -> &'static Engine {
    SHARED_ENGINE.get_or_init(|| {
        let mut config = wasmtime::Config::new();
        let _ = config.cache_config_load_default();
        Engine::new(&config).expect("failed to create wasmtime engine")
    })
}

#[derive(Debug, Error)]
pub enum PanelWasmHostError {
    #[error("failed to load panel wasm module at {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("failed to instantiate panel wasm module at {path}: {message}")]
    Instantiate { path: PathBuf, message: String },
    #[error("runtime handler failed: {0}")]
    Runtime(String),
}

#[derive(Default)]
struct HostCallContext {
    result: HandlerEffects,
    current_input: Option<HostCallInput>,
    dom_ctx: DomCtx,
}

impl HostCallContext {
    fn clear(&mut self) {
        self.result = HandlerEffects::default();
        self.current_input = None;
        // dom_ctx は call_with_dom が制御するためここではクリアしない
    }
}

pub struct PanelWasmInstance {
    store: Store<HostCallContext>,
    instance: Instance,
}

impl PanelWasmInstance {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PanelWasmHostError> {
        let path = path.as_ref().to_path_buf();
        let engine = shared_engine();
        let module = Module::from_file(engine, &path).map_err(|error| PanelWasmHostError::Load {
            path: path.clone(),
            message: error.to_string(),
        })?;
        let mut linker = Linker::new(engine);
        register_host_functions(&mut linker).map_err(|error| {
            PanelWasmHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;

        let mut store = Store::new(engine, HostCallContext::default());
        let instance = linker.instantiate(&mut store, &module).map_err(|error| {
            PanelWasmHostError::Instantiate {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;

        Ok(Self { store, instance })
    }

    pub fn on_host_change(
        &mut self,
        state: &Value,
        host_state: &Value,
    ) -> Result<HandlerEffects, PanelWasmHostError> {
        self.store.data_mut().clear();
        // on_host_change はホスト状態変化時の再描画フックであり UI イベントを伴わない。
        // event_payload は既定 (空) のままにし、疑似イベントの捏造はしない (P6)。
        self.store.data_mut().current_input = Some(HostCallInput {
            state: state.clone(),
            host_state: host_state.clone(),
            ..HostCallInput::default()
        });

        let handler = self
            .instance
            .get_func(&mut self.store, PANEL_ON_HOST_CHANGE_EXPORT)
            .ok_or_else(|| {
                PanelWasmHostError::Runtime(format!(
                    "missing lifecycle export: {PANEL_ON_HOST_CHANGE_EXPORT}"
                ))
            })?;
        call_export(&mut self.store, handler, None).map_err(PanelWasmHostError::Runtime)?;
        Ok(self.store.data().result.clone())
    }

    /// `panel_handle_<handler_name>` を呼び出す。
    ///
    /// `handler_name` は export 名の組み立てに使い、`input` (state/host_state/
    /// event_payload) は host function が引くコンテキストとして store に積む。
    pub fn handle_event(
        &mut self,
        handler_name: &str,
        input: &HostCallInput,
    ) -> Result<HandlerEffects, PanelWasmHostError> {
        let export_name = handler_export_name(handler_name);
        let numeric_value = input
            .event_payload
            .get(PAYLOAD_VALUE_KEY)
            .and_then(Value::as_i64)
            .unwrap_or_default() as i32;
        let payload = input.event_payload.get(PAYLOAD_VALUE_KEY).map(|_| numeric_value);
        self.store.data_mut().clear();
        self.store.data_mut().current_input = Some(input.clone());
        let handler = self
            .instance
            .get_func(&mut self.store, &export_name)
            .ok_or_else(|| {
                PanelWasmHostError::Runtime(format!("missing handler export: {export_name}"))
            })?;
        call_export(&mut self.store, handler, payload).map_err(PanelWasmHostError::Runtime)?;
        Ok(self.store.data().result.clone())
    }

    pub fn supports_on_host_change(&mut self) -> bool {
        self.instance
            .get_func(&mut self.store, PANEL_ON_HOST_CHANGE_EXPORT)
            .is_some()
    }

    pub fn has_handler(&mut self, handler_name: &str) -> bool {
        let export_name = handler_export_name(handler_name);
        self.instance
            .get_func(&mut self.store, &export_name)
            .is_some()
    }

    /// DOM context をスコープに設定して f を実行する。
    ///
    /// f の中で呼ばれる Wasm 関数は dom 系 host function を介して `document` を mutate できる。
    /// f が抜けたら dom_ctx は必ず None に戻る。
    ///
    /// SAFETY: 渡された `&mut HtmlDocument` の参照は f が return するまで生存している必要がある。
    pub fn call_with_dom<R>(
        &mut self,
        document: &mut HtmlDocument,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let ptr = NonNull::from(document);
        self.store.data_mut().dom_ctx.document = Some(ptr);
        let result = f(self);
        self.store.data_mut().dom_ctx.clear();
        result
    }

    /// `panel_init` export を呼び出す (DOM context 必須)。
    ///
    /// 戻り値は handler の `HandlerEffects` (commands / state_patch / diagnostics)。
    /// init 中に Wasm が DOM を mutate するなら `call_with_dom` 内で呼ぶこと。
    pub fn panel_init(&mut self) -> Result<HandlerEffects, PanelWasmHostError> {
        self.store.data_mut().clear();
        if let Some(init) = self.instance.get_func(&mut self.store, PANEL_INIT_EXPORT) {
            call_export(&mut self.store, init, None).map_err(PanelWasmHostError::Runtime)?;
        }
        Ok(self.store.data().result.clone())
    }
}

fn call_export(
    store: &mut Store<HostCallContext>,
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

/// 全 host module の host function を linker に登録する。
///
/// 関心別に分割した register モジュールを順に呼ぶ:
/// - `state_api`: パネル state の変更 (`state_set_*` / `state_toggle` / `state_apply_json`)
/// - `host_state_api`: state/host/event ソースの読み取り (`*_get_*`)
/// - `request_api`: command 発行と診断 (`command*` / `diagnostic`)
/// - `dom_api`: Blitz `HtmlDocument` の mutate (`dom` module)
fn register_host_functions(linker: &mut Linker<HostCallContext>) -> wasmtime::Result<()> {
    state_api::register_state_writers(linker)?;
    host_state_api::register_state_readers(linker)?;
    request_api::register_request_emitters(linker)?;
    dom_api::register_dom_host_functions(linker)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_protocol::{RequestDescriptor, StatePatch};
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_WAT: &str = r#"(module
  (import "host" "state_toggle" (func $state_toggle (param i32 i32)))
  (import "host" "state_set_bool" (func $state_set_bool (param i32 i32 i32)))
    (import "host" "state_apply_json" (func $state_apply_json (param i32 i32)))
    (import "host" "state_get_string_len" (func $state_get_string_len (param i32 i32) (result i32)))
    (import "host" "state_get_string_copy" (func $state_get_string_copy (param i32 i32 i32 i32)))
  (import "host" "command" (func $command (param i32 i32)))
  (import "host" "command_string" (func $command_string (param i32 i32 i32 i32 i32 i32)))
    (import "host" "command_json" (func $command_json (param i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "expanded")
  (data (i32.const 16) "project.save")
  (data (i32.const 32) "tool.set_active")
  (data (i32.const 64) "tool")
    (data (i32.const 80) "pen")
    (data (i32.const 96) "save_path")
    (data (i32.const 128) "layer.move")
    (data (i32.const 160) "{\22from_index\22:2,\22to_index\22:0}")
        (data (i32.const 224) "[{\22op\22:\22set\22,\22path\22:\22batched.value\22,\22value\22:7},{\22op\22:\22toggle\22,\22path\22:\22batched.enabled\22}]")
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
    (func (export "panel_handle_activate_pen")
    i32.const 32
    i32.const 15
    i32.const 64
    i32.const 4
    i32.const 80
        i32.const 3
        call $command_string)
    (func (export "panel_handle_move_layer")
        i32.const 128
        i32.const 10
        i32.const 160
        i32.const 29
        call $command_json)
    (func (export "panel_handle_apply_batch")
        i32.const 224
        i32.const 88
        call $state_apply_json)
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
    (func (export "panel_on_host_change")
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
    fn instance_initializes_state_and_emits_commands() {
        let wasm_path = write_temp_wat(SAMPLE_WAT);
        let mut instance = PanelWasmInstance::load(&wasm_path).expect("instance loads");

        let init = instance.panel_init().expect("panel_init runs");
        assert_eq!(init.state_patch, vec![StatePatch::set("expanded", false)]);
        let initial_state = json!({"expanded": false});

        let toggled = instance
            .handle_event("toggle-expanded", &input_with_state(initial_state.clone()))
            .expect("toggle handler runs");
        assert_eq!(toggled.state_patch, vec![StatePatch::toggle("expanded")]);

        let saved = instance
            .handle_event("save_project", &input_with_state(initial_state.clone()))
            .expect("save handler runs");
        assert_eq!(saved.commands, vec![RequestDescriptor::new("project.save")]);

        let pen = instance
            .handle_event("activate_pen", &input_with_state(initial_state))
            .expect("tool handler runs");
        let mut expected = RequestDescriptor::new("tool.set_active");
        expected
            .payload
            .insert("tool".to_string(), Value::String("pen".to_string()));
        assert_eq!(pen.commands, vec![expected]);

        let string_len = instance
            .handle_event(
                "save_path_len",
                &input_with_state(json!({"save_path": "project.altp.json"})),
            )
            .expect("string state handler runs");
        assert!(string_len.diagnostics.is_empty());

        let moved = instance
            .handle_event("move_layer", &input_with_state(json!({})))
            .expect("json payload handler runs");
        let mut expected_move = RequestDescriptor::new("layer.move");
        expected_move
            .payload
            .insert("from_index".to_string(), json!(2));
        expected_move
            .payload
            .insert("to_index".to_string(), json!(0));
        assert_eq!(moved.commands, vec![expected_move]);

        let batched = instance
            .handle_event("apply_batch", &input_with_state(json!({})))
            .expect("state batch handler runs");
        assert_eq!(
            batched.state_patch,
            vec![
                StatePatch::set("batched.value", 7),
                StatePatch::toggle("batched.enabled"),
            ]
        );
    }

    /// host state セクション JSON を 1 回で取得する ABI (BL-142)。
    /// `document` セクション全体を JSON 文字列として読み、state へ書き戻す。
    const SECTION_JSON_WAT: &str = r#"(module
    (import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
    (import "host" "host_get_section_json_len" (func $host_get_section_json_len (param i32 i32) (result i32)))
    (import "host" "host_get_section_json_copy" (func $host_get_section_json_copy (param i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "section")
    (data (i32.const 16) "document")
    (func (export "panel_on_host_change")
        (local $json_len i32)
        (local $buffer_ptr i32)
        i32.const 16
        i32.const 8
        call $host_get_section_json_len
        local.set $json_len
        i32.const 256
        local.set $buffer_ptr
        i32.const 16
        i32.const 8
        local.get $buffer_ptr
        local.get $json_len
        call $host_get_section_json_copy
        i32.const 0
        i32.const 7
        local.get $buffer_ptr
        local.get $json_len
        call $state_set_string))"#;

    #[test]
    fn instance_reads_host_state_section_as_json() {
        use panel_protocol::host_state::DocumentState;

        let wasm_path = write_temp_wat(SECTION_JSON_WAT);
        let mut instance = PanelWasmInstance::load(&wasm_path).expect("instance loads");

        let document = json!({
            "title": "Untitled",
            "page_count": 1,
            "koma_count": 1,
            "active_page_number": 1,
            "active_page_koma_count": 1,
            "active_koma_index": 0,
            "active_koma_number": 1,
            "active_koma_x": 0,
            "active_koma_y": 0,
            "active_koma_width": 320,
            "active_koma_height": 240,
            "active_layer_name": "Layer 1",
            "layer_count": 1,
            "active_layer_index": 0,
            "active_layer_blend_mode": "normal",
            "active_layer_visible": true,
            "active_layer_masked": false,
            "komas_json": "[]",
            "layers_json": "[{\"name\":\"Layer 1\",\"blend_mode\":\"normal\",\"visible\":true,\"masked\":false}]",
        });

        let synced = instance
            .on_host_change(&json!({}), &json!({ "document": document.clone() }))
            .expect("section json handler runs");
        assert!(synced.diagnostics.is_empty());

        // state へ書き戻された JSON 文字列を型付き DTO へ落とせる。
        let patch = synced
            .state_patch
            .iter()
            .find(|patch| patch.path == "section")
            .expect("section patch present");
        let json = patch.value.as_ref().and_then(Value::as_str).expect("string value");
        let state: DocumentState = serde_json::from_str(json).expect("document state parses");
        assert_eq!(state.title, "Untitled");
        assert_eq!(state.active_koma_width, 320);
        assert_eq!(state.layers().len(), 1);
    }

    #[test]
    fn instance_reads_host_state_through_host_imports() {
        let wasm_path = write_temp_wat(HOST_SYNC_WAT);
        let mut instance = PanelWasmInstance::load(&wasm_path).expect("instance loads");

        assert!(instance.supports_on_host_change());

        let synced = instance
            .on_host_change(
                &json!({}),
                &json!({
                    "document": {
                        "title": "Runtime Title",
                        "active_layer_visible": true,
                        "page_count": 7,
                    }
                }),
            )
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

    /// state ソースのみを持つ `HostCallInput` (event_payload / host_state は空)。
    fn input_with_state(state: Value) -> HostCallInput {
        HostCallInput {
            state,
            ..HostCallInput::default()
        }
    }

    fn write_temp_wat(contents: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("altpaint-panel-wasm-host-{suffix}"));
        fs::create_dir_all(&directory).expect("temp directory created");
        let path = directory.join("sample.wasm");
        fs::write(&path, contents).expect("wat file written");
        path
    }
}
