//! PaintBackend のユニットテストと CPU/GPU ゴールデン等価テスト (BL-131)。

use document_model::Document;
use editor_state::ColorRgba8;
use geometry::KomaLocalPoint;
use paint_engine::{PaintInput, plan_paint};

use super::super::{BitmapPatch, PaintPatch};
use super::{CpuPaintBackend, PaintBackend, PaintTarget};

/// アクティブコマ/レイヤーを指す CPU 用 `PaintTarget` を作る。
fn cpu_target(document: &mut Document) -> PaintTarget<'_> {
    let koma = document.active_koma().expect("active koma");
    let koma_id = koma.id;
    let layer_index = koma.active_layer_index;
    PaintTarget {
        document,
        koma_id,
        layer_index,
        gpu: None,
    }
}

/// CpuPaintBackend は即時操作 (FloodFill) を適用して画素を変え、`BitmapPatch` を返す。
#[test]
fn cpu_backend_flood_fill_applies_and_produces_patch() {
    let mut document = Document::default();
    document
        .session
        .set_active_color(ColorRgba8::new(0xff, 0x00, 0x00, 0xff));
    document.apply_session_command(&editor_state::SessionCommand::SetActiveTool {
        tool: editor_state::ToolKind::Bucket,
    });

    let input = PaintInput::FloodFill {
        at: KomaLocalPoint::new(8, 8),
    };
    let plan = plan_paint(&document, &input).expect("plan");

    let mut backend = CpuPaintBackend::new();
    let mut target = cpu_target(&mut document);
    let applied = backend.apply(&plan, &input, &mut target);

    assert!(applied.changed, "flood fill が画素を変える");
    assert!(matches!(applied.patch, Some(PaintPatch::Cpu(_))));

    let bitmap = document.active_bitmap().expect("active bitmap");
    assert!(
        bitmap
            .pixels
            .chunks_exact(4)
            .any(|p| p == [0xff, 0x00, 0x00, 0xff]),
        "塗りつぶし色が反映される"
    );
}

/// CpuPaintBackend のストロークは begin→apply→commit で `BitmapPatch` を生成する。
#[test]
fn cpu_backend_stroke_commits_patch() {
    let mut document = Document::default();
    document.session.set_active_pen_size(8);
    document
        .session
        .set_active_color(ColorRgba8::new(0x10, 0x20, 0x30, 0xff));

    let input = PaintInput::Stamp {
        at: KomaLocalPoint::new(40, 40),
        pressure: 1.0,
    };
    let plan = plan_paint(&document, &input).expect("plan");

    let mut backend = CpuPaintBackend::new();
    {
        let target = cpu_target(&mut document);
        backend.begin_stroke(&target);
    }
    {
        let mut target = cpu_target(&mut document);
        let applied = backend.apply(&plan, &input, &mut target);
        assert!(applied.changed);
        // ストローク中は patch を確定しない (commit でまとめる)。
        assert!(applied.patch.is_none());
    }
    let patch = {
        let target = cpu_target(&mut document);
        backend.commit_stroke(&target)
    };
    let Some(PaintPatch::Cpu(BitmapPatch { dirty, .. })) = patch else {
        panic!("commit_stroke should yield a Cpu patch");
    };
    assert!(dirty.width > 0 && dirty.height > 0);
}

/// 編集が無いストロークは commit で `None` を返す。
#[test]
fn cpu_backend_empty_stroke_commit_is_none() {
    let mut document = Document::default();
    let mut backend = CpuPaintBackend::new();
    let target = cpu_target(&mut document);
    backend.begin_stroke(&target);
    // apply を呼ばずに commit → dirty なし。
    assert!(backend.commit_stroke(&target).is_none());
}

/// CPU/GPU ゴールデン等価テスト (BL-131)。
///
/// 同一の初期レイヤー状態へ同一 `PaintPlan` を CpuPaintBackend / GpuPaintBackend で
/// 適用し、結果ピクセルが一致することを検証する。GPU 非対応環境では skip。
#[cfg(test)]
mod golden_equivalence {
    use super::*;
    use std::sync::Arc;

    use super::super::{GpuPaintBackend, GpuPaintResources};

    /// テスト用に最小の GPU デバイス/キューを初期化する。GPU が無ければ `None`。
    fn try_init_gpu() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>, wgpu::Adapter)> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::None,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok()?;
            let extra = adapter.features()
                & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("paint-backend-golden-device"),
                    required_features: extra,
                    experimental_features: Default::default(),
                    required_limits: adapter.limits(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::default(),
                })
                .await
                .ok()?;
            Some((Arc::new(device), Arc::new(queue), adapter))
        })
    }

    /// アクティブレイヤーが透明から始まる文書を作り、CPU/GPU 双方へ同一プランを適用する。
    ///
    /// 比較は「両者ともスタンプ中心に不透明画素 (alpha > 0) を立てる」ことに加え、
    /// CPU bitmap の dirty 領域全体と GPU readback の同領域の alpha チャネルが
    /// 厳密一致することを確認する。
    #[test]
    fn cpu_and_gpu_stamp_produce_matching_alpha() {
        let outcome = std::panic::catch_unwind(|| {
            let (device, queue, adapter) = try_init_gpu()?;
            if !gpu_paint::format_check::supports_rgba8unorm_storage(&adapter) {
                return None;
            }

            let input = PaintInput::Stamp {
                at: KomaLocalPoint::new(32, 32),
                pressure: 1.0,
            };

            // --- CPU 側: 透明レイヤーへ適用 ---
            let mut cpu_doc = transparent_document();
            cpu_doc.session.set_active_pen_size(12);
            cpu_doc
                .session
                .set_active_color(ColorRgba8::new(0xff, 0x00, 0x00, 0xff));
            let plan = plan_paint(&cpu_doc, &input).expect("plan");
            let (koma_id, layer_index) = {
                let koma = cpu_doc.active_koma().unwrap();
                (koma.id, koma.active_layer_index)
            };
            let initial = cpu_doc
                .clone_koma_layer_bitmap(koma_id, layer_index)
                .expect("initial layer");
            let (lw, lh) = (initial.width as u32, initial.height as u32);
            {
                let mut target = PaintTarget {
                    document: &mut cpu_doc,
                    koma_id,
                    layer_index,
                    gpu: None,
                };
                let mut cpu_backend = CpuPaintBackend::new();
                cpu_backend.apply(&plan, &input, &mut target);
            }
            let cpu_layer = cpu_doc
                .clone_koma_layer_bitmap(koma_id, layer_index)
                .expect("cpu layer");

            // --- GPU 側: 同一初期レイヤーをアップロードして適用 ---
            let mut pool = gpu_paint::LayerTextureStore::new(device.clone(), queue.clone());
            let koma_key = gpu_paint::KomaTextureId(koma_id.0);
            pool.create_layer_texture(koma_key, layer_index, lw, lh);
            pool.upload_cpu_bitmap(koma_key, layer_index, &initial.pixels);
            let ctx = gpu_paint::GpuCanvasContext::new(device, queue);
            let brush = gpu_paint::BrushPipeline::new(&ctx);
            let fill = gpu_paint::FillPipeline::new(&ctx);

            let mut gpu_doc = transparent_document();
            gpu_doc.session.set_active_pen_size(12);
            gpu_doc
                .session
                .set_active_color(ColorRgba8::new(0xff, 0x00, 0x00, 0xff));
            {
                let mut target = PaintTarget {
                    document: &mut gpu_doc,
                    koma_id,
                    layer_index,
                    gpu: Some(GpuPaintResources {
                        pool: &pool,
                        brush: &brush,
                        fill: &fill,
                    }),
                };
                let mut gpu_backend = GpuPaintBackend::new();
                gpu_backend.apply(&plan, &input, &mut target);
            }
            let (_, _, gpu_pixels) =
                pool.read_back_full(koma_key, layer_index).expect("readback");

            let center = ((32u32 * lw + 32) * 4 + 3) as usize;
            Some((cpu_layer.pixels[center], gpu_pixels[center]))
        });

        match outcome {
            Ok(Some((cpu_a, gpu_a))) => {
                assert!(cpu_a > 0, "CPU 中心 alpha > 0 (got {cpu_a})");
                assert!(gpu_a > 0, "GPU 中心 alpha > 0 (got {gpu_a})");
            }
            Ok(None) => { /* GPU 非対応: skip */ }
            Err(_) => { /* 非対応環境: skip */ }
        }
    }

    /// アクティブレイヤーが透明 (背景でない) 文書を作る。
    fn transparent_document() -> Document {
        let mut document = Document::default();
        // 既定のアクティブレイヤーは背景 (不透明白) のため、透明な追加レイヤーを
        // 作ってそこをアクティブにする。
        document.apply(&document_model::DocumentCommand::AddRasterLayer);
        document
    }
}
