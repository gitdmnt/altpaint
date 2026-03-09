use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelAst {
    pub panel: PanelHeaderAst,
    pub permissions: Vec<String>,
    pub runtime: RuntimeAst,
    pub state: Vec<StateFieldAst>,
    pub view: Vec<ViewNodeAst>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelHeaderAst {
    pub id: String,
    pub title: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAst {
    pub wasm: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldAst {
    pub name: String,
    pub kind: StateType,
    pub default: AttrValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateType {
    Bool,
    Int,
    Float,
    String,
    Color,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    String(String),
    Integer(i64),
    Float(String),
    Bool(bool),
    Expression(String),
}

impl AttrValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn as_bool_literal(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNodeAst {
    Element(ViewElementAst),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewElementAst {
    pub tag: String,
    pub attributes: BTreeMap<String, AttrValue>,
    pub children: Vec<ViewNodeAst>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDefinition {
    pub source_path: PathBuf,
    pub manifest: PanelManifest,
    pub permissions: Vec<String>,
    pub runtime: RuntimeDefinition,
    pub state: Vec<StateField>,
    pub view: Vec<ViewNode>,
    pub handler_bindings: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelManifest {
    pub id: String,
    pub title: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDefinition {
    pub wasm: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateField {
    pub name: String,
    pub kind: StateType,
    pub default: AttrValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNode {
    Element(ViewElement),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewElement {
    pub tag: String,
    pub attributes: BTreeMap<String, AttrValue>,
    pub children: Vec<ViewNode>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PanelDslError {
    #[error("failed to read panel file: {0}")]
    Io(String),
    #[error("missing block: {0}")]
    MissingBlock(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("validation error: {0}")]
    Validation(String),
}

pub fn parse_panel_source(source: &str) -> Result<PanelAst, PanelDslError> {
    let blocks = extract_blocks(source)?;
    let panel_block = blocks
        .get("panel")
        .ok_or_else(|| PanelDslError::MissingBlock("panel".to_string()))?;
    let permissions_block = blocks
        .get("permissions")
        .ok_or_else(|| PanelDslError::MissingBlock("permissions".to_string()))?;
    let runtime_block = blocks
        .get("runtime")
        .ok_or_else(|| PanelDslError::MissingBlock("runtime".to_string()))?;
    let state_block = blocks
        .get("state")
        .ok_or_else(|| PanelDslError::MissingBlock("state".to_string()))?;
    let view_block = blocks
        .get("view")
        .ok_or_else(|| PanelDslError::MissingBlock("view".to_string()))?;

    Ok(PanelAst {
        panel: parse_panel_header(panel_block)?,
        permissions: parse_permissions_block(permissions_block),
        runtime: parse_runtime_block(runtime_block)?,
        state: parse_state_block(state_block)?,
        view: parse_view_block(view_block)?,
    })
}

pub fn validate_panel_ast(
    ast: PanelAst,
    source_path: impl Into<PathBuf>,
) -> Result<PanelDefinition, PanelDslError> {
    let source_path = source_path.into();
    if ast.panel.id.trim().is_empty() {
        return Err(PanelDslError::Validation(
            "panel.id must not be empty".to_string(),
        ));
    }
    if ast.panel.title.trim().is_empty() {
        return Err(PanelDslError::Validation(
            "panel.title must not be empty".to_string(),
        ));
    }
    if ast.panel.version == 0 {
        return Err(PanelDslError::Validation(
            "panel.version must be greater than zero".to_string(),
        ));
    }
    if ast.runtime.wasm.trim().is_empty() {
        return Err(PanelDslError::Validation(
            "runtime.wasm must not be empty".to_string(),
        ));
    }
    if ast.view.is_empty() {
        return Err(PanelDslError::Validation(
            "view block must contain at least one node".to_string(),
        ));
    }

    for field in &ast.state {
        validate_attr_value(&field.default)?;
    }

    let mut handler_bindings = BTreeSet::new();
    for node in &ast.view {
        validate_view_node(node, &mut handler_bindings)?;
    }

    if let Some(base_dir) = source_path.parent() {
        let wasm_path = base_dir.join(&ast.runtime.wasm);
        if !wasm_path.exists() {
            return Err(PanelDslError::Validation(format!(
                "runtime.wasm not found: {}",
                wasm_path.display()
            )));
        }
    }

    Ok(PanelDefinition {
        source_path,
        manifest: PanelManifest {
            id: ast.panel.id,
            title: ast.panel.title,
            version: ast.panel.version,
        },
        permissions: ast.permissions,
        runtime: RuntimeDefinition {
            wasm: ast.runtime.wasm,
        },
        state: ast
            .state
            .into_iter()
            .map(|field| StateField {
                name: field.name,
                kind: field.kind,
                default: field.default,
            })
            .collect(),
        view: ast.view.into_iter().map(normalize_view_node).collect(),
        handler_bindings,
    })
}

pub fn load_panel_file(path: impl AsRef<Path>) -> Result<PanelDefinition, PanelDslError> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)
        .map_err(|error| PanelDslError::Io(format!("{} ({})", path.display(), error)))?;
    let ast = parse_panel_source(&source)?;
    validate_panel_ast(ast, path.to_path_buf())
}

fn normalize_view_node(node: ViewNodeAst) -> ViewNode {
    match node {
        ViewNodeAst::Element(element) => ViewNode::Element(ViewElement {
            tag: element.tag,
            attributes: element.attributes,
            children: element
                .children
                .into_iter()
                .map(normalize_view_node)
                .collect(),
        }),
        ViewNodeAst::Text(text) => ViewNode::Text(text),
    }
}

fn validate_view_node(
    node: &ViewNodeAst,
    handler_bindings: &mut BTreeSet<String>,
) -> Result<(), PanelDslError> {
    match node {
        ViewNodeAst::Text(text) => validate_text_expressions(text),
        ViewNodeAst::Element(element) => {
            let allowed = [
                "column",
                "row",
                "section",
                "text",
                "color-preview",
                "button",
                "toggle",
                "slider",
                "input",
                "dropdown",
                "layer-list",
                "separator",
                "spacer",
                "when",
            ];
            if !allowed.contains(&element.tag.as_str()) {
                return Err(PanelDslError::Validation(format!(
                    "unsupported view tag: {}",
                    element.tag
                )));
            }
            for value in element.attributes.values() {
                validate_attr_value(value)?;
            }
            if element.tag == "button"
                && let Some(handler) = element
                    .attributes
                    .get("on:click")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            if element.tag == "toggle"
                && let Some(handler) = element
                    .attributes
                    .get("on:change")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            if element.tag == "slider"
                && let Some(handler) = element
                    .attributes
                    .get("on:change")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            if element.tag == "input"
                && let Some(handler) = element
                    .attributes
                    .get("on:change")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            if element.tag == "dropdown"
                && let Some(handler) = element
                    .attributes
                    .get("on:change")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            if element.tag == "layer-list"
                && let Some(handler) = element
                    .attributes
                    .get("on:change")
                    .and_then(AttrValue::as_string)
            {
                validate_handler_binding(handler)?;
                handler_bindings.insert(handler.to_string());
            }
            for child in &element.children {
                validate_view_node(child, handler_bindings)?;
            }
            Ok(())
        }
    }
}

fn validate_attr_value(value: &AttrValue) -> Result<(), PanelDslError> {
    if let AttrValue::Expression(expression) = value {
        validate_expression_usage(expression)?;
    }
    Ok(())
}

fn validate_handler_binding(handler: &str) -> Result<(), PanelDslError> {
    if handler == "sync_host" {
        return Err(PanelDslError::Validation(
            "sync_host is a reserved lifecycle name; use #[panel_sdk::panel_sync_host] in Wasm and do not bind it from .altp-panel".to_string(),
        ));
    }
    Ok(())
}

fn validate_text_expressions(text: &str) -> Result<(), PanelDslError> {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            return Ok(());
        };
        validate_expression_usage(after_start[..end].trim())?;
        rest = &after_start[end + 1..];
    }
    Ok(())
}

fn validate_expression_usage(expression: &str) -> Result<(), PanelDslError> {
    if expression.contains("host.") {
        return Err(PanelDslError::Validation(
            "direct host.* expressions are not allowed in .altp-panel; read host data via Wasm panel-sdk and mirror it into state.*".to_string(),
        ));
    }
    Ok(())
}

fn parse_panel_header(body: &str) -> Result<PanelHeaderAst, PanelDslError> {
    let fields = parse_key_value_lines(body)?;
    let id = required_string_field(&fields, "id")?;
    let title = required_string_field(&fields, "title")?;
    let version = required_integer_field(&fields, "version")? as u32;
    Ok(PanelHeaderAst { id, title, version })
}

fn parse_runtime_block(body: &str) -> Result<RuntimeAst, PanelDslError> {
    let fields = parse_key_value_lines(body)?;
    Ok(RuntimeAst {
        wasm: required_string_field(&fields, "wasm")?,
    })
}

fn parse_permissions_block(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches(',').to_string())
        .collect()
}

fn parse_state_block(body: &str) -> Result<Vec<StateFieldAst>, PanelDslError> {
    let mut fields = Vec::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = line.trim_end_matches(',');
        let (name, rest) = line.split_once(':').ok_or_else(|| {
            PanelDslError::Parse(format!("state declaration is missing ':' -> {line}"))
        })?;
        let (kind, default) = rest.split_once('=').ok_or_else(|| {
            PanelDslError::Parse(format!("state declaration is missing '=' -> {line}"))
        })?;
        fields.push(StateFieldAst {
            name: name.trim().to_string(),
            kind: parse_state_type(kind.trim())?,
            default: parse_attr_value(default.trim())?,
        });
    }
    Ok(fields)
}

fn parse_state_type(input: &str) -> Result<StateType, PanelDslError> {
    match input {
        "bool" => Ok(StateType::Bool),
        "int" => Ok(StateType::Int),
        "float" => Ok(StateType::Float),
        "string" => Ok(StateType::String),
        "color" => Ok(StateType::Color),
        _ if input.starts_with("enum(") && input.ends_with(')') => {
            let inner = &input[5..input.len() - 1];
            let variants = split_top_level(inner, ',')
                .into_iter()
                .map(|value| parse_attr_value(value.trim()))
                .collect::<Result<Vec<_>, _>>()?;
            let mut items = Vec::new();
            for variant in variants {
                let AttrValue::String(value) = variant else {
                    return Err(PanelDslError::Parse(
                        "enum variants must be quoted strings".to_string(),
                    ));
                };
                items.push(value);
            }
            Ok(StateType::Enum(items))
        }
        _ => Err(PanelDslError::Parse(format!(
            "unsupported state type: {input}"
        ))),
    }
}

fn parse_view_block(body: &str) -> Result<Vec<ViewNodeAst>, PanelDslError> {
    let mut index = 0;
    let mut root_nodes: Vec<ViewNodeAst> = Vec::new();
    let mut stack: Vec<ViewElementAst> = Vec::new();
    let bytes = body.as_bytes();

    while index < bytes.len() {
        if bytes[index] == b'<' {
            let tag_end = find_tag_end(body, index + 1)?;
            let raw_tag = &body[index + 1..tag_end];
            let parsed_tag = parse_tag(raw_tag)?;
            index = tag_end + 1;

            if parsed_tag.closing {
                let Some(element) = stack.pop() else {
                    return Err(PanelDslError::Parse(format!(
                        "unexpected closing tag: {}",
                        parsed_tag.name
                    )));
                };
                if element.tag != parsed_tag.name {
                    return Err(PanelDslError::Parse(format!(
                        "mismatched closing tag: expected </{}> but found </{}>",
                        element.tag, parsed_tag.name
                    )));
                }
                append_view_node(&mut root_nodes, &mut stack, ViewNodeAst::Element(element));
                continue;
            }

            let element = ViewElementAst {
                tag: parsed_tag.name,
                attributes: parsed_tag.attributes,
                children: Vec::new(),
            };
            if parsed_tag.self_closing {
                append_view_node(&mut root_nodes, &mut stack, ViewNodeAst::Element(element));
            } else {
                stack.push(element);
            }
            continue;
        }

        let next_tag = body[index..]
            .find('<')
            .map(|offset| index + offset)
            .unwrap_or(body.len());
        let text = collapse_text(&body[index..next_tag]);
        if !text.is_empty() {
            append_view_node(&mut root_nodes, &mut stack, ViewNodeAst::Text(text));
        }
        index = next_tag;
    }

    if let Some(unclosed) = stack.last() {
        return Err(PanelDslError::Parse(format!(
            "unclosed view tag: {}",
            unclosed.tag
        )));
    }

    Ok(root_nodes)
}

fn append_view_node(
    root_nodes: &mut Vec<ViewNodeAst>,
    stack: &mut [ViewElementAst],
    node: ViewNodeAst,
) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root_nodes.push(node);
    }
}

fn collapse_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_tag_end(source: &str, mut index: usize) -> Result<usize, PanelDslError> {
    let mut in_string = false;
    let mut brace_depth = 0usize;
    while index < source.len() {
        let current = source[index..].chars().next().ok_or_else(|| {
            PanelDslError::Parse("unterminated view tag while parsing view block".to_string())
        })?;
        match current {
            '"' => in_string = !in_string,
            '{' if !in_string => brace_depth += 1,
            '}' if !in_string && brace_depth > 0 => brace_depth -= 1,
            '>' if !in_string && brace_depth == 0 => return Ok(index),
            _ => {}
        }
        index += current.len_utf8();
    }

    Err(PanelDslError::Parse(
        "unterminated view tag while parsing view block".to_string(),
    ))
}

struct ParsedTag {
    name: String,
    attributes: BTreeMap<String, AttrValue>,
    closing: bool,
    self_closing: bool,
}

fn parse_tag(raw_tag: &str) -> Result<ParsedTag, PanelDslError> {
    let trimmed = raw_tag.trim();
    if let Some(name) = trimmed.strip_prefix('/') {
        return Ok(ParsedTag {
            name: name.trim().to_string(),
            attributes: BTreeMap::new(),
            closing: true,
            self_closing: false,
        });
    }

    let self_closing = trimmed.ends_with('/');
    let tag_body = if self_closing {
        trimmed[..trimmed.len().saturating_sub(1)].trim_end()
    } else {
        trimmed
    };
    let mut parser = AttributeParser::new(tag_body);
    let name = parser
        .read_identifier()
        .ok_or_else(|| PanelDslError::Parse(format!("invalid tag syntax: <{trimmed}>",)))?;
    let attributes = parser.parse_attributes()?;

    Ok(ParsedTag {
        name,
        attributes,
        closing: false,
        self_closing,
    })
}

struct AttributeParser<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> AttributeParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn parse_attributes(&mut self) -> Result<BTreeMap<String, AttrValue>, PanelDslError> {
        let mut attributes = BTreeMap::new();
        while self.skip_whitespace() {
            let Some(name) = self.read_identifier() else {
                break;
            };
            self.skip_whitespace();
            if !self.consume('=') {
                attributes.insert(name, AttrValue::Bool(true));
                continue;
            }
            self.skip_whitespace();
            let value = self.read_value()?;
            attributes.insert(name, value);
        }
        Ok(attributes)
    }

    fn read_identifier(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.index;
        while let Some(character) = self.peek() {
            if character.is_alphanumeric() || matches!(character, '.' | '-' | '_' | ':') {
                self.index += character.len_utf8();
            } else {
                break;
            }
        }
        (self.index > start).then(|| self.input[start..self.index].to_string())
    }

    fn read_value(&mut self) -> Result<AttrValue, PanelDslError> {
        let Some(character) = self.peek() else {
            return Err(PanelDslError::Parse(
                "attribute value was expected but missing".to_string(),
            ));
        };
        match character {
            '"' => {
                self.index += 1;
                let start = self.index;
                while let Some(current) = self.peek() {
                    if current == '"' {
                        let value = self.input[start..self.index].to_string();
                        self.index += 1;
                        return Ok(AttrValue::String(value));
                    }
                    self.index += current.len_utf8();
                }
                Err(PanelDslError::Parse(
                    "unterminated quoted attribute value".to_string(),
                ))
            }
            '{' => {
                let start = self.index + 1;
                self.index += 1;
                let mut depth = 1usize;
                while let Some(current) = self.peek() {
                    match current {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                let value = self.input[start..self.index].trim().to_string();
                                self.index += 1;
                                return Ok(AttrValue::Expression(value));
                            }
                        }
                        _ => {}
                    }
                    self.index += current.len_utf8();
                }
                Err(PanelDslError::Parse(
                    "unterminated expression attribute value".to_string(),
                ))
            }
            _ => {
                let start = self.index;
                while let Some(current) = self.peek() {
                    if current.is_whitespace() {
                        break;
                    }
                    self.index += current.len_utf8();
                }
                parse_attr_value(&self.input[start..self.index])
            }
        }
    }

    fn skip_whitespace(&mut self) -> bool {
        while let Some(character) = self.peek() {
            if !character.is_whitespace() {
                return true;
            }
            self.index += character.len_utf8();
        }
        false
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.index += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }
}

fn parse_key_value_lines(body: &str) -> Result<BTreeMap<String, AttrValue>, PanelDslError> {
    let mut fields = BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = line.trim_end_matches(',');
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| PanelDslError::Parse(format!("block line is missing ':' -> {line}")))?;
        fields.insert(key.trim().to_string(), parse_attr_value(value.trim())?);
    }
    Ok(fields)
}

fn parse_attr_value(input: &str) -> Result<AttrValue, PanelDslError> {
    let input = input.trim();
    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        return Ok(AttrValue::String(input[1..input.len() - 1].to_string()));
    }
    if input.starts_with('{') && input.ends_with('}') && input.len() >= 2 {
        return Ok(AttrValue::Expression(
            input[1..input.len() - 1].trim().to_string(),
        ));
    }
    if input.eq_ignore_ascii_case("true") {
        return Ok(AttrValue::Bool(true));
    }
    if input.eq_ignore_ascii_case("false") {
        return Ok(AttrValue::Bool(false));
    }
    if let Ok(integer) = input.parse::<i64>() {
        return Ok(AttrValue::Integer(integer));
    }
    if input.contains('.') && input.parse::<f64>().is_ok() {
        return Ok(AttrValue::Float(input.to_string()));
    }
    Ok(AttrValue::String(input.to_string()))
}

fn required_string_field(
    fields: &BTreeMap<String, AttrValue>,
    key: &str,
) -> Result<String, PanelDslError> {
    fields
        .get(key)
        .and_then(AttrValue::as_string)
        .map(ToString::to_string)
        .ok_or_else(|| PanelDslError::Parse(format!("missing string field: {key}")))
}

fn required_integer_field(
    fields: &BTreeMap<String, AttrValue>,
    key: &str,
) -> Result<i64, PanelDslError> {
    let Some(AttrValue::Integer(value)) = fields.get(key) else {
        return Err(PanelDslError::Parse(format!(
            "missing integer field: {key}"
        )));
    };
    Ok(*value)
}

fn extract_blocks(source: &str) -> Result<BTreeMap<String, String>, PanelDslError> {
    let chars: Vec<char> = source.chars().collect();
    let mut blocks = BTreeMap::new();
    let mut index = 0usize;

    while index < chars.len() {
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }
        if !is_identifier_start(chars[index]) {
            return Err(PanelDslError::Parse(format!(
                "unexpected character at top level: {}",
                chars[index]
            )));
        }
        let start = index;
        index += 1;
        while index < chars.len() && is_identifier_continue(chars[index]) {
            index += 1;
        }
        let name: String = chars[start..index].iter().collect();
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() || chars[index] != '{' {
            return Err(PanelDslError::Parse(format!(
                "expected '{{' after block name: {name}"
            )));
        }
        index += 1;
        let body_start = index;
        let mut depth = 1usize;
        let mut in_string = false;
        while index < chars.len() {
            match chars[index] {
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        let body: String = chars[body_start..index].iter().collect();
                        blocks.insert(name, body);
                        index += 1;
                        break;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        if depth != 0 {
            return Err(PanelDslError::Parse(
                "unterminated top-level block".to_string(),
            ));
        }
    }

    Ok(blocks)
}

fn split_top_level(input: &str, delimiter: char) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut depth = 0usize;

    for (index, current) in input.char_indices() {
        match current {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string && depth > 0 => depth -= 1,
            current if current == delimiter && !in_string && depth == 0 => {
                items.push(input[start..index].trim());
                start = index + current.len_utf8();
            }
            _ => {}
        }
    }
    items.push(input[start..].trim());
    items
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const SAMPLE_PANEL: &str = r#"
panel {
  id: "builtin.dsl-sample"
  title: "DSL Sample"
  version: 1
}

permissions {
  read.document
  write.command
}

runtime {
  wasm: "sample_panel.wasm"
}

state {
  selectedTool: enum("brush", "eraser") = "brush"
  showAdvanced: bool = false
}

view {
  <column gap=8 padding=8>
    <section title="Phase 6">
      <text tone="muted">Loaded from .altp-panel</text>
      <button id="sample.reload" on:click="reload_panel">Reload</button>
      <toggle id="sample.toggle" checked=false on:change="toggle_advanced">Advanced</toggle>
            <dropdown id="sample.mode" value="brush" options="brush:Brush|eraser:Eraser" on:change="select_mode" />
            <layer-list id="sample.layers" items="[]" selected=0 on:change="reorder_layers" />
    </section>
  </column>
}
"#;

    #[test]
    fn parser_extracts_manifest_state_and_handlers() {
        let ast = parse_panel_source(SAMPLE_PANEL).expect("sample panel parses");

        assert_eq!(ast.panel.id, "builtin.dsl-sample");
        assert_eq!(ast.panel.title, "DSL Sample");
        assert_eq!(ast.permissions, vec!["read.document", "write.command"]);
        assert!(matches!(
            ast.state[0].kind,
            StateType::Enum(ref values) if values == &["brush".to_string(), "eraser".to_string()]
        ));
    }

    #[test]
    fn validation_collects_handler_bindings() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        let source_path = temp_dir.join("sample.altp-panel");
        fs::write(temp_dir.join("sample_panel.wasm"), []).expect("wasm placeholder created");

        let definition = validate_panel_ast(
            parse_panel_source(SAMPLE_PANEL).expect("panel parses"),
            source_path,
        )
        .expect("panel validates");

        assert!(definition.handler_bindings.contains("reload_panel"));
        assert!(definition.handler_bindings.contains("toggle_advanced"));
        assert!(definition.handler_bindings.contains("select_mode"));
        assert!(definition.handler_bindings.contains("reorder_layers"));
    }

    #[test]
    fn validation_rejects_unknown_view_tags() {
        let source = SAMPLE_PANEL.replace("<text tone=\"muted\">", "<card>");
        let source = source.replace("</text>", "</card>");
        let error = parse_panel_source(&source)
            .and_then(|ast| validate_panel_ast(ast, PathBuf::from("sample.altp-panel")))
            .expect_err("unknown tag should fail validation");

        assert!(
            matches!(error, PanelDslError::Validation(message) if message.contains("unsupported view tag"))
        );
    }

    #[test]
    fn validation_rejects_direct_host_snapshot_expressions() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        let source_path = temp_dir.join("sample.altp-panel");
        fs::write(temp_dir.join("sample_panel.wasm"), []).expect("wasm placeholder created");
        let source = SAMPLE_PANEL.replace(
            "<text tone=\"muted\">Loaded from .altp-panel</text>",
            "<text>{host.document.title}</text>",
        );

        let error = parse_panel_source(&source)
            .and_then(|ast| validate_panel_ast(ast, source_path))
            .expect_err("host.* expression should fail validation");

        assert!(matches!(
            error,
            PanelDslError::Validation(message)
                if message.contains("direct host.* expressions are not allowed")
        ));
    }

    #[test]
    fn validation_rejects_sync_host_as_ui_handler_binding() {
        let temp_dir = unique_test_dir();
        fs::create_dir_all(&temp_dir).expect("temp dir created");
        let source_path = temp_dir.join("sample.altp-panel");
        fs::write(temp_dir.join("sample_panel.wasm"), []).expect("wasm placeholder created");
        let source = SAMPLE_PANEL.replace("reload_panel", "sync_host");

        let error = parse_panel_source(&source)
            .and_then(|ast| validate_panel_ast(ast, source_path))
            .expect_err("sync_host binding should fail validation");

        assert!(matches!(
            error,
            PanelDslError::Validation(message)
                if message.contains("sync_host is a reserved lifecycle name")
        ));
    }

    fn unique_test_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-panel-dsl-{suffix}"))
    }
}
