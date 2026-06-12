use super::*;

/// `reconcile_panels` が登録パネル全件 + workspace 自身を workspace_layout の panels に追加する。
#[test]
fn workspace_panel_entries_include_all_registered_panels() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.reconcile_panels(vec!["builtin.mock"]);

    let layout = panel_workspace.workspace_layout();
    assert!(
        layout
            .panels
            .iter()
            .any(|entry| entry.id == workspace::WORKSPACE_PANEL_ID),
        "workspace-layout self entry must be present"
    );
    assert!(
        layout
            .panels
            .iter()
            .any(|entry| entry.id == "builtin.mock"),
        "registered panel must be present in workspace layout"
    );
}

/// Phase 4: HTML パネルの move handle (タイトルバー) を screen 座標で検索すると panel_id が返る。
#[test]
fn panel_move_handle_at_resolves_drag_handle_to_panel_id() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.update_panel_move_handle(
        "html.test",
        app_core::WindowRect {
            x: 100,
            y: 50,
            width: 280,
            height: 24,
        },
    );

    // ハンドル内
    assert_eq!(
        panel_workspace.panel_move_handle_at(app_core::WindowPoint::new(120, 60)),
        Some("html.test".to_string())
    );
    // ハンドル外 (右下)
    assert_eq!(panel_workspace.panel_move_handle_at(app_core::WindowPoint::new(120, 80)), None);
    // ハンドル外 (上端より上)
    assert_eq!(panel_workspace.panel_move_handle_at(app_core::WindowPoint::new(120, 49)), None);
}

/// Phase 4: `remove_panel_move_handle` で個別削除できる。
#[test]
fn remove_panel_move_handle_clears_handle() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.update_panel_move_handle(
        "html.test",
        app_core::WindowRect {
            x: 0,
            y: 0,
            width: 100,
            height: 24,
        },
    );
    assert!(panel_workspace.panel_move_handle_at(app_core::WindowPoint::new(50, 10)).is_some());

    panel_workspace.remove_panel_move_handle("html.test");
    assert!(panel_workspace.panel_move_handle_at(app_core::WindowPoint::new(50, 10)).is_none());
}

/// Phase 3: HTML パネル hit table を screen 座標で検索すると `(panel_id, node_id)` が返る。
#[test]
fn panel_hit_at_resolves_screen_coordinates_to_panel_event() {
    let mut panel_workspace = PanelWorkspace::new();
    let screen_rect = app_core::WindowRect {
        x: 100,
        y: 50,
        width: 280,
        height: 240,
    };
    let hits = vec![
        (
            "save_btn".to_string(),
            app_core::WindowRect {
                x: 10,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
        (
            "undo_btn".to_string(),
            app_core::WindowRect {
                x: 80,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
    ];
    panel_workspace.update_panel_hits("html.test", screen_rect, hits);

    // panel-relative (10,20) → screen (110, 70)。範囲は (110..170, 70..100)
    let inside_save = panel_workspace.panel_hit_at(app_core::WindowPoint::new(120, 80));
    assert_eq!(
        inside_save,
        Some(("html.test".to_string(), "save_btn".to_string()))
    );

    let inside_undo = panel_workspace.panel_hit_at(app_core::WindowPoint::new(190, 85));
    assert_eq!(
        inside_undo,
        Some(("html.test".to_string(), "undo_btn".to_string()))
    );

    // パネル矩形外
    assert_eq!(panel_workspace.panel_hit_at(app_core::WindowPoint::new(50, 50)), None);
    // パネル矩形内だが action 矩形外
    assert_eq!(panel_workspace.panel_hit_at(app_core::WindowPoint::new(110, 200)), None);
}

/// Phase 3: `remove_panel_hits` で hit 情報を消すと、その後の検索は None。
#[test]
fn remove_panel_hits_clears_hits_for_panel() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.update_panel_hits(
        "html.test",
        app_core::WindowRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        vec![(
            "btn".to_string(),
            app_core::WindowRect {
                x: 10,
                y: 10,
                width: 40,
                height: 20,
            },
        )],
    );
    assert!(panel_workspace.panel_hit_at(app_core::WindowPoint::new(20, 20)).is_some());

    panel_workspace.remove_panel_hits("html.test");
    assert!(panel_workspace.panel_hit_at(app_core::WindowPoint::new(20, 20)).is_none());
}

/// Phase 2: HTML パネル相当の workspace エントリは `set_panel_visibility` で切り替えられ、
/// `is_panel_visible` で可視判定が外部 crate からも取得できる必要がある。
#[test]
fn panel_visibility_can_be_toggled_and_queried() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.reconcile_panels(vec!["builtin.mock.html"]);

    assert!(panel_workspace.is_panel_visible("builtin.mock.html"));

    let changed = panel_workspace.set_panel_visibility("builtin.mock.html", false);
    assert!(changed, "visibility 変化なら true を返す");
    assert!(!panel_workspace.is_panel_visible("builtin.mock.html"));

    panel_workspace.set_panel_visibility("builtin.mock.html", true);
    assert!(panel_workspace.is_panel_visible("builtin.mock.html"));
}

/// Phase 1: HTML パネル相当 (`PanelTree` を経由せず登録した) も `reconcile_panels` で
/// workspace_layout のエントリを取得する。これが visibility / move のための前提となる。
#[test]
fn panel_with_empty_tree_gets_workspace_entry_after_reconcile() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.reconcile_panels(vec!["builtin.mock.html"]);

    let layout = panel_workspace.workspace_layout();
    let entry = layout
        .panels
        .iter()
        .find(|e| e.id == "builtin.mock.html")
        .expect("HTML panel must have a workspace entry after reconcile");
    assert!(entry.visible, "HTML panel default visibility is true");
    assert!(entry.position.is_some(), "default position assigned");
}

/// BL-051: 右下アンカーパネルの矩形が viewport 指定で正しく解決される。
/// (旧 viewport なし版は usize::MAX フォールバックで画面外座標を返す実バグがあった)
#[test]
fn panel_rect_resolves_bottom_right_anchor_within_viewport() {
    use app_core::{
        WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
        WorkspacePanelState,
    };

    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.replace_workspace_layout(WorkspaceLayout {
        panels: vec![WorkspacePanelState {
            id: "builtin.mock".to_string(),
            visible: true,
            anchor: WorkspacePanelAnchor::BottomRight,
            position: Some(WorkspacePanelPosition { x: 24, y: 24 }),
            size: Some(WorkspacePanelSize {
                width: 300,
                height: 220,
            }),
        }],
    });

    let rect = panel_workspace
        .panel_rect("builtin.mock", 1280, 800)
        .expect("rect resolves");
    // BottomRight anchor: x = 1280 - 300 - 24, y = 800 - 220 - 24
    assert_eq!(rect.x, 956);
    assert_eq!(rect.y, 556);
    assert!(rect.x + rect.width <= 1280, "rect within viewport width");
    assert!(rect.y + rect.height <= 800, "rect within viewport height");
}

/// Phase 11: TopRight anchor のパネルで W ハンドルドラッグ → 右辺の screen 座標が固定される。
#[test]
fn resize_panel_keeping_anchor_top_right_keeps_right_edge_fixed() {
    use app_core::{
        WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
        WorkspacePanelState,
    };
    use app_core::WindowRect;

    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.replace_workspace_layout(WorkspaceLayout {
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
    let new_rect = WindowRect {
        x: 980,
        y: 0,
        width: 300,
        height: 150,
    };
    let applied = panel_workspace
        .resize_panel_keeping_anchor("builtin.mock", new_rect, viewport)
        .expect("applied");
    assert_eq!(applied.x, 980);
    assert_eq!(applied.width, 300);
    // 反映後の rect の右辺が 1280 で不変
    assert_eq!(applied.x + applied.width, 1280);
}
