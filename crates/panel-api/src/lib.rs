//! `panel-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

pub mod services;

use document_model::DocumentCommand;
use editor_state::SessionCommand;

pub use services::ServiceRequest;

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
mod resize_edge_tests {
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

/// パネル (Wasm) → ホストへの要求。
///
/// P3: パネル可視性/並び替えは workspace_layout サービス経路 (`RequestService`)
/// に一本化済みのため、専用 variant (`MovePanel` / `SetPanelVisibility`) は削除した。
/// translator が `RequestDescriptor` を 3 経路へ振り分けた結果を搬送する。
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
    /// 純粋なドキュメント変異コマンドを適用する。
    DispatchDocumentCommand(DocumentCommand),
    /// エディタセッション (ツール/色/ペン/ビュー) を変更する。
    /// translator が namespace から振り分けるセッション経路。
    DispatchSessionCommand(SessionCommand),
    /// I/O を伴うホストサービス要求。
    RequestService(ServiceRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    Activate {
        panel_id: String,
        node_id: String,
    },
    SetValue {
        panel_id: String,
        node_id: String,
        value: i32,
    },
    DragValue {
        panel_id: String,
        node_id: String,
        from: i32,
        to: i32,
    },
    SetText {
        panel_id: String,
        node_id: String,
        value: String,
    },
    Keyboard {
        panel_id: String,
        shortcut: String,
        key: String,
        repeat: bool,
    },
}

