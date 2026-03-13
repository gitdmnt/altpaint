use desktop_support::FOOTER_HEIGHT;

use crate::{PixelRect, measure_text_width};

/// 現在の measured ステータス 幅 を返す。
pub fn measured_status_width(status_text: &str) -> usize {
    measure_text_width(status_text).saturating_add(16).max(1)
}

/// ステータス テキスト 矩形 に必要な処理を行う。
pub fn status_text_rect(
    window_width: usize,
    window_height: usize,
    canvas_host_rect: PixelRect,
) -> PixelRect {
    status_text_bounds(window_width, window_height, canvas_host_rect, "")
}

/// 現在の ステータス テキスト 範囲 を返す。
pub fn status_text_bounds(
    window_width: usize,
    window_height: usize,
    canvas_host_rect: PixelRect,
    status_text: &str,
) -> PixelRect {
    let text_width = measured_status_width(status_text);
    PixelRect {
        x: canvas_host_rect.x,
        y: window_height.saturating_sub(FOOTER_HEIGHT),
        width: text_width.min(window_width.saturating_sub(canvas_host_rect.x)),
        height: FOOTER_HEIGHT,
    }
}
