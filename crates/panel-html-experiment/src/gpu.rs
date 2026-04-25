//! HTML パネル毎の GPU テクスチャラッパ。
//!
//! `gpu-canvas::GpuLayerTexture` と同じ設定を採用:
//! - Format: Rgba8Unorm（vello の出力先要件）
//! - Usage: STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC | COPY_DST
//! - view_formats: [Rgba8UnormSrgb]（present 側で sRGB view を作って合成する）

pub struct PanelGpuTarget {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

impl PanelGpuTarget {
    pub fn create(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("html-panel-target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        Self {
            texture,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// vello 出力先用 view（Rgba8Unorm リニア）。
    pub fn create_render_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8Unorm),
            ..Default::default()
        })
    }

    /// 合成（present）側用 view（sRGB ガンマ補正適用）。
    /// sRGB view は STORAGE_BINDING に使えないため、TEXTURE_BINDING + COPY_SRC のみ許可する。
    pub fn create_present_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_init_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("html-panel-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .ok()?;
        Some((device, queue))
    }

    /// S4: target の usage / view_formats を確認、create_present_view が Rgba8UnormSrgb で返ること
    #[test]
    fn gpu_panel_gpu_target_create_uses_storage_and_srgb_view() {
        let Some((device, _queue)) = try_init_device() else {
            eprintln!("skipping: no compatible GPU device");
            return;
        };
        let target = PanelGpuTarget::create(&device, 64, 32);
        assert_eq!(target.width, 64);
        assert_eq!(target.height, 32);
        assert!(target
            .texture
            .usage()
            .contains(wgpu::TextureUsages::STORAGE_BINDING));
        assert!(target
            .texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING));
        // view 作成が成功すれば sRGB view_format が登録されている証拠
        let _present = target.create_present_view();
        let _render = target.create_render_view();
    }
}
