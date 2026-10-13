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

    /// 同じローカル座標系の点が範囲内に含まれるか判定する。
    pub fn contains_local(self, point: PanelSurfacePoint) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.width
            && point.y < self.y + self.height
    }

    /// 2 つの矩形を内包する最小の矩形を返す。
    pub fn union(self, other: WindowRect) -> WindowRect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        WindowRect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }

    /// 2 つの矩形の共通部分を返す。重なりがない場合は `None`。
    pub fn intersect(self, other: WindowRect) -> Option<WindowRect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if left >= right || top >= bottom {
            return None;
        }

        Some(WindowRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
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

/// ページ (作品ピクセル空間) 上のピクセル座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagePoint {
    pub x: usize,
    pub y: usize,
}

impl PagePoint {
    pub const fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

/// ページ (作品ピクセル空間) 上のサブピクセル座標を表す。
///
/// `PagePoint` の浮動小数版。手ぶれ補正のようにピクセル格子の間を
/// 扱う計算で使い、確定時に `to_page_point()` で丸める。
/// 表示座標 (スケール・回転適用済み) は `CanvasDisplayPoint` が担い、本型とは区別する。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagePointF {
    pub x: f32,
    pub y: f32,
}

impl PagePointF {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// 最近傍ピクセルへ丸めた `PagePoint` を返す (負値は 0 へクランプ)。
    pub fn to_page_point(self) -> PagePoint {
        PagePoint::new(
            self.x.round().max(0.0) as usize,
            self.y.round().max(0.0) as usize,
        )
    }

    /// `other` へ係数 `t` (0.0..=1.0) だけ近づけた点を返す。
    pub fn lerp_toward(self, other: PagePointF, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

impl From<PagePoint> for PagePointF {
    fn from(value: PagePoint) -> Self {
        Self::new(value.x as f32, value.y as f32)
    }
}

/// アクティブコマローカルの編集座標を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KomaLocalPoint {
    pub x: usize,
    pub y: usize,
}

impl KomaLocalPoint {
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

/// ページ座標系の dirty rect を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageDirtyRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PageDirtyRect {
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

impl MergeInSpace for PageDirtyRect {
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

impl ClampToCanvasBounds for PageDirtyRect {
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
    fn window_rect_union_covers_both_rects() {
        let a = WindowRect::new(10, 10, 20, 20);
        let b = WindowRect::new(25, 30, 10, 10);

        assert_eq!(a.union(b), WindowRect::new(10, 10, 25, 30));
    }

    #[test]
    fn window_rect_intersect_returns_overlap_or_none() {
        let a = WindowRect::new(10, 10, 20, 20);
        let b = WindowRect::new(20, 20, 20, 20);

        assert_eq!(a.intersect(b), Some(WindowRect::new(20, 20, 10, 10)));
        assert_eq!(a.intersect(WindowRect::new(100, 100, 5, 5)), None);
    }

    #[test]
    fn window_rect_contains_local_checks_same_space_point() {
        let rect = WindowRect::new(4, 4, 8, 8);

        assert!(rect.contains_local(PanelSurfacePoint::new(4, 4)));
        assert!(rect.contains_local(PanelSurfacePoint::new(11, 11)));
        assert!(!rect.contains_local(PanelSurfacePoint::new(12, 4)));
        assert!(!rect.contains_local(PanelSurfacePoint::new(3, 4)));
    }

    #[test]
    fn page_dirty_rect_merge_combines_bounds() {
        let left = PageDirtyRect::from_inclusive_points(2, 3, 4, 5);
        let right = PageDirtyRect::from_inclusive_points(6, 1, 7, 4);

        assert_eq!(left.merge(right), PageDirtyRect::new(2, 1, 6, 5));
    }

    /// PagePointF の丸めが最近傍ピクセルへ向かい、負値が 0 にクランプされることを検証する。
    #[test]
    fn page_point_f_rounds_to_nearest_and_clamps_negative() {
        assert_eq!(
            PagePointF::new(2.6, 3.4).to_page_point(),
            PagePoint::new(3, 3)
        );
        assert_eq!(
            PagePointF::new(-1.5, 0.5).to_page_point(),
            PagePoint::new(0, 1)
        );
    }

    /// PagePointF::lerp_toward が係数どおりに補間することを検証する。
    #[test]
    fn page_point_f_lerp_toward_blends_by_factor() {
        let from = PagePointF::new(0.0, 10.0);
        let to = PagePointF::new(10.0, 20.0);

        assert_eq!(from.lerp_toward(to, 0.5), PagePointF::new(5.0, 15.0));
        assert_eq!(from.lerp_toward(to, 1.0), to);
    }
}
