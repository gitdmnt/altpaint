//! GPU キャンバスリソースの実装。
//!
//! `wgpu` feature が有効な場合のみコンパイルされる。

use std::collections::HashMap;
use std::sync::Arc;

use app_core::{PenPreset, PenTipBitmap};

/// wgpu デバイスとキューを共有するコンテキスト。
///
/// Arc でラップされているため複数の構造体から安全に共有できる。
pub struct GpuCanvasContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

impl GpuCanvasContext {
    /// 新しいコンテキストを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self { device, queue }
    }
}

/// 1 レイヤー = 1 wgpu::Texture を保持するラッパー。
///
/// Format: Rgba8Unorm
/// Usage: STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC | COPY_DST
pub struct GpuLayerTexture {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

impl GpuLayerTexture {
    /// 指定サイズのテクスチャを GPU 上に生成する。
    pub fn create(ctx: &GpuCanvasContext, width: u32, height: u32) -> Self {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-canvas-layer"),
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
            view_formats: &[],
        });
        Self {
            texture,
            width,
            height,
        }
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

/// `(panel_id: String, layer_index: usize)` をキーにレイヤーテクスチャを管理するプール。
pub struct GpuCanvasPool {
    ctx: GpuCanvasContext,
    textures: HashMap<(String, usize), GpuLayerTexture>,
}

impl GpuCanvasPool {
    /// 新しいプールを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            ctx: GpuCanvasContext::new(device, queue),
            textures: HashMap::new(),
        }
    }

    /// 指定パネル・レイヤーインデックスのテクスチャを生成・登録する。
    ///
    /// 同じキーが既に存在する場合は上書きする。
    pub fn create_layer_texture(
        &mut self,
        panel_id: &str,
        layer_index: usize,
        width: u32,
        height: u32,
    ) {
        let texture = GpuLayerTexture::create(&self.ctx, width, height);
        self.textures
            .insert((panel_id.to_string(), layer_index), texture);
    }

    /// CPU ビットマップをテクスチャへアップロードする。
    ///
    /// テクスチャが存在しない場合は何もしない。
    pub fn upload_cpu_bitmap(&self, panel_id: &str, layer_index: usize, pixels: &[u8]) {
        let key = (panel_id.to_string(), layer_index);
        if let Some(texture) = self.textures.get(&key) {
            texture.upload_pixels(&self.ctx, pixels);
        }
    }

    /// 指定パネル・レイヤーのテクスチャを取得する。
    pub fn get(&self, panel_id: &str, layer_index: usize) -> Option<&GpuLayerTexture> {
        self.textures.get(&(panel_id.to_string(), layer_index))
    }
}

/// ペン先テクスチャのキャッシュ。
///
/// 真円ペン (`tip: None`) はテクスチャ不要。ビットマップペン先は wgpu::Texture としてアップロードする。
pub struct GpuPenTipCache {
    ctx: GpuCanvasContext,
    /// pen_preset_id → GpuLayerTexture
    textures: HashMap<String, GpuLayerTexture>,
}

impl GpuPenTipCache {
    /// 新しいキャッシュを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            ctx: GpuCanvasContext::new(device, queue),
            textures: HashMap::new(),
        }
    }

    /// ペンプリセットからペン先テクスチャをアップロードする。
    ///
    /// `tip: None`（真円ペン）の場合は何もしない。
    /// ビットマップがある場合はテクスチャを作成してアップロードする。
    pub fn upload_from_preset(&mut self, preset_id: &str, pen: &PenPreset) {
        let Some(tip) = &pen.tip else {
            return;
        };

        match tip {
            PenTipBitmap::AlphaMask8 {
                width,
                height,
                data,
            } => {
                // AlphaMask8 はグレースケール 1 バイト/ピクセルなので RGBA に変換する。
                let rgba = alpha_mask_to_rgba(data);
                let texture = GpuLayerTexture::create(&self.ctx, *width, *height);
                texture.upload_pixels(&self.ctx, &rgba);
                self.textures.insert(preset_id.to_string(), texture);
            }
            PenTipBitmap::Rgba8 {
                width,
                height,
                data,
            } => {
                let texture = GpuLayerTexture::create(&self.ctx, *width, *height);
                texture.upload_pixels(&self.ctx, data);
                self.textures.insert(preset_id.to_string(), texture);
            }
            PenTipBitmap::PngBlob { width, height, .. } => {
                // PngBlob はデコードが必要だが Phase 8A では空テクスチャを確保するのみ。
                let texture = GpuLayerTexture::create(&self.ctx, *width, *height);
                self.textures.insert(preset_id.to_string(), texture);
            }
        }
    }

    /// キャッシュ済みペン先テクスチャを取得する。
    pub fn get(&self, preset_id: &str) -> Option<&GpuLayerTexture> {
        self.textures.get(preset_id)
    }
}

/// AlphaMask8 (グレースケール 1 バイト/ピクセル) を RGBA 4 バイト/ピクセルへ変換する。
///
/// R=G=B=255 固定、A = alpha 値とする。
fn alpha_mask_to_rgba(data: &[u8]) -> Vec<u8> {
    data.iter()
        .flat_map(|&alpha| [255u8, 255, 255, alpha])
        .collect()
}
