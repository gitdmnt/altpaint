//! `panel-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

pub mod services;

use app_core::{Command, Document};
use serde_json::Value;

pub use services::ServiceRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMoveDirection {
    Up,
    Down,
}

/// パネル境界の 8 ハンドル (4 辺 + 4 角)。
/// Phase 11: 手動リサイズのドラッグ方向を識別する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl ResizeEdge {
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
    use super::ResizeEdge::*;

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

#[derive(Debug, Clone, PartialEq)]
pub enum HostAction {
    DispatchCommand(Command),
    RequestService(ServiceRequest),
    InvokePanelHandler {
        panel_id: String,
        handler_name: String,
        event_kind: String,
    },
    MovePanel {
        panel_id: String,
        direction: PanelMoveDirection,
    },
    SetPanelVisibility {
        panel_id: String,
        visible: bool,
    },
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

/// パネル型プラグインの最小インターフェース。
///
/// ADR 014 (Phase 12) で PanelTree/PanelNode/PanelView/`panel_tree()` / `view()` を撤去し、
/// HTML パネル経路 (`BuiltinPanelPlugin`) の DOM mutation に統一した。
/// ここでは識別子・表示名・ドキュメント同期・イベント受理・persistent 設定の保存だけを契約する。
pub trait PanelPlugin {
    /// ID を計算して返す。
    fn id(&self) -> &'static str;

    /// title を計算して返す。
    fn title(&self) -> &'static str;

    /// 更新 に必要な処理を行う。
    fn update(
        &mut self,
        _document: &Document,
        _can_undo: bool,
        _can_redo: bool,
        _active_jobs: usize,
        _snapshot_count: usize,
    ) {
    }

    /// commands を計算して返す。
    fn commands(&mut self) -> Vec<Command> {
        Vec::new()
    }

    /// プラグイン具体型へのダウンキャスト用ハンドル。
    /// `BuiltinPanelPlugin` がパネル間共通の workspace 情報注入に使う。
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }

    /// debug summary を計算して返す。
    fn debug_summary(&self) -> String {
        String::new()
    }

    /// handles キーボード イベント を計算して返す。
    fn handles_keyboard_event(&self) -> bool {
        false
    }

    /// 現在の persistent 設定 を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn persistent_config(&self) -> Option<Value> {
        None
    }

    /// Persistent 設定 を更新する。
    fn restore_persistent_config(&mut self, _config: &Value) {}

    /// 入力や種別に応じて処理を振り分ける。
    /// 既定実装は何も発行しない (DSL 時代の tree walker は撤去済み)。
    /// HTML パネル経路は `BuiltinPanelPlugin::handle_event` で
    /// data-action を直接見る経路を持つ。
    fn handle_event(&mut self, _event: &PanelEvent) -> Vec<HostAction> {
        Vec::new()
    }
}
