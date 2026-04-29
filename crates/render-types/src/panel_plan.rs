use crate::PixelRect;

/// パネル面を `render` に引き渡すための軽量ビュー。
#[derive(Clone, Copy)]
pub struct PanelSurfaceSource<'a> {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub pixels: &'a [u8],
}

impl<'a> PanelSurfaceSource<'a> {
    /// 矩形 を計算して返す。
    pub fn rect(&self) -> PixelRect {
        PixelRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }

    /// plan を計算して返す。
    pub fn plan(&self) -> PanelPlan {
        PanelPlan {
            surface_rect: self.rect(),
        }
    }
}

/// パネル面の配置計画を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelPlan {
    pub surface_rect: PixelRect,
}
