//! `frame` 用の固定レイアウト計算をまとめる。

use super::Rect;

/// 矩形 が収まるように矩形を計算する。
pub(crate) fn fit_rect(source_width: usize, source_height: usize, target: Rect) -> Rect {
    if source_width == 0 || source_height == 0 || target.width == 0 || target.height == 0 {
        return Rect {
            x: target.x,
            y: target.y,
            width: 0,
            height: 0,
        };
    }

    let scale_x = target.width as f32 / source_width as f32;
    let scale_y = target.height as f32 / source_height as f32;
    let scale = scale_x.min(scale_y);
    let fitted_width = ((source_width as f32 * scale).floor() as usize).max(1);
    let fitted_height = ((source_height as f32 * scale).floor() as usize).max(1);

    Rect {
        x: target.x + (target.width.saturating_sub(fitted_width)) / 2,
        y: target.y + (target.height.saturating_sub(fitted_height)) / 2,
        width: fitted_width,
        height: fitted_height,
    }
}

