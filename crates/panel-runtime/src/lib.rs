mod html_wasm_panel;
mod host_request;
mod request_translation;
mod translator_registry;
mod persistent_config;
pub mod host_state;
mod loader;
mod meta;
mod runtime;
pub mod services;

pub use html_wasm_panel::{HtmlWasmPanelError, HtmlWasmPanel};
pub use request_translation::{TranslatedRequest, register_default_translators};
pub use translator_registry::{
    TranslationDiagnostic, TranslatorFn, TranslatorRegistry,
};
pub use host_state::{
    EMPTY_WORKSPACE_PANELS_JSON, HostState, HostStateCache, build_host_state,
};
pub use loader::{BuiltinPanelDef, BuiltinPanelLoadError, register_builtin_panels};
pub use meta::{PanelMeta, PanelSizeMeta};
pub use runtime::{
    HtmlSurfaceRenderer, RenderedPanelTexture, PanelRuntime, PanelDispatchResult,
    PanelKeyboardResult,
};

// パネル契約型 (旧 panel-api、C9 で本クレートへ移設)。
// desktop はパネルイベント/要求型を panel-runtime 経由で参照する。
pub use host_request::{HostRequest, PanelEvent};
pub use services::ServiceRequest;
pub use panel_html as html;
pub use panel_html::PanelSizeConstraints;
