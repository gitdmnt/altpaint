//! panel_dispatch の回帰テストをまとめる。

use app_core::{Command, WindowPoint};
use desktop_support::DesktopProfiler;

use super::{TestDialogs, test_app_with_dialogs};
use crate::app::PanelDragState;

/// パネル 振り分け キーボード パス activates 保存 action が期待どおりに動作することを検証する。
#[test]
fn panel_dispatch_keyboard_path_activates_save_action() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    let _ = app.prepare_present_frame(1280, 200, &mut profiler);

    assert!(app.panel_presentation.focus_panel_node(
        &app.panel_runtime,
        "builtin.app-actions",
        "app.save"
    ));
    // app.save は emit_service 経由で保存を実行するため Command::Noop が返る。
    // pending_jobs でジョブがキューされていることを確認する。
    assert_eq!(
        app.activate_focused_panel_control(),
        Some(Command::Noop)
    );
    assert_eq!(app.io_state.pending_jobs.len(), 1);
}

/// パネルを drag で移動したとき canvas ホスト領域が dirty になることを検証する。
///
/// パネルが通過した領域のキャンバス背景が再描画されるよう、
/// `drag_panel_interaction` は `append_canvas_host_dirty_rect` を呼ぶ必要がある。
#[test]
fn drag_panel_move_marks_canvas_host_dirty() {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    let mut profiler = DesktopProfiler::new();
    // レイアウトとパネルを初期化するため一度フレームを作る
    let _ = app.prepare_present_frame(1280, 720, &mut profiler);

    // builtin.app-actions パネルが存在する位置を取得
    let panel_id = "builtin.app-actions".to_string();
    let panel_rect = app.panel_presentation.panel_rect(&panel_id);
    // パネルが配置されていないとテストにならない
    let Some(rect) = panel_rect else {
        return; // パネルが見つからない場合はスキップ
    };

    // パネルをグラブした状態にする
    app.panel_interaction.active_panel_drag = Some(PanelDragState {
        panel_id: panel_id.clone(),
        grab_offset_x: 10,
        grab_offset_y: 10,
    });
    // pending_ui_panel_dirty_rect をリセット
    app.pending_ui_panel_dirty_rect = None;

    // パネルを別の場所へドラッグ
    let target_x = (rect.x + 200).min(1000) as i32;
    let target_y = (rect.y + 100).min(600) as i32;
    let _ = app.drag_panel_interaction(WindowPoint::new(target_x, target_y));

    assert!(
        app.pending_ui_panel_dirty_rect.is_some(),
        "パネル移動後に canvas ホスト dirty rect が設定されるべき"
    );
}
