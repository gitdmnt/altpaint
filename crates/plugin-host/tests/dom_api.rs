//! Phase 10: DOM mutation host function の統合テスト。
//!
//! - 一切の Wasm 著者向け SDK を経由せず、生 WAT で host function を直接呼ぶ
//! - HtmlDocument を `call_with_dom` でセットし、mutation 結果を Blitz API で検証する

use blitz_dom::DocumentConfig;
use blitz_dom::node::NodeData;
use blitz_html::{HtmlDocument, HtmlProvider};
use plugin_host::WasmPanelRuntime;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_document(html: &str) -> HtmlDocument {
    let config = DocumentConfig {
        html_parser_provider: Some(Arc::new(HtmlProvider)),
        ..DocumentConfig::default()
    };
    HtmlDocument::from_html(html, config)
}

const SET_ATTR_WAT: &str = r##"(module
    (import "dom" "query_selector" (func $qs (param i32 i32) (result i64)))
    (import "dom" "set_attribute" (func $set_attr (param i64 i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "#btn")
    (data (i32.const 16) "disabled")
    (data (i32.const 64) "")
    (func (export "panel_init")
        (local $node i64)
        i32.const 0
        i32.const 4
        call $qs
        local.set $node
        local.get $node
        i32.const 16
        i32.const 8
        i32.const 64
        i32.const 0
        call $set_attr))
"##;

#[test]
fn dom_api_set_attribute_via_wasm_modifies_document() {
    let wasm_path = write_temp_wat(SET_ATTR_WAT);
    let mut runtime = WasmPanelRuntime::load(&wasm_path).expect("runtime load");

    let html = r#"<html><body><button id="btn">B</button></body></html>"#;
    let mut document = make_document(html);

    runtime
        .call_with_dom(&mut document, |rt| rt.panel_init())
        .expect("panel_init call");

    let id = document
        .query_selector("#btn")
        .expect("query")
        .expect("button found");
    let node = document.get_node(id).expect("node");
    let NodeData::Element(el) = &node.data else {
        panic!("expected element");
    };
    assert!(
        el.attr(blitz_dom::LocalName::from("disabled")).is_some(),
        "disabled attribute should be set by Wasm"
    );
}

const SET_INNER_HTML_WAT: &str = r##"(module
    (import "dom" "query_selector" (func $qs (param i32 i32) (result i64)))
    (import "dom" "set_inner_html" (func $set_html (param i64 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "#list")
    (data (i32.const 16) "<li>a</li><li>b</li>")
    (func (export "panel_init")
        (local $node i64)
        i32.const 0
        i32.const 5
        call $qs
        local.set $node
        local.get $node
        i32.const 16
        i32.const 20
        call $set_html))
"##;

#[test]
fn dom_api_set_inner_html_replaces_children() {
    let wasm_path = write_temp_wat(SET_INNER_HTML_WAT);
    let mut runtime = WasmPanelRuntime::load(&wasm_path).expect("runtime load");

    let html = r#"<html><body><ul id="list"><li>old</li></ul></body></html>"#;
    let mut document = make_document(html);

    runtime
        .call_with_dom(&mut document, |rt| rt.panel_init())
        .expect("panel_init call");

    // <ul> の子要素を直接列挙する。Blitz は inner HTML パース後にスタイル解決まで進めないと
    // 一部 selector マッチが効かないため、まず raw children を確認する。
    let list_id = document
        .query_selector("#list")
        .expect("qs")
        .expect("ul found");
    let list_node = document.get_node(list_id).expect("ul node");
    let element_children: Vec<usize> = list_node
        .children
        .iter()
        .filter_map(|child_id| {
            let n = document.get_node(*child_id)?;
            matches!(&n.data, NodeData::Element(_)).then_some(*child_id)
        })
        .collect();
    assert_eq!(
        element_children.len(),
        2,
        "expected 2 element children of <ul> after set_inner_html, got {}",
        element_children.len()
    );
}

fn write_temp_wat(contents: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("altpaint-dom-api-{suffix}"));
    fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("dom_api.wasm");
    fs::write(&path, contents).expect("write wat");
    path
}
