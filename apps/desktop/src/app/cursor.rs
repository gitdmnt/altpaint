//! Phase 11: リサイズハンドル/インタラクション状態に応じた OS カーソルアイコン解決。
//!
//! `panel_resize_hit_at` の結果や active resize edge から winit `CursorIcon` を返す
//! 純粋関数を提供する。
//!
//! - 上下辺: `NsResize`
//! - 左右辺: `EwResize`
//! - 左上 / 右下角: `NwseResize`
//! - 右上 / 左下角: `NeswResize`
//! - リサイズハンドル外: `Default`

use panel_api::ResizeEdge;
use winit::window::CursorIcon;

/// 与えられた edge (None = リサイズハンドル外) に対応する OS カーソルアイコンを返す。
pub(crate) fn cursor_icon_for_edge(edge: Option<ResizeEdge>) -> CursorIcon {
    match edge {
        None => CursorIcon::Default,
        Some(ResizeEdge::North) | Some(ResizeEdge::South) => CursorIcon::NsResize,
        Some(ResizeEdge::East) | Some(ResizeEdge::West) => CursorIcon::EwResize,
        Some(ResizeEdge::NorthWest) | Some(ResizeEdge::SouthEast) => CursorIcon::NwseResize,
        Some(ResizeEdge::NorthEast) | Some(ResizeEdge::SouthWest) => CursorIcon::NeswResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_edge_returns_default_cursor() {
        assert_eq!(cursor_icon_for_edge(None), CursorIcon::Default);
    }

    #[test]
    fn vertical_edges_return_ns_resize() {
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::North)),
            CursorIcon::NsResize
        );
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::South)),
            CursorIcon::NsResize
        );
    }

    #[test]
    fn horizontal_edges_return_ew_resize() {
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::East)),
            CursorIcon::EwResize
        );
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::West)),
            CursorIcon::EwResize
        );
    }

    #[test]
    fn nw_se_corners_return_nwse_resize() {
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::NorthWest)),
            CursorIcon::NwseResize
        );
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::SouthEast)),
            CursorIcon::NwseResize
        );
    }

    #[test]
    fn ne_sw_corners_return_nesw_resize() {
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::NorthEast)),
            CursorIcon::NeswResize
        );
        assert_eq!(
            cursor_icon_for_edge(Some(ResizeEdge::SouthWest)),
            CursorIcon::NeswResize
        );
    }
}
