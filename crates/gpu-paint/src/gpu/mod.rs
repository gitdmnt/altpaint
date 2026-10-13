//! GPU キャンバスリソースの実装 (R22: 旧 `gpu.rs` を責務別に分割)。
//!
//! - `context`: wgpu デバイス/キュー共有コンテキスト。
//! - `texture`: RGBA テクスチャラッパー。
//! - `store`: `(koma_id, layer_index)` キーのテクスチャストア本体。
//! - `snapshot`: ストローク前後スナップショット (Undo/Redo) のテクスチャ操作。
//! - `mask`: レイヤーマスクテクスチャ。
//! - `readback`: GPU → CPU 読み戻し (保存経路)。

pub(crate) mod context;
mod mask;
pub(crate) mod readback;
mod snapshot;
pub(crate) mod store;
pub(crate) mod texture;

pub use context::GpuCanvasContext;
pub use store::{KomaTextureId, LayerTextureStore, LayerUpload};
pub use texture::GpuRgbaTexture;
