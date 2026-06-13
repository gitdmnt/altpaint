//! text feature スライス (BL-111 / BL-082)。
//!
//! テキストラスタライズ (`raster`) と `text_render.*` service ハンドラ (`service`) を
//! 所有する。font8x8 依存はこの feature に閉じる。
//!
//! B7 で `app/services/{text_raster,text_render}.rs` から移設した。

mod raster;
mod service;

pub(crate) use service::handle_text_render_service_request;
