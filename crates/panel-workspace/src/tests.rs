use super::*;
use std::collections::BTreeMap;

/// always_visible を宣言した既定値を注入し、`reconcile_panels` で常時表示パネルが
/// workspace_layout の先頭に自動挿入され、登録パネルも追加されることを確認する (BL-095)。
#[test]
fn workspace_panel_entries_include_all_registered_panels() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.set_panel_layout_defaults(BTreeMap::from([(
        "builtin.always".to_string(),
        PanelLayoutDefaults {
            always_visible: true,
            ..Default::default()
        },
    )]));
    panel_workspace.reconcile_panels(vec!["builtin.mock"]);

    let layout = panel_workspace.workspace_layout();
    assert_eq!(
        layout.panels.first().map(|entry| entry.id.as_str()),
        Some("builtin.always"),
        "always_visible panel must be inserted at the front"
    );
    assert!(
        layout
            .panels
            .iter()
            .any(|entry| entry.id == "builtin.mock"),
        "registered panel must be present in workspace layout"
    );
}

/// BL-095: meta 宣言の既定値 (anchor / position / hidden_by_default / always_visible) が
/// reconcile 後の workspace_layout に正しく反映され、配置が宣言通りに固定されることを担保する
/// ゴールデンテスト。
#[test]
fn reconcile_applies_declared_layout_defaults() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.set_panel_layout_defaults(BTreeMap::from([
        (
            "builtin.workspace-layout".to_string(),
            PanelLayoutDefaults {
                anchor: Some(WorkspacePanelAnchor::TopLeft),
                position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
                always_visible: true,
                ..Default::default()
            },
        ),
        (
            "builtin.layers".to_string(),
            PanelLayoutDefaults {
                anchor: Some(WorkspacePanelAnchor::TopRight),
                position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
                ..Default::default()
            },
        ),
        (
            "builtin.koma-list".to_string(),
            PanelLayoutDefaults {
                hidden_by_default: true,
                ..Default::default()
            },
        ),
    ]));
    panel_workspace.reconcile_panels(vec![
        "builtin.workspace-layout",
        "builtin.layers",
        "builtin.koma-list",
    ]);

    let layout = panel_workspace.workspace_layout();
    let find = |id: &str| {
        layout
            .panels
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} must exist"))
    };

    let layers = find("builtin.layers");
    assert_eq!(layers.anchor, WorkspacePanelAnchor::TopRight);
    assert_eq!(layers.position, Some(WorkspacePanelPosition { x: 24, y: 72 }));
    assert!(layers.visible);

    let koma_list = find("builtin.koma-list");
    assert!(!koma_list.visible, "koma-list must be hidden by default");

    // always_visible パネルは visibility off できない。
    assert!(!panel_workspace.set_panel_visibility("builtin.workspace-layout", false));
    assert!(panel_workspace.is_panel_visible("builtin.workspace-layout"));
}

/// BL-096: `update_panel_geometry` は full_rect と chrome 高さから move handle を導出する。
/// move handle (タイトルバー) を screen 座標で検索すると panel_id が返る。
#[test]
fn panel_move_handle_at_resolves_drag_handle_to_panel_id() {
    let mut panel_workspace = PanelWorkspace::new();
    // full_rect の上端 24px が chrome (move handle)
    panel_workspace.update_panel_geometry(
        "html.test",
        geometry::WindowRect {
            x: 100,
            y: 50,
            width: 280,
            height: 240,
        },
        24,
        Vec::new(),
    );

    // ハンドル内
    assert_eq!(
        panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(120, 60)),
        Some("html.test".to_string())
    );
    // ハンドル外 (chrome より下 = body)
    assert_eq!(panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(120, 80)), None);
    // ハンドル外 (上端より上)
    assert_eq!(panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(120, 49)), None);
}

/// BL-096: `remove_panel_geometry` で move handle も含めて個別削除できる。
#[test]
fn remove_panel_geometry_clears_move_handle() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.update_panel_geometry(
        "html.test",
        geometry::WindowRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        24,
        Vec::new(),
    );
    assert!(panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(50, 10)).is_some());

    panel_workspace.remove_panel_geometry("html.test");
    assert!(panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(50, 10)).is_none());
}

/// BL-096: hit 矩形は body 原点基準。full_rect (100,50) + chrome 24 → body 原点 (100, 74)。
/// hit table を screen 座標で検索すると `(panel_id, node_id)` が返る。
#[test]
fn panel_hit_at_resolves_screen_coordinates_to_panel_event() {
    let mut panel_workspace = PanelWorkspace::new();
    let full_rect = geometry::WindowRect {
        x: 100,
        y: 50,
        width: 280,
        height: 264,
    };
    let hits = vec![
        (
            "save_btn".to_string(),
            geometry::WindowRect {
                x: 10,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
        (
            "undo_btn".to_string(),
            geometry::WindowRect {
                x: 80,
                y: 20,
                width: 60,
                height: 30,
            },
        ),
    ];
    // chrome 24px → body 原点 (100, 74)
    panel_workspace.update_panel_geometry("html.test", full_rect, 24, hits);

    // body-relative (10,20) → screen (110, 94)。範囲は (110..170, 94..124)
    let inside_save = panel_workspace.panel_hit_at(geometry::WindowPoint::new(120, 104));
    assert_eq!(
        inside_save,
        Some(("html.test".to_string(), "save_btn".to_string()))
    );

    let inside_undo = panel_workspace.panel_hit_at(geometry::WindowPoint::new(190, 109));
    assert_eq!(
        inside_undo,
        Some(("html.test".to_string(), "undo_btn".to_string()))
    );

    // パネル矩形外
    assert_eq!(panel_workspace.panel_hit_at(geometry::WindowPoint::new(50, 50)), None);
    // body 内だが action 矩形外
    assert_eq!(panel_workspace.panel_hit_at(geometry::WindowPoint::new(110, 200)), None);
    // chrome 領域 (body より上) は hit しない
    assert_eq!(panel_workspace.panel_hit_at(geometry::WindowPoint::new(120, 60)), None);
}

/// BL-096: `remove_panel_geometry` で hit 情報を消すと、その後の検索は None。
#[test]
fn remove_panel_geometry_clears_hits_for_panel() {
    let mut panel_workspace = PanelWorkspace::new();
    panel_workspace.update_panel_geometry(
        "html.test",
        geometry::WindowRect {
            x: 0,
            y: 0,
            width: 100,
            height: 124,
        },
        0,
        vec![(
            "btn".to_string(),
            geometry::WindowRect {
                x: 10,
                y: 10,
                width: 40,
                height: 20,
            },
        )],
    );
    assert!(panel_workspace.panel_hit_at(geometry::WindowPoint::new(20, 20)).is_some());

    panel_workspace.remove_panel_geometry("html.test");
    assert!(panel_workspace.panel_hit_at(geometry::WindowPoint::new(20, 20)).is_none());
}

/// BL-096: full_rect / move_handle_rect / body_rect / node_hits が原子的に揃う。
/// 1 回の update で full rect 取得・move handle hit・body hit・resize hit がすべて整合する。
#[test]
fn update_panel_geometry_keeps_all_rects_consistent() {
    let mut panel_workspace = PanelWorkspace::new();
    let full_rect = geometry::WindowRect {
        x: 200,
        y: 100,
        width: 300,
        height: 200,
    };
    panel_workspace.update_panel_geometry(
        "html.test",
        full_rect,
        24,
        vec![(
            "btn".to_string(),
            geometry::WindowRect {
                x: 10,
                y: 10,
                width: 50,
                height: 20,
            },
        )],
    );

    // full_rect は与えた矩形そのもの
    assert_eq!(panel_workspace.panel_full_rect("html.test"), Some(full_rect));
    // chrome (上端 24px) は move handle
    assert_eq!(
        panel_workspace.panel_move_handle_at(geometry::WindowPoint::new(350, 110)),
        Some("html.test".to_string())
    );
    // body 原点 (200, 124) 基準で hit (10,10) → screen (210, 134)
    assert_eq!(
        panel_workspace.panel_hit_at(geometry::WindowPoint::new(220, 140)),
        Some(("html.test".to_string(), "btn".to_string()))
    );
    // resize hit は full_rect の角 (NW)
    assert_eq!(
        panel_workspace.panel_resize_hit_at(geometry::WindowPoint::new(202, 102)),
        Some(("html.test".to_string(), ResizeHandle::NorthWest))
    );
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
    use crate::{
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
    use crate::{
        WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
        WorkspacePanelState,
    };
    use geometry::WindowRect;

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
