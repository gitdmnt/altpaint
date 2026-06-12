use std::fmt;
use std::sync::Arc;

use crate::{
    BlendMode, CanvasBitmap, ColorRgba8, PenPreset, ToolKind, ToolSettingDefinition,
};
use geometry::{KomaLocalPoint, PageDirtyRect};

/// 描画プラグインが受け取る最小入力イベント。
#[derive(Debug, Clone, PartialEq)]
pub enum PaintInput {
    Stamp {
        at: KomaLocalPoint,
        pressure: f32,
    },
    StrokeSegment {
        from: KomaLocalPoint,
        to: KomaLocalPoint,
        pressure: f32,
    },
    FloodFill {
        at: KomaLocalPoint,
    },
    LassoFill {
        points: Vec<KomaLocalPoint>,
    },
}

/// 描画プラグインへホストが渡す読み取り専用コンテキスト。
pub struct PaintPluginContext<'a> {
    pub tool: ToolKind,
    pub tool_id: &'a str,
    pub provider_plugin_id: &'a str,
    pub drawing_plugin_id: &'a str,
    pub tool_settings: &'a [ToolSettingDefinition],
    pub color: ColorRgba8,
    pub pen: &'a PenPreset,
    pub resolved_size: u32,
    pub active_layer_bitmap: &'a CanvasBitmap,
    pub composited_bitmap: &'a CanvasBitmap,
    pub active_layer_is_background: bool,
    pub active_layer_index: usize,
    pub layer_count: usize,
}

/// `bitmap_a` と既存 `bitmap_b` から結果ビットマップを作る合成関数。
pub trait BitmapCompositor: Send + Sync {
    fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap;
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

    pub fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
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
    pub bitmap: CanvasBitmap,
    pub composite: BitmapComposite,
}

impl BitmapEdit {
    pub fn new(
        dirty_rect: PageDirtyRect,
        bitmap: CanvasBitmap,
        composite: BitmapComposite,
    ) -> Self {
        Self {
            dirty_rect,
            bitmap,
            composite,
        }
    }
}

/// ペン入力からビットマップ差分を生成する描画プラグイン契約。
pub trait PaintPlugin {
    fn id(&self) -> &'static str;

    fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit>;
}

/// `incoming` を `previous` の上に `mode` で重ねた結果ビットマップを返す。
///
/// 両ビットマップは同サイズを前提とする (異なる場合は小さい方の寸法に切り詰める)。
fn compose_bitmaps(
    incoming: &CanvasBitmap,
    previous: &CanvasBitmap,
    mode: &BlendMode,
) -> CanvasBitmap {
    let width = incoming.width.min(previous.width);
    let height = incoming.height.min(previous.height);
    let mut out = CanvasBitmap::transparent(width, height);
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
            let blended = crate::blend::composite_pixel(dst, src, mode);
            out.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
    out
}
