use app_core::CanvasViewTransform;

use crate::{
    CanvasCompositeSource, CanvasOverlayState, CanvasPlan, OverlayPlan, PanelPlan,
    PanelSurfaceSource, PixelRect,
};

/// desktop host が `render` に渡す 1 フレーム分の計画を表す。
#[derive(Clone, Copy)]
pub struct FramePlan<'a> {
    pub window_width: usize,
    pub window_height: usize,
    pub canvas_source: CanvasCompositeSource<'a>,
    pub canvas: CanvasPlan,
    pub panel_surface: PanelSurfaceSource<'a>,
    pub status_text: &'a str,
}

impl<'a> FramePlan<'a> {
    /// フレーム計画を構築する。
    pub fn new(
        window_width: usize,
        window_height: usize,
        canvas_host_rect: PixelRect,
        panel_surface: PanelSurfaceSource<'a>,
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
            panel_surface,
            status_text,
        }
    }

    /// ウィンドウ全体の矩形を返す。
    pub fn window_rect(&self) -> PixelRect {
        PixelRect {
            x: 0,
            y: 0,
            width: self.window_width,
            height: self.window_height,
        }
    }

    /// パネル面の配置計画を返す。
    pub fn panel_plan(&self) -> PanelPlan {
        self.panel_surface.plan()
    }

    /// overlay 計画を構築する。
    pub fn overlay_plan(&self, overlay: CanvasOverlayState) -> OverlayPlan<'a> {
        OverlayPlan {
            canvas: self.canvas,
            panel_surface: self.panel_surface,
            overlay,
        }
    }
}
