//! `panel-html-experiment` — Blitz HTML/CSS パネル描画（GPU 直描画版）。
//!
//! altpaint の既存パネル UI（`.altp-panel` DSL + Wasm）を置き換えずに、HTML/CSS で 1 パネルを
//! GPU 直描画する。`vello::Renderer::render_to_texture` で altpaint 所有の `wgpu::Texture` に
//! 直接書き込み、CPU readback ・ CPU pixel buffer は使わない。
//!
//! ## 主要 API
//!
//! - [`engine::HtmlPanelEngine`] — `HtmlDocument` を保持し、style/layout 解決と `vello::Scene`
//!   構築までを行う。実描画（`render_to_texture`）は外部所有の `vello::Renderer` で行う
//! - [`gpu::PanelGpuTarget`] — パネル毎の GPU テクスチャ（`Rgba8Unorm` + `STORAGE_BINDING` +
//!   `view_formats=[Rgba8UnormSrgb]`）
//! - [`action`] — `data-action` / `data-args` → `ActionDescriptor`（panel-api 非依存）
//! - [`binding`] — `data-bind-*` 式評価ロジック（DOM 非依存）
//!
//! ## 再エクスポート
//!
//! - `blitz_dom` / `blitz_html` / `vello` / `wgpu` — 上位 crate が直接型を扱えるよう公開

pub mod action;
pub mod binding;
pub mod engine;
pub mod gpu;

pub use action::{ActionDescriptor, ActionParseError, parse_data_action};
pub use binding::{
    BindingAttribute, classify_binding_attribute, evaluate_as_bool, evaluate_as_string,
};
pub use engine::{
    HtmlPanelEngine, PanelHit, PixelRect, RenderOutcome, RenderedPanelHit, descriptor_from_hit,
};
pub use gpu::PanelGpuTarget;

pub use blitz_dom;
pub use blitz_html;
pub use blitz_traits;
pub use vello;
pub use wgpu;
