mod html_wasm_panel;
mod host_request;
mod panel_input;
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
pub use panel_input::{PanelPointerInput, PanelPointerKind};
pub use services::ServiceRequest;

// panel-html の facade (BL-091)。
// `pub use panel_html as html` の素通しを廃止し、desktop が必要とする最小面のみ
// 選別再公開する。ステータスバーが共有 GPU コンテキストで HtmlPanelView を直接
// 描画するため `HtmlPanelView` / `RenderOutcome` / `PanelGpuTarget` と、それらが
// 要求する `vello` / `wgpu` を再公開する。blitz_traits / blitz_dom / blitz_html は
// ホスト側に晒さない (入力は PanelPointerInput、出力は RenderedPanelTexture で受け渡す)。
pub mod html {
    pub use panel_html::{HtmlPanelView, PanelGpuTarget, RenderOutcome, vello, wgpu};
}
pub use panel_html::PanelSizeConstraints;
