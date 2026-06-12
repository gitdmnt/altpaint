//! `raster` は `altpaint` のラスタ語彙を提供する水平土台クレート。
//!
//! RGBA8 の汎用ビットマップ (`RgbaBitmap`) とそのラスタライズプリミティブ、
//! ピクセルブレンドの単一実装 (`BlendMode` とブレンド関数)、ビットマップ編集差分
//! (`BitmapEdit`) を定義する。座標型・矩形演算は `geometry` クレートに依存し、
//! ドメインモデル・GPU には依存しない。

pub mod bitmap;
pub mod blend;
pub mod edit;

pub use bitmap::RgbaBitmap;
pub use blend::{BlendMode, composite_pixel, source_over_coverage_pixel};
pub use edit::{BitmapComposite, BitmapCompositor, BitmapEdit};
