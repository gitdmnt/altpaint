//! `.altp-panel` ソース文字列を AST へ変換する純粋パーサー。

use std::collections::BTreeMap;

use crate::{
    AttrValue, PanelAst, PanelDslError, PanelHeaderAst, RuntimeAst, StateFieldAst, StateType,
    ViewElementAst, ViewNodeAst,
};

/// 入力を解析して パネル ソース に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

/// 入力を解析して パネル header に変換する。
///
/// 失敗時はエラーを返します。
fn parse_panel_header(body: &str) -> Result<PanelHeaderAst, PanelDslError> {
    let fields = parse_key_value_lines(body)?;
    let id = required_string_field(&fields, "id")?;
    let title = required_string_field(&fields, "title")?;
    let version = required_integer_field(&fields, "version")? as u32;
    Ok(PanelHeaderAst { id, title, version })
}

/// 入力を解析して runtime block に変換する。
///
/// 失敗時はエラーを返します。
fn parse_runtime_block(body: &str) -> Result<RuntimeAst, PanelDslError> {
    let fields = parse_key_value_lines(body)?;
    Ok(RuntimeAst {
        wasm: required_string_field(&fields, "wasm")?,
    })
}

/// 入力を解析して permissions block に変換する。
fn parse_permissions_block(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches(',').to_string())
        .collect()
}

/// 入力を解析して 状態 block に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

/// 入力を解析して 状態 type に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

/// 入力を解析して ビュー block に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

/// 現在の append ビュー node を返す。
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

/// collapse テキスト を計算して返す。
fn collapse_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 現在の値を tag end へ変換する。
///
/// 失敗時はエラーを返します。
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

/// 入力を解析して tag に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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
        .ok_or_else(|| PanelDslError::Parse(format!("invalid tag syntax: <{trimmed}>")))?;
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
    /// 入力値を束ねた新しいインスタンスを生成する。
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    /// 入力を解析して attributes に変換する。
    ///
    /// 失敗時はエラーを返します。
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

    /// 現在の値を identifier へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// 現在の値を 値 へ変換する。
    ///
    /// 失敗時はエラーを返します。
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

    /// Whitespace を読み飛ばす。
    fn skip_whitespace(&mut self) -> bool {
        while let Some(character) = self.peek() {
            if !character.is_whitespace() {
                return true;
            }
            self.index += character.len_utf8();
        }
        false
    }

    /// consume を計算して返す。
    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.index += expected.len_utf8();
            true
        } else {
            false
        }
    }

    /// peek を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn peek(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }
}

/// 入力を解析して key 値 lines に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
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

/// 入力を解析して attr 値 に変換する。
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

/// Required string field 用の表示文字列を組み立てる。
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

/// Required integer field 用の表示文字列を組み立てる。
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

/// 現在の値を blocks へ変換する。
///
/// 失敗時はエラーを返します。
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

/// 入力や種別に応じて処理を振り分ける。
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

/// Is identifier start かどうかを返す。
fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

/// Is identifier continue かどうかを返す。
fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}
