use app_core::{BitmapEdit, CanvasBitmap, CanvasDirtyRect, PaintPluginContext, PanelLocalPoint};

use super::{composite, stamp};

pub(crate) fn stroke_segment_edit(
    from: PanelLocalPoint,
    to: PanelLocalPoint,
    pressure: f32,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    let size = stamp::effective_size(context, pressure).max(1);
    let spacing = effective_spacing(context, size);
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let distance = dx.hypot(dy);
    let steps = (distance / spacing).ceil().max(1.0) as usize;
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = if steps == 0 {
            0.0
        } else {
            step as f32 / steps as f32
        };
        let x = from.x as f32 + dx * t;
        let y = from.y as f32 + dy * t;
        points.push(PanelLocalPoint::new(
            x.round().max(0.0) as usize,
            y.round().max(0.0) as usize,
        ));
    }
    stroke_like_edit(&points, pressure, context)
}

pub(crate) fn stroke_like_edit(
    points: &[PanelLocalPoint],
    pressure: f32,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    if points.is_empty() {
        return None;
    }
    let stamp_bitmap = stamp::build_stamp(context, pressure)?;
    let half_w = stamp_bitmap.width as isize / 2;
    let half_h = stamp_bitmap.height as isize / 2;
    let mut left = usize::MAX;
    let mut top = usize::MAX;
    let mut right = 0usize;
    let mut bottom = 0usize;
    for point in points {
        let stamp_left = point.x.saturating_sub(half_w.max(0) as usize);
        let stamp_top = point.y.saturating_sub(half_h.max(0) as usize);
        left = left.min(stamp_left);
        top = top.min(stamp_top);
        right = right.max(stamp_left.saturating_add(stamp_bitmap.width));
        bottom = bottom.max(stamp_top.saturating_add(stamp_bitmap.height));
    }
    if left == usize::MAX || right <= left || bottom <= top {
        return None;
    }

    let dirty_rect = CanvasDirtyRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    let mut bitmap = CanvasBitmap::transparent(dirty_rect.width, dirty_rect.height);
    for point in points {
        let local_x = point.x.saturating_sub(left);
        let local_y = point.y.saturating_sub(top);
        composite::blend_stamp(
            &mut bitmap,
            &stamp_bitmap,
            local_x as isize - half_w,
            local_y as isize - half_h,
        );
    }

    Some(BitmapEdit::new(
        dirty_rect,
        bitmap,
        composite::edit_composite(context),
    ))
}

fn effective_spacing(context: &PaintPluginContext<'_>, size: u32) -> f32 {
    (size as f32 * (context.pen.spacing_percent / 100.0)).clamp(1.0, size.max(1) as f32)
}
