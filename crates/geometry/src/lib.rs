//! `geometry` は `altpaint` の座標系と矩形・dirty rect 演算を提供する水平土台クレート。
//!
//! ウィンドウ / キャンバス viewport / ページ / コマローカル / パネルサーフェスの
//! 各座標型と、それらの矩形・dirty rect の統合 (union) / クランプ演算を定義する。
//! ローカルクレート依存を持たず、`serde` のみに依存する。

mod coordinates;
mod dirty;

pub use coordinates::{
    CanvasDisplayPoint, CanvasViewportPoint, ClampToCanvasBounds, KomaLocalPoint, MergeInSpace,
    PageDirtyRect, PagePoint, PagePointF, PanelSurfaceDirtyRect, PanelSurfacePoint,
    PanelSurfaceRect, WindowDirtyRect, WindowPoint, WindowRect,
};
pub use dirty::{accumulate_dirty_rect, union_optional_rect};
