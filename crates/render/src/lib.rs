//! `render` は将来のキャンバス描画基盤になるクレート。
//!
//! フェーズ2では、`Document` 内の最初のコマにあるラスタビットマップを
//! フレームバッファとして取り出す最小描画経路を定義する。

mod panel;
mod text;

use app_core::{CanvasViewTransform, DirtyRect, Document};

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
    pub rotation_turns: u8,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// `CanvasViewTransform` から導かれる表示用の幾何計画。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasScene {
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    rotated_width: usize,
    rotated_height: usize,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    rotation_turns: u8,
    flip_x: bool,
    flip_y: bool,
    drawn_rect: Option<PixelRect>,
    texture_quad: Option<TextureQuad>,
}

impl CanvasScene {
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
    pub fn map_canvas_dirty_rect(&self, dirty: DirtyRect) -> PixelRect {
        if self.rotation_turns != 0 || self.flip_x || self.flip_y {
            return self
                .map_source_rect_to_display(dirty)
                .and_then(|rect| rect.intersect(self.viewport))
                .unwrap_or(PixelRect {
                    x: self.viewport.x,
                    y: self.viewport.y,
                    width: 0,
                    height: 0,
                });
        }
        let clamped = dirty.clamp_to_bitmap(self.source_width, self.source_height);
        let start_x = (self.offset_x + clamped.x as f32 * self.scale).floor();
        let start_y = (self.offset_y + clamped.y as f32 * self.scale).floor();
        let end_x = (self.offset_x + (clamped.x + clamped.width) as f32 * self.scale).ceil();
        let end_y = (self.offset_y + (clamped.y + clamped.height) as f32 * self.scale).ceil();

        let clipped_left = start_x.max(self.viewport.x as f32);
        let clipped_top = start_y.max(self.viewport.y as f32);
        let clipped_right = end_x.min((self.viewport.x + self.viewport.width) as f32);
        let clipped_bottom = end_y.min((self.viewport.y + self.viewport.height) as f32);

        if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
            return PixelRect {
                x: self.viewport.x,
                y: self.viewport.y,
                width: 0,
                height: 0,
            };
        }

        PixelRect {
            x: clipped_left as usize,
            y: clipped_top as usize,
            width: (clipped_right - clipped_left) as usize,
            height: (clipped_bottom - clipped_top) as usize,
        }
    }

    /// キャンバス座標のブラシプレビュー領域を返す。
    pub fn brush_preview_rect(&self, canvas_position: (usize, usize)) -> Option<PixelRect> {
        self.brush_preview_rect_for_diameter(canvas_position, 1.0)
    }

    /// ソース座標上のブラシ径を考慮したプレビュー領域を返す。
    pub fn brush_preview_rect_for_diameter(
        &self,
        canvas_position: (usize, usize),
        brush_diameter: f32,
    ) -> Option<PixelRect> {
        let (center_x, center_y) = self.map_source_point_to_display(canvas_position)?;
        let radius = ((brush_diameter.max(1.0) * self.scale) * 0.5).max(4.0);

        self.viewport.intersect(PixelRect {
            x: (center_x - radius - 2.0)
                .floor()
                .max(self.viewport.x as f32) as usize,
            y: (center_y - radius - 2.0)
                .floor()
                .max(self.viewport.y as f32) as usize,
            width: ((center_x + radius + 2.0)
                .ceil()
                .min((self.viewport.x + self.viewport.width) as f32)
                - (center_x - radius - 2.0)
                    .floor()
                    .max(self.viewport.x as f32))
            .max(1.0) as usize,
            height: ((center_y + radius + 2.0)
                .ceil()
                .min((self.viewport.y + self.viewport.height) as f32)
                - (center_y - radius - 2.0)
                    .floor()
                    .max(self.viewport.y as f32))
            .max(1.0) as usize,
        })
    }

    /// キャンバス座標を表示座標へ変換する。
    pub fn map_canvas_point_to_display(
        &self,
        canvas_position: (usize, usize),
    ) -> Option<(f32, f32)> {
        self.map_source_point_to_display(canvas_position)
    }

    /// ビュー座標をキャンバスビットマップ座標へ変換する。
    pub fn map_view_to_canvas(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let drawn_width = self.rotated_width as f32 * self.scale;
        let drawn_height = self.rotated_height as f32 * self.scale;
        let local_x = x as f32 - self.offset_x;
        let local_y = y as f32 - self.offset_y;
        if local_x < 0.0 || local_y < 0.0 || local_x >= drawn_width || local_y >= drawn_height {
            return None;
        }

        let rotated_u = (local_x / drawn_width).clamp(0.0, 1.0 - f32::EPSILON);
        let rotated_v = (local_y / drawn_height).clamp(0.0, 1.0 - f32::EPSILON);
        let (source_u, source_v) = rotated_to_source_uv(
            rotated_u,
            rotated_v,
            self.rotation_turns,
            self.flip_x,
            self.flip_y,
        );
        let canvas_x = (source_u * self.source_width as f32).floor() as usize;
        let canvas_y = (source_v * self.source_height as f32).floor() as usize;

        Some((
            canvas_x.min(self.source_width.saturating_sub(1)),
            canvas_y.min(self.source_height.saturating_sub(1)),
        ))
    }

    fn map_source_rect_to_display(&self, dirty: DirtyRect) -> Option<PixelRect> {
        let dirty = dirty.clamp_to_bitmap(self.source_width, self.source_height);
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
            let (rotated_u, rotated_v) = source_to_rotated_uv(
                source_u,
                source_v,
                self.rotation_turns,
                self.flip_x,
                self.flip_y,
            );
            let display_x = self.offset_x + rotated_u * self.rotated_width as f32 * self.scale;
            let display_y = self.offset_y + rotated_v * self.rotated_height as f32 * self.scale;
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

    fn map_source_point_to_display(&self, canvas_position: (usize, usize)) -> Option<(f32, f32)> {
        if canvas_position.0 >= self.source_width || canvas_position.1 >= self.source_height {
            return None;
        }
        let source_u = (canvas_position.0 as f32 + 0.5) / self.source_width as f32;
        let source_v = (canvas_position.1 as f32 + 0.5) / self.source_height as f32;
        let (rotated_u, rotated_v) = source_to_rotated_uv(
            source_u,
            source_v,
            self.rotation_turns,
            self.flip_x,
            self.flip_y,
        );
        Some((
            self.offset_x + rotated_u * self.rotated_width as f32 * self.scale,
            self.offset_y + rotated_v * self.rotated_height as f32 * self.scale,
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

    let rotation_turns = normalized_rotation_turns(transform.rotation_degrees);
    let (rotated_width, rotated_height) =
        rotated_dimensions(source_width, source_height, rotation_turns);

    let scale_x = viewport.width as f32 / rotated_width as f32;
    let scale_y = viewport.height as f32 / rotated_height as f32;
    let scale = (scale_x.min(scale_y) * transform.zoom.max(0.25)).max(f32::EPSILON);
    let drawn_width = rotated_width as f32 * scale;
    let drawn_height = rotated_height as f32 * scale;
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
        let left = ((drawn_rect.x as f32 - offset_x) / scale).clamp(0.0, source_width as f32);
        let top = ((drawn_rect.y as f32 - offset_y) / scale).clamp(0.0, source_height as f32);
        let right = (((drawn_rect.x + drawn_rect.width) as f32 - offset_x) / scale)
            .clamp(0.0, source_width as f32);
        let bottom = (((drawn_rect.y + drawn_rect.height) as f32 - offset_y) / scale)
            .clamp(0.0, source_height as f32);

        TextureQuad {
            destination: drawn_rect,
            uv_min: [left / rotated_width as f32, top / rotated_height as f32],
            uv_max: [right / rotated_width as f32, bottom / rotated_height as f32],
            rotation_turns,
            flip_x: transform.flip_x,
            flip_y: transform.flip_y,
        }
    });

    Some(CanvasScene {
        viewport,
        source_width,
        source_height,
        rotated_width,
        rotated_height,
        scale,
        offset_x,
        offset_y,
        rotation_turns,
        flip_x: transform.flip_x,
        flip_y: transform.flip_y,
        drawn_rect,
        texture_quad,
    })
}

fn normalized_rotation_turns(rotation_degrees: f32) -> u8 {
    ((rotation_degrees / 90.0).round() as i32).rem_euclid(4) as u8
}

fn rotated_dimensions(width: usize, height: usize, rotation_turns: u8) -> (usize, usize) {
    if rotation_turns.is_multiple_of(2) {
        (width, height)
    } else {
        (height, width)
    }
}

fn source_to_rotated_uv(
    source_u: f32,
    source_v: f32,
    rotation_turns: u8,
    flip_x: bool,
    flip_y: bool,
) -> (f32, f32) {
    let (mut rotated_u, mut rotated_v) = match rotation_turns % 4 {
        0 => (source_u, source_v),
        1 => (1.0 - source_v, source_u),
        2 => (1.0 - source_u, 1.0 - source_v),
        _ => (source_v, 1.0 - source_u),
    };
    if flip_x {
        rotated_u = 1.0 - rotated_u;
    }
    if flip_y {
        rotated_v = 1.0 - rotated_v;
    }
    (rotated_u, rotated_v)
}

fn rotated_to_source_uv(
    rotated_u: f32,
    rotated_v: f32,
    rotation_turns: u8,
    flip_x: bool,
    flip_y: bool,
) -> (f32, f32) {
    let mut rotated_u = rotated_u;
    let mut rotated_v = rotated_v;
    if flip_x {
        rotated_u = 1.0 - rotated_u;
    }
    if flip_y {
        rotated_v = 1.0 - rotated_v;
    }
    match rotation_turns % 4 {
        0 => (rotated_u, rotated_v),
        1 => (rotated_v, 1.0 - rotated_u),
        2 => (1.0 - rotated_u, 1.0 - rotated_v),
        _ => (1.0 - rotated_v, rotated_u),
    }
}

/// ビットマップ dirty rect を表示先へ写像する。
pub fn map_canvas_dirty_to_display_with_transform(
    dirty: DirtyRect,
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
    canvas_position: (usize, usize),
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
    canvas_position: (usize, usize),
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
    canvas_position: (usize, usize),
) -> Option<(f32, f32)> {
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
    x: i32,
    y: i32,
    transform: CanvasViewTransform,
) -> Option<(usize, usize)> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.map_view_to_canvas(x, y))
}

/// パン・ズーム変化で露出した背景領域を返す。
pub fn exposed_canvas_background_rect(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    previous_transform: CanvasViewTransform,
    current_transform: CanvasViewTransform,
) -> Option<PixelRect> {
    let previous = canvas_drawn_rect(viewport, source_width, source_height, previous_transform)?;
    let current = canvas_drawn_rect(viewport, source_width, source_height, current_transform);

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

    /// ドキュメントから最初のコマのビットマップをフレームへ変換する。
    pub fn render_frame(&self, document: &Document) -> RenderFrame {
        let panel = &document.work.pages[0].panels[0];
        RenderFrame {
            width: panel.bitmap.width,
            height: panel.bitmap.height,
            pixels: panel.bitmap.pixels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 描画フレームが最小キャンバスサイズを正しく反映することを確認する。
    #[test]
    fn render_frame_uses_first_panel_bitmap() {
        let mut document = Document::default();
        document.draw_stroke(1, 2, 4, 2);
        let expected_bitmap = &document.work.pages[0].panels[0].bitmap;

        let context = RenderContext::new();
        let frame = context.render_frame(&document);

        // ドキュメント先頭パネルのビットマップ寸法がフレームへ反映されること。
        assert_eq!(frame.width, expected_bitmap.width);
        assert_eq!(frame.height, expected_bitmap.height);

        // ドキュメントのストローク描画結果がフレームへ反映されること。
        let index = (2 * frame.width + 1) * 4;
        assert_eq!(&frame.pixels[index..index + 4], &[0, 0, 0, 255]);
        let end_index = (2 * frame.width + 4) * 4;
        assert_eq!(&frame.pixels[end_index..end_index + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn transformed_canvas_dirty_rect_tracks_zoom_and_pan() {
        let mapped = map_canvas_dirty_to_display_with_transform(
            DirtyRect {
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
        assert_eq!(mapped.height, 72);
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
            352,
            320,
            CanvasViewTransform {
                zoom: 2.0,
                rotation_degrees: 0.0,
                pan_x: 32.0,
                pan_y: 0.0,
                flip_x: false,
                flip_y: false,
            },
        );

        assert_eq!(mapped, Some((32, 32)));
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

        assert_eq!(quad.rotation_turns, 1);
        assert!(quad.flip_x);
        assert!(!quad.flip_y);
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
            320,
            160,
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
