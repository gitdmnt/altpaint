use super::*;
use panel_api::{HostAction, PanelNode, PanelPlugin, PanelTree};
use panel_runtime::PanelRuntime;

struct MockPanel {
    id: &'static str,
    title: &'static str,
    tree: PanelTree,
}

impl PanelPlugin for MockPanel {
    /// ID を計算して返す。
    fn id(&self) -> &'static str {
        self.id
    }

    /// title を計算して返す。
    fn title(&self) -> &'static str {
        self.title
    }

    /// 現在の パネル tree を返す。
    fn panel_tree(&self) -> PanelTree {
        self.tree.clone()
    }
}

/// 現在の値を runtime へ変換する。
fn mock_runtime() -> PanelRuntime {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel {
        id: "builtin.mock",
        title: "Mock",
        tree: PanelTree {
            id: "builtin.mock",
            title: "Mock",
            children: vec![PanelNode::Button {
                id: "mock.button".to_string(),
                label: "Push".to_string(),
                action: HostAction::DispatchCommand(app_core::Command::Noop),
                active: false,
                fill_color: None,
            }],
        },
    }));
    runtime
}

/// パネル trees include ワークスペース manager and runtime panels が期待どおりに動作することを検証する。
#[test]
fn panel_trees_include_workspace_manager_and_runtime_panels() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    let trees = presentation.panel_trees(&runtime);

    assert_eq!(trees[0].id, workspace::WORKSPACE_PANEL_ID);
    assert!(trees.iter().any(|tree| tree.id == "builtin.mock"));
}

/// Phase 4: HTML パネルの move handle (タイトルバー) を screen 座標で検索すると panel_id が返る。
#[test]
fn html_panel_move_handle_at_resolves_drag_handle_to_panel_id() {
    let mut presentation = PanelPresentation::new();
    presentation.update_html_panel_move_handle(
        "html.test",
        render_types::PixelRect {
            x: 100,
            y: 50,
            width: 280,
            height: 24,
        },
    );

    // ハンドル内
    assert_eq!(
        presentation.html_panel_move_handle_at(120, 60),
        Some("html.test".to_string())
    );
    // ハンドル外 (右下)
    assert_eq!(presentation.html_panel_move_handle_at(120, 80), None);
    // ハンドル外 (上端より上)
    assert_eq!(presentation.html_panel_move_handle_at(120, 49), None);
}

/// Phase 4: `remove_html_panel_move_handle` で個別削除できる。
#[test]
fn remove_html_panel_move_handle_clears_handle() {
    let mut presentation = PanelPresentation::new();
    presentation.update_html_panel_move_handle(
        "html.test",
        render_types::PixelRect {
            x: 0,
            y: 0,
            width: 100,
            height: 24,
        },
    );
    assert!(presentation.html_panel_move_handle_at(50, 10).is_some());

    presentation.remove_html_panel_move_handle("html.test");
    assert!(presentation.html_panel_move_handle_at(50, 10).is_none());
}

/// Phase 3: HTML パネル hit table を screen 座標で検索すると `(panel_id, node_id)` が返る。
#[test]
fn html_panel_hit_at_resolves_screen_coordinates_to_panel_event() {
    let mut presentation = PanelPresentation::new();
    let screen_rect = render_types::PixelRect {
        x: 100,
        y: 50,
        width: 280,
        height: 240,
    };
    let hits = vec![
        (
            "save_btn".to_string(),
            render_types::PixelRect {
                x: 10,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
        (
            "undo_btn".to_string(),
            render_types::PixelRect {
                x: 80,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
    ];
    presentation.update_html_panel_hits("html.test", screen_rect, hits);

    // panel-relative (10,20) → screen (110, 70)。範囲は (110..170, 70..100)
    let inside_save = presentation.html_panel_hit_at(120, 80);
    assert_eq!(
        inside_save,
        Some(("html.test".to_string(), "save_btn".to_string()))
    );

    let inside_undo = presentation.html_panel_hit_at(190, 85);
    assert_eq!(
        inside_undo,
        Some(("html.test".to_string(), "undo_btn".to_string()))
    );

    // パネル矩形外
    assert_eq!(presentation.html_panel_hit_at(50, 50), None);
    // パネル矩形内だが action 矩形外
    assert_eq!(presentation.html_panel_hit_at(110, 200), None);
}

/// Phase 3: `remove_html_panel_hits` で hit 情報を消すと、その後の検索は None。
#[test]
fn remove_html_panel_hits_clears_hits_for_panel() {
    let mut presentation = PanelPresentation::new();
    presentation.update_html_panel_hits(
        "html.test",
        render_types::PixelRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        vec![(
            "btn".to_string(),
            render_types::PixelRect {
                x: 10,
                y: 10,
                width: 40,
                height: 20,
            },
        )],
    );
    assert!(presentation.html_panel_hit_at(20, 20).is_some());

    presentation.remove_html_panel_hits("html.test");
    assert!(presentation.html_panel_hit_at(20, 20).is_none());
}

/// Phase 2: HTML パネル相当の workspace エントリは `set_panel_visibility` で切り替えられ、
/// `is_panel_visible` で可視判定が外部 crate からも取得できる必要がある。
#[test]
fn html_panel_visibility_can_be_toggled_and_queried() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel {
        id: "builtin.mock.html",
        title: "Mock HTML",
        tree: PanelTree {
            id: "builtin.mock.html",
            title: "Mock HTML",
            children: Vec::new(),
        },
    }));
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    assert!(presentation.is_panel_visible("builtin.mock.html"));

    let changed = presentation.set_panel_visibility("builtin.mock.html", false);
    assert!(changed, "visibility 変化なら true を返す");
    assert!(!presentation.is_panel_visible("builtin.mock.html"));

    presentation.set_panel_visibility("builtin.mock.html", true);
    assert!(presentation.is_panel_visible("builtin.mock.html"));
}

/// Phase 1: HTML パネル相当 (children 空の tree) も `reconcile_runtime_panels` で
/// workspace_layout のエントリを取得する。これが visibility / move のための前提となる。
#[test]
fn html_panel_with_empty_tree_gets_workspace_entry_after_reconcile() {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel {
        id: "builtin.mock.html",
        title: "Mock HTML",
        tree: PanelTree {
            id: "builtin.mock.html",
            title: "Mock HTML",
            children: Vec::new(),
        },
    }));
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    let layout = presentation.workspace_layout();
    let entry = layout
        .panels
        .iter()
        .find(|e| e.id == "builtin.mock.html")
        .expect("HTML panel must have a workspace entry after reconcile");
    assert!(entry.visible, "HTML panel default visibility is true");
    assert!(entry.position.is_some(), "default position assigned");
}

/// フォーカス moves to runtime パネル node が期待どおりに動作することを検証する。
#[test]
fn focus_moves_to_runtime_panel_node() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    assert!(presentation.focus_panel_node(&runtime, "builtin.mock", "mock.button"));
    assert_eq!(
        presentation.focused_target(),
        Some(("builtin.mock", "mock.button"))
    );
}

/// Phase 11: TopRight anchor のパネルで W ハンドルドラッグ → 右辺の screen 座標が固定される。
#[test]
fn resize_panel_keeping_anchor_top_right_keeps_right_edge_fixed() {
    use app_core::{
        WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
        WorkspacePanelState,
    };
    use render_types::PixelRect;

    let mut presentation = PanelPresentation::new();
    presentation.replace_workspace_layout(WorkspaceLayout {
        panels: vec![WorkspacePanelState {
            id: "builtin.mock".to_string(),
            visible: true,
            anchor: WorkspacePanelAnchor::TopRight,
            position: Some(WorkspacePanelPosition { x: 0, y: 0 }),
            size: Some(WorkspacePanelSize {
                width: 200,
                height: 150,
            }),
        }],
    });

    let viewport = (1280usize, 800usize);
    // 元の rect: x = 1280 - 200 - 0 = 1080, width = 200 → 右辺 = 1280
    // W ハンドルで左へドラッグ: 新しい width = 300, x = 980 → 右辺 = 1280 (不変)
    let new_rect = PixelRect {
        x: 980,
        y: 0,
        width: 300,
        height: 150,
    };
    let applied = presentation
        .resize_panel_keeping_anchor("builtin.mock", new_rect, viewport)
        .expect("applied");
    assert_eq!(applied.x, 980);
    assert_eq!(applied.width, 300);
    // 反映後の rect の右辺が 1280 で不変
    assert_eq!(applied.x + applied.width, 1280);
}
