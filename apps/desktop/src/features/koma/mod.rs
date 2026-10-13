//! koma feature スライス (BL-111 / BL-081)。
//!
//! コマ作成 (KomaRect) ジェスチャの状態機械 (`gesture`) と `koma_nav.*` service
//! ハンドラ (`service`) を所有する。
//!
//! B7 で `app/koma_gesture.rs` (gesture) と `app/services/koma_navigation.rs`
//! (service) から移設した。

mod gesture;
mod service;

pub(crate) use gesture::{
    KomaGesture, KomaGestureUpdate, advance_koma_gesture, koma_creation_preview_bounds,
};
pub(crate) use service::handle_koma_navigation_service_request;
