use app_core::CanvasViewTransform;

use crate::{CanvasCompositeSource, CanvasPlan, PixelRect};

/// desktop host が `render` に渡す 1 フレーム分の計画を表す。
#[derive(Clone, Copy)]
pub struct FramePlan<'a> {
    pub window_width: usize,
    pub window_height: usize,
    pub canvas_source: CanvasCompositeSource<'a>,
    pub canvas: CanvasPlan,
    pub status_text: &'a str,
}

impl<'a> FramePlan<'a> {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new(
        window_width: usize,
        window_height: usize,
        canvas_host_rect: PixelRect,
        canvas_source: CanvasCompositeSource<'a>,
        transform: CanvasViewTransform,
        status_text: &'a str,
    ) -> Self {
        Self {
            window_width,
            window_height,
            canvas_source,
            canvas: CanvasPlan {
                host_rect: canvas_host_rect,
                source_width: canvas_source.width,
                source_height: canvas_source.height,
                transform,
            },
            status_text,
        }
    }
}
