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
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        Self {
            texture,
            width,
            height,
        }
    }

    /// Rgba8UnormSrgb view を生成して返す。Present 時にガンマ補正を自動適用するために使う。
    pub fn create_srgb_view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
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

/// `(panel_id: String, layer_index: usize)` をキーにレイヤーテクスチャを管理するプール。
pub struct GpuCanvasPool {
    ctx: GpuCanvasContext,
    textures: HashMap<(String, usize), GpuLayerTexture>,
    composite_textures: HashMap<String, GpuLayerTexture>,
    mask_textures: HashMap<(String, usize), wgpu::Texture>,
}

impl GpuCanvasPool {
    /// 新しいプールを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            ctx: GpuCanvasContext::new(device, queue),
            textures: HashMap::new(),
            composite_textures: HashMap::new(),
            mask_textures: HashMap::new(),
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

    /// 指定パネル・レイヤーの sRGB TextureView を生成して返す。
    pub fn get_view(&self, panel_id: &str, layer_index: usize) -> Option<wgpu::TextureView> {
        self.get(panel_id, layer_index)
            .map(|t| t.create_srgb_view())
    }

    /// レイヤーテクスチャの指定矩形を GPU-to-GPU でコピーして返す。
    ///
    /// ストローク前/後スナップショット作成用。返却テクスチャは `COPY_SRC | COPY_DST` を持つ。
    pub fn snapshot_region(
        &self,
        panel_id: &str,
        layer_index: usize,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) -> Option<wgpu::Texture> {
        let src = self.get(panel_id, layer_index)?;
        let dst = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-canvas-snapshot"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-canvas-snapshot-encoder"),
            });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.ctx.queue.submit(std::iter::once(encoder.finish()));
        Some(dst)
    }

    /// スナップショットテクスチャを GPU-to-GPU でレイヤーの指定位置へ復元する。
    ///
    /// Undo/Redo 用。`src` の `width/height` 全体をレイヤーへコピーする。
    pub fn restore_region(
        &self,
        panel_id: &str,
        layer_index: usize,
        x: u32,
        y: u32,
        src: &wgpu::Texture,
    ) {
        let Some(dst) = self.get(panel_id, layer_index) else {
            return;
        };
        let w = src.width();
        let h = src.height();
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-canvas-restore-encoder"),
            });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &dst.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// CPU ピクセルをレイヤーテクスチャの指定矩形へ書き込む（RGBA8、行優先）。
    ///
    /// テクスチャが存在しない場合は何もしない。
    pub fn upload_region(
        &self,
        panel_id: &str,
        layer_index: usize,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        pixels: &[u8],
    ) {
        let Some(dst) = self.get(panel_id, layer_index) else {
            return;
        };
        self.ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dst.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// レイヤーテクスチャ全体を CPU へ読み戻す（保存時のみ呼ぶ）。
    ///
    /// `bytes_per_row` は `COPY_BYTES_PER_ROW_ALIGNMENT` (256) の倍数に整列する必要があるため、
    /// パディングバッファを介して読み戻し、パック済み RGBA8 列に詰め直して返す。
    pub fn read_back_full(
        &self,
        panel_id: &str,
        layer_index: usize,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let tex = self.get(panel_id, layer_index)?;
        read_back_texture(&self.ctx, &tex.texture, tex.width, tex.height)
    }

    /// Panel ID に紐づく合成テクスチャを遅延作成する。
    ///
    /// 既存テクスチャが同サイズなら no-op。サイズが異なる場合は旧テクスチャを
    /// drop して新規作成する。
    pub fn ensure_composite_texture(&mut self, panel_id: &str, width: u32, height: u32) {
        let key = panel_id.to_string();
        if let Some(existing) = self.composite_textures.get(&key)
            && existing.width == width
            && existing.height == height
        {
            return;
        }
        let tex = GpuLayerTexture::create(&self.ctx, width, height);
        self.composite_textures.insert(key, tex);
    }

    /// Panel ID に紐づく合成テクスチャを取得する。
    pub fn get_composite(&self, panel_id: &str) -> Option<&GpuLayerTexture> {
        self.composite_textures.get(panel_id)
    }

    /// Panel ID に紐づく合成テクスチャの sRGB TextureView を生成する。
    pub fn get_composite_view(&self, panel_id: &str) -> Option<wgpu::TextureView> {
        self.get_composite(panel_id).map(|t| t.create_srgb_view())
    }

    /// レイヤーマスク（1 ch alpha）を RGBA8（R=G=B=255, A=mask）に展開して
    /// アップロードする。既存マスクは上書きする。
    pub fn upload_mask(
        &mut self,
        panel_id: &str,
        layer_index: usize,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) {
        let rgba: Vec<u8> = alpha
            .iter()
            .flat_map(|&a| [255u8, 255, 255, a])
            .collect();
        let texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-canvas-mask"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.mask_textures
            .insert((panel_id.to_string(), layer_index), texture);
    }

    /// 登録済みマスクテクスチャを取得する。
    pub fn get_mask(&self, panel_id: &str, layer_index: usize) -> Option<&wgpu::Texture> {
        self.mask_textures
            .get(&(panel_id.to_string(), layer_index))
    }

    /// 登録済みマスクテクスチャを削除する。
    pub fn remove_mask(&mut self, panel_id: &str, layer_index: usize) {
        self.mask_textures
            .remove(&(panel_id.to_string(), layer_index));
    }

    /// 指定パネルの全レイヤーテクスチャ・マスクテクスチャエントリを削除する。
    ///
    /// レイヤー追加/削除/並べ替えで古いインデックスが残存するのを防ぐため、
    /// `sync_all_layers_to_gpu` の再構築前に呼び出す。
    pub fn clear_layers_for_panel(&mut self, panel_id: &str) {
        let pid = panel_id.to_string();
        self.textures.retain(|(p, _), _| p != &pid);
        self.mask_textures.retain(|(p, _), _| p != &pid);
    }

    /// 合成テクスチャを CPU へ読み戻す（保存経路の `panel.bitmap` 更新用）。
    pub fn read_back_composite(&self, panel_id: &str) -> Option<(u32, u32, Vec<u8>)> {
        let tex = self.get_composite(panel_id)?;
        let w = tex.width;
        let h = tex.height;
        read_back_texture(&self.ctx, &tex.texture, w, h)
    }

    /// 指定ピクセル（RGBA8）を保持する新規 GPU テクスチャを作成して返す。
    ///
    /// ストローク before スナップショット用。`COPY_SRC | COPY_DST` を持つ。
    pub fn create_and_upload(&self, w: u32, h: u32, pixels: &[u8]) -> wgpu::Texture {
        let texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-canvas-upload"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        texture
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

/// 任意の `wgpu::Texture`（Rgba8Unorm, COPY_SRC）全体を CPU RGBA8 Vec へ読み戻す。
///
/// 行パディング（`COPY_BYTES_PER_ROW_ALIGNMENT`）を解除してパックされた RGBA8 列を返す。
fn read_back_texture(
    ctx: &GpuCanvasContext,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let unpadded_bpr = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bpr = unpadded_bpr.div_ceil(align) * align;
    let buf_size = (padded_bpr * h) as wgpu::BufferAddress;
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu-canvas-readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-canvas-readback-encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue.submit(std::iter::once(encoder.finish()));
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = ctx.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    rx.recv().ok()?.ok()?;
    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((unpadded_bpr * h) as usize);
    for row in 0..h {
        let start = (row * padded_bpr) as usize;
        let end = start + unpadded_bpr as usize;
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    readback.unmap();
    Some((w, h, pixels))
}
