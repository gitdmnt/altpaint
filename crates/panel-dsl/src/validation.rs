//! 解析済み DSL を実行可能な定義へ検証・正規化する層。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    parse_panel_source, AttrValue, PanelAst, PanelDefinition, PanelDslError, PanelManifest,
    RuntimeDefinition, StateField, ViewElement, ViewNode, ViewNodeAst,
};

/// AST を検証し、実行時に使う正規化済み定義へ変換する。
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

/// パネル定義ファイルを読み込み、解析と検証を一括で実行する。
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
            validate_view_tag(&element.tag)?;
            for value in element.attributes.values() {
                validate_attr_value(value)?;
            }
            collect_handler_binding(
                element.tag.as_str(),
                element.attributes.get("on:click"),
                handler_bindings,
            )?;
            collect_handler_binding(
                element.tag.as_str(),
                element.attributes.get("on:change"),
                handler_bindings,
            )?;
            for child in &element.children {
                validate_view_node(child, handler_bindings)?;
            }
            Ok(())
        }
    }
}

fn validate_view_tag(tag: &str) -> Result<(), PanelDslError> {
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
    if allowed.contains(&tag) {
        Ok(())
    } else {
        Err(PanelDslError::Validation(format!(
            "unsupported view tag: {tag}"
        )))
    }
}

fn collect_handler_binding(
    tag: &str,
    handler: Option<&AttrValue>,
    handler_bindings: &mut BTreeSet<String>,
) -> Result<(), PanelDslError> {
    let event_supported = matches!(
        tag,
        "button" | "toggle" | "slider" | "input" | "dropdown" | "layer-list"
    );
    let Some(handler) = handler.and_then(AttrValue::as_string) else {
        return Ok(());
    };
    if !event_supported {
        return Ok(());
    }
    validate_handler_binding(handler)?;
    handler_bindings.insert(handler.to_string());
    Ok(())
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
