use app_core::{
    BitmapComposite, BitmapCompositor, CanvasBitmap, PaintPluginContext, ToolKind,
};

#[derive(Clone, Copy)]
pub(crate) struct EraseComposite;

impl BitmapCompositor for EraseComposite {
    fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
        let width = bitmap_a.width.min(bitmap_b.width);
        let height = bitmap_a.height.min(bitmap_b.height);
        let mut out = CanvasBitmap::transparent(width, height);
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                let erase = bitmap_a.pixels[index + 3] as f32 / 255.0;
                let previous = [
                    bitmap_b.pixels[index],
                    bitmap_b.pixels[index + 1],
                    bitmap_b.pixels[index + 2],
                    bitmap_b.pixels[index + 3],
                ];
                if erase <= 0.0 {
                    out.pixels[index..index + 4].copy_from_slice(&previous);
                    continue;
                }
                let remaining_alpha = (previous[3] as f32 / 255.0) * (1.0 - erase);
                let mut result = previous;
                result[3] = (remaining_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                if result[3] == 0 {
                    result[0] = 0;
                    result[1] = 0;
                    result[2] = 0;
                }
                out.pixels[index..index + 4].copy_from_slice(&result);
            }
        }
        out
    }
}

pub(crate) fn fill_color(context: &PaintPluginContext<'_>) -> [u8; 4] {
    stamp_color(context)
}

pub(crate) fn stamp_color(context: &PaintPluginContext<'_>) -> [u8; 4] {
    match context.tool {
        ToolKind::Eraser if context.active_layer_is_background => [255, 255, 255, 255],
        ToolKind::Eraser => [0, 0, 0, 255],
        _ => context.color.to_rgba8(),
    }
}

pub(crate) fn edit_composite(context: &PaintPluginContext<'_>) -> BitmapComposite {
    match context.tool {
        ToolKind::Pen => BitmapComposite::source_over(),
        ToolKind::Eraser if context.active_layer_is_background => BitmapComposite::source_over(),
        ToolKind::Eraser => BitmapComposite::custom(EraseComposite),
        ToolKind::Bucket | ToolKind::LassoBucket | ToolKind::PanelRect => {
            BitmapComposite::source_over()
        }
    }
}

pub(crate) fn blend_stamp(
    target: &mut CanvasBitmap,
    stamp: &CanvasBitmap,
    offset_x: isize,
    offset_y: isize,
) {
    for y in 0..stamp.height {
        for x in 0..stamp.width {
            let target_x = offset_x + x as isize;
            let target_y = offset_y + y as isize;
            if target_x < 0
                || target_y < 0
                || target_x as usize >= target.width
                || target_y as usize >= target.height
            {
                continue;
            }
            let src_index = (y * stamp.width + x) * 4;
            let incoming = [
                stamp.pixels[src_index],
                stamp.pixels[src_index + 1],
                stamp.pixels[src_index + 2],
                stamp.pixels[src_index + 3],
            ];
            if incoming[3] == 0 {
                continue;
            }
            let dst_index = (target_y as usize * target.width + target_x as usize) * 4;
            let previous = [
                target.pixels[dst_index],
                target.pixels[dst_index + 1],
                target.pixels[dst_index + 2],
                target.pixels[dst_index + 3],
            ];
            target.pixels[dst_index..dst_index + 4]
                .copy_from_slice(&source_over_pixel(previous, incoming));
        }
    }
}

pub(crate) fn source_over_pixel(previous: [u8; 4], incoming: [u8; 4]) -> [u8; 4] {
    let src_a = incoming[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return previous;
    }
    let dst_a = previous[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0_u8; 4];
    for channel in 0..3 {
        let src = incoming[channel] as f32 / 255.0;
        let dst = previous[channel] as f32 / 255.0;
        let value = src * src_a + dst * (1.0 - src_a);
        out[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}
