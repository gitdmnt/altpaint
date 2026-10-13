//! 監査指摘 (R9 / R11 / R12) の再現テスト (GPU 不要)。
//!
//! 各テストは「正しい振る舞い」を assert する。失敗 = バグ再現。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use frame_profiler::FrameProfiler;
use geometry::WindowPoint;
use panel_runtime::{PanelEvent, PanelPointerInput, PanelPointerKind, ServiceRequest, services::names};
use panel_workspace::WorkspaceLayout;

use crate::app::{DesktopApp, DesktopAppOptions};
use crate::features::project::{
    DesktopSessionState, default_canvas_size_presets, save_canvas_size_presets, save_session_state,
};
use crate::features::workspace::{
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION, WorkspacePresetCatalog, save_workspace_preset_catalog,
};
use crate::platform::DesktopDialogs;

use super::unique_test_path;

static REPRO_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn repro_unique_dir(name: &str) -> PathBuf {
    let id = REPRO_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "altpaint-repro-{name}-{}-{id}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos()
    ))
}

/// show_error の呼出を外から観測できるダイアログ。
#[derive(Clone, Default)]
struct RecordingDialogs {
    errors: Rc<RefCell<Vec<(String, String)>>>,
}

impl DesktopDialogs for RecordingDialogs {
    fn pick_open_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        None
    }
    fn pick_save_project_path(&self, _current_path: &Path) -> Option<PathBuf> {
        None
    }
    fn pick_save_workspace_preset_path(&self, _current_path: &Path) -> Option<PathBuf> {
        None
    }
    fn pick_open_pen_path(&self, _current_path: &Path) -> Option<PathBuf> {
        None
    }
    fn show_error(&self, title: &str, message: &str) {
        self.errors
            .borrow_mut()
            .push((title.to_string(), message.to_string()));
    }
}

fn repro_app_for_project(project_path: PathBuf, dialogs: RecordingDialogs) -> DesktopApp {
    DesktopApp::with_options(DesktopAppOptions {
        project_path,
        dialogs: Box::new(dialogs),
        session_path: unique_test_path("repro-session"),
        workspace_preset_path: unique_test_path("repro-workspace-presets"),
        canvas_size_preset_path: unique_test_path("repro-canvas-size"),
    })
}

// ---------------------------------------------------------------------------
// R9: 起動時のプロジェクト読込失敗の握り潰し
// ---------------------------------------------------------------------------

/// 読めないプロジェクトファイルで起動 → Ctrl+S (project.save_current) しても、
/// 元ファイルが空文書で上書きされてはならない。
fn repro_assert_unreadable_project_survives_startup_and_save(label: &str, original: Vec<u8>) {
    let dir = repro_unique_dir(label);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let project_path = dir.join("owner-manuscript.altp.json");
    std::fs::write(&project_path, &original).expect("write unreadable project");
    assert!(
        project_store::load_project_from_path(&project_path).is_err(),
        "前提: このファイルは読み込めないこと"
    );

    let dialogs = RecordingDialogs::default();
    let mut app = repro_app_for_project(project_path.clone(), dialogs.clone());
    let bound_path = app.paths.project_path.clone();
    let errors_after_startup = dialogs.errors.borrow().len();
    let komas_after_startup = app.document.active_page_koma_count();

    let accepted = app.execute_service_request(ServiceRequest::new(names::PROJECT_SAVE_CURRENT));
    app.wait_for_pending_save_tasks();

    let after = std::fs::read(&project_path).unwrap_or_default();
    let now_loads_as_empty = project_store::load_project_from_path(&project_path).is_ok();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        after == original,
        "[{label}] 読めなかった原稿ファイルが上書きされた: \
         起動後 project_path={} (起動パスと同一={}), 起動時 show_error 件数={errors_after_startup}, \
         起動後コマ数={komas_after_startup}, save_current 受理={accepted}, \
         元 {} bytes → 保存後 {} bytes, 保存後に有効プロジェクトとして読める={now_loads_as_empty}",
        bound_path.display(),
        bound_path == project_path,
        original.len(),
        after.len(),
    );
}

#[test]
fn repro_startup_with_garbage_project_file_does_not_overwrite_it_on_save() {
    let garbage: Vec<u8> = (0..4096u32).map(|i| (i * 31 % 251) as u8).collect();
    repro_assert_unreadable_project_survives_startup_and_save("garbage-bytes", garbage);
}

#[test]
fn repro_startup_with_corrupt_sqlite_project_does_not_overwrite_it_on_save() {
    // 有効な SQLite プロジェクトを作り、ヘッダーを残して途中で切り詰める (破損 SQLite)。
    let dir = repro_unique_dir("make-corrupt-sqlite");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let source = dir.join("source.altp.json");
    let mut document = document_model::Document::new(64, 64);
    document.work.title = "Owner's manuscript".to_string();
    project_store::save_project_to_path(
        &source,
        &document,
        &WorkspaceLayout::default(),
        &BTreeMap::new(),
    )
    .expect("save source project");
    let mut bytes = std::fs::read(&source).expect("read source");
    bytes.truncate(1024);
    let _ = std::fs::remove_dir_all(&dir);
    repro_assert_unreadable_project_survives_startup_and_save("corrupt-sqlite", bytes);
}

// ---------------------------------------------------------------------------
// R11: ユーザーデータディレクトリ不在時の保存
// ---------------------------------------------------------------------------

/// 親ディレクトリ (dirs::data_dir()/altpaint 相当) が存在しない初回起動環境でも、
/// session / workspace preset / canvas size preset / 既定 project の保存は成功すべき。
#[test]
fn repro_persistence_saves_succeed_when_user_data_dir_is_missing() {
    let user_data_dir = repro_unique_dir("missing-data-dir").join("altpaint");
    assert!(!user_data_dir.exists(), "前提: ディレクトリが存在しないこと");

    let catalog = WorkspacePresetCatalog {
        format_version: CURRENT_WORKSPACE_PRESET_FORMAT_VERSION,
        default_preset_id: "default-floating".to_string(),
        presets: Vec::new(),
    };
    let results: Vec<(&str, Result<(), String>)> = vec![
        (
            "save_session_state",
            save_session_state(
                user_data_dir.join("altpaint-session.json"),
                &DesktopSessionState::default(),
            )
            .map_err(|e| e.to_string()),
        ),
        (
            "save_workspace_preset_catalog",
            save_workspace_preset_catalog(user_data_dir.join("workspace-presets.json"), &catalog)
                .map_err(|e| e.to_string()),
        ),
        (
            "save_canvas_size_presets",
            save_canvas_size_presets(
                user_data_dir.join("canvas-templates.json"),
                &default_canvas_size_presets(),
            )
            .map_err(|e| e.to_string()),
        ),
        (
            "project_store::save_project_to_path",
            project_store::save_project_to_path(
                user_data_dir.join("altpaint-project.altp.json"),
                &document_model::Document::new(32, 32),
                &WorkspaceLayout::default(),
                &BTreeMap::new(),
            )
            .map_err(|e| e.to_string()),
        ),
    ];
    let _ = std::fs::remove_dir_all(user_data_dir.parent().expect("parent"));

    let failures: Vec<String> = results
        .iter()
        .filter_map(|(name, result)| result.as_ref().err().map(|e| format!("{name}: {e}")))
        .collect();
    assert!(
        failures.is_empty(),
        "データディレクトリ不在で保存が失敗した ({}/4): {failures:#?}",
        failures.len()
    );
}

/// 初回起動 (データディレクトリ不在) で DesktopApp を構築したとき、既定の
/// workspace preset / canvas size preset ファイルと session が作られるべき。
#[test]
fn repro_first_launch_creates_persistence_files_under_missing_user_data_dir() {
    let user_data_dir = repro_unique_dir("first-launch").join("altpaint");
    let session_path = user_data_dir.join("altpaint-session.json");
    let workspace_preset_path = user_data_dir.join("workspace-presets.json");
    let canvas_size_preset_path = user_data_dir.join("canvas-templates.json");
    let app = DesktopApp::with_options(DesktopAppOptions {
        project_path: user_data_dir.join("altpaint-project.altp.json"),
        dialogs: Box::new(RecordingDialogs::default()),
        session_path: session_path.clone(),
        workspace_preset_path: workspace_preset_path.clone(),
        canvas_size_preset_path: canvas_size_preset_path.clone(),
    });
    app.persist_session_state();

    let exists = [
        ("session", session_path.exists()),
        ("workspace-presets", workspace_preset_path.exists()),
        ("canvas-templates", canvas_size_preset_path.exists()),
    ];
    let _ = std::fs::remove_dir_all(user_data_dir.parent().expect("parent"));
    assert!(
        exists.iter().all(|(_, e)| *e),
        "初回起動で永続化ファイルが作られていない: {exists:?}"
    );
}

// ---------------------------------------------------------------------------
// R12: パネル入力 (スライダー / テキスト入力 / li リスト)
// ---------------------------------------------------------------------------

const VIEWPORT: (usize, usize) = (1280, 800);

/// 対象パネルだけを表示し、キャンバス上へ移動・十分なサイズを与えて hit テーブルを更新する。
fn repro_isolate_panel(app: &mut DesktopApp, panel_id: &str, size: (u32, u32)) {
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(VIEWPORT.0, VIEWPORT.1, &mut profiler);
    let layout = app.layout.clone().expect("layout exists");
    for id in app.panel_runtime.panel_ids_with_gpu() {
        let _ = app.panel_workspace.set_panel_visibility(&id, id == panel_id);
    }
    assert!(app.panel_runtime.restore_panel_size(panel_id, size));
    let _ = app.panel_workspace.set_panel_size(panel_id, size.0 as usize, size.1 as usize);
    let _ = app.panel_workspace.move_panel_to(
        panel_id,
        layout.canvas_display_rect.x + 24,
        layout.canvas_display_rect.y + 24,
        layout.window_rect.width,
        layout.window_rect.height,
    );
    app.request_panel_reconcile();
    let _ = app.prepare_present_frame(VIEWPORT.0, VIEWPORT.1, &mut profiler);
}

/// (element_id, data_action, data_args, rect(x,y,w,h)) の一覧。
type ReproHit = (Option<String>, String, Option<String>, (u32, u32, u32, u32));

fn repro_panel_hits(app: &mut DesktopApp, panel_id: &str) -> Vec<ReproHit> {
    let hits = app.panel_runtime.collect_panel_hits(
        &[(panel_id.to_string(), VIEWPORT.0 as u32, VIEWPORT.1 as u32)],
        1.0,
        crate::app::PANEL_CHROME_HEIGHT,
    );
    hits.into_iter()
        .filter(|(id, _)| id == panel_id)
        .flat_map(|(_, hits)| hits)
        .map(|hit| {
            (
                hit.element_id,
                hit.data_action,
                hit.data_args,
                (hit.rect.x, hit.rect.y, hit.rect.width, hit.rect.height),
            )
        })
        .collect()
}

/// パネルローカル hit 矩形内の比率 (fx, fy) の screen 座標。
fn repro_screen_point(app: &DesktopApp, panel_id: &str, rect: (u32, u32, u32, u32), fx: f32, fy: f32) -> WindowPoint {
    let full = app
        .panel_workspace
        .panel_full_rect(panel_id)
        .expect("panel full rect exists");
    let chrome = crate::app::PANEL_CHROME_HEIGHT as usize;
    let x = full.x + rect.0 as usize + ((rect.2 as f32) * fx) as usize;
    let y = full.y + chrome + rect.1 as usize + ((rect.3 as f32) * fy) as usize;
    WindowPoint::new(x as i32, y as i32)
}

/// event_loop/pointer.rs と同じ順序 (Blitz 転送 → app 処理) でポインタを送る。
fn repro_forward(app: &mut DesktopApp, point: WindowPoint, kind: PanelPointerKind) {
    if let Some((panel_id, local)) = app.panel_workspace.panel_at(point) {
        let _ = app.forward_panel_input(
            &panel_id,
            PanelPointerInput {
                kind,
                local_x: local.x as f32,
                local_y: local.y as f32,
                screen_x: point.x as f32,
                screen_y: point.y as f32,
            },
        );
    }
}

fn repro_mouse_drag(app: &mut DesktopApp, from: WindowPoint, to: WindowPoint) {
    repro_forward(app, from, PanelPointerKind::Down);
    let _ = app.handle_pointer_pressed(from.x, from.y);
    repro_forward(app, to, PanelPointerKind::Move);
    let _ = app.handle_pointer_dragged(to.x, to.y);
    repro_forward(app, to, PanelPointerKind::Up);
    let _ = app.handle_pointer_released(to.x, to.y);
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(VIEWPORT.0, VIEWPORT.1, &mut profiler);
}

fn repro_find_hit(hits: &[ReproHit], element_id: &str) -> (u32, u32, u32, u32) {
    hits.iter()
        .find(|hit| hit.0.as_deref() == Some(element_id))
        .unwrap_or_else(|| panic!("hit for #{element_id} exists: {hits:#?}"))
        .3
}

/// view-controls の拡大率スライダーを右寄り 80% の位置へドラッグしたら、拡大率は
/// その位置に対応する値 (25..1600% の 80% ≒ 1285%) に近づくべき (少なくとも 100% より大きい)。
#[test]
fn repro_view_controls_zoom_slider_drag_sets_zoom_from_pointer_position() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let panel = "builtin.view-controls";
    repro_isolate_panel(&mut app, panel, (320, 640));
    let hits = repro_panel_hits(&mut app, panel);
    let slider = repro_find_hit(&hits, "zoom-slider");
    let before = app.document.session.view_transform.zoom;

    let from = repro_screen_point(&app, panel, slider, 0.5, 0.5);
    let to = repro_screen_point(&app, panel, slider, 0.8, 0.5);
    repro_mouse_drag(&mut app, from, to);

    let after = app.document.session.view_transform.zoom;
    assert!(
        after > 1.0,
        "スライダーを 80% 位置へドラッグしたのに zoom が {before} → {after} (期待: > 1.0)"
    );
}

/// PanelRuntime レベル: スライダー上で Down/Move/Up を Blitz へ送った後に、desktop が
/// 生成するのと同じ Activate を dispatch したとき、ホストへ届く要求の payload に
/// ポインタ位置由来の値が入るべき。
#[test]
fn repro_panel_runtime_slider_request_payload_carries_pointer_value() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let panel = "builtin.view-controls";
    repro_isolate_panel(&mut app, panel, (320, 640));
    let hits = repro_panel_hits(&mut app, panel);
    let slider = repro_find_hit(&hits, "zoom-slider");
    let from = repro_screen_point(&app, panel, slider, 0.5, 0.5);
    let to = repro_screen_point(&app, panel, slider, 0.8, 0.5);
    repro_forward(&mut app, from, PanelPointerKind::Down);
    repro_forward(&mut app, to, PanelPointerKind::Move);
    repro_forward(&mut app, to, PanelPointerKind::Up);

    // desktop の pointer 経路 (panel_event_from_window) が生成するイベントそのもの。
    let (hit_panel, node_id) = app
        .panel_workspace
        .panel_hit_at(to)
        .expect("slider is in hit table");
    let event = PanelEvent::Activate {
        panel_id: hit_panel,
        node_id,
    };
    let result = app.panel_runtime.dispatch_event(&event);
    let rendered: Vec<String> = result.actions.iter().map(|a| format!("{a:?}")).collect();
    let zoom_payload = result.actions.iter().find_map(|action| match action {
        panel_runtime::HostRequest::RequestService(request) if request.name == names::VIEW_SET_ZOOM => {
            request.payload.get("zoom").and_then(|v| v.as_f64())
        }
        _ => None,
    });
    assert!(
        zoom_payload.is_some_and(|zoom| zoom > 1.0),
        "event={event:?} → actions={rendered:#?} (期待: view.set_zoom の zoom が 80% 位置相当 > 1.0)"
    );
}

/// tool-settings の太さスライダーを 90% 位置でクリックしたら太さはその位置相当
/// (対数スケール ≒ 3981px) になるべき (少なくとも初期値より大きい)。
#[test]
fn repro_tool_settings_size_slider_click_sets_size_from_pointer_position() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let panel = "builtin.tool-settings";
    repro_isolate_panel(&mut app, panel, (320, 640));
    let hits = repro_panel_hits(&mut app, panel);
    let slider = repro_find_hit(&hits, "pen.size");
    let before = app.document.session.active_pen_size;

    let point = repro_screen_point(&app, panel, slider, 0.9, 0.5);
    repro_mouse_drag(&mut app, point, point);

    let after = app.document.session.active_pen_size;
    assert!(
        after > before,
        "太さスライダー 90% 位置クリックで active_pen_size が {before} → {after} (期待: 増加)"
    );
}

/// text-flow のテキスト入力をクリックして "hello" をキー入力し、描画ボタンを押したら
/// キャンバスにテキスト画素が描かれるべき。
#[test]
fn repro_text_flow_typed_text_reaches_host_render() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let panel = "builtin.text-flow";
    repro_isolate_panel(&mut app, panel, (320, 640));
    let hits = repro_panel_hits(&mut app, panel);
    let input = repro_find_hit(&hits, "input-text");
    let render = repro_find_hit(&hits, "render");
    let tool_before = app.document.session.active_tool();

    let input_point = repro_screen_point(&app, panel, input, 0.5, 0.5);
    repro_mouse_drag(&mut app, input_point, input_point);
    // event_loop/keyboard.rs と同じく、キー押下は dispatch_keyboard_shortcut に流れる。
    for key in ["H", "E", "L", "L", "O"] {
        let _ = app.dispatch_keyboard_shortcut(key, key, false);
    }
    let tool_after_typing = app.document.session.active_tool();
    let render_point = repro_screen_point(&app, panel, render, 0.5, 0.5);
    repro_mouse_drag(&mut app, render_point, render_point);

    let frame = crate::app::cpu_canvas_snapshot::build_cpu_canvas_snapshot(&app.document);
    let non_white = frame
        .pixels
        .chunks_exact(4)
        .filter(|px| *px != [255, 255, 255, 255])
        .count();
    assert!(
        non_white > 0,
        "テキスト入力→描画でキャンバスに何も描かれない (非白画素 {non_white})。\
         参考: 入力中のキーで active_tool が {tool_before:?} → {tool_after_typing:?}"
    );
}

/// 対照実験 (通るべき): テキストを SetText で直接注入すれば、同じ描画ボタンのクリック経路で
/// キャンバスに描かれる。これが通れば、上の失敗は「入力値がパネルへ届かない」ことに起因する。
#[test]
fn repro_control_text_flow_injected_text_renders_via_button_click() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let panel = "builtin.text-flow";
    repro_isolate_panel(&mut app, panel, (320, 640));
    let hits = repro_panel_hits(&mut app, panel);
    let render = repro_find_hit(&hits, "render");
    assert!(app.dispatch_panel_event(PanelEvent::SetText {
        panel_id: panel.to_string(),
        node_id: "input-text".to_string(),
        value: "hello".to_string(),
    }));
    let render_point = repro_screen_point(&app, panel, render, 0.5, 0.5);
    repro_mouse_drag(&mut app, render_point, render_point);
    assert!(repro_count_non_white(&app) > 0, "対照: 注入テキストが描画されない");
}

fn repro_count_non_white(app: &DesktopApp) -> usize {
    let frame = crate::app::cpu_canvas_snapshot::build_cpu_canvas_snapshot(&app.document);
    frame
        .pixels
        .chunks_exact(4)
        .filter(|px| *px != [255, 255, 255, 255])
        .count()
}

/// layers パネルの render_action_list 生成 `<li>` (id 無し) をクリックしたらレイヤー選択が発行されるべき。
#[test]
fn repro_layers_list_item_click_selects_layer() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let _ = app.apply_document_command(&document_model::DocumentCommand::AddRasterLayer);
    let panel = "builtin.layers";
    repro_isolate_panel(&mut app, panel, (320, 720));
    let active_before = app.document.active_koma().expect("koma").active_layer_index;
    assert_eq!(active_before, 1, "前提: 追加レイヤーが選択中");
    let background_id = app.document.active_koma().expect("koma").layers[0].id.0;

    let hits = repro_panel_hits(&mut app, panel);
    let items: Vec<&ReproHit> = hits
        .iter()
        .filter(|hit| hit.1 == "altp:activate:select_layer")
        .collect();
    assert!(items.len() >= 2, "前提: select_layer の <li> が 2 件以上: {hits:#?}");
    let target = items
        .iter()
        .find(|hit| {
            hit.2
                .as_deref()
                .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
                .and_then(|args| args.get("id").and_then(|v| v.as_u64()))
                == Some(background_id)
        })
        .unwrap_or_else(|| panic!("background layer <li> exists: {items:#?}"));
    let point = repro_screen_point(&app, panel, target.3, 0.3, 0.5);
    let in_hit_table = app.panel_workspace.panel_hit_at(point);
    let painted_before = repro_count_non_white(&app);

    repro_mouse_drag(&mut app, point, point);

    let active_after = app.document.active_koma().expect("koma").active_layer_index;
    let painted_after = repro_count_non_white(&app);
    assert_eq!(
        active_after, 0,
        "<li> (element_id={:?}) クリックでレイヤー選択されない: active_layer_index {active_before} → {active_after}; \
         hit テーブル結果={in_hit_table:?}; クリック前後のキャンバス非白画素 {painted_before} → {painted_after}",
        target.0
    );
}

/// koma-list パネルの `<li>` (id 無し) をクリックしたらコマ選択が発行されるべき。
#[test]
fn repro_koma_list_item_click_selects_koma() {
    let mut app = repro_app_for_project(super::unique_test_project_path(), RecordingDialogs::default());
    let _ = app.apply_document_command(&document_model::DocumentCommand::AddKoma);
    let panel = "builtin.koma-list";
    repro_isolate_panel(&mut app, panel, (320, 720));
    let active_before = app.document.active_koma_index();
    assert_eq!(active_before, 1, "前提: 追加コマが選択中");

    let hits = repro_panel_hits(&mut app, panel);
    let items: Vec<&ReproHit> = hits
        .iter()
        .filter(|hit| hit.1 == "altp:activate:select_koma")
        .collect();
    assert!(items.len() >= 2, "前提: select_koma の <li> が 2 件以上: {hits:#?}");
    let target = items
        .iter()
        .find(|hit| {
            hit.2
                .as_deref()
                .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
                .and_then(|args| args.get("value").and_then(|v| v.as_u64()))
                == Some(0)
        })
        .unwrap_or_else(|| panic!("koma 0 <li> exists: {items:#?}"));
    let point = repro_screen_point(&app, panel, target.3, 0.3, 0.5);
    let in_hit_table = app.panel_workspace.panel_hit_at(point);
    let painted_before = repro_count_non_white(&app);

    repro_mouse_drag(&mut app, point, point);

    let active_after = app.document.active_koma_index();
    let painted_after = repro_count_non_white(&app);
    assert_eq!(
        active_after, 0,
        "<li> (element_id={:?}) クリックでコマ選択されない: active_koma_index {active_before} → {active_after}; \
         hit テーブル結果={in_hit_table:?}; クリック前後のキャンバス非白画素 {painted_before} → {painted_after}",
        target.0
    );
}
