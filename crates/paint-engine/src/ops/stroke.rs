use crate::painting::PaintPluginContext;
use geometry::{KomaLocalPoint, PageDirtyRect};
use raster::{BitmapEdit, MAX_STAMP_STEPS, RgbaBitmap};

use super::{composite, stamp};

/// ストローク segment の始点・終点から補間スタンプ座標列を計算する。
///
/// スタンプ間隔の基準サイズは context 解決時に筆圧カーブ 1 回適用済みの
/// `resolved_size` を使う (BL-030)。
///
/// `apps/desktop` からクレート外で呼べるよう `pub` で公開する。
/// Phase 8B〜8D の暫定措置として GPU ディスパッチ呼び出し側が使用する。
/// Phase 8E（CPU bitmap 廃止）以降は `gpu-paint` が直接 dispatch を担うため削除予定。
pub fn compute_stamp_positions(
    from: KomaLocalPoint,
    to: KomaLocalPoint,
    context: &PaintPluginContext<'_>,
) -> Vec<KomaLocalPoint> {
    let size = context.resolved_size.max(1);
    let spacing = effective_spacing(context, size);
    let dx = to.x as f32 - from.x as f32;
    let dy = to.y as f32 - from.y as f32;
    let distance = dx.hypot(dy);
    let steps = ((distance / spacing).ceil().max(1.0) as usize).min(MAX_STAMP_STEPS);
    let mut points = Vec::with_capacity(steps + 1);
    for step in 0..=steps {
        let t = if steps == 0 {
            0.0
        } else {
            step as f32 / steps as f32
        };
        let x = from.x as f32 + dx * t;
        let y = from.y as f32 + dy * t;
        points.push(KomaLocalPoint::new(
            x.round().max(0.0) as usize,
            y.round().max(0.0) as usize,
        ));
    }
    points
}

pub(crate) fn stroke_segment_edit(
    from: KomaLocalPoint,
    to: KomaLocalPoint,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    let points = compute_stamp_positions(from, to, context);
    stroke_like_edit(&points, context)
}

/// スタンプ列とスタンプ寸法からストロークの dirty rect を画素生成なしで求める。
///
/// `stroke_like_edit` の境界計算と同一でなければ CPU 経路と plan の dirty rect がずれる。
/// 範囲が空 (点なし・幅/高さ 0) のときは `None`。
pub(crate) fn stroke_dirty_rect(
    points: &[KomaLocalPoint],
    stamp_width: usize,
    stamp_height: usize,
) -> Option<PageDirtyRect> {
    if points.is_empty() {
        return None;
    }
    let half_w = stamp_width as isize / 2;
    let half_h = stamp_height as isize / 2;
    let mut left = usize::MAX;
    let mut top = usize::MAX;
    let mut right = 0usize;
    let mut bottom = 0usize;
    for point in points {
        let stamp_left = point.x.saturating_sub(half_w.max(0) as usize);
        let stamp_top = point.y.saturating_sub(half_h.max(0) as usize);
        left = left.min(stamp_left);
        top = top.min(stamp_top);
        right = right.max(stamp_left.saturating_add(stamp_width));
        bottom = bottom.max(stamp_top.saturating_add(stamp_height));
    }
    if left == usize::MAX || right <= left || bottom <= top {
        return None;
    }
    Some(PageDirtyRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(crate) fn stroke_like_edit(
    points: &[KomaLocalPoint],
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    if points.is_empty() {
        return None;
    }
    let stamp_bitmap = stamp::build_stamp(context)?;
    let half_w = stamp_bitmap.width as isize / 2;
    let half_h = stamp_bitmap.height as isize / 2;
    let dirty_rect = stroke_dirty_rect(points, stamp_bitmap.width, stamp_bitmap.height)?;
    let mut bitmap = RgbaBitmap::transparent(dirty_rect.width, dirty_rect.height);
    for point in points {
        let local_x = point.x.saturating_sub(dirty_rect.x);
        let local_y = point.y.saturating_sub(dirty_rect.y);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 大きな距離でもスタンプ数が MAX_STAMP_STEPS を超えないことを検証する。
    #[test]
    fn stroke_segment_steps_capped_at_max() {
        use crate::painting::PaintPluginContext;
        use editor_state::{ColorRgba8, PenPreset, ToolKind};
        use raster::RgbaBitmap;

        let layer = RgbaBitmap::transparent(1000, 1000);
        let composited = RgbaBitmap::transparent(1000, 1000);
        let pen = PenPreset {
            spacing_percent: 1.0,
            ..Default::default()
        };
        let context = PaintPluginContext {
            tool: ToolKind::Pen,
            tool_id: "",
            provider_plugin_id: "",
            drawing_plugin_id: "",
            tool_settings: &[],
            color: ColorRgba8::new(0, 0, 0, 255),
            resolved_size: 2,
            pen: &pen,
            active_layer_bitmap: &layer,
            composited_bitmap: &composited,
            active_layer_is_background: false,
            active_layer_index: 0,
            layer_count: 1,
        };
        let from = KomaLocalPoint::new(0, 0);
        // 非常に長い距離（spacing=1px なら本来 10000 スタンプ）
        let to = KomaLocalPoint::new(999, 0);
        let size = context.resolved_size.max(1);
        let spacing = effective_spacing(&context, size);
        let distance = (to.x as f32 - from.x as f32).hypot(0.0);
        let raw_steps = (distance / spacing).ceil().max(1.0) as usize;
        let capped = raw_steps.min(MAX_STAMP_STEPS);
        assert!(raw_steps > MAX_STAMP_STEPS, "raw steps should exceed cap");
        assert_eq!(capped, MAX_STAMP_STEPS);
        // stroke_segment_edit 自体も正常に完了する
        assert!(stroke_segment_edit(from, to, &context).is_some());
    }

    /// compute_stamp_positions が MAX_STAMP_STEPS 以下の数の座標を返すことを確認する。
    #[test]
    fn compute_stamp_positions_respects_max_steps() {
        use crate::painting::PaintPluginContext;
        use editor_state::{ColorRgba8, PenPreset, ToolKind};
        use raster::RgbaBitmap;

        let layer = RgbaBitmap::transparent(1000, 1000);
        let composited = RgbaBitmap::transparent(1000, 1000);
        let pen = PenPreset {
            spacing_percent: 1.0,
            ..Default::default()
        };
        let context = PaintPluginContext {
            tool: ToolKind::Pen,
            tool_id: "",
            provider_plugin_id: "",
            drawing_plugin_id: "",
            tool_settings: &[],
            color: ColorRgba8::new(255, 0, 0, 255),
            resolved_size: 2,
            pen: &pen,
            active_layer_bitmap: &layer,
            composited_bitmap: &composited,
            active_layer_is_background: false,
            active_layer_index: 0,
            layer_count: 1,
        };
        let positions =
            compute_stamp_positions(KomaLocalPoint::new(0, 0), KomaLocalPoint::new(999, 0), &context);
        assert!(!positions.is_empty());
        assert!(positions.len() <= MAX_STAMP_STEPS + 1);
    }
}
