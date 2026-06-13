//! presenter のテクスチャリソースとアップロード操作を集約する。
//!
//! CPU ビットマップのアップロード対象 (`UploadedLayerTexture`)、GPU キャンバステクスチャ
//! および HTML パネル quad の bind group キャッシュ、`TextureQuad` の uniform バイト列化を
//! 担う。レンダーパス記録・パイプライン構築は `frame` / `pipelines` 側が持つ。

use std::time::{Duration, Instant};

use super::frame::{TextureSource, UploadRegion};
use super::shaders::LAYER_UNIFORM_SIZE;
use crate::present_quads::{TextureQuad, pixel_rect_to_ndc};

/// GPU 側に確保した 1 レイヤー分のリソース群。
/// テクスチャ・バインドグループ・ユニフォームバッファをまとめて管理する。
#[derive(Debug)]
pub(crate) struct UploadedLayerTexture {
    /// GPU テクスチャ本体（ピクセルデータを格納する VRAM 領域）。
    texture: wgpu::Texture,
    /// シェーダへリソースをバインドする束。
    /// texture + sampler + uniform_buffer の 3 つをまとめて shader の @binding に結びつける。
    bind_group: wgpu::BindGroup,
    /// 描画位置・UV・回転などを GPU へ伝えるユニフォームバッファ。
    uniform_buffer: wgpu::Buffer,
    /// テクスチャの幅（ピクセル）。サイズ変更検知に使う。
    width: u32,
    /// テクスチャの高さ（ピクセル）。サイズ変更検知に使う。
    height: u32,
    /// `true` の場合は次回フルアップロードを強制する。
    /// テクスチャを新規作成した直後は中身が未初期化なのでフラグを立てておく。
    needs_full_upload: bool,
}

impl UploadedLayerTexture {
    /// `slot` に適切なサイズのテクスチャが入っていなければ生成し直す。
    ///
    /// テクスチャのサイズはフレームごとに変わりうる（ウィンドウリサイズなど）。
    /// サイズが一致している場合は何もしない（コストゼロ）。
    pub(crate) fn ensure(
        device: &wgpu::Device,
        sampler: &wgpu::Sampler,
        bind_group_layout: &wgpu::BindGroupLayout,
        slot: &mut Option<UploadedLayerTexture>,
        width: u32,
        height: u32,
        label: &str,
    ) {
        // 既存テクスチャのサイズと要求サイズが一致していれば何もしない。
        let needs_rebuild = slot
            .as_ref()
            .is_none_or(|layer| layer.width != width || layer.height != height);
        if !needs_rebuild {
            return;
        }

        // ─── GPU テクスチャ生成 ───────────────────────────────────────────────
        // RGBA8 sRGB フォーマット（1 ピクセル 4 バイト）の 2D テクスチャ。
        // TEXTURE_BINDING: シェーダから読み取れるようにする。
        // COPY_DST: queue.write_texture でデータを書き込めるようにする。
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("altpaint-{label}-texture")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1, // 2D なので深さ 1
            },
            mip_level_count: 1, // ミップマップなし（1 解像度のみ）
            sample_count: 1,    // MSAA なし
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // 8bit RGBA + sRGB ガンマ
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // ─── ユニフォームバッファ生成 ─────────────────────────────────────────
        // 小さな定数バッファ（64 バイト）。毎フレーム write_buffer で更新する。
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("altpaint-{label}-uniform")),
            size: LAYER_UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false, // 初期データなし
        });

        // テクスチャをバインドグループに登録するためのビューを作る。
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ─── バインドグループ生成 ─────────────────────────────────────────────
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("altpaint-{label}-bind-group")),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view), // @binding(0) にテクスチャ
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler), // @binding(1) にサンプラー
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(), // @binding(2) にバッファ全体
                },
            ],
        });

        // slot を新しく作ったリソースで置き換える。
        // needs_full_upload = true にしてテクスチャの中身を初回フルアップロードさせる。
        *slot = Some(UploadedLayerTexture {
            texture,
            bind_group,
            uniform_buffer,
            width,
            height,
            needs_full_upload: true, // 新規テクスチャは未初期化なのでフルアップロード必須
        });
    }

    /// CPU ピクセルデータを GPU テクスチャへアップロードする。
    ///
    /// `needs_full_upload` が true ならテクスチャ全体を転送し、
    /// そうでなければ `upload_region` で指定された矩形だけを転送する。
    /// どちらも指定なければスキップしてゼロ統計を返す。
    pub(crate) fn upload(
        queue: &wgpu::Queue,
        layer: Option<&mut UploadedLayerTexture>,
        source: TextureSource<'_>,
        upload_region: Option<UploadRegion>,
    ) -> LayerUploadStats {
        let Some(layer) = layer else {
            return LayerUploadStats::default(); // テクスチャが未初期化なら何もしない
        };

        let started = Instant::now();

        if layer.needs_full_upload {
            // テクスチャを新規作成した直後は中身が未定義なので全面アップロードが必要。
            let bytes = upload_full_texture(queue, layer, source);
            layer.needs_full_upload = false; // 次回からは差分のみで OK
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        if let Some(region) = upload_region {
            // dirty rect 最適化: 変化した矩形だけを転送する。
            let bytes = upload_texture_region(queue, layer, source, region);
            return LayerUploadStats {
                duration: started.elapsed(),
                bytes,
            };
        }

        // upload_region も needs_full_upload もなければ何もしない（更新なし）。
        LayerUploadStats::default()
    }

    /// ユニフォームバッファに `quad` の NDC 座標・UV 範囲・回転などを書き込む。
    pub(crate) fn update_quad_uniform(
        queue: &wgpu::Queue,
        layer: Option<&UploadedLayerTexture>,
        quad: TextureQuad,
        surface_width: u32,
        surface_height: u32,
    ) {
        let Some(layer) = layer else {
            return; // テクスチャが未初期化なら更新不要
        };
        queue.write_buffer(
            &layer.uniform_buffer,
            0, // バッファの先頭から書く
            &quad_uniform_bytes(quad, surface_width, surface_height),
        );
    }

    /// レンダーパスにバインドグループをセットして draw を呼ぶ。
    pub(crate) fn draw<'a>(
        pass: &mut wgpu::RenderPass<'a>,
        layer: Option<&'a UploadedLayerTexture>,
    ) {
        let Some(layer) = layer else {
            return; // テクスチャが未初期化なら描画しない
        };
        pass.set_bind_group(0, &layer.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

/// 各レイヤーのアップロード統計（パフォーマンス計測用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayerUploadStats {
    /// アップロードにかかった時間。
    pub(crate) duration: Duration,
    /// アップロードしたバイト数。
    pub(crate) bytes: u64,
}

/// GPU キャンバステクスチャ用のバインドグループキャッシュ。
///
/// キー `(panel_id, kind, layer_index, width, height)` が変化したときのみ再生成する。
/// `kind == Composite` のとき `layer_index` は意味を持たない（`usize::MAX` を入れる）。
pub(crate) struct GpuBindGroupCache {
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) panel_id: String,
    pub(crate) kind: GpuBindGroupKind,
    pub(crate) layer_index: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuBindGroupKind {
    Single,
    Composite,
}

/// HTML パネル quad 用の bind_group 一式。
/// テクスチャは panel-runtime 側所有。サイズ一致なら同じテクスチャ実体（panel-runtime が
/// `PanelGpuTarget::create` で resize 時のみ作り直す契約）。よってサイズキーで再生成判定する。
pub(crate) struct PanelBindEntry {
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// テクスチャ全体を GPU へアップロードする。
///
/// `queue.write_texture` は CPU ピクセルバイト列を GPU テクスチャへ直接コピーする。
/// 内部的にはステージングバッファを経由して非同期転送が行われる。
fn upload_full_texture(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
) -> u64 {
    if source.width == 0 || source.height == 0 {
        return 0;
    }

    queue.write_texture(
        // 書き込み先テクスチャの情報。
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,                     // ミップレベル 0（最高解像度）
            origin: wgpu::Origin3d::ZERO,     // テクスチャ左上(0,0,0)から
            aspect: wgpu::TextureAspect::All, // 色・深度・ステンシル全て（2D カラーなら All でよい）
        },
        source.pixels, // アップロードするピクセルバイト列（RGBA 各 1 バイト）
        // ソースバイト列のレイアウト。GPU はこの情報で行の区切りを判断する。
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width * 4), // 1 行のバイト数 = 横幅 × 4(RGBA)
            rows_per_image: Some(source.height),   // 画像の行数（3D テクスチャ用だが 2D でも指定）
        },
        // アップロードする領域のサイズ。
        wgpu::Extent3d {
            width: source.width,
            height: source.height,
            depth_or_array_layers: 1, // 2D なので深さ 1
        },
    );

    // アップロードしたバイト数を返す（計測用）。
    (source.width as u64) * (source.height as u64) * 4
}

/// テクスチャの部分領域だけを GPU へアップロードする（dirty rect 最適化）。
///
/// `write_texture` の `bytes_per_row` にソース行の全幅ストライドを渡すことで、
/// scratch バッファへの行詰め直しを省略し CPU メモリコピーをゼロにする。
fn upload_texture_region(
    queue: &wgpu::Queue,
    layer: &UploadedLayerTexture,
    source: TextureSource<'_>,
    region: UploadRegion,
) -> u64 {
    if region.width == 0 || region.height == 0 || source.width == 0 || source.height == 0 {
        return 0;
    }

    // region がソース画像の外に出ていた場合はクランプして実際にコピーできる範囲を求める。
    let max_width = source.width.saturating_sub(region.x);
    let max_height = source.height.saturating_sub(region.y);
    let copy_width = region.width.min(max_width);
    let copy_height = region.height.min(max_height);
    if copy_width == 0 || copy_height == 0 {
        return 0;
    }

    // ソースバッファの (region.y, region.x) からの先頭オフセット。
    let start_offset = (region.y as usize * source.width as usize + region.x as usize) * 4;

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &layer.texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: region.x, // テクスチャ上の書き込み開始 X（ピクセル単位）
                y: region.y, // テクスチャ上の書き込み開始 Y
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &source.pixels[start_offset..], // scratch コピー不要: ソースを直接渡す
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source.width * 4), // ソース行の全幅ストライド
            rows_per_image: Some(copy_height),
        },
        wgpu::Extent3d {
            width: copy_width,
            height: copy_height,
            depth_or_array_layers: 1,
        },
    );

    (copy_width as u64) * (copy_height as u64) * 4
}

/// ウィンドウ全体を覆うフルスクリーンクワッドを作る。
///
/// destination は左上 (0,0) から (width, height) までのピクセル矩形。
/// uv_min/uv_max は (0,0)〜(1,1) でテクスチャ全体を使う。
#[cfg(test)]
pub(crate) fn fullscreen_quad(width: u32, height: u32) -> TextureQuad {
    TextureQuad {
        destination: geometry::WindowRect {
            x: 0,
            y: 0,
            width: width as usize,
            height: height as usize,
        },
        uv_min: [0.0, 0.0], // テクスチャ左上
        uv_max: [1.0, 1.0], // テクスチャ右下
        rotation_degrees: 0.0,
        bbox_size: [width as f32, height as f32],
        flip_x: false,
        flip_y: false,
    }
}

/// `TextureQuad` の情報をシェーダが受け取れる 64 バイトのユニフォームデータに変換する。
///
/// # NDC 変換
/// ピクセル座標 px を NDC 座標に変換する式:
///   ndc_x = px / surface_width  * 2.0 - 1.0  (左 = -1, 右 = +1)
///   ndc_y = 1.0 - px / surface_height * 2.0  (上 = +1, 下 = -1; Y 反転)
///
/// # バッファレイアウト（f32 × 16 = 64 バイト）
/// ```text
/// [0]  rect_min.x (NDC left)
/// [1]  rect_min.y (NDC top)     ← wgpu Y: 上が +1
/// [2]  rect_max.x (NDC right)
/// [3]  rect_max.y (NDC bottom)
/// [4]  uv_min.x
/// [5]  uv_min.y
/// [6]  uv_max.x
/// [7]  uv_max.y
/// [8]  transform.x = rotation_degrees
/// [9]  transform.y = flip_x (0.0 or 1.0)
/// [10] transform.z = flip_y (0.0 or 1.0)
/// [11] transform.w = 0 (未使用)
/// [12] metrics.x = bbox_size.x (px)
/// [13] metrics.y = bbox_size.y (px)
/// [14] metrics.z = 0 (未使用)
/// [15] metrics.w = 0 (未使用)
/// ```
pub(crate) fn quad_uniform_bytes(
    quad: TextureQuad,
    surface_width: u32,
    surface_height: u32,
) -> [u8; 64] {
    // ピクセル座標 → NDC 座標 への変換は pixel_rect_to_ndc に集約。
    let [left, top, right, bottom] =
        pixel_rect_to_ndc(quad.destination, surface_width, surface_height);

    let values = [
        left,
        top,
        right,
        bottom, // rect_min / rect_max (NDC)
        quad.uv_min[0],
        quad.uv_min[1], // uv_min
        quad.uv_max[0],
        quad.uv_max[1],                      // uv_max
        quad.rotation_degrees,               // transform.x: 回転角度(度)
        if quad.flip_x { 1.0 } else { 0.0 }, // transform.y: 左右反転フラグ
        if quad.flip_y { 1.0 } else { 0.0 }, // transform.z: 上下反転フラグ
        0.0,                                 // transform.w: 未使用パディング
        quad.bbox_size[0],                   // metrics.x: バウンディングボックス幅(px)
        quad.bbox_size[1],                   // metrics.y: バウンディングボックス高さ(px)
        0.0,                                 // metrics.z: 未使用パディング
        0.0,                                 // metrics.w: 未使用パディング
    ];

    // f32 の配列をリトルエンディアンのバイト列に変換してバッファへ詰める。
    let mut bytes = [0u8; 64];
    for (index, value) in values.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
