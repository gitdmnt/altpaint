//! view feature スライス (BL-111)。
//!
//! `view.*` service ハンドラ (`service`) を所有する。zoom/pan/rotate/flip/reset
//! を `SessionCommand` へ翻訳して適用する。
//!
//! B7 で `app/services/view.rs` から移設した。

mod service;

pub(crate) use service::handle_view_service_request;
