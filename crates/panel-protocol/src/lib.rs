pub mod abi;
pub mod host_state;
pub mod keyboard;
pub mod names;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 1 回の Wasm 呼出に対するホスト側コンテキスト (P6, 旧 `PanelEventRequest`)。
///
/// `state_get_*` / `host_get_*` / `event_get_*` host function が読む 3 つの JSON
/// ソースをまとめて保持する。**Wasm へは渡らない** (ポインタ越しに host function が
/// 引くだけ)。handler 名は呼出側が `PanelWasmInstance::handle_event` に直接渡すため、
/// 本コンテキストは保持しない (旧 `handler_name` の write-only フィールドと
/// `sync_host` の疑似イベント捏造を解消)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HostCallInput {
    /// UI イベントの payload (`event_get_*` のソース)。
    #[serde(default)]
    pub event_payload: Value,
    /// パネル自身の永続/一時 state (`state_get_*` のソース)。
    #[serde(default)]
    pub state: Value,
    /// ホストが配るドキュメント等の状態 (`host_get_*` のソース)。
    #[serde(default)]
    pub host_state: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HandlerEffects {
    #[serde(default)]
    pub state_patch: Vec<StatePatch>,
    #[serde(default)]
    pub commands: Vec<RequestDescriptor>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatePatchOp {
    Set,
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatePatch {
    pub op: StatePatchOp,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

impl StatePatch {
    pub fn set(path: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            op: StatePatchOp::Set,
            path: path.into(),
            value: Some(value.into()),
        }
    }

    pub fn toggle(path: impl Into<String>) -> Self {
        Self {
            op: StatePatchOp::Toggle,
            path: path.into(),
            value: None,
        }
    }
}

/// `StatePatch` 列を `state` JSON へ適用する唯一の実装。
///
/// `path` はドット区切りでネストしたオブジェクトを辿る。途中のオブジェクトが
/// 存在しなければ生成する。`state` がオブジェクトでなければオブジェクトに置換する。
/// `Set` は値を上書きし、`Toggle` は対象の bool を反転する (未設定は `false` 扱い)。
pub fn apply_patches(state: &mut Value, patches: &[StatePatch]) {
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
                        let next = !object.get(segment).and_then(Value::as_bool).unwrap_or(false);
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestDescriptor {
    pub name: String,
    #[serde(default)]
    pub payload: Map<String, Value>,
}

impl RequestDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

impl Diagnostic {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_patch_helpers_build_expected_shape() {
        assert_eq!(StatePatch::toggle("expanded").value, None);
        assert_eq!(
            StatePatch::set("selectedTool", "brush"),
            StatePatch {
                op: StatePatchOp::Set,
                path: "selectedTool".to_string(),
                value: Some(json!("brush")),
            }
        );
    }

    #[test]
    fn request_descriptor_starts_with_empty_payload() {
        let descriptor = RequestDescriptor::new(crate::names::tool::SET_ACTIVE);

        assert_eq!(descriptor.name, crate::names::tool::SET_ACTIVE);
        assert!(descriptor.payload.is_empty());
    }

    #[test]
    fn apply_patches_sets_nested_values_and_creates_objects() {
        let mut state = json!({});
        apply_patches(
            &mut state,
            &[
                StatePatch::set("expanded", true),
                StatePatch::set("layer.opacity", 80),
                StatePatch::set("layer.name", "background"),
            ],
        );
        assert_eq!(
            state,
            json!({
                "expanded": true,
                "layer": { "opacity": 80, "name": "background" }
            })
        );
    }

    #[test]
    fn apply_patches_toggle_flips_bool_defaulting_to_false() {
        let mut state = json!({ "visible": true });
        apply_patches(&mut state, &[StatePatch::toggle("visible")]);
        assert_eq!(state, json!({ "visible": false }));

        apply_patches(&mut state, &[StatePatch::toggle("collapsed")]);
        assert_eq!(state, json!({ "visible": false, "collapsed": true }));
    }

    #[test]
    fn apply_patches_replaces_non_object_state_and_traverses_non_object_segments() {
        let mut state = json!(42);
        apply_patches(&mut state, &[StatePatch::set("a.b", 1)]);
        assert_eq!(state, json!({ "a": { "b": 1 } }));

        let mut scalar_segment = json!({ "a": 7 });
        apply_patches(&mut scalar_segment, &[StatePatch::set("a.b", 1)]);
        assert_eq!(scalar_segment, json!({ "a": { "b": 1 } }));
    }
}
