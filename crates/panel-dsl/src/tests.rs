//! `panel-dsl` の回帰テストを保持する。

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{parse_panel_source, validate_panel_ast, PanelDslError, StateType};

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
