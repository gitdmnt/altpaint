mod html_wasm_panel;
mod request_translation;
mod persistent_config;
pub mod host_state;
mod loader;
mod meta;
mod runtime;

pub use html_wasm_panel::{HtmlWasmPanelError, HtmlWasmPanel};
pub use request_translation::command_from_descriptor;
pub use host_state::{
    EMPTY_WORKSPACE_PANELS_JSON, HostStateCache, build_host_state,
};
pub use loader::{BuiltinPanelDef, BuiltinPanelLoadError, register_builtin_panels};
pub use meta::{PanelMeta, PanelSizeMeta};
pub use runtime::{RenderedPanelTexture, PanelRuntime, PanelDispatchResult, PanelKeyboardResult};

// パネルサブシステムの facade 再公開。
// desktop はパネル関連の型を panel-runtime 経由でのみ参照する (Phase 15 / ADR 017)。
pub use panel_api::{
    HostAction, PanelEvent, PanelMoveDirection, ResizeEdge, ServiceRequest, services,
};
pub use panel_html as html;
pub use panel_html::PanelSizeConstraints;
