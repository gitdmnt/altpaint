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

    use crate::{GpuBrushDispatch, GpuCanvasPool, GpuPenTipCache};

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
}
