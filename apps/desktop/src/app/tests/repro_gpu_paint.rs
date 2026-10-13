//! 静的監査の指摘 (gpu-paint 系) を本番と同じ経路で再現する失敗テスト群。
//!
//! GPU 系テストは本番と同じく `install_gpu_resources` 済みの `DesktopApp` と
//! `GpuPaintBackend` 経路でストロークを描き、`LayerTextureStore::read_back_full` で
//! GPU テクスチャ内容を読み戻して検証する。GPU アダプタが取れない環境では
//! 黙ってパスさせず panic させる (skip を成功扱いにしない)。
//!
//! 各テストは「正しい振る舞い」を assert する。バグがあれば失敗する。

use std::sync::Arc;

use document_model::{DocumentCommand, KomaId};
use editor_state::SessionCommand;
use frame_profiler::FrameProfiler;
use geometry::{PagePoint, WindowPoint};
use paint_engine::CanvasPointerAction;
use panel_runtime::{ServiceRequest, services::names};

use super::{TestDialogs, test_app_with_dialogs, unique_test_path};
use crate::app::DesktopApp;

const PAGE: usize = 512;

// ---------------------------------------------------------------------------
// GPU 初期化
// ---------------------------------------------------------------------------

async fn try_init_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>, String)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .ok()?;
    let info = adapter.get_info();
    let desc = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
    if !gpu_paint::format_check::supports_rgba8unorm_storage(&adapter) {
        eprintln!("[repro] adapter {desc} does NOT support Rgba8Unorm storage");
        return None;
    }
    let storage_format_features =
        adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("repro-gpu-device"),
            required_features: storage_format_features,
            experimental_features: Default::default(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        })
        .await
        .ok()?;
    Some((Arc::new(device), Arc::new(queue), desc))
}

fn gpu_device() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let (device, queue, desc) = pollster::block_on(try_init_device())
        .expect("GPU アダプタ (Rgba8Unorm storage 対応) が取得できない: repro は GPU 必須");
    eprintln!("[repro] using adapter: {desc}");
    (device, queue)
}

/// 本番と同じ順序 (GPU 資源インストール → フレーム準備) で GPU 有効な app を作る。
fn gpu_app() -> DesktopApp {
    let (device, queue) = gpu_device();
    let mut app = test_app_with_dialogs(TestDialogs::default());
    app.install_gpu_resources(device, queue);
    finish_setup(&mut app);
    assert!(app.gpu.is_some());
    app
}

/// GPU 資源なし (CPU 経路) の app。
fn cpu_app() -> DesktopApp {
    let mut app = test_app_with_dialogs(TestDialogs::default());
    finish_setup(&mut app);
    assert!(app.gpu.is_none());
    app
}

fn finish_setup(app: &mut DesktopApp) {
    // 読み戻しを軽くするため小さい文書にする (Full 同期経由)。
    app.apply_document_command(&DocumentCommand::NewDocumentSized {
        width: PAGE,
        height: PAGE,
    });
    let _ = app.apply_session_command(&SessionCommand::SetActivePenSize { size: 9 });
    let _ = app.apply_session_command(&SessionCommand::SetActivePenStabilization { amount: 0 });
    frame(app);
}

fn frame(app: &mut DesktopApp) {
    let mut profiler = FrameProfiler::new();
    let _ = app.prepare_present_frame(1280, 800, &mut profiler);
}

// ---------------------------------------------------------------------------
// 入力・座標
// ---------------------------------------------------------------------------

fn canvas_center(app: &DesktopApp) -> (i32, i32) {
    let layout = app.layout.clone().expect("layout exists");
    (
        (layout.canvas_display_rect.x + layout.canvas_display_rect.width / 2) as i32,
        (layout.canvas_display_rect.y + layout.canvas_display_rect.height / 2) as i32,
    )
}

fn wp(x: i32, y: i32) -> WindowPoint {
    WindowPoint::new(x, y)
}

/// Down → Drag... → Up。最後の点は直前の Drag と異なる座標にすること
/// (同一だと R6 の「履歴が閉じない」挙動に巻き込まれるため)。
fn stroke(app: &mut DesktopApp, points: &[WindowPoint]) {
    assert!(points.len() >= 2);
    app.handle_canvas_pointer(CanvasPointerAction::Down, points[0], 1.0);
    for p in &points[1..points.len() - 1] {
        app.handle_canvas_pointer(CanvasPointerAction::Drag, *p, 1.0);
    }
    app.handle_canvas_pointer(CanvasPointerAction::Up, *points.last().unwrap(), 1.0);
}

fn tap(app: &mut DesktopApp, p: WindowPoint) {
    app.handle_canvas_pointer(CanvasPointerAction::Down, p, 1.0);
    app.handle_canvas_pointer(CanvasPointerAction::Up, p, 1.0);
}

fn page_point(app: &DesktopApp, p: WindowPoint) -> PagePoint {
    app.canvas_position_from_window_clamped(p)
        .expect("window point maps into page")
}

/// アクティブコマのローカル座標 (x, y)。
fn local(app: &DesktopApp, p: WindowPoint) -> (usize, usize) {
    let page = page_point(app, p);
    let bounds = app.document.active_koma_bounds().expect("active koma");
    let local = bounds.canvas_to_koma_local(page).expect("inside active koma");
    (local.x, local.y)
}

fn active_koma_id(app: &DesktopApp) -> KomaId {
    app.document.active_koma().expect("active koma").id
}

// ---------------------------------------------------------------------------
// 画素読み出し
// ---------------------------------------------------------------------------

struct Pixels {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Pixels {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }
    fn dark_at(&self, x: usize, y: usize) -> bool {
        is_dark(self.at(x, y))
    }
    fn dark_count(&self) -> usize {
        self.data
            .chunks_exact(4)
            .filter(|p| is_dark([p[0], p[1], p[2], p[3]]))
            .count()
    }
    fn diff_count(&self, other: &Pixels) -> usize {
        self.data
            .chunks_exact(4)
            .zip(other.data.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count()
    }
}

/// 黒ペン (既定色) で塗られた画素: 暗く、不透明。
fn is_dark(p: [u8; 4]) -> bool {
    p[0] < 128 && p[1] < 128 && p[2] < 128 && p[3] > 128
}

fn gpu_layer(app: &DesktopApp, koma_id: KomaId, layer_index: usize) -> Pixels {
    let (w, h, data) = app
        .layer_texture_store()
        .expect("gpu pool")
        .read_back_full(gpu_paint::KomaTextureId(koma_id.0), layer_index)
        .expect("layer texture readback");
    Pixels {
        width: w as usize,
        height: h as usize,
        data,
    }
}

fn cpu_layer(app: &DesktopApp, koma_id: KomaId, layer_index: usize) -> Pixels {
    let bitmap = app
        .document
        .clone_koma_layer_bitmap(koma_id, layer_index)
        .expect("cpu layer bitmap");
    Pixels {
        width: bitmap.width,
        height: bitmap.height,
        data: bitmap.pixels,
    }
}

// ---------------------------------------------------------------------------
// probe
// ---------------------------------------------------------------------------

/// このマシンで GPU アダプタが取れ、本番 GPU 経路でストロークが GPU テクスチャへ
/// 描かれることを確認する (以降の repro の前提)。
#[test]
fn repro_probe_gpu_adapter_and_gpu_stroke_reaches_texture() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    let a = wp(cx - 40, cy);
    stroke(&mut app, &[a, wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)]);
    let koma = active_koma_id(&app);
    let gpu = gpu_layer(&app, koma, 0);
    let cpu = cpu_layer(&app, koma, 0);
    let (lx, ly) = local(&app, wp(cx, cy));
    eprintln!(
        "[repro] probe: gpu dark={} cpu dark={} gpu center={:?} cpu center={:?} tex={}x{}",
        gpu.dark_count(),
        cpu.dark_count(),
        gpu.at(lx, ly),
        cpu.at(lx, ly),
        gpu.width,
        gpu.height
    );
    assert!(gpu.dark_at(lx, ly), "GPU テクスチャにストロークが描かれていない");
}

// ---------------------------------------------------------------------------
// R1: レイヤー構成変更で GPU テクスチャが古い CPU bitmap から再構築される
// ---------------------------------------------------------------------------

fn r1_stroke_then(command: impl FnOnce(&mut DesktopApp)) -> (bool, usize, usize) {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    stroke(
        &mut app,
        &[wp(cx - 40, cy), wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)],
    );
    let koma = active_koma_id(&app);
    let (lx, ly) = local(&app, wp(cx, cy));
    let before = gpu_layer(&app, koma, 0);
    assert!(before.dark_at(lx, ly), "前提: ストロークが GPU に描かれている");
    let before_dark = before.dark_count();

    command(&mut app);

    // 元のレイヤー (背景 = 元 index 0) を探す。MoveLayer 後は index が変わるので
    // id で解決する。
    let layer_index = app
        .document
        .active_koma()
        .unwrap()
        .layers
        .iter()
        .position(|l| l.bitmap.pixels.chunks_exact(4).all(|p| p[3] == 255))
        .unwrap_or(0);
    let after = gpu_layer(&app, koma, layer_index);
    (after.dark_at(lx, ly), before_dark, after.dark_count())
}

#[test]
fn repro_r1_add_raster_layer_keeps_gpu_stroke() {
    let (kept, before, after) = r1_stroke_then(|app| {
        app.apply_document_command(&DocumentCommand::AddRasterLayer);
    });
    assert!(
        kept,
        "AddRasterLayer 後に元レイヤーの GPU テクスチャからストロークが消えた (dark 画素数 {before} -> {after})"
    );
}

#[test]
fn repro_r1_remove_active_layer_keeps_other_layer_gpu_stroke() {
    let (kept, before, after) = r1_stroke_then(|app| {
        // レイヤーを追加 (これ自体で消える可能性があるので、消えるかどうかは
        // RemoveActiveLayer 後にまとめて見る) → 追加レイヤーを削除。
        app.apply_document_command(&DocumentCommand::AddRasterLayer);
        app.apply_document_command(&DocumentCommand::RemoveActiveLayer);
    });
    assert!(
        kept,
        "AddRasterLayer+RemoveActiveLayer 後にストロークが消えた (dark 画素数 {before} -> {after})"
    );
}

/// CPU 経路の対照: 同じ操作で CPU bitmap のストロークは残る (テストハーネス検証)。
#[test]
fn repro_r1_control_cpu_add_raster_layer_keeps_stroke() {
    let mut app = cpu_app();
    let (cx, cy) = canvas_center(&app);
    stroke(
        &mut app,
        &[wp(cx - 40, cy), wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)],
    );
    let koma = active_koma_id(&app);
    let (lx, ly) = local(&app, wp(cx, cy));
    app.apply_document_command(&DocumentCommand::AddRasterLayer);
    assert!(cpu_layer(&app, koma, 0).dark_at(lx, ly));
}

// ---------------------------------------------------------------------------
// R2: コマ追加 (Full 同期) で既存コマの GPU 描画が消える
// ---------------------------------------------------------------------------

#[test]
fn repro_r2_create_koma_keeps_existing_koma_gpu_stroke() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    stroke(
        &mut app,
        &[wp(cx - 40, cy), wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)],
    );
    let koma0 = active_koma_id(&app);
    let (lx, ly) = local(&app, wp(cx, cy));
    let before = gpu_layer(&app, koma0, 0);
    assert!(before.dark_at(lx, ly), "前提: ストロークが GPU に描かれている");

    // 既存コマを relayout しない CreateKoma で、座標が保たれる状況を作る。
    app.apply_document_command(&DocumentCommand::CreateKoma {
        x: 10,
        y: 10,
        width: 40,
        height: 40,
    });
    assert_eq!(app.document.work.pages[0].komas.len(), 2);
    let after = gpu_layer(&app, koma0, 0);
    assert!(
        after.dark_at(lx, ly),
        "CreateKoma 後に既存コマの GPU ストロークが消えた (dark 画素数 {} -> {})",
        before.dark_count(),
        after.dark_count()
    );
}

#[test]
fn repro_r2_add_koma_keeps_existing_koma_gpu_stroke_somewhere() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    stroke(
        &mut app,
        &[wp(cx - 40, cy), wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)],
    );
    let koma0 = active_koma_id(&app);
    let before = gpu_layer(&app, koma0, 0);
    assert!(before.dark_count() > 0);

    // AddKoma は既存コマを grid へ relayout (nearest リサイズ) するので、位置ではなく
    // 「何らかの暗画素が残っているか」を見る。
    app.apply_document_command(&DocumentCommand::AddKoma);
    let after = gpu_layer(&app, koma0, 0);
    let cpu_after = cpu_layer(&app, koma0, 0);
    assert!(
        after.dark_count() > 0,
        "AddKoma 後に既存コマの GPU テクスチャから描画が全て消えた (dark {} -> {}, CPU 側 dark={})",
        before.dark_count(),
        after.dark_count(),
        cpu_after.dark_count()
    );
}

// ---------------------------------------------------------------------------
// R3: 重なる 2 ストローク → undo 1 回で 1 本目の重なり部分が消える
// ---------------------------------------------------------------------------

#[test]
fn repro_r3_undo_second_overlapping_stroke_keeps_first_stroke_gpu() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    // 1 本目: 水平線
    stroke(
        &mut app,
        &[wp(cx - 60, cy), wp(cx, cy), wp(cx + 60, cy), wp(cx + 72, cy)],
    );
    let after_first = gpu_layer(&app, koma, 0);
    let (lx, ly) = local(&app, wp(cx, cy));
    assert!(after_first.dark_at(lx, ly), "前提: 1 本目が描かれている");

    // 2 本目: 垂直線 (中心で 1 本目と交差)
    stroke(
        &mut app,
        &[wp(cx, cy - 60), wp(cx, cy), wp(cx, cy + 60), wp(cx, cy + 72)],
    );
    let after_second = gpu_layer(&app, koma, 0);
    assert!(after_second.dark_count() > after_first.dark_count());

    assert!(app.execute_undo(), "undo が実行できる");
    let after_undo = gpu_layer(&app, koma, 0);
    let diff = after_first.diff_count(&after_undo);
    assert!(
        after_undo.dark_at(lx, ly) && diff == 0,
        "undo 1 回で 1 本目の画素が壊れた: 交差点 dark={} (値 {:?}), 1 本目直後との差分画素数={diff}, \
         dark 数 first={} second={} undo={}",
        after_undo.dark_at(lx, ly),
        after_undo.at(lx, ly),
        after_first.dark_count(),
        after_second.dark_count(),
        after_undo.dark_count()
    );
}

/// CPU 経路の対照 (CPU は before をストローク開始時のレイヤー全体から取る)。
#[test]
fn repro_r3_control_cpu_undo_second_overlapping_stroke_keeps_first() {
    let mut app = cpu_app();
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    stroke(
        &mut app,
        &[wp(cx - 60, cy), wp(cx, cy), wp(cx + 60, cy), wp(cx + 72, cy)],
    );
    let after_first = cpu_layer(&app, koma, 0);
    stroke(
        &mut app,
        &[wp(cx, cy - 60), wp(cx, cy), wp(cx, cy + 60), wp(cx, cy + 72)],
    );
    assert!(app.execute_undo());
    let after_undo = cpu_layer(&app, koma, 0);
    assert_eq!(after_first.diff_count(&after_undo), 0);
}

// ---------------------------------------------------------------------------
// R4: PNG export / スナップショット
// ---------------------------------------------------------------------------

#[test]
fn repro_r4_png_export_contains_gpu_stroke() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    stroke(
        &mut app,
        &[wp(cx - 40, cy), wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)],
    );
    let koma = active_koma_id(&app);
    let (lx, ly) = local(&app, wp(cx, cy));
    assert!(gpu_layer(&app, koma, 0).dark_at(lx, ly), "前提");

    let path = unique_test_path("repro-export").with_extension("png");
    assert!(app.execute_service_request(
        ServiceRequest::new(names::EXPORT_IMAGE).with_value("path", path.to_string_lossy().to_string())
    ));
    app.wait_for_pending_save_tasks();

    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&path).expect("exported png exists"),
    ));
    let mut reader = decoder.read_info().expect("png info");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png frame");
    let exported = Pixels {
        width: info.width as usize,
        height: info.height as usize,
        data: buf[..info.buffer_size()].to_vec(),
    };
    let _ = std::fs::remove_file(&path);
    assert!(
        exported.dark_at(lx, ly),
        "PNG export にストロークが含まれない: 画素 {:?}, dark 数={} ({}x{})",
        exported.at(lx, ly),
        exported.dark_count(),
        exported.width,
        exported.height
    );
}

#[test]
fn repro_r4_snapshot_restore_roundtrips_gpu_strokes() {
    let mut app = gpu_app();
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    // A: スナップショットに含まれるべきストローク
    let a = wp(cx - 60, cy - 60);
    stroke(&mut app, &[a, wp(cx - 20, cy - 60), wp(cx - 8, cy - 60)]);
    let (ax, ay) = local(&app, a);
    assert!(app.execute_service_request(ServiceRequest::new(names::SNAPSHOT_CREATE)));
    let snapshot_id = "0";
    assert!(app.snapshots.get(snapshot_id).is_some(), "snapshot id 0 exists");
    let snapshot_has_a = {
        let doc = &app.snapshots.get(snapshot_id).unwrap().document;
        let b = doc.clone_koma_layer_bitmap(koma, 0).unwrap();
        let i = (ay * b.width + ax) * 4;
        is_dark([b.pixels[i], b.pixels[i + 1], b.pixels[i + 2], b.pixels[i + 3]])
    };

    // B: スナップショット後のストローク (復元で消えるべき)
    let b = wp(cx + 60, cy + 60);
    stroke(&mut app, &[b, wp(cx + 20, cy + 60), wp(cx + 8, cy + 60)]);
    let (bx, by) = local(&app, b);
    assert!(gpu_layer(&app, koma, 0).dark_at(bx, by), "前提: B が GPU に描かれている");

    assert!(app.execute_service_request(
        ServiceRequest::new(names::SNAPSHOT_RESTORE).with_value("snapshot_id", snapshot_id)
    ));
    frame(&mut app);
    let restored_gpu = gpu_layer(&app, koma, 0);
    let restored_cpu = cpu_layer(&app, koma, 0);
    assert!(
        snapshot_has_a && restored_gpu.dark_at(ax, ay) && !restored_gpu.dark_at(bx, by),
        "スナップショット往復が不正: snapshot 文書に A={snapshot_has_a}, 復元後 GPU: A={} B={} (B は消えるべき), \
         復元後 CPU: A={} B={}",
        restored_gpu.dark_at(ax, ay),
        restored_gpu.dark_at(bx, by),
        restored_cpu.dark_at(ax, ay),
        restored_cpu.dark_at(bx, by)
    );
}

// ---------------------------------------------------------------------------
// R5: text.render_to_layer が GPU テクスチャに反映されない
// ---------------------------------------------------------------------------

#[test]
fn repro_r5_text_render_reaches_gpu_texture() {
    let mut app = gpu_app();
    let koma = active_koma_id(&app);
    assert!(app.execute_service_request(
        ServiceRequest::new(names::TEXT_RENDER_TO_LAYER)
            .with_value("text", "HELLO")
            .with_value("font_size", 32)
            .with_value("color_hex", "#000000")
            .with_value("x", 20)
            .with_value("y", 20)
    ));
    frame(&mut app);
    let cpu = cpu_layer(&app, koma, 0);
    let gpu = gpu_layer(&app, koma, 0);
    assert!(cpu.dark_count() > 0, "前提: CPU bitmap にはテキストが描かれる");
    assert!(
        gpu.dark_count() > 0,
        "テキストが GPU テクスチャに反映されない: CPU dark={} GPU dark={}",
        cpu.dark_count(),
        gpu.dark_count()
    );
}

// ---------------------------------------------------------------------------
// R6: タップ (Down/Up 同一座標) で履歴が閉じず、次のストロークと 1 つに統合される
// ---------------------------------------------------------------------------

fn r6_tap_then_stroke_undo(mut app: DesktopApp, read: fn(&DesktopApp, KomaId) -> Pixels) {
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    let t = wp(cx - 60, cy - 60);
    tap(&mut app, t);
    let (tx, ty) = local(&app, t);
    let after_tap = read(&app, koma);
    assert!(after_tap.dark_at(tx, ty), "前提: タップで点が描かれる");

    let s = wp(cx + 40, cy + 40);
    stroke(&mut app, &[s, wp(cx + 80, cy + 40), wp(cx + 92, cy + 40)]);
    let (sx, sy) = local(&app, s);
    assert!(read(&app, koma).dark_at(sx, sy), "前提: ストロークが描かれる");

    assert!(app.execute_undo());
    let after_undo = read(&app, koma);
    assert!(
        after_undo.dark_at(tx, ty) && !after_undo.dark_at(sx, sy),
        "undo 1 回の結果が不正: タップ点 dark={} (残るべき), ストローク dark={} (消えるべき)",
        after_undo.dark_at(tx, ty),
        after_undo.dark_at(sx, sy)
    );
}

#[test]
fn repro_r6_cpu_tap_then_stroke_undo_once_keeps_tap() {
    r6_tap_then_stroke_undo(cpu_app(), |app, koma| cpu_layer(app, koma, 0));
}

#[test]
fn repro_r6_gpu_tap_then_stroke_undo_once_keeps_tap() {
    r6_tap_then_stroke_undo(gpu_app(), |app, koma| gpu_layer(app, koma, 0));
}

/// 単独ストローク (Up が最後の Drag と同じページ画素) を 1 回 undo できること (CPU 経路)。
#[test]
fn repro_r6_cpu_single_stroke_ending_at_last_drag_point_is_undoable() {
    let mut app = cpu_app();
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    let p0 = wp(cx - 60, cy - 60);
    let p1 = wp(cx - 20, cy - 60);
    stroke(&mut app, &[p0, p1, p1]);
    let (x0, y0) = local(&app, p0);
    assert!(cpu_layer(&app, koma, 0).dark_at(x0, y0), "前提: 描かれている");
    let undone = app.execute_undo();
    let after = cpu_layer(&app, koma, 0);
    assert!(
        undone && !after.dark_at(x0, y0),
        "単独ストロークを undo できない: execute_undo={undone}, undo 後も dark={}",
        after.dark_at(x0, y0)
    );
}

/// タップだけでなく「最後の Drag と同じ座標で Up」でも履歴が閉じない (CPU 経路)。
#[test]
fn repro_r6_cpu_drag_ending_at_last_drag_point_commits_history() {
    let mut app = cpu_app();
    let (cx, cy) = canvas_center(&app);
    let koma = active_koma_id(&app);
    let p0 = wp(cx - 60, cy - 60);
    let p1 = wp(cx - 20, cy - 60);
    // Up を最後の Drag と同一座標にする (マウスでは普通に起こる)。
    stroke(&mut app, &[p0, p1, p1]);
    let (x0, y0) = local(&app, p0);
    let s = wp(cx + 40, cy + 40);
    stroke(&mut app, &[s, wp(cx + 80, cy + 40), wp(cx + 92, cy + 40)]);
    let (sx, sy) = local(&app, s);
    assert!(app.execute_undo());
    let after_undo = cpu_layer(&app, koma, 0);
    assert!(
        after_undo.dark_at(x0, y0) && !after_undo.dark_at(sx, sy),
        "undo 1 回の結果が不正: 1 本目 dark={} (残るべき), 2 本目 dark={} (消えるべき)",
        after_undo.dark_at(x0, y0),
        after_undo.dark_at(sx, sy)
    );
}

// ---------------------------------------------------------------------------
// R8: 新規文書 / 読込で履歴が消去されず、旧文書のパッチが新文書へ適用される
// ---------------------------------------------------------------------------

fn r8_redo_after_replace(
    mut app: DesktopApp,
    read: fn(&DesktopApp, KomaId) -> Pixels,
    replace: impl FnOnce(&mut DesktopApp),
) {
    let (cx, cy) = canvas_center(&app);
    let a = wp(cx - 40, cy);
    stroke(&mut app, &[a, wp(cx, cy), wp(cx + 40, cy), wp(cx + 52, cy)]);
    let (ax, ay) = local(&app, wp(cx, cy));
    assert!(read(&app, active_koma_id(&app)).dark_at(ax, ay), "前提");
    assert!(app.execute_undo(), "旧文書で undo (redo スタックへ積む)");

    replace(&mut app);
    frame(&mut app);
    let koma = active_koma_id(&app);
    let fresh = read(&app, koma);
    assert_eq!(fresh.dark_count(), 0, "前提: 新文書はまっさら");
    let can_undo = app.paint.history.can_undo();
    let can_redo = app.paint.history.can_redo();

    let redo_applied = app.execute_redo();
    let after = read(&app, koma);
    assert!(
        !can_undo && !can_redo && !redo_applied && after.dark_count() == 0,
        "文書差し替え後も旧文書の履歴が残り新文書へ適用された: can_undo={can_undo} can_redo={can_redo} \
         redo 実行={redo_applied}, 新文書 dark 数 {} -> {} (新 koma id={:?})",
        fresh.dark_count(),
        after.dark_count(),
        koma
    );
}

#[test]
fn repro_r8_cpu_new_document_clears_history() {
    r8_redo_after_replace(
        cpu_app(),
        |app, koma| cpu_layer(app, koma, 0),
        |app| {
            app.apply_document_command(&DocumentCommand::NewDocumentSized {
                width: PAGE,
                height: PAGE,
            });
        },
    );
}

#[test]
fn repro_r8_gpu_new_document_clears_history() {
    r8_redo_after_replace(
        gpu_app(),
        |app, koma| gpu_layer(app, koma, 0),
        |app| {
            app.apply_document_command(&DocumentCommand::NewDocumentSized {
                width: PAGE,
                height: PAGE,
            });
        },
    );
}

#[test]
fn repro_r8_cpu_load_project_clears_history() {
    // まっさらな同寸プロジェクトを保存しておき、後で読み込む。
    let blank_path = super::unique_test_project_path();
    {
        let mut blank = cpu_app();
        assert!(blank.save_project_to_path(blank_path.clone()));
        blank.wait_for_pending_save_tasks();
    }
    r8_redo_after_replace(
        cpu_app(),
        |app, koma| cpu_layer(app, koma, 0),
        move |app| {
            assert!(app.load_project(blank_path), "load project");
        },
    );
}

// ---------------------------------------------------------------------------
// R15: ページ全面でないコマの GPU テクスチャが presenter でページ中央に描かれる
// ---------------------------------------------------------------------------

/// presenter の PRESENT_SHADER (fs_main) の UV→テクセル計算を CPU で再現する。
/// `win` はウィンドウピクセル中心座標。戻り値はソーステクスチャのテクセル座標 (範囲外なら None)。
fn shader_texel_for_window_pixel(
    quad: canvas_geometry::TextureQuad,
    tex_w: f32,
    tex_h: f32,
    win_x: f32,
    win_y: f32,
) -> Option<(f32, f32)> {
    let d = quad.destination;
    let unit_x = (win_x - d.x as f32) / d.width as f32;
    let unit_y = (win_y - d.y as f32) / d.height as f32;
    let rot_uv_x = quad.uv_min[0] + (quad.uv_max[0] - quad.uv_min[0]) * unit_x;
    let rot_uv_y = quad.uv_min[1] + (quad.uv_max[1] - quad.uv_min[1]) * unit_y;
    let mut px = (rot_uv_x - 0.5) * quad.bbox_size[0];
    let mut py = (rot_uv_y - 0.5) * quad.bbox_size[1];
    if quad.flip_x {
        px = -px;
    }
    if quad.flip_y {
        py = -py;
    }
    let r = (-quad.rotation_degrees).to_radians();
    let (c, s) = (r.cos(), r.sin());
    let sx = px * c - py * s;
    let sy = px * s + py * c;
    let u = (sx + tex_w * 0.5) / tex_w;
    let v = (sy + tex_h * 0.5) / tex_h;
    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
        return None;
    }
    Some((u * tex_w, v * tex_h))
}

#[test]
fn repro_r15_gpu_canvas_places_non_fullpage_koma_at_its_bounds() {
    let mut app = gpu_app();
    app.apply_document_command(&DocumentCommand::AddKoma);
    frame(&mut app);
    // アクティブは新しいコマ (index 1, ページ右側)。
    let bounds = app.document.active_koma_bounds().expect("active koma");
    let spec = app.canvas_gpu_source_spec().expect("GPU source spec");
    let quad = app.canvas_texture_quad().expect("canvas quad");
    let (page_w, page_h) = app.document.active_page_dimensions();
    eprintln!(
        "[repro] r15 page={page_w}x{page_h} koma bounds={bounds:?} gpu tex={}x{} quad={quad:?}",
        spec.width, spec.height
    );

    // コマ内の page 点 P を選び、CPU 経路 (build_cpu_canvas_snapshot) と同じく
    // P は window 上で page 座標系の P の位置に表示されるべき。そこに GPU で
    // サンプルされるテクセルは P - bounds.origin であるべき。
    let p = (bounds.x + 10, bounds.y + 10);
    let d = quad.destination;
    // zoom=1 で quad はページ全体 (uv 0..1) を覆う前提で page→window を線形に写す。
    assert_eq!(quad.uv_min, [0.0, 0.0]);
    assert_eq!(quad.uv_max, [1.0, 1.0]);
    let win_x = d.x as f32 + (p.0 as f32 + 0.5) * d.width as f32 / page_w as f32;
    let win_y = d.y as f32 + (p.1 as f32 + 0.5) * d.height as f32 / page_h as f32;
    let texel = shader_texel_for_window_pixel(quad, spec.width as f32, spec.height as f32, win_x, win_y);
    let expected = (10.5f32, 10.5f32);
    let ok = texel.is_some_and(|(tx, ty)| {
        (tx - expected.0).abs() <= 1.5 && (ty - expected.1).abs() <= 1.5
    });
    // 実際に表示される原点: shader では texel = P - page/2 + tex/2 → コマ原点が
    // ((page_w - tex_w)/2, (page_h - tex_h)/2) に来る。
    let shown_origin = (
        (page_w as f32 - spec.width as f32) * 0.5,
        (page_h as f32 - spec.height as f32) * 0.5,
    );
    assert!(
        ok,
        "GPU キャンバスでコマがページ上の bounds に描かれない: page 点 {p:?} にはテクセル {expected:?} が出るべきが \
         {texel:?}。shader 上のコマ表示原点={shown_origin:?}, 期待 bounds 原点=({}, {})",
        bounds.x,
        bounds.y
    );
}
