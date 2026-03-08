use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PanelInitRequest {
    #[serde(default)]
    pub initial_state: Value,
    #[serde(default)]
    pub host_snapshot: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PanelInitResponse {
    #[serde(default)]
    pub state: Value,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PanelEventRequest {
    pub handler_name: String,
    pub event_kind: String,
    #[serde(default)]
    pub event_payload: Value,
    #[serde(default)]
    pub state_snapshot: Value,
    #[serde(default)]
    pub host_snapshot: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HandlerResult {
    #[serde(default)]
    pub state_patch: Vec<StatePatch>,
    #[serde(default)]
    pub commands: Vec<CommandDescriptor>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StatePatchOp {
    Set,
    Toggle,
    Replace,
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

    pub fn replace(path: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            op: StatePatchOp::Replace,
            path: path.into(),
            value: Some(value.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandDescriptor {
    pub name: String,
    #[serde(default)]
    pub payload: Map<String, Value>,
}

impl CommandDescriptor {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            payload: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    fn command_descriptor_starts_with_empty_payload() {
        let descriptor = CommandDescriptor::new("tool.set_active");

        assert_eq!(descriptor.name, "tool.set_active");
        assert!(descriptor.payload.is_empty());
    }
}
