use crate::command_from_descriptor;
use app_core::{Command, Document};
use panel_api::{PanelPlugin, PanelTree};
use panel_schema::CommandDescriptor;
use serde_json::Value;

use crate::registry::PanelRuntime;

/// コマンド mapping supports ビュー ズーム が期待どおりに動作することを検証する。
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

// ─── dirty panel テスト用モックパネル ───────────────────────────────────────

struct MockPanel {
    id: &'static str,
    counter: u32,
}

impl MockPanel {
    fn new(id: &'static str) -> Self {
        Self { id, counter: 0 }
    }
}

impl PanelPlugin for MockPanel {
    fn id(&self) -> &'static str {
        self.id
    }
    fn title(&self) -> &'static str {
        "Mock"
    }
    fn update(
        &mut self,
        _document: &Document,
        _can_undo: bool,
        _can_redo: bool,
        _active_jobs: usize,
        _snapshot_count: usize,
    ) {
        self.counter += 1;
    }
    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id,
            title: self.title(),
            children: vec![panel_api::PanelNode::Text {
                id: format!("counter.{}", self.counter),
                text: format!("{}", self.counter),
            }],
        }
    }
}

/// `mark_dirty` を呼ばなければ `sync_dirty_panels` は何もしない。
#[test]
fn sync_dirty_panels_skips_panels_not_marked_dirty() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    // register_panel は自動で dirty にするので、一度 sync して dirty をクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(!runtime.has_dirty_panels());

    // dirty にせず sync → 何も変わらない
    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(changed.is_empty());
}

/// `mark_dirty` で指定したパネルだけが sync される。
#[test]
fn mark_dirty_causes_only_that_panel_to_be_synced() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    runtime.register_panel(Box::new(MockPanel::new("panel.b")));
    // 初回 sync でクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    // panel.a のみ dirty にして sync
    runtime.mark_dirty("panel.a");
    assert_eq!(runtime.dirty_panel_count(), 1);
    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    // panel.a は counter が変わるので changed に入る（tree が変わる）
    assert!(changed.contains("panel.a"), "panel.a should be in changed");
    // panel.b は sync されていないので changed に入らない
    assert!(!changed.contains("panel.b"), "panel.b should not be in changed");
    // sync 後は dirty クリア
    assert!(!runtime.has_dirty_panels());
}

/// `mark_all_dirty` は登録済みの全パネルを dirty にする。
#[test]
fn mark_all_dirty_marks_every_registered_panel() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    runtime.register_panel(Box::new(MockPanel::new("panel.b")));
    runtime.register_panel(Box::new(MockPanel::new("panel.c")));
    // 初回 sync でクリア
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert!(!runtime.has_dirty_panels());

    runtime.mark_all_dirty();
    assert_eq!(runtime.dirty_panel_count(), 3);
    assert!(runtime.has_dirty_panels());

    let changed = runtime.sync_dirty_panels(&doc, false, false, 0, 0);
    assert_eq!(changed.len(), 3);
    assert!(!runtime.has_dirty_panels());
}

/// 存在しないパネル ID を `mark_dirty` しても dirty 集合に追加されない。
#[test]
fn mark_dirty_unknown_panel_id_is_ignored() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel::new("panel.a")));
    let doc = Document::new(1, 1);
    let _ = runtime.sync_dirty_panels(&doc, false, false, 0, 0);

    runtime.mark_dirty("panel.nonexistent");
    assert!(!runtime.has_dirty_panels());
}

/// S15: install_gpu_context 未呼び出しでも render_html_panels は空 Vec を返す（パニックしない）
#[cfg(feature = "html-panel")]
#[test]
fn render_html_panels_returns_empty_when_gpu_not_installed() {
    use crate::html_panel::HtmlPanelPlugin;
    let html = r#"<html><body><button id="x" data-action="command:noop">X</button></body></html>"#;
    let plugin = HtmlPanelPlugin::from_parts("html.test", "T", html, "", None);
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));
    let frames = runtime.render_html_panels(&[("html.test".to_string(), 100, 50)], 1.0, 0);
    assert!(frames.is_empty(), "no GPU context => no frames");
}

/// Phase 3: html_measured_sizes が登録された全 HTML パネルの (id, w, h) を返す
#[cfg(feature = "html-panel")]
#[test]
fn html_measured_sizes_returns_each_panel() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin_a = HtmlPanelPlugin::from_parts(
        "html.size.a",
        "A",
        r#"<html><body style="margin:0"><div style="width:100px;height:40px"></div></body></html>"#,
        "",
        Some((100, 40)),
    );
    let plugin_b = HtmlPanelPlugin::from_parts(
        "html.size.b",
        "B",
        r#"<html><body><div></div></body></html>"#,
        "",
        Some((222, 333)),
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin_a));
    runtime.register_panel(Box::new(plugin_b));
    let sizes = runtime.html_measured_sizes();
    let a = sizes.iter().find(|(id, _, _)| id == "html.size.a").expect("a present");
    let b = sizes.iter().find(|(id, _, _)| id == "html.size.b").expect("b present");
    assert_eq!((a.1, a.2), (100, 40));
    assert_eq!((b.1, b.2), (222, 333));
}

/// Phase 3: forward_html_input は対象パネルの engine.on_input を呼び layout_dirty を立てる
#[cfg(feature = "html-panel")]
#[test]
fn forward_html_input_routes_to_correct_plugin() {
    use crate::html_panel::HtmlPanelPlugin;
    use panel_html_experiment::blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
        PointerDetails, UiEvent,
    };
    let html = r#"<html><body><button id="b" data-action="command:noop" style="display:block;width:80px;height:40px">B</button></body></html>"#;
    let plugin = HtmlPanelPlugin::from_parts("html.input.target", "T", html, "", Some((80, 40)));
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    let event = UiEvent::PointerMove(BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: 10.0,
            page_y: 10.0,
            client_x: 10.0,
            client_y: 10.0,
            screen_x: 10.0,
            screen_y: 10.0,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::empty(),
        mods: keyboard_types::Modifiers::empty(),
        details: PointerDetails::default(),
    });
    let routed = runtime.forward_html_input("html.input.target", event);
    assert!(routed, "forward_html_input should route to existing panel");
    let routed_unknown = runtime.forward_html_input("html.input.nonexistent", make_dummy_pointer_move());
    assert!(!routed_unknown, "unknown panel id should not route");
}

#[cfg(feature = "html-panel")]
fn make_dummy_pointer_move() -> panel_html_experiment::blitz_traits::events::UiEvent {
    use panel_html_experiment::blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons, PointerCoords,
        PointerDetails, UiEvent,
    };
    UiEvent::PointerMove(BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: 0.0,
            page_y: 0.0,
            client_x: 0.0,
            client_y: 0.0,
            screen_x: 0.0,
            screen_y: 0.0,
        },
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::empty(),
        mods: keyboard_types::Modifiers::empty(),
        details: PointerDetails::default(),
    })
}

/// Phase 3: take_html_size_changes は変化があった panel_id だけを返し、二回目は空
#[cfg(feature = "html-panel")]
#[test]
fn take_html_size_changes_yields_changed_panels_then_empty() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin = HtmlPanelPlugin::from_parts(
        "html.size.changes",
        "T",
        r#"<html><body style="margin:0"><div style="width:50px;height:25px"></div></body></html>"#,
        "",
        Some((300, 200)), // ロード時にコンテンツサイズと違う値を渡す
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    // 初期サイズは restored = (300, 200)
    let initial = runtime.html_measured_sizes();
    assert_eq!(
        initial.iter().find(|(id, _, _)| id == "html.size.changes").map(|s| (s.1, s.2)),
        Some((300, 200))
    );

    // take は GPU render が走らないと変化を検知しない。サイズ変化シミュレートのため
    // forcibly mark size change via テストフック (本実装では on_render 経由で起きる)。
    runtime.test_mark_html_size_changed("html.size.changes", (50, 25));
    let changes = runtime.take_html_size_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "html.size.changes");
    assert_eq!(changes[0].1, (50, 25));

    // 二回目は空
    let changes_again = runtime.take_html_size_changes();
    assert!(changes_again.is_empty());
}

/// Phase 1: HTML パネルは workspace 統合のために `panel_trees()` に id 付きで現れる必要がある。
/// `panel_tree()` は children 空でも tree 自体を返すため、ui-shell の `reconcile_runtime_panels`
/// で workspace_layout エントリが作られる前提が満たされる。
#[cfg(feature = "html-panel")]
#[test]
fn html_panel_appears_in_panel_trees_with_static_id() {
    use crate::html_panel::HtmlPanelPlugin;
    let plugin = HtmlPanelPlugin::from_parts(
        "html.workspace.fixture",
        "T",
        "<html><body></body></html>",
        "",
        None,
    );
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(plugin));

    let trees = runtime.panel_trees();
    let tree = trees
        .iter()
        .find(|t| t.id == "html.workspace.fixture")
        .expect("HTML panel must appear in panel_trees() so workspace can register it");
    assert!(
        tree.children.is_empty(),
        "HTML panel tree intentionally has empty children (GPU 直描画)"
    );
}
