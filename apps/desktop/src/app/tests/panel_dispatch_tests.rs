//! panel_dispatch の回帰テストをまとめる。

use app_core::{Command, WindowPoint};
use desktop_support::FrameProfiler;
use panel_runtime::HostAction;

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::PanelDragState;

#[test]
fn panel_dispatch_keyboard_path_activates_save_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(
        app.panel_workspace
            .focus_panel_node("builtin.app-actions", "app.save")
    );
    // app.save は emit_service 経由で保存を実行するため Command::Noop が返る。
    // pending_jobs でジョブがキューされていることを確認する。
    assert_eq!(
        app.activate_focused_panel_control(),
        Some(Command::Noop)
    );
    assert_eq!(app.background_jobs.len(), 1);
}

/// パネルを drag で移動したとき canvas ホスト領域が dirty になることを検証する。
///
/// パネルが通過した領域のキャンバス背景が再描画されるよう、
/// `drag_panel_interaction` は `append_canvas_host_dirty_rect` を呼ぶ必要がある。
#[test]
fn drag_panel_move_marks_canvas_host_dirty() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    // レイアウトとパネルを初期化するため一度フレームを作る
    let _ = app.prepare_present_frame(1280, 720, &mut profiler);

    // builtin.app-actions パネルが存在する位置を取得
    let panel_id = "builtin.app-actions".to_string();
    let panel_rect = app.panel_workspace.panel_rect(&panel_id, 1280, 720);
    // パネルが配置されていないとテストにならない
    let Some(rect) = panel_rect else {
        return; // パネルが見つからない場合はスキップ
    };

    // パネルをグラブした状態にする
    app.panel_interaction.active_panel_drag = Some(PanelDragState {
        panel_id: panel_id.clone(),
        grab_offset: app_core::PanelSurfacePoint::new(10, 10),
    });
    // pending_ui_panel_dirty_rect をリセット
    app.invalidation.ui_panel_dirty_rect = None;

    // パネルを別の場所へドラッグ
    let target_x = (rect.x + 200).min(1000) as i32;
    let target_y = (rect.y + 100).min(600) as i32;
    let _ = app.drag_panel_interaction(WindowPoint::new(target_x, target_y));

    assert!(
        app.invalidation.ui_panel_dirty_rect.is_some(),
        "パネル移動後に canvas ホスト dirty rect が設定されるべき"
    );
}

/// BL-051: 右/下アンカーパネルを非表示にしたとき、直前矩形の dirty rect が
/// viewport 内で解決される回帰テスト。
///
/// 旧 viewport なし版 `panel_rect` は `usize::MAX` フォールバックで
/// BottomRight アンカーのパネル矩形を画面外座標に解決し、dirty rect を
/// 無効化していた (= 再描画されない)。
#[test]
fn hiding_bottom_right_anchored_panel_marks_dirty_rect_within_viewport() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
    app.invalidation.ui_panel_dirty_rect = None;

    // builtin.tool-settings は BottomRight アンカーが既定
    assert!(app.execute_host_action(HostAction::SetPanelVisibility {
        panel_id: "builtin.tool-settings".to_string(),
        visible: false,
    }));

    let dirty = app
        .invalidation
        .ui_panel_dirty_rect
        .expect("パネル非表示時に直前矩形が dirty になるべき");
    assert!(
        dirty.x + dirty.width <= 1280 && dirty.y + dirty.height <= 800,
        "dirty rect は viewport 内で解決されるべき: {dirty:?}"
    );
}
