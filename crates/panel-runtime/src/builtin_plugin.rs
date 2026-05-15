//! `BuiltinPanelPlugin` — Phase 10 同梱パネルの統一実装型。
//!
//! 構成要素:
//! - `HtmlPanelEngine` (Blitz HTML/CSS + parley + vello)
//! - `WasmPanelRuntime` (wasmtime, panel_init / panel_handle_* / panel_sync_host export を呼ぶ)
//! - 各 Wasm 呼出は `WasmPanelRuntime::call_with_dom` で engine の document を context にし、
//!   Wasm 内 DOM mutation host function (`set_attribute` / `set_inner_html` 等) で直接 DOM を書換える
//!
//! `update` (host snapshot 同期) と `handle_event` (UI イベント) のいずれでも DOM mutation を
//! 行う可能性があるため、両経路で `call_with_dom` を必ず通すこと。

use std::any::Any;
use std::path::Path;

use app_core::{Command, Document};
use panel_api::{HostAction, PanelEvent, PanelPlugin, ServiceRequest};
use panel_html_experiment::{
    ActionDescriptor, HtmlPanelEngine, blitz_dom::LocalName, blitz_dom::node::NodeData,
    parse_data_action,
};
use crate::commands::command_from_descriptor;
use crate::host_sync::{
    EMPTY_WORKSPACE_PANELS_JSON, HostSnapshotCache, build_host_snapshot_cached,
};
use crate::meta::PanelMeta;
use plugin_host::{PluginHostError, WasmPanelRuntime};
use serde_json::{Value, json};

pub struct BuiltinPanelPlugin {
    id: &'static str,
    title: &'static str,
    default_size: (u32, u32),
    engine: HtmlPanelEngine,
    wasm: WasmPanelRuntime,
    /// Wasm 側が保持する state (panel_init で初期化、handler 戻り値の patch を蓄積)。
    state: Value,
    /// host snapshot のキャッシュ。
    snapshot_cache: HostSnapshotCache,
    /// 最新の host snapshot (handler 内 host_get_* で利用される)。
    last_host_snapshot: Value,
    /// ワークスペースに登録されたパネル一覧 (id / title / visible) を JSON 化したもの。
    /// `PanelRuntime::set_workspace_panels_json` 経由で更新され、次回 `update` で
    /// host snapshot に含められる。builtin.workspace-layout 用。
    workspace_panels_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BuiltinPanelError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid panel.meta.json: {0}")]
    Meta(#[from] serde_json::Error),
    #[error("plugin host: {0}")]
    Host(#[from] PluginHostError),
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

        let mut engine = HtmlPanelEngine::new(&html, &css);
        engine.on_load(restored_size.unwrap_or(default_size));
        let mut wasm = WasmPanelRuntime::load(directory.join(wasm_filename))?;

        // panel_init は DOM context 必須 (Wasm が初期 DOM を mutate する可能性)。
        let init = wasm.call_with_dom(engine.document_mut(), |rt| rt.panel_init())?;
        engine.mark_mutated();

        // panel_init が返した state_patch を空 state に適用して初期 state を確定する。
        let mut state = json!({});
        apply_state_patches(&mut state, &init.state_patch);

        Ok(Self {
            id: Box::leak(meta.id.into_boxed_str()),
            title: Box::leak(meta.title.into_boxed_str()),
            default_size,
            engine,
            wasm,
            state,
            snapshot_cache: HostSnapshotCache::default(),
            last_host_snapshot: json!({}),
            workspace_panels_json: EMPTY_WORKSPACE_PANELS_JSON.to_string(),
        })
    }

    /// ワークスペースに登録されたパネル一覧 JSON を更新する。
    /// 次回 `update` で host snapshot に反映される。
    pub fn set_workspace_panels_json(&mut self, json: String) {
        self.workspace_panels_json = json;
    }

    /// panel.meta.json の `default_size` を返す。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入される。
    pub fn default_size(&self) -> (u32, u32) {
        self.default_size
    }

    pub fn engine(&self) -> &HtmlPanelEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut HtmlPanelEngine {
        &mut self.engine
    }

    /// Wasm が `data-action="command:..."` ボタンをクリックされた等のイベントを処理する。
    ///
    /// Wasm export 名は `panel_handle_<sanitized_handler_name>`。
    fn dispatch_to_wasm(
        &mut self,
        handler_name: &str,
        event_kind: &str,
        event_payload: Value,
    ) -> Result<Vec<HostAction>, PluginHostError> {
        if !self.wasm.has_handler(handler_name) {
            return Ok(Vec::new());
        }
        let request = panel_host_request(
            handler_name,
            event_kind,
            event_payload,
            &self.state,
            &self.last_host_snapshot,
        );
        let result = self
            .wasm
            .call_with_dom(self.engine.document_mut(), |rt| rt.handle_event(&request))?;
        self.engine.mark_mutated();
        apply_state_patches(&mut self.state, &result.state_patch);
        Ok(result
            .commands
            .into_iter()
            .filter_map(command_descriptor_to_host_action)
            .collect())
    }
}

fn panel_host_request(
    handler_name: &str,
    event_kind: &str,
    event_payload: Value,
    state_snapshot: &Value,
    host_snapshot: &Value,
) -> panel_schema::PanelEventRequest {
    panel_schema::PanelEventRequest {
        handler_name: handler_name.to_string(),
        event_kind: event_kind.to_string(),
        event_payload,
        state_snapshot: state_snapshot.clone(),
        host_snapshot: host_snapshot.clone(),
    }
}

fn apply_state_patches(state: &mut Value, patches: &[panel_schema::StatePatch]) {
    use panel_schema::StatePatchOp;
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
                    StatePatchOp::Set | StatePatchOp::Replace => {
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

fn command_descriptor_to_host_action(
    descriptor: panel_schema::CommandDescriptor,
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
        let host_snapshot = build_host_snapshot_cached(
            document,
            can_undo,
            can_redo,
            active_jobs,
            snapshot_count,
            &mut self.snapshot_cache,
            &self.workspace_panels_json,
        );
        self.last_host_snapshot = host_snapshot.clone();
        if !self.wasm.supports_sync_host() {
            return;
        }
        let state = &self.state;
        let outcome = self
            .wasm
            .call_with_dom(self.engine.document_mut(), |rt| {
                rt.sync_host(state, &host_snapshot)
            });
        if let Ok(result) = outcome {
            apply_state_patches(&mut self.state, &result.state_patch);
            self.engine.mark_mutated();
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

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        match event {
            PanelEvent::Activate { panel_id, node_id } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, "activate", json!({}))
            }
            PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, "set_value", json!({ "value": value }))
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
                    "drag_value",
                    json!({ "from": from, "to": to, "value": to }),
                )
            }
            PanelEvent::SetText {
                panel_id,
                node_id,
                value,
            } if panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(
                    descriptor,
                    "set_text",
                    json!({ "value": value.clone() }),
                )
            }
            _ => Vec::new(),
        }
    }
}

impl BuiltinPanelPlugin {
    fn lookup_action_descriptor(&self, node_id: &str) -> Option<ActionDescriptor> {
        let document = self.engine.document();
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
        event_kind: &str,
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
            Some(ActionDescriptor::Altp { node_id: handler, mut payload, .. }) => {
                if let Some(extra_obj) = extra_payload.as_object() {
                    for (k, v) in extra_obj {
                        payload.insert(k.clone(), v.clone());
                    }
                }
                let event_payload = Value::Object(payload);
                self.dispatch_to_wasm(&handler, event_kind, event_payload)
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

