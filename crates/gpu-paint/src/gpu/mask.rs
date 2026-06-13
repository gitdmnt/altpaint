//! レイヤーマスク (1ch alpha) テクスチャの管理。
//!
//! 合成時に `CompositePipeline` がレイヤー色とマスクを掛け合わせる。マスクは
//! `R=G=B=255, A=mask` の RGBA8 テクスチャへ展開して保持する。

use crate::gpu::store::{KomaTextureId, LayerTextureStore};

impl LayerTextureStore {
    /// レイヤーマスク（1 ch alpha）を RGBA8（R=G=B=255, A=mask）に展開して
    /// アップロードする。既存マスクは上書きする。
    pub fn upload_mask(
        &mut self,
        koma_id: KomaTextureId,
        layer_index: usize,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) {
        let rgba: Vec<u8> = alpha
            .iter()
            .flat_map(|&a| [255u8, 255, 255, a])
            .collect();
        let texture = self.ctx().device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu-paint-mask"),
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
        self.ctx().queue.write_texture(
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
        self.insert_mask_texture(koma_id, layer_index, texture);
    }

    /// 登録済みマスクテクスチャを取得する。
    pub fn get_mask(&self, koma_id: KomaTextureId, layer_index: usize) -> Option<&wgpu::Texture> {
        self.mask_texture(koma_id, layer_index)
    }

    /// 登録済みマスクテクスチャを削除する。
    pub fn remove_mask(&mut self, koma_id: KomaTextureId, layer_index: usize) {
        self.remove_mask_texture(koma_id, layer_index);
    }
}
