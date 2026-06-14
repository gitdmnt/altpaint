//! tools feature スライス (BL-111 / BL-119)。
//!
//! ツールカタログのディレクトリロード (`catalog`)、desktop 既定カタログ
//! (`default_catalog`)、`tool_catalog.*` service ハンドラ + ペン import (`service`)
//! を所有する。
//!
//! B7 で `project-store::tool_catalog` (catalog)・`app/default_tool_catalog.rs`
//! (default_catalog)・`app/services/tool_catalog.rs` (service) から移設した。

mod catalog;
mod default_catalog;
mod service;

pub(crate) use catalog::load_tool_directory;
pub(crate) use default_catalog::desktop_default_tool_catalog;
pub(crate) use service::handle_tool_catalog_service_request;
