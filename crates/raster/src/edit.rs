//! ビットマップ編集差分 (`BitmapEdit`) と合成方法 (`BitmapComposite`)。
//!
//! 描画経路が生成する「更新矩形 + 更新ビットマップ + 合成方法」の純粋な
//! ラスタ差分表現。ドメインモデルや GPU には依存しない。

use std::fmt;
use std::sync::Arc;

use geometry::PageDirtyRect;

use crate::blend::{BlendMode, composite_pixel};
use crate::bitmap::RgbaBitmap;

/// `bitmap_a` と既存 `bitmap_b` から結果ビットマップを作る合成関数。
pub trait BitmapCompositor: Send + Sync {
    fn compose(&self, bitmap_a: &RgbaBitmap, bitmap_b: &RgbaBitmap) -> RgbaBitmap;
}

#[derive(Clone)]
pub enum BitmapComposite {
    SourceOver,
    Multiply,
    Custom(Arc<dyn BitmapCompositor>),
}

impl fmt::Debug for BitmapComposite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceOver => f.write_str("SourceOver"),
            Self::Multiply => f.write_str("Multiply"),
            Self::Custom(_) => f.write_str("Custom"),
        }
    }
}

impl BitmapComposite {
    pub fn source_over() -> Self {
        Self::SourceOver
    }

    pub fn multiply() -> Self {
        Self::Multiply
    }

    pub fn custom(compositor: impl BitmapCompositor + 'static) -> Self {
        Self::Custom(Arc::new(compositor))
    }

    pub fn compose(&self, bitmap_a: &RgbaBitmap, bitmap_b: &RgbaBitmap) -> RgbaBitmap {
        match self {
            Self::SourceOver => compose_bitmaps(bitmap_a, bitmap_b, &BlendMode::Normal),
            Self::Multiply => compose_bitmaps(bitmap_a, bitmap_b, &BlendMode::Multiply),
            Self::Custom(compositor) => compositor.compose(bitmap_a, bitmap_b),
        }
    }
}

/// 描画プラグインが返すビットマップ更新要求。
/// 更新が必要な矩形領域と、更新内容を表すビットマップ、合成方法を指定する。
#[derive(Debug, Clone)]
pub struct BitmapEdit {
    pub dirty_rect: PageDirtyRect,
    pub bitmap: RgbaBitmap,
    pub composite: BitmapComposite,
}

impl BitmapEdit {
    pub fn new(
        dirty_rect: PageDirtyRect,
        bitmap: RgbaBitmap,
        composite: BitmapComposite,
    ) -> Self {
        Self {
            dirty_rect,
            bitmap,
            composite,
        }
    }
}

/// `incoming` を `previous` の上に `mode` で重ねた結果ビットマップを返す。
///
/// 両ビットマップは同サイズを前提とする (異なる場合は小さい方の寸法に切り詰める)。
fn compose_bitmaps(
    incoming: &RgbaBitmap,
    previous: &RgbaBitmap,
    mode: &BlendMode,
) -> RgbaBitmap {
    let width = incoming.width.min(previous.width);
    let height = incoming.height.min(previous.height);
    let mut out = RgbaBitmap::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let src = [
                incoming.pixels[index],
                incoming.pixels[index + 1],
                incoming.pixels[index + 2],
                incoming.pixels[index + 3],
            ];
            let dst = [
                previous.pixels[index],
                previous.pixels[index + 1],
                previous.pixels[index + 2],
                previous.pixels[index + 3],
            ];
            let blended = composite_pixel(dst, src, mode);
            out.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
    out
}
