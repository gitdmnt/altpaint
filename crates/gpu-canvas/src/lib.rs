//! GPU キャンバスリソース管理クレート。
//!
//! `gpu` feature が有効な場合に wgpu テクスチャを管理する型を提供する。
//! `crates/canvas` の wgpu 非依存を維持しつつ、GPU ペイント処理の基盤となる。

#[cfg(feature = "gpu")]
pub mod format_check;

#[cfg(feature = "gpu")]
pub mod brush;

#[cfg(feature = "gpu")]
mod gpu;

#[cfg(feature = "gpu")]
pub use brush::GpuBrushDispatch;

#[cfg(feature = "gpu")]
pub use gpu::{GpuCanvasContext, GpuCanvasPool, GpuLayerTexture, GpuPenTipCache};

#[cfg(test)]
mod tests;
