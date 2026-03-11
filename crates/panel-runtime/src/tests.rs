use crate::command_from_descriptor;
use app_core::Command;
use panel_schema::CommandDescriptor;
use serde_json::Value;

#[test]
fn command_mapping_supports_view_zoom() {
    let mut descriptor = CommandDescriptor::new("view.zoom");
    descriptor
        .payload
        .insert("zoom".to_string(), Value::String("1.5".to_string()));

    assert_eq!(
        command_from_descriptor(&descriptor),
        Ok(Command::SetViewZoom { zoom: 1.5 })
    );
}
