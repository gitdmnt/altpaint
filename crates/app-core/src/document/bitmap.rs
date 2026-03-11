//! `CanvasBitmap` の画素操作ロジックをまとめる。
//!
//! ドメイン型定義から描画アルゴリズムを分離し、`Document` 本体の責務を
//! 状態遷移に集中させる。

use crate::{CanvasDirtyRect, ClampToCanvasBounds};

use super::CanvasBitmap;

impl CanvasBitmap {
    /// 白背景で初期化された新しいビットマップを作る。
    pub fn new(width: usize, height: usize) -> Self {
        let mut pixels = vec![0; width * height * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
            chunk[3] = 255;
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// 完全透明で初期化されたビットマップを作る。
    pub fn transparent(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }

    /// ビットマップ内の1点を黒で塗る。
    pub fn draw_point(&mut self, x: usize, y: usize) -> CanvasDirtyRect {
        self.draw_point_rgba(x, y, [0, 0, 0, 255])
    }

    /// ビットマップ内の1点を任意色で塗る。
    pub fn draw_point_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> CanvasDirtyRect {
        self.write_pixel(x, y, rgba)
    }

    /// ビットマップ内の1点を白で塗る。
    pub fn erase_point(&mut self, x: usize, y: usize) -> CanvasDirtyRect {
        self.write_pixel(x, y, [255, 255, 255, 255])
    }

    /// 指定サイズの円形ブラシで1点描画する。
    pub fn draw_point_sized_rgba(
        &mut self,
        x: usize,
        y: usize,
        rgba: [u8; 4],
        size: u32,
        antialias: bool,
    ) -> CanvasDirtyRect {
        if size <= 1 {
            return self.draw_point_rgba(x, y, rgba);
        }
        self.paint_disk(x as isize, y as isize, size, rgba, antialias)
    }

    /// 指定サイズの円形ブラシで1点消去する。
    pub fn erase_point_sized(
        &mut self,
        x: usize,
        y: usize,
        size: u32,
        antialias: bool,
    ) -> CanvasDirtyRect {
        if size <= 1 {
            return self.erase_point(x, y);
        }
        self.paint_disk(
            x as isize,
            y as isize,
            size,
            [255, 255, 255, 255],
            antialias,
        )
    }

    /// 2点間を結ぶ最小ストロークを描く。
    pub fn draw_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> CanvasDirtyRect {
        self.draw_line_rgba(from_x, from_y, to_x, to_y, [0, 0, 0, 255])
    }

    /// 2点間を任意色で線描画する。
    pub fn draw_line_rgba(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        rgba: [u8; 4],
    ) -> CanvasDirtyRect {
        let mut x0 = from_x as isize;
        let mut y0 = from_y as isize;
        let x1 = to_x as isize;
        let y1 = to_y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 {
                let _ = self.draw_point_rgba(x0 as usize, y0 as usize, rgba);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }

            let doubled_error = 2 * error;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }

        CanvasDirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }

    /// 2点間を白で線消去する。
    pub fn erase_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> CanvasDirtyRect {
        let mut x0 = from_x as isize;
        let mut y0 = from_y as isize;
        let x1 = to_x as isize;
        let y1 = to_y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 {
                let _ = self.erase_point(x0 as usize, y0 as usize);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }

            let doubled_error = 2 * error;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }

        CanvasDirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }

    /// 指定サイズの円形ブラシで線描画する。
    #[allow(clippy::too_many_arguments)]
    pub fn draw_line_sized_rgba(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        rgba: [u8; 4],
        size: u32,
        antialias: bool,
    ) -> CanvasDirtyRect {
        if size <= 1 {
            return self.draw_line_rgba(from_x, from_y, to_x, to_y, rgba);
        }
        self.paint_line_disks(from_x, from_y, to_x, to_y, size, rgba, antialias)
    }

    /// 指定サイズの円形ブラシで線消去する。
    pub fn erase_line_sized(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
        antialias: bool,
    ) -> CanvasDirtyRect {
        if size <= 1 {
            return self.erase_line(from_x, from_y, to_x, to_y);
        }
        self.paint_line_disks(
            from_x,
            from_y,
            to_x,
            to_y,
            size,
            [255, 255, 255, 255],
            antialias,
        )
    }

    /// 指定座標のRGBA値を返す。
    pub fn pixel_rgba(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (y * self.width + x) * 4;
        Some([
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ])
    }

    /// 単一ピクセルを上書きし、その dirty rect を返す。
    pub fn set_pixel_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> CanvasDirtyRect {
        self.write_pixel(x, y, rgba)
    }

    /// 単一ピクセルを書き換え、その dirty rect を返す。
    fn write_pixel(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> CanvasDirtyRect {
        if x >= self.width || y >= self.height {
            return CanvasDirtyRect::from_inclusive_points(
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
            );
        }

        let index = (y * self.width + x) * 4;
        self.pixels[index] = rgba[0];
        self.pixels[index + 1] = rgba[1];
        self.pixels[index + 2] = rgba[2];
        self.pixels[index + 3] = rgba[3];

        CanvasDirtyRect::from_inclusive_points(x, y, x, y)
    }

    /// 円形ブラシを線分上に連続配置して太線を描く。
    #[allow(clippy::too_many_arguments)]
    fn paint_line_disks(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
        rgba: [u8; 4],
        antialias: bool,
    ) -> CanvasDirtyRect {
        self.paint_capsule(
            from_x as f32 + 0.5,
            from_y as f32 + 0.5,
            to_x as f32 + 0.5,
            to_y as f32 + 0.5,
            size,
            rgba,
            antialias,
        )
    }

    /// 円形ブラシ 1 個ぶんを描画する。
    fn paint_disk(
        &mut self,
        center_x: isize,
        center_y: isize,
        size: u32,
        rgba: [u8; 4],
        antialias: bool,
    ) -> CanvasDirtyRect {
        self.paint_capsule(
            center_x as f32 + 0.5,
            center_y as f32 + 0.5,
            center_x as f32 + 0.5,
            center_y as f32 + 0.5,
            size,
            rgba,
            antialias,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_capsule(
        &mut self,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        size: u32,
        rgba: [u8; 4],
        antialias: bool,
    ) -> CanvasDirtyRect {
        let radius = (size.max(1) as f32) * 0.5;
        let antialias_outer = radius + if antialias { 0.75 } else { 0.0 };
        let left = (start_x.min(end_x) - antialias_outer).floor().max(0.0) as usize;
        let top = (start_y.min(end_y) - antialias_outer).floor().max(0.0) as usize;
        let right = (start_x.max(end_x) + antialias_outer).ceil().max(0.0) as usize;
        let bottom = (start_y.max(end_y) + antialias_outer).ceil().max(0.0) as usize;

        if self.width == 0 || self.height == 0 {
            return CanvasDirtyRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
        }

        let segment_dx = end_x - start_x;
        let segment_dy = end_y - start_y;
        let segment_length_sq = segment_dx * segment_dx + segment_dy * segment_dy;
        let radius_sq = radius * radius;
        let full_coverage_sq = (radius - 0.5).max(0.0).powi(2);
        let top = top.min(self.height.saturating_sub(1));
        let bottom = bottom.min(self.height.saturating_sub(1));
        let mut changed_bounds: Option<(usize, usize, usize, usize)> = None;

        for y in top..=bottom {
            let row_center_y = y as f32 + 0.5;
            let Some((outer_left, outer_right)) = capsule_row_pixel_span(
                start_x,
                start_y,
                end_x,
                end_y,
                row_center_y,
                antialias_outer,
                self.width,
            ) else {
                continue;
            };

            let full_span = if antialias {
                capsule_row_pixel_span(
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                    row_center_y,
                    (radius - 0.5).max(0.0),
                    self.width,
                )
            } else {
                Some((outer_left, outer_right))
            };

            if let Some((full_left, full_right)) = full_span {
                if rgba[3] == u8::MAX {
                    self.fill_opaque_span(y, full_left, full_right, rgba);
                } else {
                    self.blend_span_with_constant_coverage(y, full_left, full_right, rgba, 1.0);
                }
                changed_bounds = Some(match changed_bounds {
                    Some((min_x, min_y, max_x, max_y)) => (
                        min_x.min(full_left),
                        min_y.min(y),
                        max_x.max(full_right),
                        max_y.max(y),
                    ),
                    None => (full_left, y, full_right, y),
                });
            }

            let left_edge_end = full_span
                .map(|(full_left, _)| full_left.saturating_sub(1))
                .unwrap_or(outer_right);
            if outer_left <= left_edge_end {
                self.blend_capsule_edge_span(
                    y,
                    outer_left,
                    left_edge_end,
                    start_x,
                    start_y,
                    segment_dx,
                    segment_dy,
                    segment_length_sq,
                    radius_sq,
                    full_coverage_sq,
                    radius,
                    antialias,
                    rgba,
                );
                changed_bounds = Some(match changed_bounds {
                    Some((min_x, min_y, max_x, max_y)) => (
                        min_x.min(outer_left),
                        min_y.min(y),
                        max_x.max(left_edge_end),
                        max_y.max(y),
                    ),
                    None => (outer_left, y, left_edge_end, y),
                });
            }

            let right_edge_start = full_span
                .map(|(_, full_right)| full_right.saturating_add(1))
                .unwrap_or(outer_left);
            if right_edge_start <= outer_right {
                self.blend_capsule_edge_span(
                    y,
                    right_edge_start,
                    outer_right,
                    start_x,
                    start_y,
                    segment_dx,
                    segment_dy,
                    segment_length_sq,
                    radius_sq,
                    full_coverage_sq,
                    radius,
                    antialias,
                    rgba,
                );
                changed_bounds = Some(match changed_bounds {
                    Some((min_x, min_y, max_x, max_y)) => (
                        min_x.min(right_edge_start),
                        min_y.min(y),
                        max_x.max(outer_right),
                        max_y.max(y),
                    ),
                    None => (right_edge_start, y, outer_right, y),
                });
            }
        }

        changed_bounds
            .map(|(min_x, min_y, max_x, max_y)| {
                CanvasDirtyRect::from_inclusive_points(min_x, min_y, max_x, max_y)
            })
            .unwrap_or_else(|| {
                CanvasDirtyRect::from_inclusive_points(left, top, right, bottom)
                    .clamp_to_canvas_bounds(self.width, self.height)
            })
    }

    fn fill_opaque_span(&mut self, y: usize, start_x: usize, end_x: usize, rgba: [u8; 4]) {
        if start_x > end_x || y >= self.height || start_x >= self.width {
            return;
        }
        let end_x = end_x.min(self.width.saturating_sub(1));
        let row_start = (y * self.width + start_x) * 4;
        let row_end = (y * self.width + end_x + 1) * 4;
        let row = &mut self.pixels[row_start..row_end];
        row[..4].copy_from_slice(&rgba);
        let mut filled = 4usize;
        while filled < row.len() {
            let copy_len = filled.min(row.len() - filled);
            row.copy_within(0..copy_len, filled);
            filled += copy_len;
        }
    }

    fn blend_span_with_constant_coverage(
        &mut self,
        y: usize,
        start_x: usize,
        end_x: usize,
        rgba: [u8; 4],
        coverage: f32,
    ) {
        if start_x > end_x || y >= self.height {
            return;
        }
        for x in start_x..=end_x.min(self.width.saturating_sub(1)) {
            let _ = self.blend_pixel(x, y, rgba, coverage);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blend_capsule_edge_span(
        &mut self,
        y: usize,
        start_x: usize,
        end_x: usize,
        start_center_x: f32,
        start_center_y: f32,
        segment_dx: f32,
        segment_dy: f32,
        segment_length_sq: f32,
        radius_sq: f32,
        full_coverage_sq: f32,
        radius: f32,
        antialias: bool,
        rgba: [u8; 4],
    ) {
        if start_x > end_x || y >= self.height {
            return;
        }
        let point_y = y as f32 + 0.5;
        for x in start_x..=end_x.min(self.width.saturating_sub(1)) {
            let point_x = x as f32 + 0.5;
            let distance_sq = distance_sq_to_segment(
                point_x,
                point_y,
                start_center_x,
                start_center_y,
                segment_dx,
                segment_dy,
                segment_length_sq,
            );
            let coverage = if antialias {
                if distance_sq <= full_coverage_sq {
                    1.0
                } else {
                    let distance = distance_sq.sqrt();
                    (radius + 0.5 - distance).clamp(0.0, 1.0)
                }
            } else if distance_sq <= radius_sq {
                1.0
            } else {
                0.0
            };
            if coverage > f32::EPSILON {
                let _ = self.blend_pixel(x, y, rgba, coverage);
            }
        }
    }

    fn blend_pixel(&mut self, x: usize, y: usize, rgba: [u8; 4], coverage: f32) -> CanvasDirtyRect {
        if x >= self.width || y >= self.height {
            return CanvasDirtyRect::from_inclusive_points(
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
            );
        }

        if coverage >= 1.0 - f32::EPSILON && rgba[3] == u8::MAX {
            return self.write_pixel(x, y, rgba);
        }

        let index = (y * self.width + x) * 4;
        let dst = [
            self.pixels[index] as f32 / 255.0,
            self.pixels[index + 1] as f32 / 255.0,
            self.pixels[index + 2] as f32 / 255.0,
            self.pixels[index + 3] as f32 / 255.0,
        ];
        let src_alpha = (rgba[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
        let out_alpha = src_alpha + dst[3] * (1.0 - src_alpha);

        let (out_r, out_g, out_b) = if out_alpha <= f32::EPSILON {
            (0.0, 0.0, 0.0)
        } else {
            let src = [
                rgba[0] as f32 / 255.0,
                rgba[1] as f32 / 255.0,
                rgba[2] as f32 / 255.0,
            ];
            (
                (src[0] * src_alpha + dst[0] * dst[3] * (1.0 - src_alpha)) / out_alpha,
                (src[1] * src_alpha + dst[1] * dst[3] * (1.0 - src_alpha)) / out_alpha,
                (src[2] * src_alpha + dst[2] * dst[3] * (1.0 - src_alpha)) / out_alpha,
            )
        };

        self.pixels[index] = (out_r * 255.0).round().clamp(0.0, 255.0) as u8;
        self.pixels[index + 1] = (out_g * 255.0).round().clamp(0.0, 255.0) as u8;
        self.pixels[index + 2] = (out_b * 255.0).round().clamp(0.0, 255.0) as u8;
        self.pixels[index + 3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        CanvasDirtyRect::from_inclusive_points(x, y, x, y)
    }
}

impl Default for CanvasBitmap {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

#[allow(clippy::too_many_arguments)]
fn distance_sq_to_segment(
    point_x: f32,
    point_y: f32,
    start_x: f32,
    start_y: f32,
    segment_dx: f32,
    segment_dy: f32,
    segment_length_sq: f32,
) -> f32 {
    if segment_length_sq <= f32::EPSILON {
        let dx = point_x - start_x;
        let dy = point_y - start_y;
        return dx * dx + dy * dy;
    }

    let projection = (((point_x - start_x) * segment_dx) + ((point_y - start_y) * segment_dy))
        / segment_length_sq;
    let t = projection.clamp(0.0, 1.0);
    let closest_x = start_x + segment_dx * t;
    let closest_y = start_y + segment_dy * t;
    let dx = point_x - closest_x;
    let dy = point_y - closest_y;
    dx * dx + dy * dy
}

fn capsule_row_pixel_span(
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
    row_center_y: f32,
    radius: f32,
    bitmap_width: usize,
) -> Option<(usize, usize)> {
    if radius <= f32::EPSILON || bitmap_width == 0 {
        return None;
    }

    let mut left = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    for (center_x, center_y) in [(start_x, start_y), (end_x, end_y)] {
        let dy = row_center_y - center_y;
        let dy_sq = dy * dy;
        if dy_sq <= radius * radius {
            let half = (radius * radius - dy_sq).sqrt();
            left = left.min(center_x - half);
            right = right.max(center_x + half);
        }
    }

    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let length_sq = dx * dx + dy * dy;
    if length_sq > f32::EPSILON {
        let length = length_sq.sqrt();
        let ux = dx / length;
        let uy = dy / length;
        let projection_offset = (row_center_y - start_y) * uy;
        let perpendicular_offset = (row_center_y - start_y) * ux;

        let projection_interval = if ux.abs() <= f32::EPSILON {
            if projection_offset >= 0.0 && projection_offset <= length {
                Some((f32::NEG_INFINITY, f32::INFINITY))
            } else {
                None
            }
        } else {
            let x0 = start_x - projection_offset / ux;
            let x1 = start_x + (length - projection_offset) / ux;
            Some((x0.min(x1), x0.max(x1)))
        };

        let perpendicular_interval = if uy.abs() <= f32::EPSILON {
            if perpendicular_offset.abs() <= radius {
                Some((f32::NEG_INFINITY, f32::INFINITY))
            } else {
                None
            }
        } else {
            let x0 = start_x + (perpendicular_offset - radius) / uy;
            let x1 = start_x + (perpendicular_offset + radius) / uy;
            Some((x0.min(x1), x0.max(x1)))
        };

        if let (Some((proj_left, proj_right)), Some((perp_left, perp_right))) =
            (projection_interval, perpendicular_interval)
        {
            let body_left = proj_left.max(perp_left);
            let body_right = proj_right.min(perp_right);
            if body_left <= body_right {
                left = left.min(body_left);
                right = right.max(body_right);
            }
        }
    }

    if !left.is_finite() || !right.is_finite() || left > right {
        return None;
    }

    let pixel_left = ((left - 0.5).ceil().max(0.0)) as usize;
    let pixel_right = ((right - 0.5)
        .floor()
        .min(bitmap_width.saturating_sub(1) as f32)) as isize;
    if pixel_right < pixel_left as isize {
        return None;
    }
    Some((pixel_left, pixel_right as usize))
}
