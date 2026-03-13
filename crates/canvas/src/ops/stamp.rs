use app_core::{BitmapEdit, CanvasBitmap, PaintPluginContext, PanelLocalPoint, PenTipBitmap};

use super::{composite, stroke};

/// スタンプ 編集 に対応するビットマップ処理を行う。
pub(crate) fn stamp_edit(
    at: PanelLocalPoint,
    pressure: f32,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    stroke::stroke_like_edit(&[at], pressure, context)
}

/// スタンプ を構築する。
///
/// 値を生成できない場合は `None` を返します。
pub(crate) fn build_stamp(context: &PaintPluginContext<'_>, pressure: f32) -> Option<CanvasBitmap> {
    let size = effective_size(context, pressure).max(1) as usize;
    let opacity = (context.pen.opacity * context.pen.flow).clamp(0.0, 1.0);
    let color = composite::stamp_color(context);
    match context.pen.tip.as_ref() {
        Some(PenTipBitmap::AlphaMask8 {
            width,
            height,
            data,
        }) if !data.is_empty() => Some(resample_alpha_tip(
            *width as usize,
            *height as usize,
            data,
            size,
            color,
            opacity,
        )),
        Some(PenTipBitmap::Rgba8 {
            width,
            height,
            data,
        }) if !data.is_empty() => Some(resample_rgba_tip(
            *width as usize,
            *height as usize,
            data,
            size,
            color,
            opacity,
        )),
        Some(PenTipBitmap::AlphaMask8 { .. }) | Some(PenTipBitmap::Rgba8 { .. }) => Some(
            generated_round_stamp(size, color, opacity, context.pen.antialias),
        ),
        Some(PenTipBitmap::PngBlob { .. }) | None => Some(generated_round_stamp(
            size,
            color,
            opacity,
            context.pen.antialias,
        )),
    }
}

/// 実効的な サイズ を返す。
pub(crate) fn effective_size(context: &PaintPluginContext<'_>, pressure: f32) -> u32 {
    match context.tool {
        app_core::ToolKind::Pen | app_core::ToolKind::Eraser => {
            let base = context.resolved_size.max(1);
            if !context.pen.pressure_enabled || context.tool == app_core::ToolKind::Eraser {
                return base;
            }
            let clamped = pressure.clamp(0.0, 1.0);
            (base as f32 * (0.2 + clamped * 0.8)).round().max(1.0) as u32
        }
        app_core::ToolKind::Bucket
        | app_core::ToolKind::LassoBucket
        | app_core::ToolKind::PanelRect => 1,
    }
}

/// ピクセル走査を行い、generated round スタンプ 用のビットマップ結果を生成する。
fn generated_round_stamp(
    size: usize,
    color: [u8; 4],
    opacity: f32,
    antialias: bool,
) -> CanvasBitmap {
    let size = size.max(1);
    let radius = size as f32 * 0.5;
    let center = radius - 0.5;
    let mut bitmap = CanvasBitmap::transparent(size, size);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = if antialias {
                (radius + 0.5 - distance).clamp(0.0, 1.0)
            } else if distance <= radius {
                1.0
            } else {
                0.0
            };
            if coverage <= 0.0 {
                continue;
            }
            let alpha = ((color[3] as f32) * opacity * coverage)
                .round()
                .clamp(0.0, 255.0) as u8;
            let index = (y * bitmap.width + x) * 4;
            bitmap.pixels[index] = color[0];
            bitmap.pixels[index + 1] = color[1];
            bitmap.pixels[index + 2] = color[2];
            bitmap.pixels[index + 3] = alpha;
        }
    }
    bitmap
}

/// ピクセル走査を行い、resample アルファ 先端形状 用のビットマップ結果を生成する。
fn resample_alpha_tip(
    source_width: usize,
    source_height: usize,
    data: &[u8],
    target_size: usize,
    color: [u8; 4],
    opacity: f32,
) -> CanvasBitmap {
    let aspect = if source_width == 0 {
        1.0
    } else {
        source_height.max(1) as f32 / source_width.max(1) as f32
    };
    let target_width = target_size.max(1);
    let target_height = ((target_size as f32 * aspect).round() as usize).max(1);
    let mut bitmap = CanvasBitmap::transparent(target_width, target_height);
    for y in 0..target_height {
        for x in 0..target_width {
            let src_x = x * source_width.max(1) / target_width.max(1);
            let src_y = y * source_height.max(1) / target_height.max(1);
            let src_index = src_y
                .saturating_mul(source_width.max(1))
                .saturating_add(src_x);
            if src_index >= data.len() {
                continue;
            }
            let alpha = ((data[src_index] as f32) * opacity)
                .round()
                .clamp(0.0, 255.0) as u8;
            let index = (y * bitmap.width + x) * 4;
            bitmap.pixels[index] = color[0];
            bitmap.pixels[index + 1] = color[1];
            bitmap.pixels[index + 2] = color[2];
            bitmap.pixels[index + 3] = alpha;
        }
    }
    bitmap
}

/// ピクセル走査を行い、resample RGBA 先端形状 用のビットマップ結果を生成する。
fn resample_rgba_tip(
    source_width: usize,
    source_height: usize,
    data: &[u8],
    target_size: usize,
    tint: [u8; 4],
    opacity: f32,
) -> CanvasBitmap {
    let aspect = if source_width == 0 {
        1.0
    } else {
        source_height.max(1) as f32 / source_width.max(1) as f32
    };
    let target_width = target_size.max(1);
    let target_height = ((target_size as f32 * aspect).round() as usize).max(1);
    let mut bitmap = CanvasBitmap::transparent(target_width, target_height);
    for y in 0..target_height {
        for x in 0..target_width {
            let src_x = x * source_width.max(1) / target_width.max(1);
            let src_y = y * source_height.max(1) / target_height.max(1);
            let src_index = (src_y
                .saturating_mul(source_width.max(1))
                .saturating_add(src_x))
                * 4;
            if src_index + 3 >= data.len() {
                continue;
            }
            let index = (y * bitmap.width + x) * 4;
            bitmap.pixels[index] = ((data[src_index] as u16 * tint[0] as u16) / 255) as u8;
            bitmap.pixels[index + 1] = ((data[src_index + 1] as u16 * tint[1] as u16) / 255) as u8;
            bitmap.pixels[index + 2] = ((data[src_index + 2] as u16 * tint[2] as u16) / 255) as u8;
            bitmap.pixels[index + 3] = ((data[src_index + 3] as f32) * opacity)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    bitmap
}
