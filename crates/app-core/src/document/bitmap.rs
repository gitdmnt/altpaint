//! `CanvasBitmap` の画素操作ロジックをまとめる。
//!
//! ドメイン型定義から描画アルゴリズムを分離し、`Document` 本体の責務を
//! 状態遷移に集中させる。

use super::{CanvasBitmap, DirtyRect};

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
    pub fn draw_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.draw_point_rgba(x, y, [0, 0, 0, 255])
    }

    /// ビットマップ内の1点を任意色で塗る。
    pub fn draw_point_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> DirtyRect {
        self.write_pixel(x, y, rgba)
    }

    /// ビットマップ内の1点を白で塗る。
    pub fn erase_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.write_pixel(x, y, [255, 255, 255, 255])
    }

    /// 指定サイズの円形ブラシで1点描画する。
    pub fn draw_point_sized_rgba(
        &mut self,
        x: usize,
        y: usize,
        rgba: [u8; 4],
        size: u32,
    ) -> DirtyRect {
        self.paint_disk(x as isize, y as isize, size, rgba)
    }

    /// 指定サイズの円形ブラシで1点消去する。
    pub fn erase_point_sized(&mut self, x: usize, y: usize, size: u32) -> DirtyRect {
        self.paint_disk(x as isize, y as isize, size, [255, 255, 255, 255])
    }

    /// 2点間を結ぶ最小ストロークを描く。
    pub fn draw_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> DirtyRect {
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
    ) -> DirtyRect {
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

        DirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }

    /// 2点間を白で線消去する。
    pub fn erase_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> DirtyRect {
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

        DirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }

    /// 指定サイズの円形ブラシで線描画する。
    pub fn draw_line_sized_rgba(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        rgba: [u8; 4],
        size: u32,
    ) -> DirtyRect {
        self.paint_line_disks(from_x, from_y, to_x, to_y, size, rgba)
    }

    /// 指定サイズの円形ブラシで線消去する。
    pub fn erase_line_sized(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
    ) -> DirtyRect {
        self.paint_line_disks(from_x, from_y, to_x, to_y, size, [255, 255, 255, 255])
    }

    /// 単一ピクセルを書き換え、その dirty rect を返す。
    fn write_pixel(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> DirtyRect {
        if x >= self.width || y >= self.height {
            return DirtyRect::from_inclusive_points(
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

        DirtyRect::from_inclusive_points(x, y, x, y)
    }

    /// 円形ブラシを線分上に連続配置して太線を描く。
    fn paint_line_disks(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
        rgba: [u8; 4],
    ) -> DirtyRect {
        let mut x0 = from_x as isize;
        let mut y0 = from_y as isize;
        let x1 = to_x as isize;
        let y1 = to_y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let mut dirty = self.paint_disk(x0, y0, size, rgba);

        loop {
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
            dirty = dirty.union(self.paint_disk(x0, y0, size, rgba));
        }

        dirty
    }

    /// 円形ブラシ 1 個ぶんを描画する。
    fn paint_disk(
        &mut self,
        center_x: isize,
        center_y: isize,
        size: u32,
        rgba: [u8; 4],
    ) -> DirtyRect {
        let radius = (size.max(1) as f32) * 0.5;
        let left = (center_x as f32 - radius).floor().max(0.0) as usize;
        let top = (center_y as f32 - radius).floor().max(0.0) as usize;
        let right = (center_x as f32 + radius).ceil().max(0.0) as usize;
        let bottom = (center_y as f32 + radius).ceil().max(0.0) as usize;
        let mut dirty: Option<DirtyRect> = None;

        for y in top..=bottom {
            for x in left..=right {
                let dx = x as f32 + 0.5 - (center_x as f32 + 0.5);
                let dy = y as f32 + 0.5 - (center_y as f32 + 0.5);
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let rect = self.write_pixel(x, y, rgba);
                dirty = Some(match dirty {
                    Some(current) => current.union(rect),
                    None => rect,
                });
            }
        }

        dirty.unwrap_or_else(|| DirtyRect::from_inclusive_points(left, top, right, bottom))
    }
}

impl Default for CanvasBitmap {
    fn default() -> Self {
        Self::new(64, 64)
    }
}
