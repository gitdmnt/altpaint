use crate::host_sync::{build_host_snapshot, parse_document_size, parse_hex_color};
use app_core::{Command, Document, ToolKind};
use panel_api::{
    DropdownOption, HostAction, LayerListItem, PanelEvent, PanelNode, PanelPlugin, PanelTree,
    PanelView, ServiceRequest, TextInputMode,
};
use panel_dsl::{AttrValue as DslAttrValue, PanelDefinition, StateField, ViewElement, ViewNode};
use panel_schema::{
    CommandDescriptor, Diagnostic, PanelEventRequest, PanelInitRequest, StatePatch, StatePatchOp,
};
use plugin_host::{PluginHostError, WasmPanelRuntime};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub(crate) struct DslPanelPlugin {
    id: &'static str,
    title: &'static str,
    definition: PanelDefinition,
    runtime: WasmPanelRuntime,
    state: Value,
    host_snapshot: Value,
    diagnostics: Vec<Diagnostic>,
    has_keyboard_handler: bool,
    supports_sync_host: bool,
}

impl DslPanelPlugin {
    /// 既定値を使って新しいインスタンスを生成する。
    ///
    /// 失敗時はエラーを返します。
    pub(crate) fn from_definition(definition: PanelDefinition) -> Result<Self, String> {
        let id = leak_string(definition.manifest.id.clone());
        let title = leak_string(definition.manifest.title.clone());
        let runtime_path = definition
            .source_path
            .parent()
            .map(|directory| directory.join(&definition.runtime.wasm))
            .unwrap_or_else(|| definition.source_path.clone());
        let mut runtime = WasmPanelRuntime::load(&runtime_path)
            .map_err(|error: PluginHostError| error.to_string())?;
        let has_keyboard_handler = runtime.has_handler("keyboard");
        let supports_sync_host = runtime.supports_sync_host();
        let initial_state = state_defaults_to_json(&definition.state);
        let init = runtime
            .initialize(&PanelInitRequest {
                initial_state,
                host_snapshot: json!({}),
            })
            .map_err(|error: PluginHostError| error.to_string())?;

        Ok(Self {
            id,
            title,
            definition,
            runtime,
            state: init.state,
            host_snapshot: json!({}),
            diagnostics: init.diagnostics,
            has_keyboard_handler,
            supports_sync_host,
        })
    }

    /// 現在の値を tree へ変換する。
    fn evaluate_tree(&self) -> PanelTree {
        let mut context = DslEvaluationContext {
            panel_id: self.id.to_string(),
            state: &self.state,
            generated_ids: 0,
        };
        let mut children = self
            .definition
            .view
            .iter()
            .flat_map(|node| convert_dsl_view_node(node, &mut context))
            .collect::<Vec<_>>();
        if !self.diagnostics.is_empty() {
            children.push(PanelNode::Section {
                id: "dsl.diagnostics".to_string(),
                title: "Diagnostics".to_string(),
                children: self
                    .diagnostics
                    .iter()
                    .enumerate()
                    .map(|(index, diagnostic)| PanelNode::Text {
                        id: format!("dsl.diagnostics.{index}"),
                        text: format!("{:?}: {}", diagnostic.level, diagnostic.message),
                    })
                    .collect(),
            });
        }

        PanelTree {
            id: self.id,
            title: self.title,
            children,
        }
    }

    /// 現在の値を ハンドラ action へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn resolve_handler_action(&self, event: &PanelEvent) -> Option<HostAction> {
        let tree = self.evaluate_tree();
        match event {
            PanelEvent::Activate { node_id, .. }
            | PanelEvent::SetValue { node_id, .. }
            | PanelEvent::DragValue { node_id, .. }
            | PanelEvent::SetText { node_id, .. } => find_panel_action(&tree.children, node_id),
            PanelEvent::Keyboard { .. } if self.has_keyboard_handler => {
                Some(HostAction::InvokePanelHandler {
                    panel_id: self.id.to_string(),
                    handler_name: "keyboard".to_string(),
                    event_kind: "keyboard".to_string(),
                })
            }
            PanelEvent::Keyboard { .. } => None,
        }
    }

    /// 現在の値を テキスト 入力 イベント へ変換する。
    fn apply_text_input_event(&mut self, node_id: &str, value: &str) -> bool {
        let tree = self.evaluate_tree();
        let Some((binding, _input_mode)) = find_text_input_binding(&tree.children, node_id) else {
            return false;
        };
        apply_state_patch(
            &mut self.state,
            &StatePatch::set(binding, Value::String(value.to_string())),
        );
        true
    }

    /// 現在の値を ホスト 状態 へ変換する。
    fn sync_host_state(&mut self) {
        if !self.supports_sync_host {
            return;
        }

        let result = match self.runtime.sync_host(&self.state, &self.host_snapshot) {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics = vec![Diagnostic::error(error.to_string())];
                return;
            }
        };

        apply_state_patches(&mut self.state, &result.state_patch);
        self.diagnostics = result.diagnostics;
    }
}

impl PanelPlugin for DslPanelPlugin {
    /// ID を計算して返す。
    fn id(&self) -> &'static str {
        self.id
    }

    /// 現在の値を output へ変換する。
    fn title(&self) -> &'static str {
        self.title
    }

    /// 更新 に必要な処理を行う。
    fn update(&mut self, document: &Document, can_undo: bool, can_redo: bool, active_jobs: usize, snapshot_count: usize) {
        self.host_snapshot = build_host_snapshot(document, can_undo, can_redo, active_jobs, snapshot_count);
        self.sync_host_state();
    }

    /// Debug summary 用の表示文字列を組み立てる。
    fn debug_summary(&self) -> String {
        format!(
            "dsl runtime={} handlers={} diagnostics={} state={}",
            self.runtime.path().display(),
            self.definition.handler_bindings.len(),
            self.diagnostics.len(),
            self.state
        )
    }

    /// ビュー 用の表示文字列を組み立てる。
    fn view(&self) -> PanelView {
        let mut lines = vec![format!("runtime: {}", self.definition.runtime.wasm)];
        if !self.diagnostics.is_empty() {
            lines.push(format!("diagnostics: {}", self.diagnostics.len()));
        }
        PanelView {
            id: self.id,
            title: self.title,
            lines,
        }
    }

    /// パネル tree を計算して返す。
    fn panel_tree(&self) -> PanelTree {
        self.evaluate_tree()
    }

    /// handles キーボード イベント を計算して返す。
    fn handles_keyboard_event(&self) -> bool {
        self.has_keyboard_handler
    }

    /// persistent 設定 を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn persistent_config(&self) -> Option<Value> {
        lookup_json_path(&self.state, "config").cloned()
    }

    /// Persistent 設定 を更新する。
    fn restore_persistent_config(&mut self, config: &Value) {
        apply_state_patch(
            &mut self.state,
            &StatePatch::replace("config", config.clone()),
        );
    }

    /// 現在の値を イベント へ変換する。
    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        let mut updated_text = false;
        if let PanelEvent::SetText { node_id, value, .. } = event {
            updated_text = self.apply_text_input_event(node_id, value);
        }
        let Some(HostAction::InvokePanelHandler {
            panel_id,
            handler_name,
            event_kind,
        }) = self.resolve_handler_action(event)
        else {
            if updated_text {
                self.diagnostics.clear();
            }
            return Vec::new();
        };
        if panel_id != self.id {
            return Vec::new();
        }

        let event_payload = match event {
            PanelEvent::Activate { .. } => json!({}),
            PanelEvent::SetValue { value, .. } => json!({ "value": value }),
            PanelEvent::DragValue { from, to, .. } => {
                json!({ "from": from.to_string(), "to": to.to_string(), "value": to })
            }
            PanelEvent::SetText { value, .. } => json!({ "value": value }),
            PanelEvent::Keyboard {
                shortcut,
                key,
                repeat,
                ..
            } => json!({ "shortcut": shortcut, "key": key, "repeat": repeat }),
        };
        let result = match self.runtime.handle_event(&PanelEventRequest {
            handler_name,
            event_kind,
            event_payload,
            state_snapshot: self.state.clone(),
            host_snapshot: self.host_snapshot.clone(),
        }) {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics = vec![Diagnostic::error(error.to_string())];
                return Vec::new();
            }
        };

        apply_state_patches(&mut self.state, &result.state_patch);
        self.diagnostics = result.diagnostics.clone();
        command_descriptors_to_actions(result.commands, &mut self.diagnostics)
    }
}

/// leak string を計算して返す。
fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

struct DslEvaluationContext<'a> {
    panel_id: String,
    state: &'a Value,
    generated_ids: usize,
}

/// 現在の値を dsl ビュー node へ変換する。
fn convert_dsl_view_node(
    node: &ViewNode,
    context: &mut DslEvaluationContext<'_>,
) -> Vec<PanelNode> {
    match node {
        ViewNode::Text(text) => {
            let text = evaluate_text_content(text, context);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![PanelNode::Text {
                    id: next_generated_node_id(context, "text"),
                    text,
                }]
            }
        }
        ViewNode::Element(element) => match element.tag.as_str() {
            "column" => vec![PanelNode::Column {
                id: node_id_for(element, context, "column"),
                children: convert_dsl_children(&element.children, context),
            }],
            "row" => vec![PanelNode::Row {
                id: node_id_for(element, context, "row"),
                children: convert_dsl_children(&element.children, context),
            }],
            "section" => vec![PanelNode::Section {
                id: node_id_for(element, context, "section"),
                title: attribute_string(&element.attributes, "title", context)
                    .unwrap_or_else(|| "Section".to_string()),
                children: convert_dsl_children(&element.children, context),
            }],
            "text" => vec![PanelNode::Text {
                id: node_id_for(element, context, "text"),
                text: collect_dsl_text(&element.children, context),
            }],
            "color-preview" => vec![PanelNode::ColorPreview {
                id: node_id_for(element, context, "color-preview"),
                label: attribute_string(&element.attributes, "label", context)
                    .unwrap_or_else(|| collect_dsl_text(&element.children, context)),
                color: attribute_string(&element.attributes, "color", context)
                    .and_then(|value| parse_hex_color(&value))
                    .unwrap_or_default(),
            }],
            "color-wheel" => vec![PanelNode::ColorWheel {
                id: node_id_for(element, context, "color-wheel"),
                label: attribute_string(&element.attributes, "label", context)
                    .unwrap_or_else(|| "Color".to_string()),
                hue_degrees: attribute_usize(&element.attributes, "hue", context).unwrap_or(0)
                    % 360,
                saturation: attribute_usize(&element.attributes, "saturation", context)
                    .unwrap_or(100)
                    .min(100),
                value: attribute_usize(&element.attributes, "value", context)
                    .unwrap_or(100)
                    .min(100),
                action: handler_action(
                    &context.panel_id,
                    element.attributes.get("on:change"),
                    "change",
                ),
            }],
            "button" => vec![PanelNode::Button {
                id: node_id_for(element, context, "button"),
                label: collect_dsl_text(&element.children, context),
                action: handler_action(
                    &context.panel_id,
                    element.attributes.get("on:click"),
                    "click",
                ),
                active: attribute_bool(&element.attributes, "active", context).unwrap_or(false),
                fill_color: None,
            }],
            "toggle" => {
                let checked =
                    attribute_bool(&element.attributes, "checked", context).unwrap_or(false);
                vec![PanelNode::Button {
                    id: node_id_for(element, context, "toggle"),
                    label: format!(
                        "[{}] {}",
                        if checked { "x" } else { " " },
                        collect_dsl_text(&element.children, context)
                    ),
                    action: handler_action(
                        &context.panel_id,
                        element.attributes.get("on:change"),
                        "change",
                    ),
                    active: checked,
                    fill_color: None,
                }]
            }
            "slider" => vec![PanelNode::Slider {
                id: node_id_for(element, context, "slider"),
                label: attribute_string(&element.attributes, "label", context)
                    .unwrap_or_else(|| "Value".to_string()),
                action: handler_action(
                    &context.panel_id,
                    element.attributes.get("on:change"),
                    "change",
                ),
                min: attribute_usize(&element.attributes, "min", context).unwrap_or(0),
                max: attribute_usize(&element.attributes, "max", context).unwrap_or(100),
                value: attribute_usize(&element.attributes, "value", context).unwrap_or(0),
                fill_color: attribute_string(&element.attributes, "fill", context)
                    .and_then(|value| parse_hex_color(&value)),
            }],
            "input" => vec![PanelNode::TextInput {
                id: node_id_for(element, context, "input"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                value: attribute_string(&element.attributes, "value", context).unwrap_or_default(),
                placeholder: attribute_string(&element.attributes, "placeholder", context)
                    .unwrap_or_default(),
                binding_path: attribute_string(&element.attributes, "bind", context)
                    .unwrap_or_default(),
                action: element
                    .attributes
                    .get("on:change")
                    .and_then(DslAttrValue::as_string)
                    .map(|handler_name| HostAction::InvokePanelHandler {
                        panel_id: context.panel_id.clone(),
                        handler_name: handler_name.to_string(),
                        event_kind: "change".to_string(),
                    }),
                input_mode: attribute_string(&element.attributes, "mode", context)
                    .map(|mode| {
                        if mode.eq_ignore_ascii_case("numeric")
                            || mode.eq_ignore_ascii_case("number")
                        {
                            TextInputMode::Numeric
                        } else {
                            TextInputMode::Text
                        }
                    })
                    .unwrap_or(TextInputMode::Text),
            }],
            "dropdown" => vec![PanelNode::Dropdown {
                id: node_id_for(element, context, "dropdown"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                value: attribute_string(&element.attributes, "value", context).unwrap_or_default(),
                action: handler_action(
                    &context.panel_id,
                    element.attributes.get("on:change"),
                    "change",
                ),
                options: attribute_dropdown_options(&element.attributes, "options", context),
            }],
            "layer-list" => vec![PanelNode::LayerList {
                id: node_id_for(element, context, "layer-list"),
                label: attribute_string(&element.attributes, "label", context).unwrap_or_default(),
                selected_index: attribute_usize(&element.attributes, "selected", context)
                    .unwrap_or_default(),
                action: handler_action(
                    &context.panel_id,
                    element.attributes.get("on:change"),
                    "change",
                ),
                items: attribute_layer_list_items(&element.attributes, "items", context),
            }],
            "separator" => vec![PanelNode::Text {
                id: node_id_for(element, context, "separator"),
                text: "────────".to_string(),
            }],
            "spacer" => vec![PanelNode::Text {
                id: node_id_for(element, context, "spacer"),
                text: String::new(),
            }],
            "when" => {
                if attribute_bool(&element.attributes, "test", context).unwrap_or(false) {
                    convert_dsl_children(&element.children, context)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        },
    }
}

/// 現在の値を action へ変換する。
fn handler_action(
    panel_id: &str,
    handler_name: Option<&DslAttrValue>,
    event_kind: &str,
) -> HostAction {
    handler_name
        .and_then(DslAttrValue::as_string)
        .map(|handler_name| HostAction::InvokePanelHandler {
            panel_id: panel_id.to_string(),
            handler_name: handler_name.to_string(),
            event_kind: event_kind.to_string(),
        })
        .unwrap_or(HostAction::DispatchCommand(Command::Noop))
}

/// 既存データを走査して convert dsl children を組み立てる。
fn convert_dsl_children(
    children: &[ViewNode],
    context: &mut DslEvaluationContext<'_>,
) -> Vec<PanelNode> {
    children
        .iter()
        .flat_map(|child| convert_dsl_view_node(child, context))
        .collect()
}

/// 現在の値を dsl テキスト へ変換する。
fn collect_dsl_text(children: &[ViewNode], context: &DslEvaluationContext<'_>) -> String {
    let text = children
        .iter()
        .filter_map(|child| match child {
            ViewNode::Text(text) => Some(evaluate_text_content(text, context)),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() {
        String::from("Unnamed")
    } else {
        text
    }
}

/// evaluate テキスト content を計算して返す。
fn evaluate_text_content(text: &str, context: &DslEvaluationContext<'_>) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            rendered.push_str(&rest[start..]);
            return rendered;
        };
        let expression = after_start[..end].trim();
        rendered.push_str(&expression_to_string(expression, context));
        rest = &after_start[end + 1..];
    }
    if rendered.is_empty() {
        text.to_string()
    } else {
        rendered.push_str(rest);
        rendered
    }
}

/// 既存データを走査して attribute string を組み立てる。
fn attribute_string(
    attributes: &BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<String> {
    attributes
        .get(key)
        .map(|value| attr_value_to_string(value, context))
}

/// 既存データを走査して attribute bool を組み立てる。
fn attribute_bool(
    attributes: &BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<bool> {
    attributes
        .get(key)
        .map(|value| attr_value_to_bool(value, context))
}

/// 現在の値を 値 to string へ変換する。
fn attr_value_to_string(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> String {
    match value {
        DslAttrValue::String(text) => text.clone(),
        DslAttrValue::Integer(number) => number.to_string(),
        DslAttrValue::Float(number) => number.clone(),
        DslAttrValue::Bool(value) => value.to_string(),
        DslAttrValue::Expression(expression) => expression_to_string(expression, context),
    }
}

/// 入力を解析して 値 to bool に変換する。
fn attr_value_to_bool(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> bool {
    match value {
        DslAttrValue::Bool(value) => *value,
        DslAttrValue::Expression(expression) => expression_to_bool(expression, context),
        DslAttrValue::String(text) => !text.is_empty(),
        DslAttrValue::Integer(number) => *number != 0,
        DslAttrValue::Float(number) => number != "0" && number != "0.0",
    }
}

/// 入力を解析して 値 to usize に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn attr_value_to_usize(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> Option<usize> {
    match value {
        DslAttrValue::Integer(number) => usize::try_from(*number).ok(),
        DslAttrValue::Expression(expression) => match evaluate_expression(expression, context) {
            Value::Number(number) => number
                .as_u64()
                .and_then(|value| usize::try_from(value).ok()),
            Value::String(text) => text.parse::<usize>().ok(),
            _ => None,
        },
        DslAttrValue::String(text) => text.parse::<usize>().ok(),
        DslAttrValue::Float(_) | DslAttrValue::Bool(_) => None,
    }
}

/// attribute usize に必要な処理を行う。
fn attribute_usize(
    attributes: &BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Option<usize> {
    attributes
        .get(key)
        .and_then(|value| attr_value_to_usize(value, context))
}

/// 入力を解析して dropdown オプション に変換する。
fn attribute_dropdown_options(
    attributes: &BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Vec<DropdownOption> {
    let Some(raw) = attribute_string(attributes, key, context) else {
        return Vec::new();
    };
    raw.split('|')
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            let (value, label) = item.split_once(':').unwrap_or((item, item));
            Some(DropdownOption {
                label: label.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect()
}

/// attribute レイヤー 一覧 items に必要な処理を行う。
fn attribute_layer_list_items(
    attributes: &BTreeMap<String, DslAttrValue>,
    key: &str,
    context: &DslEvaluationContext<'_>,
) -> Vec<LayerListItem> {
    let Some(value) = attributes.get(key) else {
        return Vec::new();
    };
    layer_list_items_from_value(value, context)
}

/// 入力を解析して 一覧 items from 値 に変換する。
fn layer_list_items_from_value(
    value: &DslAttrValue,
    context: &DslEvaluationContext<'_>,
) -> Vec<LayerListItem> {
    let json_value = match value {
        DslAttrValue::Expression(expression) => evaluate_expression(expression, context),
        DslAttrValue::String(text) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        DslAttrValue::Integer(number) => Value::Number((*number).into()),
        DslAttrValue::Float(number) => number
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        DslAttrValue::Bool(value) => Value::Bool(*value),
    };
    layer_list_items_from_json(&json_value)
}

/// 入力を解析して 一覧 items from JSON に変換する。
fn layer_list_items_from_json(value: &Value) -> Vec<LayerListItem> {
    match value {
        Value::Array(items) => items.iter().filter_map(layer_list_item_from_json).collect(),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .map(|parsed| layer_list_items_from_json(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// 現在の値を 一覧 item from JSON へ変換する。
///
/// 値を生成できない場合は `None` を返します。
fn layer_list_item_from_json(value: &Value) -> Option<LayerListItem> {
    let object = value.as_object()?;
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| object.get("name").and_then(Value::as_str))?
        .to_string();
    let detail = object
        .get("detail")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            let blend_mode = object
                .get("blend_mode")
                .and_then(Value::as_str)
                .unwrap_or("normal");
            let visible = object
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let masked = object
                .get("masked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            format!(
                "合成: {blend_mode} / {} / マスク: {}",
                if visible { "表示" } else { "非表示" },
                if masked { "あり" } else { "なし" }
            )
        });
    Some(LayerListItem { label, detail })
}

/// 現在の値を to string へ変換する。
fn expression_to_string(expression: &str, context: &DslEvaluationContext<'_>) -> String {
    match evaluate_expression(expression, context) {
        Value::String(text) => text,
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 入力を解析して to bool に変換する。
fn expression_to_bool(expression: &str, context: &DslEvaluationContext<'_>) -> bool {
    match evaluate_expression(expression, context) {
        Value::Bool(value) => value,
        Value::String(text) => !text.is_empty() && text != "false",
        Value::Number(number) => number.as_i64().unwrap_or_default() != 0,
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

/// 入力を解析して expression に変換する。
fn evaluate_expression(expression: &str, context: &DslEvaluationContext<'_>) -> Value {
    let expression = expression.trim();
    if let Some(inner) = expression.strip_prefix('!') {
        return Value::Bool(!expression_to_bool(inner, context));
    }
    // || と && は複合 bool 条件 (例: state.a || state.b) に対応するため、
    // != / == よりも先にチェックして優先度を下げる
    if let Some((left, right)) = expression.split_once("||") {
        return Value::Bool(
            expression_to_bool(left.trim(), context) || expression_to_bool(right.trim(), context),
        );
    }
    if let Some((left, right)) = expression.split_once("&&") {
        return Value::Bool(
            expression_to_bool(left.trim(), context) && expression_to_bool(right.trim(), context),
        );
    }
    if let Some((left, right)) = expression.split_once("!=") {
        return Value::Bool(
            evaluate_expression(left.trim(), context) != evaluate_expression(right.trim(), context),
        );
    }
    if let Some((left, right)) = expression.split_once("==") {
        return Value::Bool(
            evaluate_expression(left.trim(), context) == evaluate_expression(right.trim(), context),
        );
    }
    if expression.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if expression.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if expression.starts_with('"') && expression.ends_with('"') && expression.len() >= 2 {
        return Value::String(expression[1..expression.len() - 1].to_string());
    }
    if let Ok(number) = expression.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Some(path) = expression.strip_prefix("state.") {
        return lookup_json_path(context.state, path)
            .cloned()
            .unwrap_or(Value::Null);
    }
    Value::String(expression.to_string())
}

/// 現在の lookup JSON パス を返す。
///
/// 値を生成できない場合は `None` を返します。
fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// 既存データを走査して node ID for を組み立てる。
fn node_id_for(
    element: &ViewElement,
    context: &mut DslEvaluationContext<'_>,
    prefix: &str,
) -> String {
    element
        .attributes
        .get("id")
        .map(|value| attr_value_to_string(value, context))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| next_generated_node_id(context, prefix))
}

/// Generated node ID をひとつ先へ切り替える。
fn next_generated_node_id(context: &mut DslEvaluationContext<'_>, prefix: &str) -> String {
    context.generated_ids += 1;
    format!("dsl.{prefix}.{}", context.generated_ids)
}

/// 現在の 状態 defaults to JSON を返す。
fn state_defaults_to_json(fields: &[StateField]) -> Value {
    let mut state = Value::Object(Map::new());
    for field in fields {
        apply_state_patch(
            &mut state,
            &StatePatch::set(
                field.name.clone(),
                default_attr_value_to_json(&field.default),
            ),
        );
    }
    state
}

/// 既定の attr 値 to JSON を返す。
fn default_attr_value_to_json(value: &DslAttrValue) -> Value {
    match value {
        DslAttrValue::String(text) => Value::String(text.clone()),
        DslAttrValue::Integer(number) => Value::Number((*number).into()),
        DslAttrValue::Float(number) => number
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        DslAttrValue::Bool(value) => Value::Bool(*value),
        DslAttrValue::Expression(_) => Value::Null,
    }
}

/// 状態 patches を現在の状態へ適用する。
fn apply_state_patches(state: &mut Value, patches: &[StatePatch]) {
    if !state.is_object() {
        *state = Value::Object(Map::new());
    }
    for patch in patches {
        apply_state_patch(state, patch);
    }
}

/// 現在の値を 状態 patch へ変換する。
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
                StatePatchOp::Set | StatePatchOp::Replace => {
                    object.insert(
                        segment.to_string(),
                        patch.value.clone().unwrap_or(Value::Null),
                    );
                }
                StatePatchOp::Toggle => {
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

/// 入力や種別に応じて処理を振り分ける。
fn command_descriptors_to_actions(
    commands: Vec<CommandDescriptor>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<HostAction> {
    commands
        .into_iter()
        .filter_map(|descriptor| {
            if let Some(request) = service_request_from_descriptor(&descriptor) {
                return Some(HostAction::RequestService(request));
            }

            match command_from_descriptor(&descriptor) {
                Ok(command) => Some(HostAction::DispatchCommand(command)),
                Err(message) => {
                    diagnostics.push(Diagnostic::warning(message));
                    None
                }
            }
        })
        .collect()
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn service_request_from_descriptor(descriptor: &CommandDescriptor) -> Option<ServiceRequest> {
    use panel_api::services::names;

    let request = match descriptor.name.as_str() {
        names::PROJECT_NEW_DOCUMENT
        | names::PROJECT_NEW_DOCUMENT_SIZED
        | names::PROJECT_SAVE_CURRENT
        | names::PROJECT_SAVE_AS
        | names::PROJECT_SAVE_TO_PATH
        | names::PROJECT_LOAD_DIALOG
        | names::PROJECT_LOAD_FROM_PATH
        | names::WORKSPACE_RELOAD_PRESETS
        | names::WORKSPACE_APPLY_PRESET
        | names::WORKSPACE_SAVE_PRESET
        | names::WORKSPACE_EXPORT_PRESET
        | names::WORKSPACE_EXPORT_PRESET_TO_PATH
        | names::TOOL_CATALOG_RELOAD_TOOLS
        | names::TOOL_CATALOG_RELOAD_PEN_PRESETS
        | names::TOOL_CATALOG_IMPORT_PEN_PRESETS
        | names::TOOL_CATALOG_IMPORT_PEN_PATH
        | names::VIEW_SET_ZOOM
        | names::VIEW_SET_PAN
        | names::VIEW_SET_ROTATION
        | names::VIEW_FLIP_HORIZONTAL
        | names::VIEW_FLIP_VERTICAL
        | names::VIEW_RESET
        | names::PANEL_NAV_ADD
        | names::PANEL_NAV_REMOVE
        | names::PANEL_NAV_SELECT
        | names::PANEL_NAV_SELECT_NEXT
        | names::PANEL_NAV_SELECT_PREVIOUS
        | names::PANEL_NAV_FOCUS_ACTIVE
        | names::HISTORY_UNDO
        | names::HISTORY_REDO => {
            let mut request = ServiceRequest::new(descriptor.name.clone());
            request.payload = descriptor.payload.clone();
            request
        }
        _ => return None,
    };
    Some(request)
}

/// 現在の値を from 記述子 へ変換する。
///
/// 失敗時はエラーを返します。
pub fn command_from_descriptor(descriptor: &CommandDescriptor) -> Result<Command, String> {
    match descriptor.name.as_str() {
        "project.new" => Ok(Command::NewDocument),
        "project.new_sized" => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.new_sized is missing payload.size".to_string())?;
            let (width, height) = parse_document_size(size)
                .ok_or_else(|| format!("invalid project.new_sized payload: {size}"))?;
            Ok(Command::NewDocumentSized { width, height })
        }
        "project.save" => Ok(Command::SaveProject),
        "project.save_as" => Ok(Command::SaveProjectAs),
        "project.save_as_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.save_as_path is missing payload.path".to_string())?;
            Ok(Command::SaveProjectToPath {
                path: path.to_string(),
            })
        }
        "project.load" => Ok(Command::LoadProject),
        "project.load_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "project.load_path is missing payload.path".to_string())?;
            Ok(Command::LoadProjectFromPath {
                path: path.to_string(),
            })
        }
        "workspace.reload_presets" => Ok(Command::ReloadWorkspacePresets),
        "workspace.apply_preset" => {
            let preset_id = descriptor
                .payload
                .get("preset_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "workspace.apply_preset is missing payload.preset_id".to_string())?;
            Ok(Command::ApplyWorkspacePreset {
                preset_id: preset_id.to_string(),
            })
        }
        "workspace.save_preset" => {
            let preset_id = descriptor
                .payload
                .get("preset_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "workspace.save_preset is missing payload.preset_id".to_string())?;
            let label = descriptor
                .payload
                .get("label")
                .and_then(Value::as_str)
                .ok_or_else(|| "workspace.save_preset is missing payload.label".to_string())?;
            Ok(Command::SaveWorkspacePreset {
                preset_id: preset_id.to_string(),
                label: label.to_string(),
            })
        }
        "workspace.export_preset" => {
            let preset_id = descriptor
                .payload
                .get("preset_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "workspace.export_preset is missing payload.preset_id".to_string()
                })?;
            let label = descriptor
                .payload
                .get("label")
                .and_then(Value::as_str)
                .ok_or_else(|| "workspace.export_preset is missing payload.label".to_string())?;
            Ok(Command::ExportWorkspacePreset {
                preset_id: preset_id.to_string(),
                label: label.to_string(),
            })
        }
        "tool.set_active" => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_active is missing payload.tool".to_string())?;
            let tool = match tool {
                "pen" => ToolKind::Pen,
                "eraser" => ToolKind::Eraser,
                "bucket" => ToolKind::Bucket,
                "lasso_bucket" => ToolKind::LassoBucket,
                "panel_rect" => ToolKind::PanelRect,
                other => return Err(format!("unsupported tool kind: {other}")),
            };
            Ok(Command::SetActiveTool { tool })
        }
        "tool.select" => {
            let tool_id = descriptor
                .payload
                .get("tool_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.select is missing payload.tool_id".to_string())?;
            Ok(Command::SelectTool {
                tool_id: tool_id.to_string(),
            })
        }
        "tool.select_child" => {
            let child_id = descriptor
                .payload
                .get("child_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.select_child is missing payload.child_id".to_string())?;
            Ok(Command::SelectChildTool {
                child_id: child_id.to_string(),
            })
        }
        "tool.set_size" => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| "tool.set_size is missing payload.size".to_string())?;
            Ok(Command::SetActivePenSize { size: size as u32 })
        }
        "tool.set_pressure_enabled" => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    "tool.set_pressure_enabled is missing payload.enabled".to_string()
                })?;
            Ok(Command::SetActivePenPressureEnabled { enabled })
        }
        "tool.set_antialias" => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "tool.set_antialias is missing payload.enabled".to_string())?;
            Ok(Command::SetActivePenAntialias { enabled })
        }
        "tool.set_stabilization" => {
            let amount = descriptor
                .payload
                .get("amount")
                .and_then(payload_u64)
                .ok_or_else(|| "tool.set_stabilization is missing payload.amount".to_string())?;
            Ok(Command::SetActivePenStabilization {
                amount: amount.min(100) as u8,
            })
        }
        "tool.pen_next" => Ok(Command::SelectNextPenPreset),
        "tool.pen_prev" => Ok(Command::SelectPreviousPenPreset),
        "tool.reload_pen_presets" => Ok(Command::ReloadPenPresets),
        "tool.import_pen_presets" => Ok(Command::ImportPenPresets),
        "tool.import_pen_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.import_pen_path is missing payload.path".to_string())?;
            Ok(Command::ImportPenPresetsFromPath {
                path: path.to_string(),
            })
        }
        "tool.set_color" => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_color is missing payload.color".to_string())?;
            parse_hex_color(color)
                .map(|color| Command::SetActiveColor { color })
                .ok_or_else(|| format!("invalid color payload: {color}"))
        }
        "layer.add" => Ok(Command::AddRasterLayer),
        "layer.remove" => Ok(Command::RemoveActiveLayer),
        "layer.select" => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.select is missing payload.index".to_string())?;
            Ok(Command::SelectLayer {
                index: index as usize,
            })
        }
        "layer.rename_active" => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.rename_active is missing payload.name".to_string())?;
            Ok(Command::RenameActiveLayer {
                name: name.to_string(),
            })
        }
        "layer.move" => {
            let from_index = descriptor
                .payload
                .get("from_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.from_index".to_string())?;
            let to_index = descriptor
                .payload
                .get("to_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.to_index".to_string())?;
            Ok(Command::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            })
        }
        "layer.select_next" => Ok(Command::SelectNextLayer),
        "layer.cycle_blend_mode" => Ok(Command::CycleActiveLayerBlendMode),
        "layer.set_blend_mode" => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.set_blend_mode is missing payload.mode".to_string())?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(Command::SetActiveLayerBlendMode { mode })
        }
        "layer.toggle_visibility" => Ok(Command::ToggleActiveLayerVisibility),
        "layer.toggle_mask" => Ok(Command::ToggleActiveLayerMask),
        "panel.add" => Ok(Command::AddPanel),
        "panel.remove" => Ok(Command::RemoveActivePanel),
        "panel.select" => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| "panel.select is missing payload.index".to_string())?;
            Ok(Command::SelectPanel {
                index: index as usize,
            })
        }
        "panel.select_next" => Ok(Command::SelectNextPanel),
        "panel.select_previous" => Ok(Command::SelectPreviousPanel),
        "panel.focus_active" => Ok(Command::FocusActivePanel),
        "view.reset" => Ok(Command::ResetView),
        "view.zoom" => {
            let zoom = descriptor
                .payload
                .get("zoom")
                .and_then(payload_f64)
                .ok_or_else(|| "view.zoom is missing payload.zoom".to_string())?;
            Ok(Command::SetViewZoom { zoom: zoom as f32 })
        }
        "view.pan" => {
            let delta_x = descriptor
                .payload
                .get("delta_x")
                .and_then(payload_f64)
                .ok_or_else(|| "view.pan is missing payload.delta_x".to_string())?;
            let delta_y = descriptor
                .payload
                .get("delta_y")
                .and_then(payload_f64)
                .ok_or_else(|| "view.pan is missing payload.delta_y".to_string())?;
            Ok(Command::PanView {
                delta_x: delta_x as f32,
                delta_y: delta_y as f32,
            })
        }
        "view.set_pan" => {
            let pan_x = descriptor
                .payload
                .get("pan_x")
                .and_then(payload_f64)
                .ok_or_else(|| "view.set_pan is missing payload.pan_x".to_string())?;
            let pan_y = descriptor
                .payload
                .get("pan_y")
                .and_then(payload_f64)
                .ok_or_else(|| "view.set_pan is missing payload.pan_y".to_string())?;
            Ok(Command::SetViewPan {
                pan_x: pan_x as f32,
                pan_y: pan_y as f32,
            })
        }
        "view.rotate" => {
            let quarter_turns = descriptor
                .payload
                .get("quarter_turns")
                .and_then(payload_i32)
                .ok_or_else(|| "view.rotate is missing payload.quarter_turns".to_string())?;
            Ok(Command::RotateView { quarter_turns })
        }
        "view.set_rotation" => {
            let rotation_degrees = descriptor
                .payload
                .get("rotation_degrees")
                .and_then(payload_f64)
                .ok_or_else(|| {
                    "view.set_rotation is missing payload.rotation_degrees".to_string()
                })?;
            Ok(Command::SetViewRotation {
                rotation_degrees: rotation_degrees as f32,
            })
        }
        "view.flip_horizontal" => Ok(Command::FlipViewHorizontally),
        "view.flip_vertical" => Ok(Command::FlipViewVertically),
        other => Err(format!("unsupported command descriptor: {other}")),
    }
}

/// 入力を解析して u64 に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

/// 入力を解析して i32 に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn payload_i32(value: &Value) -> Option<i32> {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .or_else(|| value.as_u64().and_then(|number| i32::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<i32>().ok()))
}

/// 入力を解析して f64 に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn payload_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn find_panel_action(nodes: &[PanelNode], target_id: &str) -> Option<HostAction> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(action) = find_panel_action(children, target_id) {
                    return Some(action);
                }
            }
            PanelNode::ColorWheel { id, action, .. } if id == target_id => {
                return Some(action.clone());
            }
            PanelNode::Button { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::Slider { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::TextInput {
                id,
                action: Some(action),
                ..
            } if id == target_id => return Some(action.clone()),
            PanelNode::Dropdown { id, action, .. } if id == target_id => {
                return Some(action.clone());
            }
            PanelNode::LayerList { id, action, .. } if id == target_id => {
                return Some(action.clone());
            }
            _ => {}
        }
    }
    None
}

/// 入力や種別に応じて処理を振り分ける。
fn find_text_input_binding(
    nodes: &[PanelNode],
    target_id: &str,
) -> Option<(String, TextInputMode)> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(binding) = find_text_input_binding(children, target_id) {
                    return Some(binding);
                }
            }
            PanelNode::TextInput {
                id,
                binding_path,
                input_mode,
                ..
            } if id == target_id => {
                if binding_path.is_empty() {
                    return None;
                }
                return Some((binding_path.clone(), *input_mode));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_api::services::names;
    use panel_schema::CommandDescriptor;

    /// expression_to_bool が || と && 演算子を正しく評価することを検証する。
    #[test]
    fn expression_evaluator_supports_or_and_and_operators() {
        let state_pressure_only = serde_json::json!({
            "supports_pressure": true,
            "supports_antialias": false,
            "supports_stabilization": false,
        });
        let ctx = DslEvaluationContext {
            panel_id: "test".to_string(),
            state: &state_pressure_only,
            generated_ids: 0,
        };

        // true || false = true
        assert!(
            expression_to_bool("state.supports_pressure || state.supports_antialias", &ctx),
            "true || false should be true"
        );

        // false || false = false
        assert!(
            !expression_to_bool(
                "state.supports_antialias || state.supports_stabilization",
                &ctx
            ),
            "false || false should be false"
        );

        // three-way OR: false || false || true via chaining
        let state_abc = serde_json::json!({ "a": false, "b": false, "c": true });
        let ctx2 = DslEvaluationContext {
            panel_id: "test".to_string(),
            state: &state_abc,
            generated_ids: 0,
        };
        assert!(
            expression_to_bool("state.a || state.b || state.c", &ctx2),
            "false || false || true should be true"
        );

        // && operator: true && false = false
        let state_xy_false = serde_json::json!({ "x": true, "y": false });
        let ctx3 = DslEvaluationContext {
            panel_id: "test".to_string(),
            state: &state_xy_false,
            generated_ids: 0,
        };
        assert!(
            !expression_to_bool("state.x && state.y", &ctx3),
            "true && false should be false"
        );

        // && operator: true && true = true
        let state_xy_true = serde_json::json!({ "x": true, "y": true });
        let ctx4 = DslEvaluationContext {
            panel_id: "test".to_string(),
            state: &state_xy_true,
            generated_ids: 0,
        };
        assert!(
            expression_to_bool("state.x && state.y", &ctx4),
            "true && true should be true"
        );
    }

    /// コマンド from 記述子 maps ワークスペース commands が期待どおりに動作することを検証する。
    #[test]
    fn command_from_descriptor_maps_workspace_commands() {
        assert_eq!(
            command_from_descriptor(&CommandDescriptor::new("workspace.reload_presets")),
            Ok(Command::ReloadWorkspacePresets)
        );

        let mut descriptor = CommandDescriptor::new("workspace.apply_preset");
        descriptor.payload.insert(
            "preset_id".to_string(),
            Value::String("illustration".to_string()),
        );

        assert_eq!(
            command_from_descriptor(&descriptor),
            Ok(Command::ApplyWorkspacePreset {
                preset_id: "illustration".to_string(),
            })
        );
    }

    /// サービス 記述子 maps to サービス 要求 が期待どおりに動作することを検証する。
    #[test]
    fn service_descriptor_maps_to_service_request() {
        let mut descriptor = CommandDescriptor::new(names::WORKSPACE_APPLY_PRESET);
        descriptor
            .payload
            .insert("preset_id".to_string(), Value::String("review".to_string()));

        assert_eq!(
            service_request_from_descriptor(&descriptor),
            Some(
                ServiceRequest::new(names::WORKSPACE_APPLY_PRESET)
                    .with_value("preset_id", Value::String("review".to_string()),)
            )
        );
    }
}
