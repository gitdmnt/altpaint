//! GPU キャンバスリソース管理クレート。
//!
//! wgpu テクスチャを管理する型を提供する。
//! `crates/paint-engine` の wgpu 非依存を維持しつつ、GPU ペイント処理の基盤となる。

pub mod format_check;
pub mod brush;
pub mod composite;
pub mod fill;
mod gpu;

pub use brush::{BrushStrokeParams, GpuBrushDispatch};
pub use composite::{CompositeLayerEntry, GpuLayerCompositor};
pub use fill::{FloodFillOutcome, GpuFillDispatch};
pub use gpu::{GpuCanvasContext, GpuCanvasPool, GpuLayerTexture};

#[cfg(test)]
mod tests;
