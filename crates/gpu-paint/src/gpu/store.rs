//! `(koma_id, layer_index)` キーでレイヤー/マスク/合成テクスチャを管理するストア。
//!
//! レイヤーテクスチャの生成・部分アップロード・差分同期 (`sync_koma_layers`) を担う。
//! スナップショット (`snapshot`)・読み戻し (`readback`)・マスク (`mask`) の各操作は
//! 同名モジュールが `impl LayerTextureStore` で拡張する。

use std::collections::HashMap;
use std::sync::Arc;

use geometry::PageDirtyRect;

use crate::gpu::context::GpuCanvasContext;
use crate::gpu::texture::GpuRgbaTexture;

/// 1 レイヤー分の CPU ピクセル + 任意マスクの差分同期入力。
///
/// [`LayerTextureStore::sync_koma_layers`] へ渡し、コマ単位でテクスチャを
/// 差分的に再構築するために使う。`pixels` はレイヤー本体の RGBA8 列、`mask` は
/// `(width, height, alpha)` の 1ch アルファ列 (なければ `None`)。
pub struct LayerUpload<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
    pub mask: Option<(u32, u32, &'a [u8])>,
}

/// `(koma_id: String, layer_index: usize)` をキーにレイヤーテクスチャを管理するストア。
pub struct LayerTextureStore {
    ctx: GpuCanvasContext,
    textures: HashMap<(String, usize), GpuRgbaTexture>,
    composite_textures: HashMap<String, GpuRgbaTexture>,
    mask_textures: HashMap<(String, usize), wgpu::Texture>,
}

impl LayerTextureStore {
    /// 新しいストアを生成する。
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            ctx: GpuCanvasContext::new(device, queue),
            textures: HashMap::new(),
            composite_textures: HashMap::new(),
            mask_textures: HashMap::new(),
        }
    }

    /// 共有 GPU コンテキストへの参照を返す (兄弟モジュールの dispatch/readback 用)。
    pub(crate) fn ctx(&self) -> &GpuCanvasContext {
        &self.ctx
    }

    /// 指定コマ・レイヤーインデックスのテクスチャを生成・登録する。
    ///
    /// 同じキーが既に存在する場合は上書きする。
    pub fn create_layer_texture(
        &mut self,
        koma_id: &str,
        layer_index: usize,
        width: u32,
        height: u32,
    ) {
        let texture = GpuRgbaTexture::create(&self.ctx, width, height);
        self.textures
            .insert((koma_id.to_string(), layer_index), texture);
    }

    /// CPU ビットマップをテクスチャへアップロードする。
    ///
    /// テクスチャが存在しない場合は何もしない。
    pub fn upload_cpu_bitmap(&self, koma_id: &str, layer_index: usize, pixels: &[u8]) {
        let key = (koma_id.to_string(), layer_index);
        if let Some(texture) = self.textures.get(&key) {
            texture.upload_pixels(&self.ctx, pixels);
        }
    }

    /// 指定コマ・レイヤーのテクスチャを取得する。
    pub fn get(&self, koma_id: &str, layer_index: usize) -> Option<&GpuRgbaTexture> {
        self.textures.get(&(koma_id.to_string(), layer_index))
    }

    /// 指定コマ・レイヤーの sRGB TextureView を生成して返す。
    pub fn get_view(&self, koma_id: &str, layer_index: usize) -> Option<wgpu::TextureView> {
        self.get(koma_id, layer_index)
            .map(|t| t.create_srgb_view())
    }

    /// CPU ピクセルをレイヤーテクスチャの指定矩形（コマローカル座標）へ書き込む（RGBA8、行優先）。
    ///
    /// テクスチャが存在しない場合は何もしない。
    pub fn upload_region(
        &self,
        koma_id: &str,
        layer_index: usize,
        region: PageDirtyRect,
        pixels: &[u8],
    ) {
        let (x, y) = (region.x as u32, region.y as u32);
        let (w, h) = (region.width as u32, region.height as u32);
        let Some(dst) = self.get(koma_id, layer_index) else {
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

    /// コマ ID に紐づく合成テクスチャを遅延作成する。
    ///
    /// 既存テクスチャが同サイズなら no-op。サイズが異なる場合は旧テクスチャを
    /// drop して新規作成する。
    pub fn ensure_composite_texture(&mut self, koma_id: &str, width: u32, height: u32) {
        let key = koma_id.to_string();
        if let Some(existing) = self.composite_textures.get(&key)
            && existing.width == width
            && existing.height == height
        {
            return;
        }
        let tex = GpuRgbaTexture::create(&self.ctx, width, height);
        self.composite_textures.insert(key, tex);
    }

    /// コマ ID に紐づく合成テクスチャを取得する。
    pub fn get_composite(&self, koma_id: &str) -> Option<&GpuRgbaTexture> {
        self.composite_textures.get(koma_id)
    }

    /// コマ ID に紐づく合成テクスチャの sRGB TextureView を生成する。
    pub fn get_composite_view(&self, koma_id: &str) -> Option<wgpu::TextureView> {
        self.get_composite(koma_id).map(|t| t.create_srgb_view())
    }

    /// 指定コマの全レイヤーテクスチャ・マスクテクスチャエントリを削除する。
    ///
    /// レイヤー追加/削除/並べ替えで古いインデックスが残存するのを防ぐため、
    /// `sync_all_layers_to_gpu` の再構築前に呼び出す。
    pub fn clear_layers_for_koma(&mut self, koma_id: &str) {
        let pid = koma_id.to_string();
        self.textures.retain(|(p, _), _| p != &pid);
        self.mask_textures.retain(|(p, _), _| p != &pid);
    }

    /// 指定コマに登録済みのレイヤーテクスチャ数を返す。
    ///
    /// 差分同期 (`sync_koma_layers`) が当該コマのみを対象としていることを
    /// 検証するためのクエリ。
    pub fn layer_count_for_koma(&self, koma_id: &str) -> usize {
        self.textures.keys().filter(|(p, _)| p == koma_id).count()
    }

    /// 1 コマ分のレイヤー/マスク/合成テクスチャを差分的に再構築する (BL-117)。
    ///
    /// 当該コマの既存レイヤー/マスクエントリのみをクリアしてから、`layers` を
    /// インデックス順に再登録・アップロードする。`sync_all_layers_to_gpu` の
    /// 全ページ全コマ全転送と異なり、コマ集合が不変でアクティブコマのレイヤー構成
    /// だけが変わった場合 (レイヤー追加/削除/並べ替え) に、当該コマだけを同期する。
    ///
    /// `composite_size` は合成テクスチャのサイズ (コマの composite_cache 寸法)。
    pub fn sync_koma_layers(
        &mut self,
        koma_id: &str,
        composite_size: (u32, u32),
        layers: &[LayerUpload<'_>],
    ) {
        self.clear_layers_for_koma(koma_id);
        let (cw, ch) = composite_size;
        self.ensure_composite_texture(koma_id, cw, ch);
        for (idx, layer) in layers.iter().enumerate() {
            self.create_layer_texture(koma_id, idx, layer.width, layer.height);
            self.upload_cpu_bitmap(koma_id, idx, layer.pixels);
            match layer.mask {
                Some((mw, mh, alpha)) => self.upload_mask(koma_id, idx, mw, mh, alpha),
                None => self.remove_mask(koma_id, idx),
            }
        }
    }

    /// マスクテクスチャを登録する (`mask` モジュールから呼ぶ)。
    pub(crate) fn insert_mask_texture(
        &mut self,
        koma_id: &str,
        layer_index: usize,
        texture: wgpu::Texture,
    ) {
        self.mask_textures
            .insert((koma_id.to_string(), layer_index), texture);
    }

    /// マスクテクスチャを取得する (`mask` モジュールから呼ぶ)。
    pub(crate) fn mask_texture(
        &self,
        koma_id: &str,
        layer_index: usize,
    ) -> Option<&wgpu::Texture> {
        self.mask_textures
            .get(&(koma_id.to_string(), layer_index))
    }

    /// マスクテクスチャを削除する (`mask` モジュールから呼ぶ)。
    pub(crate) fn remove_mask_texture(&mut self, koma_id: &str, layer_index: usize) {
        self.mask_textures
            .remove(&(koma_id.to_string(), layer_index));
    }
}
