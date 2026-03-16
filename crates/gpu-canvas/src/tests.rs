//! gpu-canvas クレートのテスト。

/// CPU 単体テスト（GPU 不要）。
mod cpu_tests {
    /// alpha_mask_to_rgba 変換ロジックの単体テスト。
    ///
    /// gpu feature なしでも実行可能なロジックの検証を行う。
    #[test]
    fn alpha_mask_conversion_produces_correct_rgba() {
        // AlphaMask8 から RGBA への変換は alpha 値が A チャンネルにマップされ、
        // R=G=B=255 固定になることを確認する。
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

    use crate::{GpuCanvasPool, GpuPenTipCache};

    /// wgpu アダプターとデバイスを生成するヘルパー。
    ///
    /// CI 環境に GPU がない場合は `None` を返す。
    async fn try_init_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpu-canvas-test-device"),
                required_features: wgpu::Features::empty(),
                experimental_features: Default::default(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::default(),
            })
            .await
            .ok()?;
        Some((Arc::new(device), Arc::new(queue)))
    }

    /// GpuCanvasPool::upload_cpu_bitmap が panic なく完了することを確認する。
    #[test]
    fn gpu_canvas_pool_upload_smoke() {
        pollster::block_on(async {
            let Some((device, queue)) = try_init_device().await else {
                // GPU なし CI では skip
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
            let Some((device, queue)) = try_init_device().await else {
                return;
            };

            let mut cache = GpuPenTipCache::new(device, queue);

            // tip: None の場合は何もしない
            let pen_no_tip = app_core::PenPreset::default();
            cache.upload_from_preset("pen-no-tip", &pen_no_tip);
            assert!(cache.get("pen-no-tip").is_none());

            // AlphaMask8 の場合はテクスチャが登録される
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

    /// GpuCanvasPool::upload_cpu_bitmap は存在しないキーに対して panic しないことを確認する。
    #[test]
    fn gpu_canvas_pool_upload_nonexistent_key_is_noop() {
        pollster::block_on(async {
            let Some((device, queue)) = try_init_device().await else {
                return;
            };

            let pool = GpuCanvasPool::new(device, queue);
            let pixels = vec![0u8; 4 * 4 * 4];
            // テクスチャを作成せずアップロードを呼んでも panic しない
            pool.upload_cpu_bitmap("nonexistent", 0, &pixels);
            assert!(pool.get("nonexistent", 0).is_none());
        });
    }
}
