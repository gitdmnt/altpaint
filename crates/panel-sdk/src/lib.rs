pub use panel_schema::{
    CommandDescriptor, Diagnostic, DiagnosticLevel, HandlerResult, PanelEventRequest,
    PanelInitRequest, PanelInitResponse, StatePatch, StatePatchOp,
};

use serde_json::Value;

pub fn command(name: impl Into<String>) -> CommandBuilder {
    CommandBuilder {
        descriptor: CommandDescriptor::new(name),
    }
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    descriptor: CommandDescriptor,
}

impl CommandBuilder {
    pub fn string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::String(value.into()));
        self
    }

    pub fn bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.descriptor
            .payload
            .insert(key.into(), Value::Bool(value));
        self
    }

    pub fn color(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.string(key, value)
    }

    pub fn value(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.descriptor.payload.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> CommandDescriptor {
        self.descriptor
    }
}

pub fn handler_result() -> HandlerResult {
    HandlerResult::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_builder_collects_payload_fields() {
        let descriptor = command("tool.set_active")
            .string("tool", "brush")
            .bool("pinned", true)
            .value("weight", json!(1))
            .build();

        assert_eq!(descriptor.name, "tool.set_active");
        assert_eq!(descriptor.payload.get("tool"), Some(&json!("brush")));
        assert_eq!(descriptor.payload.get("pinned"), Some(&json!(true)));
        assert_eq!(descriptor.payload.get("weight"), Some(&json!(1)));
    }
}
