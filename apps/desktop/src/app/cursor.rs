//! Phase 11: リサイズハンドル/インタラクション状態に応じた OS カーソルアイコン解決。
//!
//! `panel_resize_hit_at` の結果や active resize handle から winit `CursorIcon` を返す
//! 純粋関数を提供する。
//!
//! - 上下辺: `NsResize`
//! - 左右辺: `EwResize`
//! - 左上 / 右下角: `NwseResize`
//! - 右上 / 左下角: `NeswResize`
//! - リサイズハンドル外: `Default`

use panel_runtime::ResizeHandle;
use winit::window::CursorIcon;

/// 与えられた handle (None = リサイズハンドル外) に対応する OS カーソルアイコンを返す。
pub(crate) fn cursor_icon_for_resize_handle(handle: Option<ResizeHandle>) -> CursorIcon {
    match handle {
        None => CursorIcon::Default,
        Some(ResizeHandle::North) | Some(ResizeHandle::South) => CursorIcon::NsResize,
        Some(ResizeHandle::East) | Some(ResizeHandle::West) => CursorIcon::EwResize,
        Some(ResizeHandle::NorthWest) | Some(ResizeHandle::SouthEast) => CursorIcon::NwseResize,
        Some(ResizeHandle::NorthEast) | Some(ResizeHandle::SouthWest) => CursorIcon::NeswResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_edge_returns_default_cursor() {
        assert_eq!(cursor_icon_for_resize_handle(None), CursorIcon::Default);
    }

    #[test]
    fn vertical_edges_return_ns_resize() {
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::North)),
            CursorIcon::NsResize
        );
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::South)),
            CursorIcon::NsResize
        );
    }

    #[test]
    fn horizontal_edges_return_ew_resize() {
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::East)),
            CursorIcon::EwResize
        );
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::West)),
            CursorIcon::EwResize
        );
    }

    #[test]
    fn nw_se_corners_return_nwse_resize() {
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::NorthWest)),
            CursorIcon::NwseResize
        );
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::SouthEast)),
            CursorIcon::NwseResize
        );
    }

    #[test]
    fn ne_sw_corners_return_nesw_resize() {
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::NorthEast)),
            CursorIcon::NeswResize
        );
        assert_eq!(
            cursor_icon_for_resize_handle(Some(ResizeHandle::SouthWest)),
            CursorIcon::NeswResize
        );
    }
}
