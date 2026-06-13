//! パネルへ転送するポインタ入力 DTO (BL-091)。
//!
//! desktop (ホスト) が blitz の `UiEvent` を直接構築するのをやめ、panel-runtime が
//! 公開する [`PanelPointerInput`] を介してパネルへ入力を渡す。blitz の型は本クレート
//! 内部でのみ構築し、ホスト側 facade から blitz 依存を切断する。

use panel_html::blitz_traits;

/// パネルへ転送するポインタイベントの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelPointerKind {
    Down,
    Up,
    Move,
}

/// パネルへ転送するポインタ入力。座標はパネルローカル (`local_*`) と
/// 画面 (`screen_*`) の両方を持つ。
///
/// ローカル座標は chrome を含むパネル全体原点 (screen_rect 基準) で、
/// body オフセット (chrome_height) は View 側で扱う。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPointerInput {
    pub kind: PanelPointerKind,
    pub local_x: f32,
    pub local_y: f32,
    pub screen_x: f32,
    pub screen_y: f32,
}

impl PanelPointerInput {
    /// blitz の `UiEvent` へ変換する (panel-runtime 内部専用)。
    pub(crate) fn into_ui_event(self) -> blitz_traits::events::UiEvent {
        use blitz_traits::events::{
            BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons,
            PointerCoords, PointerDetails, UiEvent,
        };
        let coords = PointerCoords {
            page_x: self.local_x,
            page_y: self.local_y,
            client_x: self.local_x,
            client_y: self.local_y,
            screen_x: self.screen_x,
            screen_y: self.screen_y,
        };
        let pointer = BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords,
            button: MouseEventButton::Main,
            buttons: match self.kind {
                PanelPointerKind::Down => MouseEventButtons::Primary,
                _ => MouseEventButtons::empty(),
            },
            // keyboard_types を直接名前解決せず Default (= 修飾子なし) を使う。
            mods: Default::default(),
            details: PointerDetails::default(),
        };
        match self.kind {
            PanelPointerKind::Down => UiEvent::PointerDown(pointer),
            PanelPointerKind::Up => UiEvent::PointerUp(pointer),
            PanelPointerKind::Move => UiEvent::PointerMove(pointer),
        }
    }
}
