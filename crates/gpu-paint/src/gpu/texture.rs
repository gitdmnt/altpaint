//! 1 レイヤー = 1 wgpu::Texture を保持する RGBA テクスチャラッパー。

use crate::gpu::context::GpuCanvasContext;

/// 1 レイヤー = 1 wgpu::Texture を保持するラッパー。
///
/// Format: Rgba8Unorm
/// Usage: STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC | COPY_DST
pub struct GpuRgbaTexture {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

impl GpuRgbaTexture {
    /// 指定サイズのテクスチャを GPU 上に生成する。
    pub fn create(ctx: &GpuCanvasContext, width: u32, height: u32) -> Self {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-paint-layer"),
            size: wgpu::Extent3d {
                width,
                height,
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
            width,
            height,
        }
    }

    /// Rgba8UnormSrgb view を生成して返す。Present 時にガンマ補正を自動適用するために使う。
    ///
    /// テクスチャ本体は `STORAGE_BINDING` を含むが、sRGB フォーマットはストレージバインディング
    /// 非対応のため、view の usage は `TEXTURE_BINDING | COPY_SRC` のみに絞る。
    pub fn create_srgb_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC),
            ..Default::default()
        })
    }

    /// CPU ピクセルデータをテクスチャへフルアップロードする。
    ///
    /// `pixels` は RGBA 各 1 バイト = 1 ピクセル 4 バイトのフラットなバイト列。
    /// `write_texture` でテクスチャ全体を書き換える。
    pub fn upload_pixels(&self, ctx: &GpuCanvasContext, pixels: &[u8]) {
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }
}
