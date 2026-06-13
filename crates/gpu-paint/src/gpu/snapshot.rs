//! ストローク前後スナップショット (Undo/Redo 用) の GPU テクスチャ操作。
//!
//! `snapshot_region` でレイヤーの矩形を GPU-to-GPU コピーし、`restore_region` で
//! 書き戻す。`create_snapshot_texture` は CPU ピクセルを保持する野良テクスチャを
//! 作る (R23: 旧 `create_and_upload` をストア外テクスチャ生成と明示する名へ)。

use geometry::{KomaLocalPoint, PageDirtyRect};

use crate::gpu::store::{KomaTextureId, LayerTextureStore};

impl LayerTextureStore {
    /// レイヤーテクスチャの指定矩形（コマローカル座標）を GPU-to-GPU でコピーして返す。
    ///
    /// ストローク前/後スナップショット作成用。返却テクスチャは `COPY_SRC | COPY_DST` を持つ。
    pub fn snapshot_region(
        &self,
        koma_id: KomaTextureId,
        layer_index: usize,
        region: PageDirtyRect,
    ) -> Option<wgpu::Texture> {
        let (x, y) = (region.x as u32, region.y as u32);
        let (w, h) = (region.width as u32, region.height as u32);
        let src = self.get(koma_id, layer_index)?;
        let ctx = self.ctx();
        let dst = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-paint-snapshot"),
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
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-paint-snapshot-encoder"),
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
        ctx.queue.submit(std::iter::once(encoder.finish()));
        Some(dst)
    }

    /// スナップショットテクスチャを GPU-to-GPU でレイヤーの指定位置（コマローカル座標）へ復元する。
    ///
    /// Undo/Redo 用。`src` の `width/height` 全体をレイヤーへコピーする。
    pub fn restore_region(
        &self,
        koma_id: KomaTextureId,
        layer_index: usize,
        origin: KomaLocalPoint,
        src: &wgpu::Texture,
    ) {
        let (x, y) = (origin.x as u32, origin.y as u32);
        let Some(dst) = self.get(koma_id, layer_index) else {
            return;
        };
        let w = src.width();
        let h = src.height();
        let ctx = self.ctx();
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-paint-restore-encoder"),
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
        ctx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// 指定ピクセル（RGBA8）を保持する新規 GPU テクスチャを作成して返す。
    ///
    /// ストローク before スナップショット用。`COPY_SRC | COPY_DST` を持つ。
    pub fn create_snapshot_texture(&self, w: u32, h: u32, pixels: &[u8]) -> wgpu::Texture {
        let ctx = self.ctx();
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-paint-upload"),
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
        ctx.queue.write_texture(
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
