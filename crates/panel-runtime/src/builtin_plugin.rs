//! `BuiltinPanelPlugin` — Phase 10 同梱パネルの統一実装型。
//!
//! 構成要素:
//! - `HtmlPanelView` (Blitz HTML/CSS + parley + vello)
//! - `PanelWasmInstance` (wasmtime, panel_init / panel_handle_* / panel_sync_host export を呼ぶ)
//! - 各 Wasm 呼出は `PanelWasmInstance::call_with_dom` で view の document を context にし、
//!   Wasm 内 DOM mutation host function (`set_attribute` / `set_inner_html` 等) で直接 DOM を書換える
//!
//! `update` (host state 同期) と `handle_event` (UI イベント) のいずれでも DOM mutation を
//! 行う可能性があるため、両経路で `call_with_dom` を必ず通すこと。

use std::any::Any;
use std::path::Path;

use app_core::{Command, Document};
use panel_api::{HostAction, PanelEvent, PanelPlugin, ServiceRequest};
use panel_html::{
    ActionDescriptor, HtmlPanelView, blitz_dom::LocalName, blitz_dom::node::NodeData,
    parse_data_action,
};
use crate::commands::command_from_descriptor;
use crate::host_state::{
    EMPTY_WORKSPACE_PANELS_JSON, HostStateCache, build_host_state,
};
use crate::meta::PanelMeta;
use panel_wasm_host::{PanelWasmHostError, PanelWasmInstance};
use serde_json::{Value, json};

pub struct BuiltinPanelPlugin {
    id: &'static str,
    title: &'static str,
    default_size: (u32, u32),
    view: HtmlPanelView,
    wasm: PanelWasmInstance,
    /// Wasm 側が保持する state (panel_init で初期化、handler 戻り値の patch を蓄積)。
    state: Value,
    /// host state のキャッシュ。
    host_state_cache: HostStateCache,
    /// 最新の host state (handler 内 host_get_* で利用される)。
    last_host_state: Value,
    /// ワークスペースに登録されたパネル一覧 (id / title / visible) を JSON 化したもの。
    /// `PanelRuntime::set_workspace_panels_json` 経由で更新され、次回 `update` で
    /// host state に含められる。builtin.workspace-layout 用。
    workspace_panels_json: String,
    /// Wasm が `panel_handle_keyboard` を export しているか (load 時に確定)。
    /// `handles_keyboard_event` は `&self` のため、`PanelWasmInstance::has_handler`
    /// (`&mut self`) を毎回呼べずキャッシュする。
    has_keyboard_handler: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BuiltinPanelError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid panel.meta.json: {0}")]
    Meta(#[from] serde_json::Error),
    #[error("panel wasm host: {0}")]
    Host(#[from] PanelWasmHostError),
}

impl BuiltinPanelPlugin {
    /// パネルディレクトリを読み込み、HTML/CSS/Wasm を初期化する。
    ///
    /// ディレクトリ構成:
    /// - `panel.html`: 初期 DOM (必須)
    /// - `panel.css`: ユーザー CSS (任意)
    /// - `panel.meta.json`: `{ "id", "title", "default_size": { "width", "height" } }` (必須)
    /// - `<wasm_filename>`: コンパイル済み Wasm モジュール (必須)
    pub fn load(
        directory: &Path,
        wasm_filename: &str,
        restored_size: Option<(u32, u32)>,
    ) -> Result<Self, BuiltinPanelError> {
        let html = std::fs::read_to_string(directory.join("panel.html"))?;
        let css = directory.join("panel.css");
        let css = if css.exists() {
            std::fs::read_to_string(&css)?
        } else {
            String::new()
        };
        let meta_raw = std::fs::read_to_string(directory.join("panel.meta.json"))?;
        let meta: PanelMeta = serde_json::from_str(&meta_raw)?;
        let default_size = meta.default_size.as_tuple();

        let mut view = HtmlPanelView::new(&html, &css);
        view.set_panel_size(restored_size.unwrap_or(default_size));
        let mut wasm = PanelWasmInstance::load(directory.join(wasm_filename))?;

        // panel_init は DOM context 必須 (Wasm が初期 DOM を mutate する可能性)。
        let init = wasm.call_with_dom(view.document_mut(), |rt| rt.panel_init())?;
        view.mark_mutated();
        let has_keyboard_handler = wasm.has_handler("keyboard");

        // panel_init が返した state_patch を空 state に適用して初期 state を確定する。
        let mut state = json!({});
        apply_state_patches(&mut state, &init.state_patch);

        Ok(Self {
            id: Box::leak(meta.id.into_boxed_str()),
            title: Box::leak(meta.title.into_boxed_str()),
            default_size,
            view,
            wasm,
            state,
            host_state_cache: HostStateCache::default(),
            last_host_state: json!({}),
            workspace_panels_json: EMPTY_WORKSPACE_PANELS_JSON.to_string(),
            has_keyboard_handler,
        })
    }

    /// ワークスペースに登録されたパネル一覧 JSON を更新する。
    /// 次回 `update` で host state に反映される。
    pub fn set_workspace_panels_json(&mut self, json: String) {
        self.workspace_panels_json = json;
    }

    /// panel.meta.json の `default_size` を返す。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入される。
    pub fn default_size(&self) -> (u32, u32) {
        self.default_size
    }

    pub fn view(&self) -> &HtmlPanelView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut HtmlPanelView {
        &mut self.view
    }

    /// Wasm が `data-action="command:..."` ボタンをクリックされた等のイベントを処理する。
    ///
    /// Wasm export 名は `panel_handle_<sanitized_handler_name>`。
    fn dispatch_to_wasm(
        &mut self,
        handler_name: &str,
        event_payload: Value,
    ) -> Result<Vec<HostAction>, PanelWasmHostError> {
        if !self.wasm.has_handler(handler_name) {
            return Ok(Vec::new());
        }
        let request = panel_host_request(
            handler_name,
            event_payload,
            &self.state,
            &self.last_host_state,
        );
        let result = self
            .wasm
            .call_with_dom(self.view.document_mut(), |rt| rt.handle_event(&request))?;
        self.view.mark_mutated();
        apply_state_patches(&mut self.state, &result.state_patch);
        Ok(result
            .commands
            .into_iter()
            .filter_map(request_descriptor_to_host_action)
            .collect())
    }
}

fn panel_host_request(
    handler_name: &str,
    event_payload: Value,
    state_snapshot: &Value,
    host_state: &Value,
) -> panel_protocol::PanelEventRequest {
    panel_protocol::PanelEventRequest {
        handler_name: handler_name.to_string(),
        event_payload,
        state_snapshot: state_snapshot.clone(),
        host_state: host_state.clone(),
    }
}

fn apply_state_patches(state: &mut Value, patches: &[panel_protocol::StatePatch]) {
    use panel_protocol::StatePatchOp;
    use serde_json::Map;
    if !state.is_object() {
        *state = Value::Object(Map::new());
    }
    for patch in patches {
        let mut current = &mut *state;
        let mut segments = patch.path.split('.').peekable();
        while let Some(segment) = segments.next() {
            let is_last = segments.peek().is_none();
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }
            let object = current.as_object_mut().expect("object ensured");
            if is_last {
                match patch.op {
                    StatePatchOp::Set => {
                        object.insert(
                            segment.to_string(),
                            patch.value.clone().unwrap_or(Value::Null),
                        );
                    }
                    StatePatchOp::Toggle => {
                        let next =
                            !object.get(segment).and_then(Value::as_bool).unwrap_or(false);
                        object.insert(segment.to_string(), Value::Bool(next));
                    }
                }
                break;
            }
            current = object
                .entry(segment.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
        }
    }
}

fn request_descriptor_to_host_action(
    descriptor: panel_protocol::RequestDescriptor,
) -> Option<HostAction> {
    // 1. 命令名が Command enum に翻訳できれば DispatchCommand
    if let Ok(command) = command_from_descriptor(&descriptor) {
        return Some(HostAction::DispatchCommand(command));
    }
    // 2. 翻訳できなければ ServiceRequest として扱う (services::*)
    let mut request = ServiceRequest::new(descriptor.name);
    for (k, v) in descriptor.payload {
        request = request.with_value(k, v);
    }
    Some(HostAction::RequestService(request))
}

impl PanelPlugin for BuiltinPanelPlugin {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn update(
        &mut self,
        document: &Document,
        can_undo: bool,
        can_redo: bool,
        active_jobs: usize,
        snapshot_count: usize,
    ) {
        let host_state = build_host_state(
            document,
            can_undo,
            can_redo,
            active_jobs,
            snapshot_count,
            &mut self.host_state_cache,
            &self.workspace_panels_json,
        );
        self.last_host_state = host_state.clone();
        if !self.wasm.supports_sync_host() {
            return;
        }
        let state = &self.state;
        let outcome = self
            .wasm
            .call_with_dom(self.view.document_mut(), |rt| {
                rt.sync_host(state, &host_state)
            });
        if let Ok(result) = outcome {
            apply_state_patches(&mut self.state, &result.state_patch);
            self.view.mark_mutated();
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn persistent_config(&self) -> Option<Value> {
        self.state.get("config").cloned()
    }

    fn restore_persistent_config(&mut self, config: &Value) {
        if !self.state.is_object() {
            self.state = json!({});
        }
        if let Some(obj) = self.state.as_object_mut() {
            obj.insert("config".to_string(), config.clone());
        }
    }

    fn handles_keyboard_event(&self) -> bool {
        self.has_keyboard_handler
    }

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        match event {
            PanelEvent::Keyboard {
                panel_id,
                shortcut,
                key,
                repeat,
            } if panel_id == self.id => self
                .dispatch_to_wasm(
                    "keyboard",
                    json!({ "shortcut": shortcut, "key": key, "repeat": repeat }),
                )
                .unwrap_or_default(),
            PanelEvent::Activate { panel_id, node_id } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({}))
            }
            PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({ "value": value }))
            }
            PanelEvent::DragValue {
                panel_id,
                node_id,
                from,
                to,
            } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(
                    descriptor,
                    json!({ "from": from, "to": to, "value": to }),
                )
            }
            PanelEvent::SetText {
                panel_id,
                node_id,
                value,
            } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({ "value": value.clone() }))
            }
            _ => Vec::new(),
        }
    }
}

impl BuiltinPanelPlugin {
    fn lookup_action_descriptor(&self, node_id: &str) -> Option<ActionDescriptor> {
        let document = self.view.document();
        let id_selector = format!("#{}", css_escape_id(node_id));
        let id = document.query_selector(&id_selector).ok().flatten()?;
        let node = document.get_node(id)?;
        let NodeData::Element(element) = &node.data else {
            return None;
        };
        let raw_action = element.attr(LocalName::from("data-action"))?;
        let raw_args = element.attr(LocalName::from("data-args"));
        parse_data_action(raw_action, raw_args).ok()
    }

    fn descriptor_to_actions(
        &mut self,
        descriptor: Option<ActionDescriptor>,
        extra_payload: Value,
    ) -> Vec<HostAction> {
        match descriptor {
            Some(ActionDescriptor::Command { id, .. }) => {
                command_id_to_host_action(&id).map(|a| vec![a]).unwrap_or_default()
            }
            Some(ActionDescriptor::Service { name, mut payload }) => {
                if let Some(extra_obj) = extra_payload.as_object() {
                    for (k, v) in extra_obj {
                        payload.insert(k.clone(), v.clone());
                    }
                }
                let mut request = ServiceRequest::new(name);
                for (k, v) in payload {
                    request = request.with_value(k, v);
                }
                vec![HostAction::RequestService(request)]
            }
            Some(ActionDescriptor::Altp { node_id: handler, mut payload }) => {
                if let Some(extra_obj) = extra_payload.as_object() {
                    for (k, v) in extra_obj {
                        payload.insert(k.clone(), v.clone());
                    }
                }
                let event_payload = Value::Object(payload);
                self.dispatch_to_wasm(&handler, event_payload)
                    .unwrap_or_default()
            }
            None => Vec::new(),
        }
    }
}

fn command_id_to_host_action(command_id: &str) -> Option<HostAction> {
    match command_id {
        "noop" => Some(HostAction::DispatchCommand(Command::Noop)),
        _ => None,
    }
}

/// CSS セレクタ用に id をエスケープする (`.` や `:` を含む id 対応)。
fn css_escape_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    out
}

/// テスト用パネルディレクトリ生成 (registry テストとも共有)。
#[cfg(test)]
pub(crate) mod test_fixture {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// keyboard handler を持つパネル: event payload の "shortcut" を
    /// state "config.last_shortcut" へコピーする。
    pub(crate) const KEYBOARD_WAT: &str = r#"(module
    (import "host" "event_get_string_len" (func $event_get_string_len (param i32 i32) (result i32)))
    (import "host" "event_get_string_copy" (func $event_get_string_copy (param i32 i32 i32 i32)))
    (import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "shortcut")
    (data (i32.const 16) "config.last_shortcut")
    (func (export "panel_init"))
    (func (export "panel_handle_keyboard")
        (local $len i32)
        i32.const 0
        i32.const 8
        call $event_get_string_len
        local.set $len
        i32.const 0
        i32.const 8
        i32.const 64
        local.get $len
        call $event_get_string_copy
        i32.const 16
        i32.const 20
        i32.const 64
        local.get $len
        call $state_set_string))"#;

    /// keyboard handler を持たないパネル。
    pub(crate) const NO_KEYBOARD_WAT: &str = r#"(module
    (memory (export "memory") 1)
    (func (export "panel_init")))"#;

    pub(crate) fn write_panel_fixture(name: &str, wat: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("altpaint-builtin-{name}-{suffix}"));
        std::fs::create_dir_all(&directory).expect("temp directory created");
        std::fs::write(
            directory.join("panel.html"),
            r#"<div class="panel"><button id="kb.test" data-action="command:noop">x</button></div>"#,
        )
        .expect("panel.html written");
        std::fs::write(
            directory.join("panel.meta.json"),
            r#"{ "id": "builtin.test-kb", "title": "KB", "default_size": { "width": 200, "height": 120 } }"#,
        )
        .expect("panel.meta.json written");
        std::fs::write(directory.join("panel.wasm"), wat).expect("panel.wasm written");
        directory
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixture::{KEYBOARD_WAT, NO_KEYBOARD_WAT, write_panel_fixture};
    use super::*;

    /// Wasm が panel_handle_keyboard を export していれば handles_keyboard_event は true。
    #[test]
    fn handles_keyboard_event_true_when_wasm_exports_keyboard_handler() {
        let dir = write_panel_fixture("kb-true", KEYBOARD_WAT);
        let panel = BuiltinPanelPlugin::load(&dir, "panel.wasm", None).expect("panel loads");
        assert!(panel.handles_keyboard_event());
    }

    /// keyboard handler が無ければ handles_keyboard_event は false。
    #[test]
    fn handles_keyboard_event_false_without_keyboard_handler() {
        let dir = write_panel_fixture("kb-false", NO_KEYBOARD_WAT);
        let panel = BuiltinPanelPlugin::load(&dir, "panel.wasm", None).expect("panel loads");
        assert!(!panel.handles_keyboard_event());
    }

    /// Keyboard イベントが Wasm keyboard handler へ届き、state patch 経由で
    /// persistent_config に反映される。
    #[test]
    fn keyboard_event_dispatches_to_wasm_and_updates_persistent_config() {
        let dir = write_panel_fixture("kb-dispatch", KEYBOARD_WAT);
        let mut panel = BuiltinPanelPlugin::load(&dir, "panel.wasm", None).expect("panel loads");

        let actions = panel.handle_event(&PanelEvent::Keyboard {
            panel_id: "builtin.test-kb".to_string(),
            shortcut: "Ctrl+Alt+N".to_string(),
            key: "N".to_string(),
            repeat: false,
        });

        assert!(actions.is_empty());
        assert_eq!(
            panel.persistent_config(),
            Some(json!({ "last_shortcut": "Ctrl+Alt+N" }))
        );
    }

    /// panel_id が一致しない Keyboard イベントは無視される。
    #[test]
    fn keyboard_event_for_other_panel_is_ignored() {
        let dir = write_panel_fixture("kb-other", KEYBOARD_WAT);
        let mut panel = BuiltinPanelPlugin::load(&dir, "panel.wasm", None).expect("panel loads");

        let actions = panel.handle_event(&PanelEvent::Keyboard {
            panel_id: "builtin.other".to_string(),
            shortcut: "Ctrl+Alt+N".to_string(),
            key: "N".to_string(),
            repeat: false,
        });

        assert!(actions.is_empty());
        assert_eq!(panel.persistent_config(), None);
    }
}

