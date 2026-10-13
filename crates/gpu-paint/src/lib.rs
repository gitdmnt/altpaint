//! GPU キャンバスリソース管理クレート。
//!
//! wgpu テクスチャを管理する型を提供する。
//! `crates/paint-engine` の wgpu 非依存を維持しつつ、GPU ペイント処理の基盤となる。

pub mod format_check;
pub mod brush;
pub mod composite;
pub mod fill;
mod gpu;
mod pipeline;

pub use brush::{BrushStrokeParams, BrushPipeline};
pub use composite::{CompositeLayerEntry, CompositePipeline};
pub use fill::{FloodFillOutcome, FillPipeline};
pub use gpu::{GpuCanvasContext, KomaTextureId, LayerTextureStore, GpuRgbaTexture, LayerUpload};

#[cfg(test)]
mod tests;
