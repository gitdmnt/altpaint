//! GPU テクスチャ → CPU RGBA8 読み戻し。
//!
//! 行パディング (`COPY_BYTES_PER_ROW_ALIGNMENT`) を解除し、パック済み RGBA8 列を返す。
//! 保存経路でのみ呼ぶ (readback コストが大きいため)。

use crate::gpu::context::GpuCanvasContext;
use crate::gpu::store::{KomaTextureId, LayerTextureStore};

impl LayerTextureStore {
    /// レイヤーテクスチャ全体を CPU へ読み戻す（保存時のみ呼ぶ）。
    ///
    /// `bytes_per_row` は `COPY_BYTES_PER_ROW_ALIGNMENT` (256) の倍数に整列する必要があるため、
    /// パディングバッファを介して読み戻し、パック済み RGBA8 列に詰め直して返す。
    pub fn read_back_full(
        &self,
        koma_id: KomaTextureId,
        layer_index: usize,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let tex = self.get(koma_id, layer_index)?;
        read_back_texture(self.ctx(), &tex.texture, tex.width, tex.height)
    }

    /// 合成テクスチャを CPU へ読み戻す（保存経路の `koma.composite_cache` 更新用）。
    pub fn read_back_composite(&self, koma_id: KomaTextureId) -> Option<(u32, u32, Vec<u8>)> {
        let tex = self.get_composite(koma_id)?;
        let w = tex.width;
        let h = tex.height;
        read_back_texture(self.ctx(), &tex.texture, w, h)
    }
}

/// 任意の `wgpu::Texture`（Rgba8Unorm, COPY_SRC）全体を CPU RGBA8 Vec へ読み戻す。
///
/// 行パディング（`COPY_BYTES_PER_ROW_ALIGNMENT`）を解除してパックされた RGBA8 列を返す。
pub(crate) fn read_back_texture(
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
        label: Some("gpu-paint-readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-paint-readback-encoder"),
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
