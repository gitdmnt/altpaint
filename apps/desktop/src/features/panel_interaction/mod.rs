//! panel_interaction feature スライス (BL-111 / D14)。
//!
//! パネルの drag / resize / press の幾何ステートマシン (`state`) を所有する。
//! ホストアクションのルーティングは `app/host_request_router.rs`。
//!
//! B7 で `app/panel_dispatch.rs` の幾何部を features/panel_interaction へ分離した。

mod state;

pub(crate) use state::PanelInteractionState;
#[cfg(test)]
pub(crate) use state::PanelDragState;
