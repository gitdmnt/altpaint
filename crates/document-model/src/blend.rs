//! レイヤー合成 (`RasterLayer` 列 → 合成ビットマップ)。
//!
//! ピクセル単位のブレンド (`composite_pixel` / `BlendMode`) と `RgbaBitmap` は
//! `raster` クレートが持つ。本モジュールはドメインの `RasterLayer` / `LayerMask` を
//! 参照する層合成のみを担う。

use raster::{RgbaBitmap, composite_pixel};

use crate::RasterLayer;
use geometry::{ClampToCanvasBounds, PageDirtyRect};

/// レイヤー列を可視レイヤーのみ bottom→top で透明地に合成した新しいビットマップを返す。
pub fn composite_layers(width: usize, height: usize, layers: &[RasterLayer]) -> RgbaBitmap {
    let mut result = RgbaBitmap::transparent(width, height);
    let full = PageDirtyRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    for layer in layers.iter().filter(|layer| layer.visible) {
        composite_layer_region_into(&mut result, layer, full);
    }
    result
}

/// 1 レイヤーを dirty 領域に限り target へ合成する (レイヤーマスク考慮)。
pub fn composite_layer_region_into(
    target: &mut RgbaBitmap,
    layer: &RasterLayer,
    dirty: PageDirtyRect,
) {
    let dirty = dirty.clamp_to_canvas_bounds(
        target.width.min(layer.bitmap.width).max(1),
        target.height.min(layer.bitmap.height).max(1),
    );
    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let target_index = (y * target.width + x) * 4;
            let source_index = (y * layer.bitmap.width + x) * 4;
            let mut src = [
                layer.bitmap.pixels[source_index],
                layer.bitmap.pixels[source_index + 1],
                layer.bitmap.pixels[source_index + 2],
                layer.bitmap.pixels[source_index + 3],
            ];
            if let Some(mask) = &layer.mask {
                src[3] = ((src[3] as u16 * mask.alpha_at(x, y) as u16) / 255) as u8;
            }
            let dst = [
                target.pixels[target_index],
                target.pixels[target_index + 1],
                target.pixels[target_index + 2],
                target.pixels[target_index + 3],
            ];
            let blended = composite_pixel(dst, src, &layer.blend_mode);
            target.pixels[target_index..target_index + 4].copy_from_slice(&blended);
        }
    }
}
