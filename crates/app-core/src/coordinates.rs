use serde::{Deserialize, Serialize};

/// 同一座標空間の値同士を統合する操作を表す。
pub trait MergeInSpace: Sized {
    fn merge(self, other: Self) -> Self;
}

/// キャンバス境界へクランプする操作を表す。
pub trait ClampToCanvasBounds: Sized {
    fn clamp_to_canvas_bounds(self, width: usize, height: usize) -> Self;
}

/// ウィンドウ左上基準のグローバル座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPoint {
    pub x: i32,
    pub y: i32,
}

impl WindowPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// ウィンドウ左上基準のグローバル矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl WindowRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, point: WindowPoint) -> bool {
        point.x >= self.x as i32
            && point.y >= self.y as i32
            && point.x < (self.x + self.width) as i32
            && point.y < (self.y + self.height) as i32
    }

    pub fn clamp_point(self, point: WindowPoint) -> Option<WindowPoint> {
        if self.width == 0 || self.height == 0 {
            return None;
        }

        Some(WindowPoint {
            x: point.x.clamp(
                self.x as i32,
                (self.x + self.width.saturating_sub(1)) as i32,
            ),
            y: point.y.clamp(
                self.y as i32,
                (self.y + self.height.saturating_sub(1)) as i32,
            ),
        })
    }

    pub fn to_canvas_viewport_point(self, point: WindowPoint) -> Option<CanvasViewportPoint> {
        self.contains(point).then_some(CanvasViewportPoint::new(
            point.x - self.x as i32,
            point.y - self.y as i32,
        ))
    }

    pub fn clamp_to_canvas_viewport_point(self, point: WindowPoint) -> Option<CanvasViewportPoint> {
        let point = self.clamp_point(point)?;
        self.to_canvas_viewport_point(point)
    }

    pub fn to_panel_surface_point(self, point: WindowPoint) -> Option<PanelSurfacePoint> {
        self.contains(point).then_some(PanelSurfacePoint::new(
            (point.x - self.x as i32) as usize,
            (point.y - self.y as i32) as usize,
        ))
    }

    pub fn clamp_to_panel_surface_point(self, point: WindowPoint) -> Option<PanelSurfacePoint> {
        let point = self.clamp_point(point)?;
        self.to_panel_surface_point(point)
    }
}

/// キャンバス表示 viewport 左上基準のローカル座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasViewportPoint {
    pub x: i32,
    pub y: i32,
}

impl CanvasViewportPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// ページ / キャンバス上のピクセル座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasPoint {
    pub x: usize,
    pub y: usize,
}

impl CanvasPoint {
    pub const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// アクティブコマローカルの編集座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelLocalPoint {
    pub x: usize,
    pub y: usize,
}

impl PanelLocalPoint {
    pub const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// パネルサーフェス左上基準のローカル座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSurfacePoint {
    pub x: usize,
    pub y: usize,
}

impl PanelSurfacePoint {
    pub const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// パネルサーフェスのグローバル矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSurfaceRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PanelSurfaceRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn as_window_rect(self) -> WindowRect {
        WindowRect::new(self.x, self.y, self.width, self.height)
    }

    pub fn to_surface_point(self, point: WindowPoint) -> Option<PanelSurfacePoint> {
        self.as_window_rect().to_panel_surface_point(point)
    }

    pub fn clamp_to_surface_point(self, point: WindowPoint) -> Option<PanelSurfacePoint> {
        self.as_window_rect().clamp_to_panel_surface_point(point)
    }
}

/// キャンバス上の表示位置を表す浮動小数座標。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasDisplayPoint {
    pub x: f32,
    pub y: f32,
}

impl CanvasDisplayPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// キャンバス / ページ座標系の dirty rect を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasDirtyRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl CanvasDirtyRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_inclusive_points(from_x: usize, from_y: usize, to_x: usize, to_y: usize) -> Self {
        let min_x = from_x.min(to_x);
        let min_y = from_y.min(to_y);
        let max_x = from_x.max(to_x);
        let max_y = from_y.max(to_y);

        Self {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        }
    }
}

impl MergeInSpace for CanvasDirtyRect {
    fn merge(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

impl ClampToCanvasBounds for CanvasDirtyRect {
    fn clamp_to_canvas_bounds(self, width: usize, height: usize) -> Self {
        if width == 0 || height == 0 {
            return Self {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
        }

        let max_x = width - 1;
        let max_y = height - 1;
        let left = self.x.min(max_x);
        let top = self.y.min(max_y);
        let right = self
            .x
            .saturating_add(self.width.saturating_sub(1))
            .min(max_x);
        let bottom = self
            .y
            .saturating_add(self.height.saturating_sub(1))
            .min(max_y);

        Self::from_inclusive_points(left, top, right, bottom)
    }
}

/// ウィンドウグローバル座標系の dirty rect を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowDirtyRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl WindowDirtyRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn as_window_rect(self) -> WindowRect {
        WindowRect::new(self.x, self.y, self.width, self.height)
    }
}

impl MergeInSpace for WindowDirtyRect {
    fn merge(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }
}

impl From<WindowRect> for WindowDirtyRect {
    fn from(value: WindowRect) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

/// パネルサーフェスローカル座標系の dirty rect を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSurfaceDirtyRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PanelSurfaceDirtyRect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

impl MergeInSpace for PanelSurfaceDirtyRect {
    fn merge(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_rect_maps_window_point_to_canvas_viewport_point() {
        let rect = WindowRect::new(100, 80, 320, 240);
        let point = WindowPoint::new(132, 144);

        assert_eq!(
            rect.to_canvas_viewport_point(point),
            Some(CanvasViewportPoint::new(32, 64))
        );
    }

    #[test]
    fn panel_surface_rect_clamps_window_point_into_surface_space() {
        let rect = PanelSurfaceRect::new(120, 80, 8, 6);

        assert_eq!(
            rect.clamp_to_surface_point(WindowPoint::new(999, -10)),
            Some(PanelSurfacePoint::new(7, 0))
        );
    }

    #[test]
    fn canvas_dirty_rect_merge_combines_bounds() {
        let left = CanvasDirtyRect::from_inclusive_points(2, 3, 4, 5);
        let right = CanvasDirtyRect::from_inclusive_points(6, 1, 7, 4);

        assert_eq!(left.merge(right), CanvasDirtyRect::new(2, 1, 6, 5));
    }
}
