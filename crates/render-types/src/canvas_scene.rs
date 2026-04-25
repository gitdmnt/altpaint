use app_core::{
    CanvasDirtyRect, CanvasDisplayPoint, CanvasPoint, CanvasViewTransform, CanvasViewportPoint,
    ClampToCanvasBounds,
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
    /// 対象 が範囲内に含まれるか判定する。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x as i32
            && y >= self.y as i32
            && x < (self.x + self.width) as i32
            && y < (self.y + self.height) as i32
    }

    /// union を計算して返す。
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

    /// intersect を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
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
    cos_theta: f32,
    sin_theta: f32,
    flip_x: bool,
    flip_y: bool,
    drawn_rect: Option<PixelRect>,
    texture_quad: Option<TextureQuad>,
}

impl CanvasScene {
    /// UV 変換 を計算して返す。
    fn uv_transform(&self) -> UvTransform {
        UvTransform {
            source_width: self.source_width as f32,
            source_height: self.source_height as f32,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
            bbox_width: self.bbox_width,
            bbox_height: self.bbox_height,
            cos_theta: self.cos_theta,
            sin_theta: self.sin_theta,
        }
    }

    /// drawn 矩形 を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn drawn_rect(&self) -> Option<PixelRect> {
        self.drawn_rect
    }

    /// texture quad を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn texture_quad(&self) -> Option<TextureQuad> {
        self.texture_quad
    }

    /// 拡大率 を計算して返す。
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// オフセット を計算して返す。
    pub fn offset(&self) -> (f32, f32) {
        (self.offset_x, self.offset_y)
    }

    /// キャンバス 差分 矩形 を別座標系へ変換する。
    ///
    /// 必要に応じて dirty 状態も更新します。
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

    /// ブラシ プレビュー 矩形 を計算して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn brush_preview_rect(&self, canvas_position: CanvasPoint) -> Option<PixelRect> {
        self.brush_preview_rect_for_diameter(canvas_position, 1.0)
    }

    /// ブラシ プレビュー 矩形 for diameter に必要な処理を行う。
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

    /// キャンバス 点 to 表示 を別座標系へ変換する。
    pub fn map_canvas_point_to_display(
        &self,
        canvas_position: CanvasPoint,
    ) -> Option<CanvasDisplayPoint> {
        self.map_source_point_to_display(canvas_position)
    }

    /// ビュー to キャンバス を別座標系へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// ソース 矩形 to 表示 を別座標系へ変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// ソース 点 to 表示 を別座標系へ変換する。
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

/// prepare キャンバス シーン に必要な処理を行う。
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
    let radians = rotation_degrees.to_radians();
    let cos_theta = radians.cos();
    let sin_theta = radians.sin();
    let (bbox_width, bbox_height) =
        rotated_bounding_box(source_width as f32, source_height as f32, cos_theta, sin_theta);

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
        cos_theta,
        sin_theta,
        flip_x: transform.flip_x,
        flip_y: transform.flip_y,
        drawn_rect,
        texture_quad,
    })
}

/// normalized 回転 degrees を計算して返す。
fn normalized_rotation_degrees(rotation_degrees: f32) -> f32 {
    rotation_degrees.rem_euclid(360.0)
}

/// rotated bounding box を計算して返す。
fn rotated_bounding_box(width: f32, height: f32, cos_theta: f32, sin_theta: f32) -> (f32, f32) {
    let cos = cos_theta.abs();
    let sin = sin_theta.abs();
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

/// ソース to rotated UV を計算して返す。
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

/// rotated to ソース UV を計算して返す。
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

/// キャンバス 差分 to 表示 with 変換 を別座標系へ変換する。
///
/// 必要に応じて dirty 状態も更新します。
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

/// キャンバス drawn 矩形 に必要な処理を行う。
pub fn canvas_drawn_rect(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<PixelRect> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.drawn_rect())
}

/// ブラシ プレビュー 矩形 に必要な処理を行う。
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

/// ブラシ プレビュー 矩形 for diameter に必要な処理を行う。
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

/// キャンバス 点 to 表示 を別座標系へ変換する。
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

/// キャンバス texture quad に必要な処理を行う。
pub fn canvas_texture_quad(
    viewport: PixelRect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<TextureQuad> {
    prepare_canvas_scene(viewport, source_width, source_height, transform)
        .and_then(|scene| scene.texture_quad())
}

/// ビュー to キャンバス with 変換 を別座標系へ変換する。
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

/// exposed キャンバス 背景 矩形 に必要な処理を行う。
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

/// 入力や種別に応じて処理を振り分ける。
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
