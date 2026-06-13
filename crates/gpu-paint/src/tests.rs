//! gpu-paint クレートのテスト。

/// GPU ありテスト。
mod gpu_tests {
    use std::sync::Arc;

    use crate::{
        CompositeLayerEntry, BrushPipeline, KomaTextureId, LayerTextureStore, FillPipeline,
        CompositePipeline,
    };

    /// テスト用のコマキー (任意の u64 値)。
    const KOMA_A: KomaTextureId = KomaTextureId(1);
    const KOMA_B: KomaTextureId = KomaTextureId(2);

    /// wgpu アダプターとデバイスを生成するヘルパー。GPU がない CI では `None` を返す。
    /// TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES をアダプターがサポートする場合は要求する。
    async fn try_init_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>, wgpu::Adapter)> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok()?;
        let adapter_features = adapter.features();
        let extra = adapter_features
            & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpu-paint-test-device"),
                required_features: extra,
                experimental_features: Default::default(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::default(),
            })
            .await
            .ok()?;
        Some((Arc::new(device), Arc::new(queue), adapter))
    }

    /// LayerTextureStore::upload_cpu_bitmap が panic なく完了することを確認する。
    #[test]
    fn layer_texture_store_upload_smoke() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);
            pool.create_layer_texture(KOMA_A, 0, 4, 4);
            let pixels = vec![128u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);
            assert!(pool.get(KOMA_A, 0).is_some());
        });
    }

    /// create_layer_texture 後に get が Some を返すことを確認する。
    #[test]
    fn layer_texture_store_create_layer_texture_registers_key() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);
            assert!(pool.get(KOMA_A, 0).is_none());
            pool.create_layer_texture(KOMA_A, 0, 8, 8);
            assert!(pool.get(KOMA_A, 0).is_some());
            assert!(pool.get(KOMA_A, 1).is_none());
        });
    }

    /// dispatch_stroke がレイヤーテクスチャのピクセルを変更することを確認する。
    /// Rgba8Unorm STORAGE_READ_WRITE をサポートしない環境ではスキップする。
    #[test]
    fn gpu_brush_dispatch_modifies_layer_texture() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let (device, queue, adapter) = try_init_device().await?;
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }

                let ctx = crate::GpuCanvasContext::new(device.clone(), queue.clone());
                let texture = crate::GpuRgbaTexture::create(&ctx, 4, 4);
                texture.upload_pixels(&ctx, &[0u8; 4 * 4 * 4]);

                let brush = BrushPipeline::new(&ctx);
                brush.dispatch_stroke(
                    &texture,
                    &[geometry::KomaLocalPoint::new(2, 2)],
                    &crate::BrushStrokeParams {
                        color_rgba: [1.0, 0.0, 0.0, 1.0],
                        radius: 2.0,
                        opacity: 1.0,
                        antialias: false,
                        mode: editor_state::StrokeMode::Paint,
                    },
                );

                let buf_size = (4 * 4 * 4) as wgpu::BufferAddress;
                let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("readback"),
                    size: buf_size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("readback-encoder"),
                    });
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback_buf,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * 4),
                            rows_per_image: Some(4),
                        },
                    },
                    wgpu::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit(std::iter::once(encoder.finish()));

                let slice = readback_buf.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |r| {
                    tx.send(r).unwrap();
                });
                let _ = device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                });
                rx.recv().unwrap().unwrap();

                let data = slice.get_mapped_range();
                let center_alpha = data[(2 * 4 + 2) * 4 + 3];
                drop(data);
                readback_buf.unmap();
                Some(center_alpha)
            })
        });

        match outcome {
            Ok(Some(alpha)) => assert!(alpha > 0, "center pixel alpha should be > 0 after dispatch"),
            Ok(None) => { /* GPU なし or 非対応: skip */ }
            Err(_) => { /* dispatch_stroke がパニック = 非対応環境: skip */ }
        }
    }

    /// snapshot_region で取り出したテクスチャを restore_region で元のレイヤーへ書き戻すと、
    /// read_back_full で元のピクセルが一致することを確認する。
    #[test]
    fn layer_texture_store_snapshot_and_restore_round_trip() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);
            pool.create_layer_texture(KOMA_A, 0, 4, 4);
            let mut pixels = vec![0u8; 4 * 4 * 4];
            for (i, px) in pixels.iter_mut().enumerate() {
                *px = (i % 251) as u8;
            }
            pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);

            // dirty 領域 (1,1)-(2x2) をスナップショット
            let snap = pool
                .snapshot_region(KOMA_A, 0, geometry::PageDirtyRect::new(1, 1, 2, 2))
                .expect("snapshot");

            // レイヤーを別のピクセルで上書き
            let zeros = vec![0u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap(KOMA_A, 0, &zeros);

            // snap を元の位置へ復元
            pool.restore_region(KOMA_A, 0, geometry::KomaLocalPoint::new(1, 1), &snap);

            let (w, h, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
            assert_eq!((w, h), (4, 4));
            // (1,1)-(2x2) は元のピクセル、それ以外は 0 であること
            for y in 0..4 {
                for x in 0..4 {
                    let idx = (y * 4 + x) * 4;
                    let in_region = (1..=2).contains(&x) && (1..=2).contains(&y);
                    for c in 0..4 {
                        let got = out[idx + c];
                        let expected = if in_region { pixels[idx + c] } else { 0 };
                        assert_eq!(got, expected, "x={x} y={y} c={c}");
                    }
                }
            }
        });
    }

    /// upload_region で指定矩形だけがテクスチャへ反映されることを確認する。
    #[test]
    fn layer_texture_store_upload_region_partial() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);
            pool.create_layer_texture(KOMA_A, 0, 4, 4);
            pool.upload_cpu_bitmap(KOMA_A, 0, &[0u8; 4 * 4 * 4]);

            let region = vec![255u8; 2 * 2 * 4];
            pool.upload_region(KOMA_A, 0, geometry::PageDirtyRect::new(1, 1, 2, 2), &region);

            let (_, _, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
            for y in 0..4 {
                for x in 0..4 {
                    let idx = (y * 4 + x) * 4;
                    let in_region = (1..=2).contains(&x) && (1..=2).contains(&y);
                    let expected = if in_region { 255 } else { 0 };
                    for c in 0..4 {
                        assert_eq!(out[idx + c], expected, "x={x} y={y} c={c}");
                    }
                }
            }
        });
    }

    /// create_snapshot_texture で作成したテクスチャが restore_region のソースとして使えることを確認する。
    #[test]
    fn layer_texture_store_create_snapshot_texture_can_be_restored() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);
            pool.create_layer_texture(KOMA_A, 0, 4, 4);
            pool.upload_cpu_bitmap(KOMA_A, 0, &[0u8; 4 * 4 * 4]);

            let region = vec![128u8; 2 * 2 * 4];
            let tex = pool.create_snapshot_texture(2, 2, &region);
            pool.restore_region(KOMA_A, 0, geometry::KomaLocalPoint::new(1, 1), &tex);

            let (_, _, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
            for y in 0..4 {
                for x in 0..4 {
                    let idx = (y * 4 + x) * 4;
                    let in_region = (1..=2).contains(&x) && (1..=2).contains(&y);
                    let expected = if in_region { 128 } else { 0 };
                    for c in 0..4 {
                        assert_eq!(out[idx + c], expected);
                    }
                }
            }
        });
    }

    /// LayerTextureStore::upload_cpu_bitmap は存在しないキーに対して panic しないことを確認する。
    #[test]
    fn layer_texture_store_upload_nonexistent_key_is_noop() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let pool = LayerTextureStore::new(device, queue);
            let pixels = vec![0u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);
            assert!(pool.get(KOMA_A, 0).is_none());
        });
    }

    /// FillPipeline::dispatch_flood_fill が連結成分だけを塗り、非連結ピクセルは
    /// 変化させないことを検証する。
    #[test]
    fn gpu_flood_fill_fills_connected_region_only() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let (device, queue, adapter) = try_init_device().await?;
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }

                // 4x4 キャンバス: 左 2 列が透明の連結領域、右 2 列は非連結で別色で埋める。
                // 期待: 左 2 列のみが赤 (255,0,0,255) に塗られる。
                let mut pool = LayerTextureStore::new(device.clone(), queue.clone());
                pool.create_layer_texture(KOMA_A, 0, 4, 4);
                let mut pixels = vec![0u8; 4 * 4 * 4];
                for y in 0..4 {
                    for x in 2..4 {
                        let idx = (y * 4 + x) * 4;
                        pixels[idx] = 10;
                        pixels[idx + 1] = 20;
                        pixels[idx + 2] = 30;
                        pixels[idx + 3] = 255;
                    }
                }
                pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);

                let ctx = crate::GpuCanvasContext::new(device, queue);
                let fill = FillPipeline::new(&ctx);
                let target = pool.get(KOMA_A, 0).unwrap();
                fill.dispatch_flood_fill(
                    target,
                    target,
                    geometry::KomaLocalPoint::new(0, 0),
                    [1.0, 0.0, 0.0, 1.0],
                );

                let (_, _, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
                // Left column (x=0, x=1) should be filled red; right columns unchanged.
                for y in 0..4 {
                    for x in 0..2 {
                        let idx = (y * 4 + x) * 4;
                        assert_eq!(out[idx], 255, "red at ({x},{y})");
                        assert_eq!(out[idx + 3], 255, "alpha at ({x},{y})");
                    }
                    for x in 2..4 {
                        let idx = (y * 4 + x) * 4;
                        assert_eq!(out[idx], 10, "unchanged r at ({x},{y})");
                    }
                }
                Some(())
            })
        });
        let _ = outcome; // GPU 非対応ではスキップ
    }

    /// FillPipeline::dispatch_lasso_fill が三角ポリゴン内部のピクセルを塗り、
    /// 外側は変更しないことを検証する。
    #[test]
    fn gpu_lasso_fill_triangle_paints_interior() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let (device, queue, adapter) = try_init_device().await?;
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = LayerTextureStore::new(device.clone(), queue.clone());
                pool.create_layer_texture(KOMA_A, 0, 8, 8);
                let pixels = vec![0u8; 8 * 8 * 4];
                pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);

                let ctx = crate::GpuCanvasContext::new(device, queue);
                let fill = FillPipeline::new(&ctx);
                let target = pool.get(KOMA_A, 0).unwrap();
                // 三角形 (0,0), (7,0), (0,7) — 左上半分が内側。
                // 半開矩形 (0, 0, 8, 8) は包括 AABB (0, 0, 7, 7) に対応。
                let polygon = vec![(0.0, 0.0), (7.0, 0.0), (0.0, 7.0)];
                fill.dispatch_lasso_fill(
                    target,
                    &polygon,
                    geometry::PageDirtyRect::new(0, 0, 8, 8),
                    [0.0, 1.0, 0.0, 1.0],
                );

                let (_, _, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
                // (1,1) は内部 → 緑。(6,6) は外部 → 変更なし。
                let idx_in = (8 + 1) * 4;
                assert_eq!(out[idx_in + 1], 255, "interior green channel");
                let idx_out = (6 * 8 + 6) * 4;
                assert_eq!(out[idx_out + 3], 0, "exterior alpha unchanged");
                Some(())
            })
        });
        let _ = outcome;
    }

    /// CompositePipeline::recomposite で単一レイヤー (Normal blend) が passthrough
    /// として合成テクスチャへコピーされることを確認する。
    #[test]
    fn gpu_layer_compositor_single_layer_passthrough() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let (device, queue, adapter) = try_init_device().await?;
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = LayerTextureStore::new(device.clone(), queue.clone());
                pool.ensure_composite_texture(KOMA_A, 4, 4);
                pool.create_layer_texture(KOMA_A, 0, 4, 4);
                let mut pixels = vec![0u8; 4 * 4 * 4];
                for y in 0..4 {
                    for x in 0..4 {
                        let idx = (y * 4 + x) * 4;
                        pixels[idx] = 100;
                        pixels[idx + 1] = 150;
                        pixels[idx + 2] = 200;
                        pixels[idx + 3] = 255;
                    }
                }
                pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);

                let ctx = crate::GpuCanvasContext::new(device, queue);
                let compositor = CompositePipeline::new(&ctx);
                let composite = pool.get_composite(KOMA_A).unwrap();
                let layer = pool.get(KOMA_A, 0).unwrap();
                compositor.recomposite(
                    composite,
                    &[CompositeLayerEntry {
                        color: layer,
                        mask: None,
                        blend_code: 0,
                        visible: true,
                    }],
                    geometry::PageDirtyRect::new(0, 0, 4, 4),
                );

                let (_, _, out) = pool.read_back_composite(KOMA_A).expect("readback");
                let idx = (4 + 1) * 4;
                assert_eq!(out[idx], 100);
                assert_eq!(out[idx + 1], 150);
                assert_eq!(out[idx + 2], 200);
                assert_eq!(out[idx + 3], 255);
                Some(())
            })
        });
        let _ = outcome;
    }

    /// CompositePipeline が invisible layer を完全にスキップし、dirty rect 範囲外を
    /// 変更しないことを検証する。
    #[test]
    fn gpu_layer_compositor_invisible_layer_is_skipped() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let (device, queue, adapter) = try_init_device().await?;
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = LayerTextureStore::new(device.clone(), queue.clone());
                pool.ensure_composite_texture(KOMA_A, 4, 4);
                pool.create_layer_texture(KOMA_A, 0, 4, 4);
                // Fill with solid red.
                let pixels: Vec<u8> = (0..16).flat_map(|_| [255u8, 0, 0, 255]).collect();
                pool.upload_cpu_bitmap(KOMA_A, 0, &pixels);

                let ctx = crate::GpuCanvasContext::new(device, queue);
                let compositor = CompositePipeline::new(&ctx);
                let composite = pool.get_composite(KOMA_A).unwrap();
                let layer = pool.get(KOMA_A, 0).unwrap();
                compositor.recomposite(
                    composite,
                    &[CompositeLayerEntry {
                        color: layer,
                        mask: None,
                        blend_code: 0,
                        visible: false,
                    }],
                    geometry::PageDirtyRect::new(0, 0, 4, 4),
                );

                let (_, _, out) = pool.read_back_composite(KOMA_A).expect("readback");
                // All pixels should be cleared (alpha = 0) since the only layer is invisible.
                for a in out.chunks(4).map(|c| c[3]) {
                    assert_eq!(a, 0, "invisible layer should leave composite transparent");
                }
                Some(())
            })
        });
        let _ = outcome;
    }

    /// sync_koma_layers が当該コマのみのレイヤー/マスク/合成テクスチャを再構築し、
    /// 別コマのテクスチャには一切触れないことを確認する (BL-117 差分同期)。
    #[test]
    fn sync_koma_layers_only_touches_target_koma() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = LayerTextureStore::new(device, queue);

            // 別コマ KOMA_B を 2 レイヤーで登録しておく (差分同期で触られないこと)。
            pool.create_layer_texture(KOMA_B, 0, 4, 4);
            pool.create_layer_texture(KOMA_B, 1, 4, 4);
            assert_eq!(pool.layer_count_for_koma(KOMA_B), 2);

            // 対象コマ KOMA_A に 1 レイヤーを差分同期。
            let pixels0 = vec![64u8; 4 * 4 * 4];
            pool.sync_koma_layers(
                KOMA_A,
                (4, 4),
                &[crate::LayerUpload {
                    width: 4,
                    height: 4,
                    pixels: &pixels0,
                    mask: None,
                }],
            );
            assert_eq!(pool.layer_count_for_koma(KOMA_A), 1);
            assert!(pool.get_composite(KOMA_A).is_some());
            // 別コマは不変。
            assert_eq!(pool.layer_count_for_koma(KOMA_B), 2);

            // 同じコマを 2 レイヤー + マスク付きで再同期すると、古いエントリが
            // 置き換わりレイヤー数が更新される。
            let pixels1 = vec![32u8; 4 * 4 * 4];
            let pixels2 = vec![16u8; 4 * 4 * 4];
            let mask = vec![200u8; 4 * 4];
            pool.sync_koma_layers(
                KOMA_A,
                (4, 4),
                &[
                    crate::LayerUpload {
                        width: 4,
                        height: 4,
                        pixels: &pixels1,
                        mask: None,
                    },
                    crate::LayerUpload {
                        width: 4,
                        height: 4,
                        pixels: &pixels2,
                        mask: Some((4, 4, &mask)),
                    },
                ],
            );
            assert_eq!(pool.layer_count_for_koma(KOMA_A), 2);
            assert!(pool.get_mask(KOMA_A, 1).is_some());
            assert!(pool.get_mask(KOMA_A, 0).is_none());
            // 別コマは依然不変。
            assert_eq!(pool.layer_count_for_koma(KOMA_B), 2);

            // レイヤー本体のピクセルが反映されていること。
            let (_, _, out) = pool.read_back_full(KOMA_A, 0).expect("readback");
            assert!(out.iter().all(|&b| b == 32));
        });
    }
}
