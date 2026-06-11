//! `data-action` / `data-args` 属性 → ニュートラルな `ActionDescriptor` への変換。
//!
//! プラン S3 で定めた規約:
//! - `data-action="service:<service_name>"` → `ActionDescriptor::Service { name, payload }`
//! - `data-action="command:<command_id>"` → `ActionDescriptor::Command { id, payload }`
//! - `data-args` が JSON オブジェクトなら payload として添付。不正なら無視。
//!
//! 本 crate は `panel-api` に依存しないため、`HostAction` への最終変換は
//! `panel-runtime::html_panel` 側で行う。

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionDescriptor {
    Command {
        id: String,
        payload: Map<String, Value>,
    },
    Service {
        name: String,
        payload: Map<String, Value>,
    },
    /// DSL パネル翻訳結果が出力する `altp:<kind>:<node_id>` 形式。
    /// 9E-1 で導入、9E-2 以降で `panel-runtime::dsl_panel` が `PanelEvent` に解決する。
    Altp {
        kind: AltpKind,
        node_id: String,
        payload: Map<String, Value>,
    },
}

/// `altp:` data-action のサブ種別。Phase 9E で DSL パネルが GPU 経路に乗る際に使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltpKind {
    /// `<button>` クリック → `PanelEvent::Activate`
    Activate,
    /// `<input type="range">` change → `PanelEvent::SetValue`
    Slider,
    /// `<select>` change → `PanelEvent::SetText` (option value を文字列で送る)
    Select,
    /// `<input type="text|number">` change → `PanelEvent::SetText`
    Input,
    /// `<input type="color">` change → `PanelEvent::SetText` (#rrggbb)
    Color,
    /// `<li data-action="altp:layer-select:...">` クリック → `PanelEvent::SetValue` (index)
    LayerSelect,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ActionParseError {
    #[error("empty data-action")]
    Empty,
    #[error("unknown data-action prefix: {raw}")]
    UnknownPrefix { raw: String },
    #[error("data-action missing identifier after '{prefix}:'")]
    MissingIdentifier { prefix: String },
}

/// `data-action` と `data-args`（任意）から `ActionDescriptor` を組み立てる。
///
/// `data-args` の JSON パースに失敗した場合は payload を空にして descriptor を返す
/// （PoC では緩やかに扱う。将来厳格化する場合はエラーを返すように変更）。
pub fn parse_data_action(
    data_action: &str,
    data_args: Option<&str>,
) -> Result<ActionDescriptor, ActionParseError> {
    let trimmed = data_action.trim();
    if trimmed.is_empty() {
        return Err(ActionParseError::Empty);
    }

    let (prefix, rest) = trimmed
        .split_once(':')
        .ok_or_else(|| ActionParseError::UnknownPrefix {
            raw: trimmed.to_string(),
        })?;

    let identifier = rest.trim();
    if identifier.is_empty() {
        return Err(ActionParseError::MissingIdentifier {
            prefix: prefix.to_string(),
        });
    }

    let payload = parse_data_args(data_args);

    match prefix {
        "command" => Ok(ActionDescriptor::Command {
            id: identifier.to_string(),
            payload,
        }),
        "service" => Ok(ActionDescriptor::Service {
            name: identifier.to_string(),
            payload,
        }),
        "altp" => parse_altp(identifier, payload),
        other => Err(ActionParseError::UnknownPrefix {
            raw: other.to_string(),
        }),
    }
}

fn parse_altp(
    identifier: &str,
    payload: Map<String, Value>,
) -> Result<ActionDescriptor, ActionParseError> {
    let (kind_raw, node_id) =
        identifier
            .split_once(':')
            .ok_or_else(|| ActionParseError::UnknownPrefix {
                raw: format!("altp:{identifier}"),
            })?;
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err(ActionParseError::MissingIdentifier {
            prefix: format!("altp:{kind_raw}"),
        });
    }
    let kind = match kind_raw.trim() {
        "activate" => AltpKind::Activate,
        "slider" => AltpKind::Slider,
        "select" => AltpKind::Select,
        "input" => AltpKind::Input,
        "color" => AltpKind::Color,
        "layer-select" => AltpKind::LayerSelect,
        other => {
            return Err(ActionParseError::UnknownPrefix {
                raw: format!("altp:{other}"),
            });
        }
    };
    Ok(ActionDescriptor::Altp {
        kind,
        node_id: node_id.to_string(),
        payload,
    })
}

fn parse_data_args(raw: Option<&str>) -> Map<String, Value> {
    let Some(raw) = raw else {
        return Map::new();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Map::new();
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_command_descriptor_without_args() {
        let desc = parse_data_action("command:undo", None).unwrap();
        assert_eq!(
            desc,
            ActionDescriptor::Command {
                id: "undo".to_string(),
                payload: Map::new(),
            }
        );
    }

    #[test]
    fn parse_service_descriptor_with_json_args() {
        let desc = parse_data_action(
            "service:export.image",
            Some(r#"{"format":"png","quality":90}"#),
        )
        .unwrap();
        let expected_payload = {
            let mut m = Map::new();
            m.insert("format".into(), json!("png"));
            m.insert("quality".into(), json!(90));
            m
        };
        assert_eq!(
            desc,
            ActionDescriptor::Service {
                name: "export.image".to_string(),
                payload: expected_payload,
            }
        );
    }

    #[test]
    fn parse_empty_data_action_returns_error() {
        assert_eq!(parse_data_action("", None), Err(ActionParseError::Empty));
        assert_eq!(parse_data_action("   ", None), Err(ActionParseError::Empty));
    }

    #[test]
    fn parse_unknown_prefix_returns_error() {
        let err = parse_data_action("macro:reload", None).unwrap_err();
        assert!(matches!(err, ActionParseError::UnknownPrefix { .. }));
    }

    #[test]
    fn parse_missing_identifier_returns_error() {
        let err = parse_data_action("command:", None).unwrap_err();
        assert_eq!(
            err,
            ActionParseError::MissingIdentifier {
                prefix: "command".into()
            }
        );
    }

    #[test]
    fn parse_without_colon_returns_unknown_prefix() {
        let err = parse_data_action("undo", None).unwrap_err();
        assert!(matches!(err, ActionParseError::UnknownPrefix { .. }));
    }

    #[test]
    fn parse_invalid_json_args_yields_empty_payload() {
        let desc = parse_data_action("command:save", Some("not json")).unwrap();
        match desc {
            ActionDescriptor::Command { payload, .. } => assert!(payload.is_empty()),
            _ => panic!("expected command"),
        }
    }

    #[test]
    fn parse_whitespace_around_identifier_is_trimmed() {
        let desc = parse_data_action("command:  save  ", None).unwrap();
        match desc {
            ActionDescriptor::Command { id, .. } => assert_eq!(id, "save"),
            _ => panic!("expected command"),
        }
    }

    #[test]
    fn parse_altp_activate_descriptor() {
        let desc = parse_data_action("altp:activate:tool.pen", None).unwrap();
        assert_eq!(
            desc,
            ActionDescriptor::Altp {
                kind: AltpKind::Activate,
                node_id: "tool.pen".to_string(),
                payload: Map::new(),
            }
        );
    }

    #[test]
    fn parse_altp_slider_descriptor_with_args() {
        let desc =
            parse_data_action("altp:slider:color.red", Some(r#"{"min":0,"max":255}"#)).unwrap();
        match desc {
            ActionDescriptor::Altp { kind, node_id, payload } => {
                assert_eq!(kind, AltpKind::Slider);
                assert_eq!(node_id, "color.red");
                assert_eq!(payload.get("min").and_then(|v| v.as_i64()), Some(0));
                assert_eq!(payload.get("max").and_then(|v| v.as_i64()), Some(255));
            }
            _ => panic!("expected altp"),
        }
    }

    #[test]
    fn parse_altp_layer_select_descriptor() {
        let desc = parse_data_action("altp:layer-select:layers", Some(r#"{"index":3}"#)).unwrap();
        match desc {
            ActionDescriptor::Altp { kind, payload, .. } => {
                assert_eq!(kind, AltpKind::LayerSelect);
                assert_eq!(payload.get("index").and_then(|v| v.as_i64()), Some(3));
            }
            _ => panic!("expected altp"),
        }
    }

    #[test]
    fn parse_altp_unknown_kind_returns_error() {
        let err = parse_data_action("altp:bogus:x", None).unwrap_err();
        assert!(matches!(err, ActionParseError::UnknownPrefix { .. }));
    }

    #[test]
    fn parse_altp_missing_node_id_returns_error() {
        let err = parse_data_action("altp:activate:", None).unwrap_err();
        assert!(matches!(err, ActionParseError::MissingIdentifier { .. }));
    }
}
