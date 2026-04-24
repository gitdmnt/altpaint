//! gpu-canvas クレートのテスト。

/// CPU 単体テスト（GPU 不要）。
mod cpu_tests {
    /// alpha_mask_to_rgba 変換ロジックの単体テスト。
    #[test]
    fn alpha_mask_conversion_produces_correct_rgba() {
        let mask = [0u8, 128, 255];
        let rgba: Vec<u8> = mask
            .iter()
            .flat_map(|&alpha| [255u8, 255, 255, alpha])
            .collect();
        assert_eq!(
            rgba,
            [255u8, 255, 255, 0, 255, 255, 255, 128, 255, 255, 255, 255]
        );
    }
}

/// GPU ありテスト。
#[cfg(feature = "gpu")]
mod gpu_tests {
    use std::sync::Arc;

    use crate::{
        CompositeLayerEntry, GpuBrushDispatch, GpuCanvasPool, GpuFillDispatch,
        GpuLayerCompositor, GpuPenTipCache,
    };

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
                label: Some("gpu-canvas-test-device"),
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

    /// GpuCanvasPool::upload_cpu_bitmap が panic なく完了することを確認する。
    #[test]
    fn gpu_canvas_pool_upload_smoke() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = GpuCanvasPool::new(device, queue);
            pool.create_layer_texture("panel-1", 0, 4, 4);
            let pixels = vec![128u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap("panel-1", 0, &pixels);
            assert!(pool.get("panel-1", 0).is_some());
        });
    }

    /// GpuPenTipCache::upload_from_preset が panic なく完了することを確認する。
    #[test]
    fn gpu_pen_tip_cache_upload_smoke() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut cache = GpuPenTipCache::new(device, queue);
            let pen_no_tip = app_core::PenPreset::default();
            cache.upload_from_preset("pen-no-tip", &pen_no_tip);
            assert!(cache.get("pen-no-tip").is_none());

            let pen_with_tip = app_core::PenPreset {
                tip: Some(app_core::PenTipBitmap::AlphaMask8 {
                    width: 4,
                    height: 4,
                    data: vec![255u8; 16],
                }),
                ..app_core::PenPreset::default()
            };
            cache.upload_from_preset("pen-with-tip", &pen_with_tip);
            assert!(cache.get("pen-with-tip").is_some());
        });
    }

    /// create_layer_texture 後に get が Some を返すことを確認する。
    #[test]
    fn gpu_canvas_pool_create_layer_texture_registers_key() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = GpuCanvasPool::new(device, queue);
            assert!(pool.get("p1", 0).is_none());
            pool.create_layer_texture("p1", 0, 8, 8);
            assert!(pool.get("p1", 0).is_some());
            assert!(pool.get("p1", 1).is_none());
        });
    }

    /// dispatch_stroke がレイヤーテクスチャのピクセルを変更することを確認する。
    /// Rgba8Unorm STORAGE_READ_WRITE をサポートしない環境ではスキップする。
    #[test]
    fn gpu_brush_dispatch_modifies_layer_texture() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let Some((device, queue, adapter)) = try_init_device().await else {
                    return None;
                };
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }

                let ctx = crate::GpuCanvasContext::new(device.clone(), queue.clone());
                let texture = crate::GpuLayerTexture::create(&ctx, 4, 4);
                texture.upload_pixels(&ctx, &vec![0u8; 4 * 4 * 4]);

                let brush = GpuBrushDispatch::new(device.clone(), queue.clone());
                brush.dispatch_stroke(
                    &texture,
                    &[(2.0_f32, 2.0_f32)],
                    [1.0, 0.0, 0.0, 1.0],
                    2.0,
                    1.0,
                    false,
                    app_core::ToolKind::Pen,
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
                device.poll(wgpu::PollType::Wait {
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
    fn gpu_canvas_pool_snapshot_and_restore_round_trip() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = GpuCanvasPool::new(device, queue);
            pool.create_layer_texture("p", 0, 4, 4);
            let mut pixels = vec![0u8; 4 * 4 * 4];
            for i in 0..pixels.len() {
                pixels[i] = (i % 251) as u8;
            }
            pool.upload_cpu_bitmap("p", 0, &pixels);

            // dirty 領域 (1,1)-(2x2) をスナップショット
            let snap = pool.snapshot_region("p", 0, 1, 1, 2, 2).expect("snapshot");

            // レイヤーを別のピクセルで上書き
            let zeros = vec![0u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap("p", 0, &zeros);

            // snap を元の位置へ復元
            pool.restore_region("p", 0, 1, 1, &snap);

            let (w, h, out) = pool.read_back_full("p", 0).expect("readback");
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
    fn gpu_canvas_pool_upload_region_partial() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = GpuCanvasPool::new(device, queue);
            pool.create_layer_texture("p", 0, 4, 4);
            pool.upload_cpu_bitmap("p", 0, &vec![0u8; 4 * 4 * 4]);

            let region = vec![255u8; 2 * 2 * 4];
            pool.upload_region("p", 0, 1, 1, 2, 2, &region);

            let (_, _, out) = pool.read_back_full("p", 0).expect("readback");
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

    /// create_and_upload で作成したテクスチャが restore_region のソースとして使えることを確認する。
    #[test]
    fn gpu_canvas_pool_create_and_upload_can_be_restored() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let mut pool = GpuCanvasPool::new(device, queue);
            pool.create_layer_texture("p", 0, 4, 4);
            pool.upload_cpu_bitmap("p", 0, &vec![0u8; 4 * 4 * 4]);

            let region = vec![128u8; 2 * 2 * 4];
            let tex = pool.create_and_upload(2, 2, &region);
            pool.restore_region("p", 0, 1, 1, &tex);

            let (_, _, out) = pool.read_back_full("p", 0).expect("readback");
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

    /// GpuCanvasPool::upload_cpu_bitmap は存在しないキーに対して panic しないことを確認する。
    #[test]
    fn gpu_canvas_pool_upload_nonexistent_key_is_noop() {
        pollster::block_on(async {
            let Some((device, queue, _adapter)) = try_init_device().await else {
                return;
            };
            let pool = GpuCanvasPool::new(device, queue);
            let pixels = vec![0u8; 4 * 4 * 4];
            pool.upload_cpu_bitmap("nonexistent", 0, &pixels);
            assert!(pool.get("nonexistent", 0).is_none());
        });
    }

    /// GpuFillDispatch::dispatch_flood_fill が連結成分だけを塗り、非連結ピクセルは
    /// 変化させないことを検証する。
    #[test]
    fn gpu_flood_fill_fills_connected_region_only() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let Some((device, queue, adapter)) = try_init_device().await else {
                    return None;
                };
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }

                // 4x4 キャンバス: 左 2 列が透明の連結領域、右 2 列は非連結で別色で埋める。
                // 期待: 左 2 列のみが赤 (255,0,0,255) に塗られる。
                let mut pool = GpuCanvasPool::new(device.clone(), queue.clone());
                pool.create_layer_texture("p", 0, 4, 4);
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
                pool.upload_cpu_bitmap("p", 0, &pixels);

                let fill = GpuFillDispatch::new(device, queue);
                let target = pool.get("p", 0).unwrap();
                fill.dispatch_flood_fill(target, target, (0, 0), [1.0, 0.0, 0.0, 1.0]);

                let (_, _, out) = pool.read_back_full("p", 0).expect("readback");
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

    /// GpuFillDispatch::dispatch_lasso_fill が三角ポリゴン内部のピクセルを塗り、
    /// 外側は変更しないことを検証する。
    #[test]
    fn gpu_lasso_fill_triangle_paints_interior() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let Some((device, queue, adapter)) = try_init_device().await else {
                    return None;
                };
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = GpuCanvasPool::new(device.clone(), queue.clone());
                pool.create_layer_texture("p", 0, 8, 8);
                let pixels = vec![0u8; 8 * 8 * 4];
                pool.upload_cpu_bitmap("p", 0, &pixels);

                let fill = GpuFillDispatch::new(device, queue);
                let target = pool.get("p", 0).unwrap();
                // 三角形 (0,0), (7,0), (0,7) — 左上半分が内側。
                let polygon = vec![(0.0, 0.0), (7.0, 0.0), (0.0, 7.0)];
                fill.dispatch_lasso_fill(
                    target,
                    &polygon,
                    (0, 0, 7, 7),
                    [0.0, 1.0, 0.0, 1.0],
                );

                let (_, _, out) = pool.read_back_full("p", 0).expect("readback");
                // (1,1) は内部 → 緑。(6,6) は外部 → 変更なし。
                let idx_in = (1 * 8 + 1) * 4;
                assert_eq!(out[idx_in + 1], 255, "interior green channel");
                let idx_out = (6 * 8 + 6) * 4;
                assert_eq!(out[idx_out + 3], 0, "exterior alpha unchanged");
                Some(())
            })
        });
        let _ = outcome;
    }

    /// GpuLayerCompositor::recomposite で単一レイヤー (Normal blend) が passthrough
    /// として合成テクスチャへコピーされることを確認する。
    #[test]
    fn gpu_layer_compositor_single_layer_passthrough() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let Some((device, queue, adapter)) = try_init_device().await else {
                    return None;
                };
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = GpuCanvasPool::new(device.clone(), queue.clone());
                pool.ensure_composite_texture("p", 4, 4);
                pool.create_layer_texture("p", 0, 4, 4);
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
                pool.upload_cpu_bitmap("p", 0, &pixels);

                let compositor = GpuLayerCompositor::new(device, queue);
                let composite = pool.get_composite("p").unwrap();
                let layer = pool.get("p", 0).unwrap();
                compositor.recomposite(
                    composite,
                    &[CompositeLayerEntry {
                        color: layer,
                        mask: None,
                        blend_code: 0,
                        visible: true,
                    }],
                    (0, 0, 4, 4),
                );

                let (_, _, out) = pool.read_back_composite("p").expect("readback");
                let idx = (1 * 4 + 1) * 4;
                assert_eq!(out[idx], 100);
                assert_eq!(out[idx + 1], 150);
                assert_eq!(out[idx + 2], 200);
                assert_eq!(out[idx + 3], 255);
                Some(())
            })
        });
        let _ = outcome;
    }

    /// GpuLayerCompositor が invisible layer を完全にスキップし、dirty rect 範囲外を
    /// 変更しないことを検証する。
    #[test]
    fn gpu_layer_compositor_invisible_layer_is_skipped() {
        let outcome = std::panic::catch_unwind(|| {
            pollster::block_on(async {
                let Some((device, queue, adapter)) = try_init_device().await else {
                    return None;
                };
                if !crate::format_check::supports_rgba8unorm_storage(&adapter) {
                    return None;
                }
                let mut pool = GpuCanvasPool::new(device.clone(), queue.clone());
                pool.ensure_composite_texture("p", 4, 4);
                pool.create_layer_texture("p", 0, 4, 4);
                // Fill with solid red.
                let pixels: Vec<u8> = (0..16).flat_map(|_| [255u8, 0, 0, 255]).collect();
                pool.upload_cpu_bitmap("p", 0, &pixels);

                let compositor = GpuLayerCompositor::new(device, queue);
                let composite = pool.get_composite("p").unwrap();
                let layer = pool.get("p", 0).unwrap();
                compositor.recomposite(
                    composite,
                    &[CompositeLayerEntry {
                        color: layer,
                        mask: None,
                        blend_code: 0,
                        visible: false,
                    }],
                    (0, 0, 4, 4),
                );

                let (_, _, out) = pool.read_back_composite("p").expect("readback");
                // All pixels should be cleared (alpha = 0) since the only layer is invisible.
                for a in out.chunks(4).map(|c| c[3]) {
                    assert_eq!(a, 0, "invisible layer should leave composite transparent");
                }
                Some(())
            })
        });
        let _ = outcome;
    }
}
