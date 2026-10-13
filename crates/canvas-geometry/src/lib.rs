//! `canvas-geometry` はキャンバス表示幾何 (view↔page 座標写像と提示用テクスチャ矩形) を提供する。
//!
//! wgpu/fontdb 等の重量依存を持たず、`geometry` / `editor-state` のドメイン型のみに依存する。

mod view_geometry;

pub use view_geometry::{CanvasViewGeometry, TextureQuad};

#[cfg(test)]
mod tests;
