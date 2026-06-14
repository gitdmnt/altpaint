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

/// ストロークセグメントあたりのスタンプ最大数。
///
/// GPU TDR（Windows のタイムアウト検出）を回避し、CPU 過負荷を防ぐための上限値。
/// CPU 経路 (`paint_engine::ops::stroke`) と GPU 経路 (`gpu_paint::brush`) の両方が
/// この定数を参照するため、両クレートが依存する `raster` に置く。R40/BL-023 が
/// 最終的に paint-engine の stroke モジュールへ移す予定だが、それは B8 (PaintPlan 化で
/// stamps が計画側へ移り gpu-paint 参照が消える) で行う。
pub const MAX_STAMP_STEPS: usize = 64;
