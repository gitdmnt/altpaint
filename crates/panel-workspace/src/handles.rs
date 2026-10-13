//! パネル配置操作のジオメトリ型 (旧 `panel-api`、C9 で panel-workspace へ移設)。

/// パネル並び替えの方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMoveDirection {
    Up,
    Down,
}

/// パネル境界の 8 ハンドル (4 辺 + 4 角)。
/// Phase 11: 手動リサイズのドラッグ方向を識別する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeHandle {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl ResizeHandle {
    /// 左辺 (x = panel.left) を掴んでいるか。
    pub fn touches_left(self) -> bool {
        matches!(self, Self::NorthWest | Self::West | Self::SouthWest)
    }

    /// 上辺 (y = panel.top) を掴んでいるか。
    pub fn touches_top(self) -> bool {
        matches!(self, Self::NorthWest | Self::North | Self::NorthEast)
    }

    /// 右辺 (x = panel.right) を掴んでいるか。
    pub fn touches_right(self) -> bool {
        matches!(self, Self::NorthEast | Self::East | Self::SouthEast)
    }

    /// 下辺 (y = panel.bottom) を掴んでいるか。
    pub fn touches_bottom(self) -> bool {
        matches!(self, Self::SouthEast | Self::South | Self::SouthWest)
    }
}

#[cfg(test)]
mod resize_handle_tests {
    use super::ResizeHandle::*;

    #[test]
    fn touches_left_returns_true_for_west_variants() {
        assert!(NorthWest.touches_left());
        assert!(West.touches_left());
        assert!(SouthWest.touches_left());
        assert!(!North.touches_left());
        assert!(!East.touches_left());
        assert!(!NorthEast.touches_left());
    }

    #[test]
    fn touches_top_returns_true_for_north_variants() {
        assert!(NorthWest.touches_top());
        assert!(North.touches_top());
        assert!(NorthEast.touches_top());
        assert!(!South.touches_top());
        assert!(!East.touches_top());
    }

    #[test]
    fn touches_right_returns_true_for_east_variants() {
        assert!(NorthEast.touches_right());
        assert!(East.touches_right());
        assert!(SouthEast.touches_right());
        assert!(!NorthWest.touches_right());
        assert!(!West.touches_right());
    }

    #[test]
    fn touches_bottom_returns_true_for_south_variants() {
        assert!(SouthEast.touches_bottom());
        assert!(South.touches_bottom());
        assert!(SouthWest.touches_bottom());
        assert!(!North.touches_bottom());
    }
}
