//! DSL panel の読込・評価・runtime bridge をまとめる。
//!
//! `.altp-panel` の発見、DSL -> `PanelTree` 変換、Wasm runtime 呼び出し、
//! state patch / command bridge をこの module に閉じ込める。

use super::tree_query::{find_panel_action, find_text_input_binding};
use super::*;

pub(super) const MAX_DOCUMENT_DIMENSION: usize = 8192;
pub(super) const MAX_DOCUMENT_PIXELS: usize = 16_777_216;

/// 指定ディレクトリ以下から `.altp-panel` を再帰収集する。
pub(super) fn collect_panel_files_recursive(
    directory: &Path,
    panel_files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_panel_files_recursive(&path, panel_files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("altp-panel") {
            panel_files.push(path);
        }
    }
    Ok(())
}

pub(super) struct DslPanelPlugin {
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
    pub(super) fn from_definition(definition: PanelDefinition) -> Result<Self, String> {
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

    fn resolve_handler_action(&self, event: &PanelEvent) -> Option<HostAction> {
        let tree = self.evaluate_tree();
        match event {
            PanelEvent::Activate { node_id, .. }
            | PanelEvent::SetValue { node_id, .. }
            | PanelEvent::DragValue { node_id, .. }
            | PanelEvent::SetText { node_id, .. } => find_panel_action(&tree.children, node_id),
            PanelEvent::Keyboard { .. } if self.has_keyboard_handler => Some(HostAction::InvokePanelHandler {
                panel_id: self.id.to_string(),
                handler_name: "keyboard".to_string(),
                event_kind: "keyboard".to_string(),
            }),
            PanelEvent::Keyboard { .. } => None,
        }
    }

    fn apply_text_input_event(&mut self, node_id: &str, value: &str) -> bool {
        let tree = self.evaluate_tree();
        let Some((binding, input_mode)) = find_text_input_binding(&tree.children, node_id) else {
            return false;
        };
        let _ = input_mode;
        let next_value = Value::String(value.to_string());
        apply_state_patch(&mut self.state, &StatePatch::set(binding, next_value));
        true
    }

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
    fn id(&self) -> &'static str { self.id }
    fn title(&self) -> &'static str { self.title }

    fn update(&mut self, document: &Document) {
        self.host_snapshot = build_host_snapshot(document);
        self.sync_host_state();
    }

    fn debug_summary(&self) -> String {
        format!(
            "dsl runtime={} handlers={} diagnostics={} state={}",
            self.runtime.path().display(),
            self.definition.handler_bindings.len(),
            self.diagnostics.len(),
            self.state
        )
    }

    fn view(&self) -> PanelView {
        let mut lines = vec![format!("runtime: {}", self.definition.runtime.wasm)];
        if !self.diagnostics.is_empty() {
            lines.push(format!("diagnostics: {}", self.diagnostics.len()));
        }
        PanelView { id: self.id, title: self.title, lines }
    }

    fn panel_tree(&self) -> PanelTree { self.evaluate_tree() }
    fn handles_keyboard_event(&self) -> bool { self.has_keyboard_handler }
    fn persistent_config(&self) -> Option<Value> { lookup_json_path(&self.state, "config").cloned() }

    fn restore_persistent_config(&mut self, config: &Value) {
        apply_state_patch(&mut self.state, &StatePatch::replace("config", config.clone()));
    }

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        let mut updated_text = false;
        if let PanelEvent::SetText { node_id, value, .. } = event {
            updated_text = self.apply_text_input_event(node_id, value);
        }
        let Some(HostAction::InvokePanelHandler { panel_id, handler_name, event_kind }) = self.resolve_handler_action(event) else {
            if updated_text { self.diagnostics.clear(); }
            return Vec::new();
        };
        if panel_id != self.id { return Vec::new(); }

        let event_payload = match event {
            PanelEvent::Activate { .. } => json!({}),
            PanelEvent::SetValue { value, .. } => json!({ "value": value }),
            PanelEvent::DragValue { from, to, .. } => json!({ "from": from.to_string(), "to": to.to_string(), "value": to }),
            PanelEvent::SetText { value, .. } => json!({ "value": value }),
            PanelEvent::Keyboard { shortcut, key, repeat, .. } => json!({ "shortcut": shortcut, "key": key, "repeat": repeat }),
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

fn leak_string(value: String) -> &'static str { Box::leak(value.into_boxed_str()) }

struct DslEvaluationContext<'a> {
    panel_id: String,
    state: &'a Value,
    generated_ids: usize,
}

fn convert_dsl_view_node(node: &ViewNode, context: &mut DslEvaluationContext<'_>) -> Vec<PanelNode> {
    match node {
        ViewNode::Text(text) => {
            let text = evaluate_text_content(text, context);
            if text.is_empty() { Vec::new() } else { vec![PanelNode::Text { id: next_generated_node_id(context, "text"), text }] }
        }
        ViewNode::Element(element) => match element.tag.as_str() {
            "column" => vec![PanelNode::Column { id: node_id_for(element, context, "column"), children: convert_dsl_children(&element.children, context) }],
            "row" => vec![PanelNode::Row { id: node_id_for(element, context, "row"), children: convert_dsl_children(&element.children, context) }],
            "section" => vec![PanelNode::Section { id: node_id_for(element, context, "section"), title: attribute_string(&element.attributes, "title", context).unwrap_or_else(|| "Section".to_string()), children: convert_dsl_children(&element.children, context) }],
            "text" => vec![PanelNode::Text { id: node_id_for(element, context, "text"), text: collect_dsl_text(&element.children, context) }],
            "color-preview" => vec![PanelNode::ColorPreview { id: node_id_for(element, context, "color-preview"), label: attribute_string(&element.attributes, "label", context).unwrap_or_else(|| collect_dsl_text(&element.children, context)), color: attribute_string(&element.attributes, "color", context).and_then(|value| parse_hex_color(&value)).unwrap_or_default() }],
            "button" => vec![PanelNode::Button { id: node_id_for(element, context, "button"), label: collect_dsl_text(&element.children, context), action: element.attributes.get("on:click").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "click".to_string() }).unwrap_or(HostAction::DispatchCommand(Command::Noop)), active: attribute_bool(&element.attributes, "active", context).unwrap_or(false), fill_color: None }],
            "toggle" => {
                let checked = attribute_bool(&element.attributes, "checked", context).unwrap_or(false);
                vec![PanelNode::Button { id: node_id_for(element, context, "toggle"), label: format!("[{}] {}", if checked { "x" } else { " " }, collect_dsl_text(&element.children, context)), action: element.attributes.get("on:change").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "change".to_string() }).unwrap_or(HostAction::DispatchCommand(Command::Noop)), active: checked, fill_color: None }]
            }
            "slider" => vec![PanelNode::Slider { id: node_id_for(element, context, "slider"), label: attribute_string(&element.attributes, "label", context).unwrap_or_else(|| "Value".to_string()), action: element.attributes.get("on:change").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "change".to_string() }).unwrap_or(HostAction::DispatchCommand(Command::Noop)), min: attribute_usize(&element.attributes, "min", context).unwrap_or(0), max: attribute_usize(&element.attributes, "max", context).unwrap_or(100), value: attribute_usize(&element.attributes, "value", context).unwrap_or(0), fill_color: attribute_string(&element.attributes, "fill", context).and_then(|value| parse_hex_color(&value)) }],
            "input" => vec![PanelNode::TextInput { id: node_id_for(element, context, "input"), label: attribute_string(&element.attributes, "label", context).unwrap_or_default(), value: attribute_string(&element.attributes, "value", context).unwrap_or_default(), placeholder: attribute_string(&element.attributes, "placeholder", context).unwrap_or_default(), binding_path: attribute_string(&element.attributes, "bind", context).unwrap_or_default(), action: element.attributes.get("on:change").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "change".to_string() }), input_mode: attribute_string(&element.attributes, "mode", context).map(|mode| if mode.eq_ignore_ascii_case("numeric") || mode.eq_ignore_ascii_case("number") { TextInputMode::Numeric } else { TextInputMode::Text }).unwrap_or(TextInputMode::Text) }],
            "dropdown" => vec![PanelNode::Dropdown { id: node_id_for(element, context, "dropdown"), label: attribute_string(&element.attributes, "label", context).unwrap_or_default(), value: attribute_string(&element.attributes, "value", context).unwrap_or_default(), action: element.attributes.get("on:change").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "change".to_string() }).unwrap_or(HostAction::DispatchCommand(Command::Noop)), options: attribute_dropdown_options(&element.attributes, "options", context) }],
            "layer-list" => vec![PanelNode::LayerList { id: node_id_for(element, context, "layer-list"), label: attribute_string(&element.attributes, "label", context).unwrap_or_default(), selected_index: attribute_usize(&element.attributes, "selected", context).unwrap_or_default(), action: element.attributes.get("on:change").and_then(DslAttrValue::as_string).map(|handler_name| HostAction::InvokePanelHandler { panel_id: context.panel_id.clone(), handler_name: handler_name.to_string(), event_kind: "change".to_string() }).unwrap_or(HostAction::DispatchCommand(Command::Noop)), items: attribute_layer_list_items(&element.attributes, "items", context) }],
            "separator" => vec![PanelNode::Text { id: node_id_for(element, context, "separator"), text: "────────".to_string() }],
            "spacer" => vec![PanelNode::Text { id: node_id_for(element, context, "spacer"), text: String::new() }],
            "when" => if attribute_bool(&element.attributes, "test", context).unwrap_or(false) { convert_dsl_children(&element.children, context) } else { Vec::new() },
            _ => Vec::new(),
        },
    }
}

fn convert_dsl_children(children: &[ViewNode], context: &mut DslEvaluationContext<'_>) -> Vec<PanelNode> {
    children.iter().flat_map(|child| convert_dsl_view_node(child, context)).collect()
}

fn collect_dsl_text(children: &[ViewNode], context: &DslEvaluationContext<'_>) -> String {
    let text = children
        .iter()
        .filter_map(|child| match child { ViewNode::Text(text) => Some(evaluate_text_content(text, context)), _ => None })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if text.is_empty() { String::from("Unnamed") } else { text }
}

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
    if rendered.is_empty() { text.to_string() } else { rendered.push_str(rest); rendered }
}

fn attribute_string(attributes: &std::collections::BTreeMap<String, DslAttrValue>, key: &str, context: &DslEvaluationContext<'_>) -> Option<String> {
    attributes.get(key).map(|value| attr_value_to_string(value, context))
}
fn attribute_bool(attributes: &std::collections::BTreeMap<String, DslAttrValue>, key: &str, context: &DslEvaluationContext<'_>) -> Option<bool> {
    attributes.get(key).map(|value| attr_value_to_bool(value, context))
}
fn attr_value_to_string(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> String { match value { DslAttrValue::String(text) => text.clone(), DslAttrValue::Integer(number) => number.to_string(), DslAttrValue::Float(number) => number.clone(), DslAttrValue::Bool(value) => value.to_string(), DslAttrValue::Expression(expression) => expression_to_string(expression, context), } }
fn attr_value_to_bool(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> bool { match value { DslAttrValue::Bool(value) => *value, DslAttrValue::Expression(expression) => expression_to_bool(expression, context), DslAttrValue::String(text) => !text.is_empty(), DslAttrValue::Integer(number) => *number != 0, DslAttrValue::Float(number) => number != "0" && number != "0.0", } }
fn attr_value_to_usize(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> Option<usize> { match value { DslAttrValue::Integer(number) => usize::try_from(*number).ok(), DslAttrValue::Expression(expression) => match evaluate_expression(expression, context) { Value::Number(number) => number.as_u64().and_then(|value| usize::try_from(value).ok()), Value::String(text) => text.parse::<usize>().ok(), _ => None }, DslAttrValue::String(text) => text.parse::<usize>().ok(), DslAttrValue::Float(_) | DslAttrValue::Bool(_) => None, } }
fn attribute_usize(attributes: &std::collections::BTreeMap<String, DslAttrValue>, key: &str, context: &DslEvaluationContext<'_>) -> Option<usize> { attributes.get(key).and_then(|value| attr_value_to_usize(value, context)) }
fn attribute_dropdown_options(attributes: &std::collections::BTreeMap<String, DslAttrValue>, key: &str, context: &DslEvaluationContext<'_>) -> Vec<DropdownOption> {
    let Some(raw) = attribute_string(attributes, key, context) else { return Vec::new(); };
    raw.split('|').filter_map(|item| {
        let item = item.trim();
        if item.is_empty() { return None; }
        let (value, label) = item.split_once(':').unwrap_or((item, item));
        Some(DropdownOption { label: label.trim().to_string(), value: value.trim().to_string() })
    }).collect()
}
fn attribute_layer_list_items(attributes: &std::collections::BTreeMap<String, DslAttrValue>, key: &str, context: &DslEvaluationContext<'_>) -> Vec<LayerListItem> {
    let Some(value) = attributes.get(key) else { return Vec::new(); };
    layer_list_items_from_value(value, context)
}
fn layer_list_items_from_value(value: &DslAttrValue, context: &DslEvaluationContext<'_>) -> Vec<LayerListItem> {
    let json_value = match value {
        DslAttrValue::Expression(expression) => evaluate_expression(expression, context),
        DslAttrValue::String(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone())),
        DslAttrValue::Integer(number) => Value::Number((*number).into()),
        DslAttrValue::Float(number) => number.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null),
        DslAttrValue::Bool(value) => Value::Bool(*value),
    };
    layer_list_items_from_json(&json_value)
}
fn layer_list_items_from_json(value: &Value) -> Vec<LayerListItem> { match value { Value::Array(items) => items.iter().filter_map(layer_list_item_from_json).collect(), Value::String(text) => serde_json::from_str::<Value>(text).ok().map(|parsed| layer_list_items_from_json(&parsed)).unwrap_or_default(), _ => Vec::new(), } }
fn layer_list_item_from_json(value: &Value) -> Option<LayerListItem> {
    let object = value.as_object()?;
    let label = object.get("label").and_then(Value::as_str).or_else(|| object.get("name").and_then(Value::as_str))?.to_string();
    let detail = object.get("detail").and_then(Value::as_str).map(ToString::to_string).unwrap_or_else(|| {
        let blend_mode = object.get("blend_mode").and_then(Value::as_str).unwrap_or("normal");
        let visible = object.get("visible").and_then(Value::as_bool).unwrap_or(true);
        let masked = object.get("masked").and_then(Value::as_bool).unwrap_or(false);
        format!("blend: {blend_mode} / {} / mask: {}", if visible { "visible" } else { "hidden" }, masked)
    });
    Some(LayerListItem { label, detail })
}
fn expression_to_string(expression: &str, context: &DslEvaluationContext<'_>) -> String { match evaluate_expression(expression, context) { Value::String(text) => text, Value::Bool(value) => value.to_string(), Value::Number(value) => value.to_string(), Value::Null => String::new(), other => other.to_string(), } }
fn expression_to_bool(expression: &str, context: &DslEvaluationContext<'_>) -> bool { match evaluate_expression(expression, context) { Value::Bool(value) => value, Value::String(text) => !text.is_empty() && text != "false", Value::Number(number) => number.as_i64().unwrap_or_default() != 0, Value::Null => false, Value::Array(items) => !items.is_empty(), Value::Object(object) => !object.is_empty(), } }
fn evaluate_expression(expression: &str, context: &DslEvaluationContext<'_>) -> Value {
    let expression = expression.trim();
    if let Some(inner) = expression.strip_prefix('!') { return Value::Bool(!expression_to_bool(inner, context)); }
    if let Some((left, right)) = expression.split_once("!=") { return Value::Bool(evaluate_expression(left.trim(), context) != evaluate_expression(right.trim(), context)); }
    if let Some((left, right)) = expression.split_once("==") { return Value::Bool(evaluate_expression(left.trim(), context) == evaluate_expression(right.trim(), context)); }
    if expression.eq_ignore_ascii_case("true") { return Value::Bool(true); }
    if expression.eq_ignore_ascii_case("false") { return Value::Bool(false); }
    if expression.starts_with('"') && expression.ends_with('"') && expression.len() >= 2 { return Value::String(expression[1..expression.len() - 1].to_string()); }
    if let Ok(number) = expression.parse::<i64>() { return Value::Number(number.into()); }
    if let Some(path) = expression.strip_prefix("state.") { return lookup_json_path(context.state, path).cloned().unwrap_or(Value::Null); }
    Value::String(expression.to_string())
}
fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> { let mut current = value; for segment in path.split('.') { current = current.get(segment)?; } Some(current) }
fn node_id_for(element: &ViewElement, context: &mut DslEvaluationContext<'_>, prefix: &str) -> String { element.attributes.get("id").map(|value| attr_value_to_string(value, context)).filter(|value| !value.is_empty()).unwrap_or_else(|| next_generated_node_id(context, prefix)) }
fn next_generated_node_id(context: &mut DslEvaluationContext<'_>, prefix: &str) -> String { context.generated_ids += 1; format!("dsl.{prefix}.{}", context.generated_ids) }
fn state_defaults_to_json(fields: &[StateField]) -> Value {
    let mut state = Value::Object(Map::new());
    for field in fields { apply_state_patch(&mut state, &StatePatch::set(field.name.clone(), default_attr_value_to_json(&field.default))); }
    state
}
fn default_attr_value_to_json(value: &DslAttrValue) -> Value { match value { DslAttrValue::String(text) => Value::String(text.clone()), DslAttrValue::Integer(number) => Value::Number((*number).into()), DslAttrValue::Float(number) => number.parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null), DslAttrValue::Bool(value) => Value::Bool(*value), DslAttrValue::Expression(_) => Value::Null, } }
fn build_host_snapshot(document: &Document) -> Value {
    let active_panel = document.work.pages.first().and_then(|page| page.panels.first());
    let layers = active_panel.map(|panel| panel.layers.iter().map(|layer| json!({ "name": layer.name, "blend_mode": layer.blend_mode.as_str(), "visible": layer.visible, "masked": layer.mask.is_some() })).collect::<Vec<_>>()).unwrap_or_else(|| vec![json!({ "name": "Layer 1", "blend_mode": "normal", "visible": true, "masked": false })]);
    let layers_json = serde_json::to_string(&layers).unwrap_or_else(|_| "[]".to_string());
    let layer_count = active_panel.map(|panel| panel.layers.len()).unwrap_or(1);
    let active_layer_index = active_panel.map(|panel| panel.active_layer_index).unwrap_or(0);
    let active_layer = active_panel.and_then(|panel| panel.layers.get(panel.active_layer_index));
    let page_count = document.work.pages.len();
    let panel_count = document.work.pages.iter().map(|page| page.panels.len()).sum::<usize>();
    let active_layer_name = active_layer.map(|layer| layer.name.clone()).unwrap_or_else(|| "<no layer>".to_string());
    let active_pen = document.active_pen_preset().cloned().unwrap_or_default();
    json!({
        "document": {
            "title": document.work.title,
            "page_count": page_count,
            "panel_count": panel_count,
            "active_layer_name": active_layer_name,
            "layer_count": layer_count,
            "active_layer_index": active_layer_index,
            "active_layer_blend_mode": active_layer.map(|layer| layer.blend_mode.as_str()).unwrap_or("normal"),
            "active_layer_visible": active_layer.map(|layer| layer.visible).unwrap_or(true),
            "active_layer_masked": active_layer.and_then(|layer| layer.mask.as_ref()).is_some(),
            "layers": layers,
            "layers_json": layers_json,
        },
        "tool": {
            "active": match document.active_tool { ToolKind::Brush => "brush", ToolKind::Pen => "pen", ToolKind::Eraser => "eraser" },
            "pen_name": active_pen.name,
            "pen_id": active_pen.id,
            "pen_index": document.active_pen_index(),
            "pen_count": document.pen_presets.len(),
            "pen_size": document.active_pen_size,
        },
        "color": {
            "active": document.active_color.hex_rgb(),
            "red": document.active_color.r,
            "green": document.active_color.g,
            "blue": document.active_color.b,
        },
        "jobs": { "active": 0, "queued": 0, "status": format!("idle / work={}", document.work.title) },
        "snapshot": { "storage_status": "pending" },
        "view": {
            "zoom": document.view_transform.zoom,
            "zoom_milli": (document.view_transform.zoom * 1000.0).round() as i32,
            "pan_x": document.view_transform.pan_x.round() as i32,
            "pan_y": document.view_transform.pan_y.round() as i32,
            "quarter_turns": ((document.view_transform.rotation_degrees / 90.0).round() as i32).rem_euclid(4),
            "flip_x": document.view_transform.flip_x,
            "flip_y": document.view_transform.flip_y,
        },
    })
}
fn apply_state_patches(state: &mut Value, patches: &[StatePatch]) { if !state.is_object() { *state = Value::Object(Map::new()); } for patch in patches { apply_state_patch(state, patch); } }
fn apply_state_patch(state: &mut Value, patch: &StatePatch) {
    let mut current = state;
    let mut segments = patch.path.split('.').peekable();
    while let Some(segment) = segments.next() {
        let is_last = segments.peek().is_none();
        if !current.is_object() { *current = Value::Object(Map::new()); }
        let object = current.as_object_mut().expect("object ensured");
        if is_last {
            match patch.op {
                panel_schema::StatePatchOp::Set | panel_schema::StatePatchOp::Replace => { object.insert(segment.to_string(), patch.value.clone().unwrap_or(Value::Null)); }
                panel_schema::StatePatchOp::Toggle => {
                    let next = !object.get(segment).and_then(Value::as_bool).unwrap_or(false);
                    object.insert(segment.to_string(), Value::Bool(next));
                }
            }
            return;
        }
        current = object.entry(segment.to_string()).or_insert_with(|| Value::Object(Map::new()));
    }
}
fn command_descriptors_to_actions(commands: Vec<CommandDescriptor>, diagnostics: &mut Vec<Diagnostic>) -> Vec<HostAction> {
    commands.into_iter().filter_map(|descriptor| match command_from_descriptor(&descriptor) { Ok(command) => Some(HostAction::DispatchCommand(command)), Err(message) => { diagnostics.push(Diagnostic::warning(message)); None } }).collect()
}
pub(super) fn command_from_descriptor(descriptor: &CommandDescriptor) -> Result<Command, String> {
    match descriptor.name.as_str() {
        "project.new" => Ok(Command::NewDocument),
        "project.new_sized" => {
            let size = descriptor.payload.get("size").and_then(Value::as_str).ok_or_else(|| "project.new_sized is missing payload.size".to_string())?;
            let (width, height) = parse_document_size(size).ok_or_else(|| format!("invalid project.new_sized payload: {size}"))?;
            Ok(Command::NewDocumentSized { width, height })
        }
        "project.save" => Ok(Command::SaveProject),
        "project.save_as" => Ok(Command::SaveProjectAs),
        "project.save_as_path" => {
            let path = descriptor.payload.get("path").and_then(Value::as_str).ok_or_else(|| "project.save_as_path is missing payload.path".to_string())?;
            Ok(Command::SaveProjectToPath { path: path.to_string() })
        }
        "project.load" => Ok(Command::LoadProject),
        "project.load_path" => {
            let path = descriptor.payload.get("path").and_then(Value::as_str).ok_or_else(|| "project.load_path is missing payload.path".to_string())?;
            Ok(Command::LoadProjectFromPath { path: path.to_string() })
        }
        "tool.set_active" => {
            let tool = descriptor.payload.get("tool").and_then(Value::as_str).ok_or_else(|| "tool.set_active is missing payload.tool".to_string())?;
            let tool = match tool { "brush" => ToolKind::Brush, "pen" => ToolKind::Pen, "eraser" => ToolKind::Eraser, other => return Err(format!("unsupported tool kind: {other}")), };
            Ok(Command::SetActiveTool { tool })
        }
        "tool.set_size" => { let size = descriptor.payload.get("size").and_then(payload_u64).ok_or_else(|| "tool.set_size is missing payload.size".to_string())?; Ok(Command::SetActivePenSize { size: size as u32 }) }
        "tool.pen_next" => Ok(Command::SelectNextPenPreset),
        "tool.pen_prev" => Ok(Command::SelectPreviousPenPreset),
        "tool.reload_pen_presets" => Ok(Command::ReloadPenPresets),
        "tool.set_color" => {
            let color = descriptor.payload.get("color").and_then(Value::as_str).ok_or_else(|| "tool.set_color is missing payload.color".to_string())?;
            parse_hex_color(color).map(|color| Command::SetActiveColor { color }).ok_or_else(|| format!("invalid color payload: {color}"))
        }
        "layer.add" => Ok(Command::AddRasterLayer),
        "layer.remove" => Ok(Command::RemoveActiveLayer),
        "layer.select" => { let index = descriptor.payload.get("index").and_then(payload_u64).ok_or_else(|| "layer.select is missing payload.index".to_string())?; Ok(Command::SelectLayer { index: index as usize }) }
        "layer.rename_active" => { let name = descriptor.payload.get("name").and_then(Value::as_str).ok_or_else(|| "layer.rename_active is missing payload.name".to_string())?; Ok(Command::RenameActiveLayer { name: name.to_string() }) }
        "layer.move" => {
            let from_index = descriptor.payload.get("from_index").and_then(payload_u64).ok_or_else(|| "layer.move is missing payload.from_index".to_string())?;
            let to_index = descriptor.payload.get("to_index").and_then(payload_u64).ok_or_else(|| "layer.move is missing payload.to_index".to_string())?;
            Ok(Command::MoveLayer { from_index: from_index as usize, to_index: to_index as usize })
        }
        "layer.select_next" => Ok(Command::SelectNextLayer),
        "layer.cycle_blend_mode" => Ok(Command::CycleActiveLayerBlendMode),
        "layer.set_blend_mode" => {
            let mode = descriptor.payload.get("mode").and_then(Value::as_str).ok_or_else(|| "layer.set_blend_mode is missing payload.mode".to_string())?;
            let mode = app_core::BlendMode::parse_name(mode).ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(Command::SetActiveLayerBlendMode { mode })
        }
        "layer.toggle_visibility" => Ok(Command::ToggleActiveLayerVisibility),
        "layer.toggle_mask" => Ok(Command::ToggleActiveLayerMask),
        "view.reset" => Ok(Command::ResetView),
        "view.zoom" => { let zoom = descriptor.payload.get("zoom").and_then(payload_f64).ok_or_else(|| "view.zoom is missing payload.zoom".to_string())?; Ok(Command::SetViewZoom { zoom: zoom as f32 }) }
        "view.pan" => {
            let delta_x = descriptor.payload.get("delta_x").and_then(payload_f64).ok_or_else(|| "view.pan is missing payload.delta_x".to_string())?;
            let delta_y = descriptor.payload.get("delta_y").and_then(payload_f64).ok_or_else(|| "view.pan is missing payload.delta_y".to_string())?;
            Ok(Command::PanView { delta_x: delta_x as f32, delta_y: delta_y as f32 })
        }
        "view.rotate" => {
            let quarter_turns = descriptor.payload.get("quarter_turns").and_then(payload_i32).ok_or_else(|| "view.rotate is missing payload.quarter_turns".to_string())?;
            Ok(Command::RotateView { quarter_turns })
        }
        "view.flip_horizontal" => Ok(Command::FlipViewHorizontally),
        "view.flip_vertical" => Ok(Command::FlipViewVertically),
        other => Err(format!("unsupported command descriptor: {other}")),
    }
}
fn payload_u64(value: &Value) -> Option<u64> { value.as_u64().or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok())).or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok())) }
fn payload_i32(value: &Value) -> Option<i32> { value.as_i64().and_then(|number| i32::try_from(number).ok()).or_else(|| value.as_u64().and_then(|number| i32::try_from(number).ok())).or_else(|| value.as_str().and_then(|text| text.parse::<i32>().ok())) }
fn payload_f64(value: &Value) -> Option<f64> { value.as_f64().or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok())) }
fn parse_hex_color(input: &str) -> Option<ColorRgba8> {
    let hex = input.strip_prefix('#')?;
    if hex.len() != 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(ColorRgba8::new(r, g, b, 0xff))
}
fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized.split(|ch: char| ch == 'x' || ch.is_whitespace()).filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
    if parts.len() != 2 { return None; }
    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0 || height == 0 || width > MAX_DOCUMENT_DIMENSION || height > MAX_DOCUMENT_DIMENSION || width.saturating_mul(height) > MAX_DOCUMENT_PIXELS { return None; }
    Some((width, height))
}
