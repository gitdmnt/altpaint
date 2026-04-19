use app_core::{
    BitmapEdit, CanvasBitmap, CanvasDirtyRect, PaintPluginContext, PanelLocalPoint,
    paint_params::MAX_STAMP_STEPS,
};

use super::{composite, stamp};

/// ストローク segment の始点・終点から補間スタンプ座標列を計算する。
///
/// `apps/desktop` からクレート外で呼べるよう `pub` で公開する。
/// Phase 8B〜8D の暫定措置として GPU ディスパッチ呼び出し側が使用する。
/// Phase 8E（CPU bitmap 廃止）以降は `gpu-canvas` が直接 dispatch を担うため削除予定。
pub fn compute_stamp_positions(
    from: PanelLocalPoint,
    to: PanelLocalPoint,
    pressure: f32,
    context: &PaintPluginContext<'_>,
) -> Vec<PanelLocalPoint> {
    let size = stamp::effective_size(context, pressure).max(1);
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
        points.push(PanelLocalPoint::new(
            x.round().max(0.0) as usize,
            y.round().max(0.0) as usize,
        ));
    }
    points
}

/// ストローク segment 編集 に必要な描画内容を組み立てる。
pub(crate) fn stroke_segment_edit(
    from: PanelLocalPoint,
    to: PanelLocalPoint,
    pressure: f32,
    context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
    let points = compute_stamp_positions(from, to, pressure, context);
    stroke_like_edit(&points, pressure, context)
}

/// ストローク like 編集 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
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

/// 実効的な spacing を返す。
fn effective_spacing(context: &PaintPluginContext<'_>, size: u32) -> f32 {
    (size as f32 * (context.pen.spacing_percent / 100.0)).clamp(1.0, size.max(1) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 大きな距離でもスタンプ数が MAX_STAMP_STEPS を超えないことを検証する。
    #[test]
    fn stroke_segment_steps_capped_at_max() {
        use app_core::{CanvasBitmap, ColorRgba8, PaintPluginContext, PenPreset, ToolKind};

        let layer = CanvasBitmap::transparent(1000, 1000);
        let composited = CanvasBitmap::transparent(1000, 1000);
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
        let from = PanelLocalPoint::new(0, 0);
        // 非常に長い距離（spacing=1px なら本来 10000 スタンプ）
        let to = PanelLocalPoint::new(999, 0);
        let size = stamp::effective_size(&context, 1.0).max(1);
        let spacing = effective_spacing(&context, size);
        let distance = (to.x as f32 - from.x as f32).hypot(0.0);
        let raw_steps = (distance / spacing).ceil().max(1.0) as usize;
        let capped = raw_steps.min(MAX_STAMP_STEPS);
        assert!(raw_steps > MAX_STAMP_STEPS, "raw steps should exceed cap");
        assert_eq!(capped, MAX_STAMP_STEPS);
        // stroke_segment_edit 自体も正常に完了する
        assert!(stroke_segment_edit(from, to, 1.0, &context).is_some());
    }

    /// compute_stamp_positions が MAX_STAMP_STEPS 以下の数の座標を返すことを確認する。
    #[test]
    fn compute_stamp_positions_respects_max_steps() {
        use app_core::{CanvasBitmap, ColorRgba8, PaintPluginContext, PenPreset, ToolKind};

        let layer = CanvasBitmap::transparent(1000, 1000);
        let composited = CanvasBitmap::transparent(1000, 1000);
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
            compute_stamp_positions(PanelLocalPoint::new(0, 0), PanelLocalPoint::new(999, 0), 1.0, &context);
        assert!(!positions.is_empty());
        assert!(positions.len() <= MAX_STAMP_STEPS + 1);
    }
}
