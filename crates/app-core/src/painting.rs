use std::fmt;
use std::sync::Arc;

use crate::{
    CanvasBitmap, CanvasDirtyRect, ColorRgba8, PanelLocalPoint, PenPreset, ToolKind,
};

/// 描画プラグインが受け取る最小入力イベント。
#[derive(Debug, Clone, PartialEq)]
pub enum PaintInput {
    Stamp {
        at: PanelLocalPoint,
        pressure: f32,
    },
    StrokeSegment {
        from: PanelLocalPoint,
        to: PanelLocalPoint,
        pressure: f32,
    },
    FloodFill {
        at: PanelLocalPoint,
    },
    LassoFill {
        points: Vec<PanelLocalPoint>,
    },
}

/// 描画プラグインへホストが渡す読み取り専用コンテキスト。
pub struct PaintPluginContext<'a> {
    pub tool: ToolKind,
    pub color: ColorRgba8,
    pub pen: &'a PenPreset,
    pub resolved_size: u32,
    pub active_layer_bitmap: &'a CanvasBitmap,
    pub composited_bitmap: &'a CanvasBitmap,
    pub active_layer_is_background: bool,
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
            Self::SourceOver => source_over_bitmap(bitmap_a, bitmap_b),
            Self::Multiply => multiply_bitmap(bitmap_a, bitmap_b),
            Self::Custom(compositor) => compositor.compose(bitmap_a, bitmap_b),
        }
    }
}

/// 描画プラグインが返すビットマップ更新要求。
#[derive(Debug, Clone)]
pub struct BitmapEdit {
    pub dirty_rect: CanvasDirtyRect,
    pub bitmap: CanvasBitmap,
    pub composite: BitmapComposite,
}

impl BitmapEdit {
    pub fn new(dirty_rect: CanvasDirtyRect, bitmap: CanvasBitmap, composite: BitmapComposite) -> Self {
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

fn source_over_bitmap(bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
    let width = bitmap_a.width.min(bitmap_b.width);
    let height = bitmap_a.height.min(bitmap_b.height);
    let mut out = CanvasBitmap::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let incoming = [
                bitmap_a.pixels[index],
                bitmap_a.pixels[index + 1],
                bitmap_a.pixels[index + 2],
                bitmap_a.pixels[index + 3],
            ];
            let previous = [
                bitmap_b.pixels[index],
                bitmap_b.pixels[index + 1],
                bitmap_b.pixels[index + 2],
                bitmap_b.pixels[index + 3],
            ];
            let blended = source_over_pixel(previous, incoming);
            out.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
    out
}

fn multiply_bitmap(bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
    let width = bitmap_a.width.min(bitmap_b.width);
    let height = bitmap_a.height.min(bitmap_b.height);
    let mut out = CanvasBitmap::transparent(width, height);
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            let incoming = [
                bitmap_a.pixels[index],
                bitmap_a.pixels[index + 1],
                bitmap_a.pixels[index + 2],
                bitmap_a.pixels[index + 3],
            ];
            let previous = [
                bitmap_b.pixels[index],
                bitmap_b.pixels[index + 1],
                bitmap_b.pixels[index + 2],
                bitmap_b.pixels[index + 3],
            ];
            let blended = multiply_pixel(previous, incoming);
            out.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
    out
}

fn source_over_pixel(previous: [u8; 4], incoming: [u8; 4]) -> [u8; 4] {
    let src_a = incoming[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return previous;
    }
    let dst_a = previous[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0_u8; 4];
    for channel in 0..3 {
        let src = incoming[channel] as f32 / 255.0;
        let dst = previous[channel] as f32 / 255.0;
        let value = src * src_a + dst * (1.0 - src_a);
        out[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}

fn multiply_pixel(previous: [u8; 4], incoming: [u8; 4]) -> [u8; 4] {
    let src_a = incoming[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return previous;
    }
    let dst_a = previous[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0_u8; 4];
    for channel in 0..3 {
        let src = incoming[channel] as f32 / 255.0;
        let dst = previous[channel] as f32 / 255.0;
        let multiplied = src * dst;
        let value = multiplied * src_a + dst * (1.0 - src_a);
        out[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}
