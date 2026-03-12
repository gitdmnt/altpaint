use app_core::{CanvasDirtyRect, CanvasPoint, CanvasViewTransform};

use crate::{
    CanvasScene, PixelRect, TextureQuad, brush_preview_rect_for_diameter,
    exposed_canvas_background_rect, map_canvas_dirty_to_display_with_transform,
    prepare_canvas_scene,
};

/// キャンバス合成元を `RenderFrame` に依存させずに渡すための軽量ビュー。
#[derive(Clone, Copy)]
pub struct CanvasCompositeSource<'a> {
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [u8],
}

/// `render` が扱うキャンバス表示計画を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasPlan {
    pub host_rect: PixelRect,
    pub source_width: usize,
    pub source_height: usize,
    pub transform: CanvasViewTransform,
}

impl CanvasPlan {
    /// ビュー変換込みのキャンバスシーンを返す。
    pub fn scene(&self) -> Option<CanvasScene> {
        prepare_canvas_scene(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    /// GPU 提示用のクアッドを返す。
    pub fn texture_quad(&self) -> Option<TextureQuad> {
        self.scene().and_then(|scene| scene.texture_quad())
    }

    /// dirty rect を表示座標へ写像する。
    pub fn map_dirty_rect(&self, dirty: CanvasDirtyRect) -> PixelRect {
        map_canvas_dirty_to_display_with_transform(
            dirty,
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
        )
    }

    /// ブラシプレビュー矩形を返す。
    pub fn brush_preview_rect(
        &self,
        canvas_position: CanvasPoint,
        brush_diameter: f32,
    ) -> Option<PixelRect> {
        brush_preview_rect_for_diameter(
            self.host_rect,
            self.source_width,
            self.source_height,
            self.transform,
            canvas_position,
            brush_diameter,
        )
    }

    /// 前回表示との差分で露出した背景領域を返す。
    pub fn exposed_background_rect(
        &self,
        previous_transform: CanvasViewTransform,
    ) -> Option<PixelRect> {
        exposed_canvas_background_rect(
            self.host_rect,
            self.source_width,
            self.source_height,
            previous_transform,
            self.transform,
        )
    }
}
