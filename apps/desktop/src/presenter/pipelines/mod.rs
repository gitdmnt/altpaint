//! presenter の GPU パイプライン群。
//!
//! - `present`: テクスチャ提示パイプライン (キャンバス / パネル / ステータス共用)
//! - `quad`: 単一 uniform バインディングの quad パイプライン共通実装 (solid / circle / line)
//!
//! quad 系の uniform バイト列エンコーダは本モジュールに集約する。3 パイプラインの
//! コピペ (build_pipeline / prepare / record) は `QuadPipeline` 1 つへ統合済み (BL-116)。

mod present;
mod quad;

pub(crate) use present::PresentPipeline;
pub(crate) use quad::{QuadLayerRanges, QuadPipeline};

use geometry::WindowRect;

use crate::present_quads::{CircleQuad, LineQuad, SolidQuad, pixel_rect_to_ndc};

/// `SolidQuad` を 32 バイトの uniform バイト列へ変換する。
pub(crate) fn solid_quad_uniform_bytes(
    quad: &SolidQuad,
    surface_width: u32,
    surface_height: u32,
) -> Vec<u8> {
    let ndc = pixel_rect_to_ndc(quad.rect, surface_width, surface_height);
    let color = [
        quad.color[0] as f32 / 255.0,
        quad.color[1] as f32 / 255.0,
        quad.color[2] as f32 / 255.0,
        quad.color[3] as f32 / 255.0,
    ];
    let values = [
        ndc[0], ndc[1], ndc[2], ndc[3], color[0], color[1], color[2], color[3],
    ];
    f32_values_to_le_bytes(&values)
}

/// 円リングの bbox を NDC 化し、ピクセル座標と SDF パラメータを 64 バイトの uniform へ詰める。
pub(crate) fn circle_quad_uniform_bytes(
    quad: &CircleQuad,
    surface_width: u32,
    surface_height: u32,
) -> Vec<u8> {
    let pad = quad.radius + quad.thickness + 1.0;
    let bbox_min_x = (quad.center_px[0] - pad).floor();
    let bbox_min_y = (quad.center_px[1] - pad).floor();
    let bbox_max_x = (quad.center_px[0] + pad).ceil();
    let bbox_max_y = (quad.center_px[1] + pad).ceil();
    let rect = WindowRect {
        x: bbox_min_x.max(0.0) as usize,
        y: bbox_min_y.max(0.0) as usize,
        width: (bbox_max_x - bbox_min_x).max(1.0) as usize,
        height: (bbox_max_y - bbox_min_y).max(1.0) as usize,
    };
    let ndc = pixel_rect_to_ndc(rect, surface_width, surface_height);
    let color = [
        quad.color[0] as f32 / 255.0,
        quad.color[1] as f32 / 255.0,
        quad.color[2] as f32 / 255.0,
        quad.color[3] as f32 / 255.0,
    ];
    let values = [
        ndc[0],
        ndc[1],
        ndc[2],
        ndc[3],
        bbox_min_x,
        bbox_min_y,
        bbox_max_x,
        bbox_max_y,
        quad.center_px[0],
        quad.center_px[1],
        quad.radius,
        quad.thickness,
        color[0],
        color[1],
        color[2],
        color[3],
    ];
    f32_values_to_le_bytes(&values)
}

/// 線分カプセルの bbox を NDC 化し、ピクセル座標と SDF パラメータを 80 バイトの uniform へ詰める。
pub(crate) fn line_quad_uniform_bytes(
    quad: &LineQuad,
    surface_width: u32,
    surface_height: u32,
) -> Vec<u8> {
    let pad = quad.thickness + 1.0;
    let bbox_min_x = quad.start_px[0].min(quad.end_px[0]) - pad;
    let bbox_min_y = quad.start_px[1].min(quad.end_px[1]) - pad;
    let bbox_max_x = quad.start_px[0].max(quad.end_px[0]) + pad;
    let bbox_max_y = quad.start_px[1].max(quad.end_px[1]) + pad;
    let rect = WindowRect {
        x: bbox_min_x.max(0.0).floor() as usize,
        y: bbox_min_y.max(0.0).floor() as usize,
        width: (bbox_max_x.ceil() - bbox_min_x.floor()).max(1.0) as usize,
        height: (bbox_max_y.ceil() - bbox_min_y.floor()).max(1.0) as usize,
    };
    let ndc = pixel_rect_to_ndc(rect, surface_width, surface_height);
    let color = [
        quad.color[0] as f32 / 255.0,
        quad.color[1] as f32 / 255.0,
        quad.color[2] as f32 / 255.0,
        quad.color[3] as f32 / 255.0,
    ];
    let values = [
        ndc[0],
        ndc[1],
        ndc[2],
        ndc[3],
        bbox_min_x.floor(),
        bbox_min_y.floor(),
        bbox_max_x.ceil(),
        bbox_max_y.ceil(),
        quad.start_px[0],
        quad.start_px[1],
        quad.end_px[0],
        quad.end_px[1],
        // thickness: vec4 (.x のみ使用、残りはパディング)
        quad.thickness,
        0.0,
        0.0,
        0.0,
        // color: vec4
        color[0],
        color[1],
        color[2],
        color[3],
    ];
    f32_values_to_le_bytes(&values)
}

/// f32 配列をリトルエンディアンのバイト列へ変換する。
fn f32_values_to_le_bytes(values: &[f32]) -> Vec<u8> {
    let mut bytes = vec![0u8; values.len() * 4];
    for (index, value) in values.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
