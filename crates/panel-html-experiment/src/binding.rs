//! `data-bind-*` 属性の分類と式評価（Blitz DOM 非依存の純粋ロジック）。
//!
//! 実際の DOM 反映は `panel-runtime::html_panel` 側で Blitz の API を使って適用する。
//! ここは式のサブセットを定義し、変更可能箇所を絞る。
//!
//! 対応する属性カテゴリ:
//! - `data-bind-text="<expr>"` — textContent を置き換える
//! - `data-bind-disabled="<expr>"` — truthy なら disabled 付加、falsy なら削除
//! - `data-bind-class-<class-name>="<expr>"` — truthy なら class 付加、falsy なら削除
//!
//! 式のサブセット（PoC）:
//! - 単純パス: `host.can_undo` / `jobs.active`
//! - 否定: `!host.can_undo`
//! - リテラルは未対応（常にパス参照として解決）

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingAttribute<'a> {
    /// `data-bind-text`
    Text,
    /// `data-bind-disabled`
    Disabled,
    /// `data-bind-class-<class-name>`
    Class(&'a str),
    /// `data-bind-*` 以外
    None,
}

pub fn classify_binding_attribute(attr_name: &str) -> BindingAttribute<'_> {
    if attr_name == "data-bind-text" {
        BindingAttribute::Text
    } else if attr_name == "data-bind-disabled" {
        BindingAttribute::Disabled
    } else if let Some(class_name) = attr_name.strip_prefix("data-bind-class-") {
        BindingAttribute::Class(class_name)
    } else {
        BindingAttribute::None
    }
}

pub fn evaluate_as_bool(expr: &str, snapshot: &Value) -> bool {
    let trimmed = expr.trim();
    let (negate, path) = if let Some(rest) = trimmed.strip_prefix('!') {
        (true, rest.trim())
    } else {
        (false, trimmed)
    };
    let raw = lookup_path(path, snapshot);
    let truthy = is_truthy(raw);
    if negate { !truthy } else { truthy }
}

pub fn evaluate_as_string(expr: &str, snapshot: &Value) -> String {
    let raw = lookup_path(expr.trim(), snapshot);
    match raw {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn lookup_path<'a>(path: &str, snapshot: &'a Value) -> &'a Value {
    static NULL: Value = Value::Null;
    let mut current = snapshot;
    for segment in path.split('.') {
        if segment.is_empty() {
            return &NULL;
        }
        match current {
            Value::Object(map) => match map.get(segment) {
                Some(v) => current = v,
                None => return &NULL,
            },
            _ => return &NULL,
        }
    }
    current
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classify_binding_attribute_recognizes_text_disabled_class() {
        assert_eq!(
            classify_binding_attribute("data-bind-text"),
            BindingAttribute::Text
        );
        assert_eq!(
            classify_binding_attribute("data-bind-disabled"),
            BindingAttribute::Disabled
        );
        assert_eq!(
            classify_binding_attribute("data-bind-class-enabled"),
            BindingAttribute::Class("enabled")
        );
        assert_eq!(
            classify_binding_attribute("data-action"),
            BindingAttribute::None
        );
    }

    #[test]
    fn evaluate_as_bool_handles_negation_and_missing_paths() {
        let snap = json!({"host": {"can_undo": true}});
        assert!(evaluate_as_bool("host.can_undo", &snap));
        assert!(!evaluate_as_bool("!host.can_undo", &snap));

        // missing path should evaluate to false
        let empty = json!({});
        assert!(!evaluate_as_bool("host.can_undo", &empty));
        assert!(evaluate_as_bool("!host.can_undo", &empty));
    }

    #[test]
    fn evaluate_as_string_serializes_common_types() {
        let snap = json!({
            "count": 3,
            "name": "altpaint",
            "flag": true,
            "missing": null,
        });
        assert_eq!(evaluate_as_string("count", &snap), "3");
        assert_eq!(evaluate_as_string("name", &snap), "altpaint");
        assert_eq!(evaluate_as_string("flag", &snap), "true");
        assert_eq!(evaluate_as_string("missing", &snap), "");
        assert_eq!(evaluate_as_string("not.exist", &snap), "");
    }

    #[test]
    fn evaluate_as_bool_supports_dot_nested_paths() {
        let snap = json!({
            "host": {
                "capabilities": {
                    "undo": true,
                    "redo": false
                }
            }
        });
        assert!(evaluate_as_bool("host.capabilities.undo", &snap));
        assert!(!evaluate_as_bool("host.capabilities.redo", &snap));
        assert!(!evaluate_as_bool("host.capabilities.missing", &snap));
    }
}
