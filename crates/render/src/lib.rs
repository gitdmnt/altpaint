//! `render` は将来のキャンバス描画基盤になるクレート。
//!
//! フェーズ2では、`Document` 内の最初のコマにあるラスタビットマップを
//! フレームバッファとして取り出す最小描画経路を定義する。

mod panel;
mod text;

use app_core::{
    CanvasDirtyRect, CanvasDisplayPoint, CanvasPoint, CanvasViewTransform,
    CanvasViewportPoint, ClampToCanvasBounds, Document,
};

pub use panel::{
    FloatingPanel, MeasuredPanelSize, PanelFocusTarget, PanelHitKind, PanelHitRegion,
    PanelRenderState, PanelTextInputState, RasterizedPanelLayer, measure_panel_size,
    rasterize_panel_layer,
};
pub use text::{
    draw_text_rgba, line_height as text_line_height, measure_text_width, text_backend_name,
    wrap_text_lines,
};

/// 画面上のピクセル矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PixelRect {
    /// 指定座標が矩形内に入っているかを判定する。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32
            && y >= self.y as i32
            && x < (self.x + self.width) as i32
            && y < (self.y + self.height) as i32
    }

    /// 2 つの矩形を包む最小の矩形を返す。
    pub fn union(&self, other: PixelRect) -> PixelRect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        PixelRect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    /// 2 つの矩形の共通部分を返す。
    pub fn intersect(&self, other: PixelRect) -> Option<PixelRect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if left >= right || top >= bottom {
            return None;
        }

        Some(PixelRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// GPU 上で提示するテクスチャ付き矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureQuad {
    pub destination: PixelRect,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub rotation_degrees: f32,
    pub bbox_size: [f32; 2],
    pub flip_x: bool,
    pub flip_y: bool,
}

/// `CanvasViewTransform` から導かれる表示用の幾何計画。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasScene {
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    bbox_width: f32,
    bbox_height: f32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    rotation_degrees: f32,
    flip_x: bool,
    flip_y: bool,
    drawn_rect: Option<PixelRect>,
    texture_quad: Option<TextureQuad>,
}

impl CanvasScene {
    fn uv_transform(&self) -> UvTransform {
        let radians = self.rotation_degrees.to_radians();
        UvTransform {
            source_width: self.source_width as f32,
            source_height: self.source_height as f32,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
            bbox_width: self.bbox_width,
            bbox_height: self.bbox_height,
            cos_theta: radians.cos(),
            sin_theta: radians.sin(),
        }
    }

    /// 実際に表示されるキャンバス矩形を返す。
    pub fn drawn_rect(&self) -> Option<PixelRect> {
        self.drawn_rect
    }

    /// GPU 提示用のクアッドを返す。
    pub fn texture_quad(&self) -> Option<TextureQuad> {
        self.texture_quad
    }

    /// 現在の表示スケールを返す。
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// 現在の描画左上オフセットを返す。
    pub fn offset(&self) -> (f32, f32) {
        (self.offset_x, self.offset_y)
    }

    /// ビットマップ dirty rect を表示先へ写像する。
    pub fn map_canvas_dirty_rect(&self, dirty: CanvasDirtyRect) -> PixelRect {
        self.map_source_rect_to_display(dirty)
            .and_then(|rect| rect.intersect(self.viewport))
            .unwrap_or(PixelRect {
                x: self.viewport.x,
                y: self.viewport.y,
                width: 0,
                height: 0,
            })
    }

    /// キャンバス座標のブラシプレビュー領域を返す。
    pub fn brush_preview_rect(&self, canvas_position: CanvasPoint) -> Option<PixelRect> {
        self.brush_preview_rect_for_diameter(canvas_position, 1.0)
    }

    /// ソース座標上のブラシ径を考慮したプレビュー領域を返す。
    pub fn brush_preview_rect_for_diameter(
        &self,
        canvas_position: CanvasPoint,
        brush_diameter: f32,
    ) -> Option<PixelRect> {
        let center = self.map_source_point_to_display(canvas_position)?;
        let radius = ((brush_diameter.max(1.0) * self.scale) * 0.5).max(4.0);

        self.viewport.intersect(PixelRect {
            x: (center.x - radius - 2.0)
                .floor()
                .max(self.viewport.x as f32) as usize,
            y: (center.y - radius - 2.0)
                .floor()
                .max(self.viewport.y as f32) as usize,
            width: ((center.x + radius + 2.0)
                .ceil()
                .min((self.viewport.x + self.viewport.width) as f32)
                - (center.x - radius - 2.0)
                    .floor()
                    .max(self.viewport.x as f32))
            .max(1.0) as usize,
            height: ((center.y + radius + 2.0)
                .ceil()
                .min((self.viewport.y + self.viewport.height) as f32)
                - (center.y - radius - 2.0)
                    .floor()
                    .max(self.viewport.y as f32))
            .max(1.0) as usize,
        })
    }

    /// キャンバス座標を表示座標へ変換する。
    pub fn map_canvas_point_to_display(
        &self,
        canvas_position: CanvasPoint,
    ) -> Option<CanvasDisplayPoint> {
        self.map_source_point_to_display(canvas_position)
    }

    /// ビュー座標をキャンバスビットマップ座標へ変換する。
    pub fn map_view_to_canvas(&self, point: CanvasViewportPoint) -> Option<CanvasPoint> {
        let drawn_width = self.bbox_width * self.scale;
        let drawn_height = self.bbox_height * self.scale;
        let local_x = point.x as f32 - self.offset_x;
        let local_y = point.y as f32 - self.offset_y;
        if local_x < 0.0 || local_y < 0.0 || local_x >= drawn_width || local_y >= drawn_height {
            return None;
        }

        let rotated_u = (local_x / drawn_width).clamp(0.0, 1.0 - f32::EPSILON);
        let rotated_v = (local_y / drawn_height).clamp(0.0, 1.0 - f32::EPSILON);
        let (source_u, source_v) = rotated_to_source_uv(rotated_u, rotated_v, self.uv_transform());
        if !(0.0..1.0).contains(&source_u) || !(0.0..1.0).contains(&source_v) {
            return None;
        }
        let canvas_x = (source_u * self.source_width as f32).floor() as usize;
        let canvas_y = (source_v * self.source_height as f32).floor() as usize;

        Some(CanvasPoint::new(
            canvas_x.min(self.source_width.saturating_sub(1)),
            canvas_y.min(self.source_height.saturating_sub(1)),
        ))
    }

    fn map_source_rect_to_display(&self, dirty: CanvasDirtyRect) -> Option<PixelRect> {
        let dirty = dirty.clamp_to_canvas_bounds(self.source_width, self.source_height);
        let corners = [
            (
                dirty.x as f32 / self.source_width as f32,
                dirty.y as f32 / self.source_height as f32,
            ),
            (
                (dirty.x + dirty.width) as f32 / self.source_width as f32,
                dirty.y as f32 / self.source_height as f32,
            ),
            (
                dirty.x as f32 / self.source_width as f32,
                (dirty.y + dirty.height) as f32 / self.source_height as f32,
            ),
            (
                (dirty.x + dirty.width) as f32 / self.source_width as f32,
                (dirty.y + dirty.height) as f32 / self.source_height as f32,
            ),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for (source_u, source_v) in corners {
            let (rotated_u, rotated_v) =
                source_to_rotated_uv(source_u, source_v, self.uv_transform());
            let display_x = self.offset_x + rotated_u * self.bbox_width * self.scale;
            let display_y = self.offset_y + rotated_v * self.bbox_height * self.scale;
            min_x = min_x.min(display_x);
            min_y = min_y.min(display_y);
            max_x = max_x.max(display_x);
            max_y = max_y.max(display_y);
        }

        (min_x < max_x && min_y < max_y).then_some(PixelRect {
            x: min_x.floor().max(self.viewport.x as f32) as usize,
            y: min_y.floor().max(self.viewport.y as f32) as usize,
            width: (max_x.ceil() - min_x.floor()).max(1.0) as usize,
            height: (max_y.ceil() - min_y.floor()).max(1.0) as usize,
        })
    }

    fn map_source_point_to_display(
        &self,
        canvas_position: CanvasPoint,
    ) -> Option<CanvasDisplayPoint> {
        if canvas_position.x >= self.source_width || canvas_position.y >= self.source_height {
            return None;
        }
        let source_u = (canvas_position.x as f32 + 0.5) / self.source_width as f32;
        let source_v = (canvas_position.y as f32 + 0.5) / self.source_height as f32;
        let (rotated_u, rotated_v) = source_to_rotated_uv(source_u, source_v, self.uv_transform());
        Some(CanvasDisplayPoint::new(
            self.offset_x + rotated_u * self.bbox_width * self.scale,
            self.offset_y + rotated_v * self.bbox_height * self.scale,
        ))
    }
}

/// `CanvasViewTransform` から表示幾何を構築する。
pub fn prepare_canvas_scene(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<CanvasScene> {
    if viewport.width == 0 || viewport.height == 0 || source_width == 0 || source_height == 0 {
        return None;
    }

    let rotation_degrees = normalized_rotation_degrees(transform.rotation_degrees);
    let (bbox_width, bbox_height) =
        rotated_bounding_box(source_width as f32, source_height as f32, rotation_degrees);

    let fit_scale_x = viewport.width as f32 / (source_width as f32).max(f32::EPSILON);
    let fit_scale_y = viewport.height as f32 / (source_height as f32).max(f32::EPSILON);
    let scale = (fit_scale_x.min(fit_scale_y) * transform.zoom.max(0.25)).max(f32::EPSILON);
    let drawn_width = bbox_width * scale;
    let drawn_height = bbox_height * scale;
    let offset_x =
        viewport.x as f32 + (viewport.width as f32 - drawn_width) * 0.5 + transform.pan_x;
    let offset_y =
        viewport.y as f32 + (viewport.height as f32 - drawn_height) * 0.5 + transform.pan_y;

    let left = offset_x.floor();
    let top = offset_y.floor();
    let right = (offset_x + drawn_width).ceil();
    let bottom = (offset_y + drawn_height).ceil();

    let clipped_left = left.max(viewport.x as f32);
    let clipped_top = top.max(viewport.y as f32);
    let clipped_right = right.min((viewport.x + viewport.width) as f32);
    let clipped_bottom = bottom.min((viewport.y + viewport.height) as f32);

    let drawn_rect =
        (clipped_left < clipped_right && clipped_top < clipped_bottom).then_some(PixelRect {
            x: clipped_left as usize,
            y: clipped_top as usize,
            width: (clipped_right - clipped_left) as usize,
            height: (clipped_bottom - clipped_top) as usize,
        });

    let texture_quad = drawn_rect.map(|drawn_rect| {
        let left = ((drawn_rect.x as f32 - offset_x) / scale).clamp(0.0, bbox_width);
        let top = ((drawn_rect.y as f32 - offset_y) / scale).clamp(0.0, bbox_height);
        let right =
            (((drawn_rect.x + drawn_rect.width) as f32 - offset_x) / scale).clamp(0.0, bbox_width);
        let bottom = (((drawn_rect.y + drawn_rect.height) as f32 - offset_y) / scale)
            .clamp(0.0, bbox_height);

        TextureQuad {
            destination: drawn_rect,
            uv_min: [left / bbox_width, top / bbox_height],
            uv_max: [right / bbox_width, bottom / bbox_height],
            rotation_degrees,
            bbox_size: [bbox_width, bbox_height],
            flip_x: transform.flip_x,
            flip_y: transform.flip_y,
        }
    });

    Some(CanvasScene {
        viewport,
        source_width,
        source_height,
        bbox_width,
        bbox_height,
        scale,
        offset_x,
        offset_y,
        rotation_degrees,
        flip_x: transform.flip_x,
        flip_y: transform.flip_y,
        drawn_rect,
        texture_quad,
    })
}

fn normalized_rotation_degrees(rotation_degrees: f32) -> f32 {
    rotation_degrees.rem_euclid(360.0)
}

fn rotated_bounding_box(width: f32, height: f32, rotation_degrees: f32) -> (f32, f32) {
    let radians = rotation_degrees.to_radians();
    let cos = radians.cos().abs();
    let sin = radians.sin().abs();
    (
        (width * cos + height * sin).max(f32::EPSILON),
        (width * sin + height * cos).max(f32::EPSILON),
    )
}

#[derive(Debug, Clone, Copy)]
struct UvTransform {
    source_width: f32,
    source_height: f32,
    flip_x: bool,
    flip_y: bool,
    bbox_width: f32,
    bbox_height: f32,
    cos_theta: f32,
    sin_theta: f32,
}

fn source_to_rotated_uv(source_u: f32, source_v: f32, uv_transform: UvTransform) -> (f32, f32) {
    let centered_x = source_u * uv_transform.source_width - uv_transform.source_width * 0.5;
    let centered_y = source_v * uv_transform.source_height - uv_transform.source_height * 0.5;
    let mut rotated_x = centered_x * uv_transform.cos_theta - centered_y * uv_transform.sin_theta;
    let mut rotated_y = centered_x * uv_transform.sin_theta + centered_y * uv_transform.cos_theta;
    if uv_transform.flip_x {
        rotated_x = -rotated_x;
    }
    if uv_transform.flip_y {
        rotated_y = -rotated_y;
    }
    (
        (rotated_x + uv_transform.bbox_width * 0.5) / uv_transform.bbox_width,
        (rotated_y + uv_transform.bbox_height * 0.5) / uv_transform.bbox_height,
    )
}

fn rotated_to_source_uv(rotated_u: f32, rotated_v: f32, uv_transform: UvTransform) -> (f32, f32) {
    let mut rotated_x = rotated_u * uv_transform.bbox_width - uv_transform.bbox_width * 0.5;
    let mut rotated_y = rotated_v * uv_transform.bbox_height - uv_transform.bbox_height * 0.5;
    if uv_transform.flip_x {
        rotated_x = -rotated_x;
    }
    if uv_transform.flip_y {
        rotated_y = -rotated_y;
    }
    let source_x = rotated_x * uv_transform.cos_theta + rotated_y * uv_transform.sin_theta;
    let source_y = -rotated_x * uv_transform.sin_theta + rotated_y * uv_transform.cos_theta;
    (
        (source_x + uv_transform.source_width * 0.5) / uv_transform.source_width,
        (source_y + uv_transform.source_height * 0.5) / uv_transform.source_height,
    )
}

/// ビットマップ dirty rect を表示先へ写像する。
pub fn map_canvas_dirty_to_display_with_transform(
    dirty: CanvasDirtyRect,
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> PixelRect {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .map(|scene| scene.map_canvas_dirty_rect(dirty))
        .unwrap_or(PixelRect {
            x: viewport.x,
            y: viewport.y,
            width: 0,
            height: 0,
        })
}

/// キャンバス上に描かれている領域を返す。
pub fn canvas_drawn_rect(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<PixelRect> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.drawn_rect())
}

/// ブラシプレビュー矩形を返す。
pub fn brush_preview_rect(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: CanvasPoint,
) -> Option<PixelRect> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.brush_preview_rect(canvas_position))
}

/// ブラシ径を考慮したブラシプレビュー矩形を返す。
pub fn brush_preview_rect_for_diameter(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: CanvasPoint,
    brush_diameter: f32,
) -> Option<PixelRect> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.brush_preview_rect_for_diameter(canvas_position, brush_diameter))
}

/// キャンバス座標を表示座標へ変換する。
pub fn map_canvas_point_to_display(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: CanvasPoint,
) -> Option<CanvasDisplayPoint> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.map_canvas_point_to_display(canvas_position))
}

/// キャンバス quad を返す。
pub fn canvas_texture_quad(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<TextureQuad> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.texture_quad())
}

/// ビュー座標をキャンバス座標へ変換する。
pub fn map_view_to_canvas_with_transform(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    point: CanvasViewportPoint,
    transform: CanvasViewTransform,
) -> Option<CanvasPoint> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.map_view_to_canvas(point))
}

/// パン・ズーム変化で露出した背景領域を返す。
pub fn exposed_canvas_background_rect(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    previous_transform: CanvasViewTransform,
    current_transform: CanvasViewTransform,
) -> Option<PixelRect> {
    let previous = prepare_canvas_scene(viewport, source_width, source_height, previous_transform);
    let current = prepare_canvas_scene(viewport, source_width, source_height, current_transform);

    exposed_canvas_background_rect_from_scenes(previous, current)
}

pub fn exposed_canvas_background_rect_from_scenes(
    previous: Option<CanvasScene>,
    current: Option<CanvasScene>,
) -> Option<PixelRect> {
    let previous = previous.and_then(|scene| scene.drawn_rect())?;
    let current = current.and_then(|scene| scene.drawn_rect());

    let Some(current) = current else {
        return Some(previous);
    };

    if previous == current {
        return None;
    }

    let overlap = previous.intersect(current);
    let mut exposed = Vec::with_capacity(4);
    match overlap {
        None => exposed.push(previous),
        Some(overlap) => {
            let candidates = [
                PixelRect {
                    x: previous.x,
                    y: previous.y,
                    width: previous.width,
                    height: overlap.y.saturating_sub(previous.y),
                },
                PixelRect {
                    x: previous.x,
                    y: overlap.y + overlap.height,
                    width: previous.width,
                    height: (previous.y + previous.height)
                        .saturating_sub(overlap.y + overlap.height),
                },
                PixelRect {
                    x: previous.x,
                    y: overlap.y,
                    width: overlap.x.saturating_sub(previous.x),
                    height: overlap.height,
                },
                PixelRect {
                    x: overlap.x + overlap.width,
                    y: overlap.y,
                    width: (previous.x + previous.width).saturating_sub(overlap.x + overlap.width),
                    height: overlap.height,
                },
            ];
            for rect in candidates {
                if rect.width > 0 && rect.height > 0 {
                    exposed.push(rect);
                }
            }
        }
    }

    exposed.into_iter().reduce(|acc, rect| acc.union(rect))
}

/// 画面へ転送するための最小フレームデータ。
/// フレームバッファとしての役割を果たす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrame {
    /// フレームの横幅ピクセル数。
    pub width: usize,
    /// フレームの高さピクセル数。
    pub height: usize,
    /// RGBA8 のピクセル列。
    pub pixels: Vec<u8>,
}

/// キャンバス描画のための最小コンテキスト。
///
/// 将来的にはキャッシュ、カメラ、描画ターゲットなどを保持する。
#[derive(Debug, Default)]
pub struct RenderContext;

impl RenderContext {
    /// 空のレンダリングコンテキストを作成する。
    pub fn new() -> Self {
        Self
    }

    /// 現段階では描画対象 `Document` をそのまま返す。
    ///
    /// 将来的にはここで可視範囲の解決やレンダリング前処理を行う。
    pub fn document<'a>(&self, document: &'a Document) -> &'a Document {
        document
    }

    /// ドキュメントからアクティブコマをページ座標系へ配置したフレームを作る。
    pub fn render_frame(&self, document: &Document) -> RenderFrame {
        let page = document.active_page().unwrap_or(&document.work.pages[0]);
        let panel = document.active_panel().unwrap_or(&page.panels[0]);
        let mut frame = RenderFrame {
            width: page.width.max(1),
            height: page.height.max(1),
            pixels: vec![255; page.width.max(1) * page.height.max(1) * 4],
        };

        let copy_width = panel
            .bitmap
            .width
            .min(panel.bounds.width)
            .min(frame.width.saturating_sub(panel.bounds.x));
        let copy_height = panel
            .bitmap
            .height
            .min(panel.bounds.height)
            .min(frame.height.saturating_sub(panel.bounds.y));
        for row in 0..copy_height {
            let src_row_start = row * panel.bitmap.width * 4;
            let dst_row_start = ((panel.bounds.y + row) * frame.width + panel.bounds.x) * 4;
            let row_bytes = copy_width * 4;
            frame.pixels[dst_row_start..dst_row_start + row_bytes]
                .copy_from_slice(&panel.bitmap.pixels[src_row_start..src_row_start + row_bytes]);
        }

        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 描画フレームが最小キャンバスサイズを正しく反映することを確認する。
    #[test]
    fn render_frame_places_active_panel_bitmap_inside_page() {
        let mut document = Document::new(320, 240);
        let _ = document.apply_command(&app_core::Command::CreatePanel {
            x: 40,
            y: 32,
            width: 120,
            height: 80,
        });
        if let Some(panel) = document.active_panel_mut() {
            let _ = panel.layers[0]
                .bitmap
                .draw_line_sized_rgba(1, 2, 4, 2, [0, 0, 0, 255], 1, true);
            panel.bitmap = panel.layers[0].bitmap.clone();
        }

        let context = RenderContext::new();
        let frame = context.render_frame(&document);

        assert_eq!(frame.width, 320);
        assert_eq!(frame.height, 240);

        let index = ((32 + 2) * frame.width + (40 + 1)) * 4;
        assert_eq!(&frame.pixels[index..index + 4], &[0, 0, 0, 255]);
        let end_index = ((32 + 2) * frame.width + (40 + 4)) * 4;
        assert_eq!(&frame.pixels[end_index..end_index + 4], &[0, 0, 0, 255]);
        assert_eq!(&frame.pixels[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn transformed_canvas_dirty_rect_tracks_zoom_and_pan() {
        let mapped = map_canvas_dirty_to_display_with_transform(
            CanvasDirtyRect {
                x: 16,
                y: 16,
                width: 8,
                height: 8,
            },
            PixelRect {
                x: 100,
                y: 50,
                width: 320,
                height: 320,
            },
            64,
            64,
            CanvasViewTransform {
                zoom: 2.0,
                rotation_degrees: 0.0,
                pan_x: 16.0,
                pan_y: -8.0,
                flip_x: false,
                flip_y: false,
            },
        );

        assert!(mapped.width >= 80);
        assert_eq!(mapped.height, 80);
        assert!(mapped.x >= 100);
        assert_eq!(mapped.y, 50);
    }

    #[test]
    fn canvas_texture_quad_clips_uv_when_panned_outside_display() {
        let quad = canvas_texture_quad(
            PixelRect {
                x: 100,
                y: 80,
                width: 320,
                height: 320,
            },
            64,
            64,
            CanvasViewTransform {
                zoom: 2.0,
                rotation_degrees: 0.0,
                pan_x: 48.0,
                pan_y: -16.0,
                flip_x: false,
                flip_y: false,
            },
        )
        .expect("quad exists");

        assert_eq!(quad.destination.width, 320);
        assert!(quad.uv_min[0] > 0.0);
        assert!(quad.uv_max[0] <= 1.0);
        assert!(quad.uv_min[1] >= 0.0);
    }

    #[test]
    fn map_view_to_canvas_tracks_shifted_scene() {
        let mapped = map_view_to_canvas_with_transform(
            PixelRect {
                x: 0,
                y: 0,
                width: 640,
                height: 640,
            },
            64,
            64,
            CanvasViewportPoint::new(352, 320),
            CanvasViewTransform {
                zoom: 2.0,
                rotation_degrees: 0.0,
                pan_x: 32.0,
                pan_y: 0.0,
                flip_x: false,
                flip_y: false,
            },
        );

        assert_eq!(mapped, Some(CanvasPoint::new(32, 32)));
    }

    #[test]
    fn canvas_texture_quad_carries_rotation_and_flip_flags() {
        let quad = canvas_texture_quad(
            PixelRect {
                x: 0,
                y: 0,
                width: 640,
                height: 640,
            },
            64,
            32,
            CanvasViewTransform {
                zoom: 1.0,
                rotation_degrees: 90.0,
                pan_x: 0.0,
                pan_y: 0.0,
                flip_x: true,
                flip_y: false,
            },
        )
        .expect("quad exists");

        assert_eq!(quad.rotation_degrees, 90.0);
        assert!(quad.bbox_size[0] > 0.0);
        assert!(quad.bbox_size[1] > 0.0);
        assert!(quad.flip_x);
        assert!(!quad.flip_y);
    }

    #[test]
    fn arbitrary_rotation_roundtrips_view_to_canvas() {
        let viewport = PixelRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        };
        let transform = CanvasViewTransform {
            zoom: 1.0,
            rotation_degrees: 37.5,
            pan_x: 0.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        };
        let display = map_canvas_point_to_display(viewport, 64, 32, transform, CanvasPoint::new(24, 12))
            .expect("display point exists");

        let mapped = map_view_to_canvas_with_transform(
            viewport,
            64,
            32,
            CanvasViewportPoint::new(display.x.round() as i32, display.y.round() as i32),
            transform,
        );

        assert_eq!(mapped, Some(CanvasPoint::new(24, 12)));
    }

    #[test]
    fn arbitrary_rotation_keeps_canvas_scale_stable() {
        let viewport = PixelRect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        };
        let base_transform = CanvasViewTransform {
            zoom: 1.0,
            rotation_degrees: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        };
        let rotated_transform = CanvasViewTransform {
            rotation_degrees: 37.5,
            ..base_transform
        };

        let base_scene =
            prepare_canvas_scene(viewport, 64, 32, base_transform).expect("base scene exists");
        let rotated_scene = prepare_canvas_scene(viewport, 64, 32, rotated_transform)
            .expect("rotated scene exists");

        assert!((base_scene.scale() - rotated_scene.scale()).abs() < 0.001);
    }

    #[test]
    fn map_view_to_canvas_tracks_rotated_scene() {
        let mapped = map_view_to_canvas_with_transform(
            PixelRect {
                x: 0,
                y: 0,
                width: 640,
                height: 640,
            },
            64,
            32,
            CanvasViewportPoint::new(320, 160),
            CanvasViewTransform {
                zoom: 1.0,
                rotation_degrees: 90.0,
                pan_x: 0.0,
                pan_y: 0.0,
                flip_x: false,
                flip_y: false,
            },
        );

        assert!(mapped.is_some());
    }
}
