//! `document-model` は `altpaint` の作品ドメインモデルを保持する水平土台クレート。
//!
//! 作品 (`Work`) → ページ (`Page`) → コマ (`Koma`) → ラスタレイヤー (`RasterLayer`) の
//! ドメイン構造と、その変更経路の入口になる `DocumentCommand` 型、ロード後の不変条件修復
//! (`normalize_after_load`)、コマグリッドレイアウト、レイヤー合成を定義する。
//!
//! 座標系・矩形・dirty rect 演算は `geometry`、`RgbaBitmap`・ブレンド・ラスタライズ・
//! `BitmapEdit` は `raster`、エディタの一過性編集状態 (`EditorSession` / `SessionCommand` /
//! ツール・ペン定義) は `editor-state` が担う。本クレートは GPU/ウィンドウ/Wasm に依存しない。

pub mod blend;
pub mod command;
pub mod document;

pub use command::DocumentCommand;
pub use document::{
    DEFAULT_PAGE_HEIGHT, DEFAULT_PAGE_WIDTH, Document, Koma, KomaBounds, KomaId, LayerMask,
    LayerNodeId, MAX_PAGE_DIMENSION, MAX_PAGE_PIXELS, Page, PageId, RasterLayer, Work, WorkId,
    parse_document_size,
};
