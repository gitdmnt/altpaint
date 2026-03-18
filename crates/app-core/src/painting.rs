use std::fmt;
use std::sync::Arc;

use crate::{
    CanvasBitmap, CanvasDirtyRect, ColorRgba8, PanelLocalPoint, PenPreset, ToolKind,
    ToolSettingDefinition,
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
    /// 合成 を計算して返す。
    fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap;
}

#[derive(Clone)]
pub enum BitmapComposite {
    SourceOver,
    Multiply,
    Custom(Arc<dyn BitmapCompositor>),
}

impl fmt::Debug for BitmapComposite {
    /// 入力値を束ねた新しいインスタンスを生成する。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceOver => f.write_str("SourceOver"),
            Self::Multiply => f.write_str("Multiply"),
            Self::Custom(_) => f.write_str("Custom"),
        }
    }
}

impl BitmapComposite {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn source_over() -> Self {
        Self::SourceOver
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn multiply() -> Self {
        Self::Multiply
    }

    /// custom を計算して返す。
    pub fn custom(compositor: impl BitmapCompositor + 'static) -> Self {
        Self::Custom(Arc::new(compositor))
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
        match self {
            Self::SourceOver => source_over_bitmap(bitmap_a, bitmap_b),
            Self::Multiply => multiply_bitmap(bitmap_a, bitmap_b),
            Self::Custom(compositor) => compositor.compose(bitmap_a, bitmap_b),
        }
    }
}

/// 描画プラグインが返すビットマップ更新要求。
/// 更新が必要な矩形領域と、更新内容を表すビットマップ、合成方法を指定する。
#[derive(Debug, Clone)]
pub struct BitmapEdit {
    pub dirty_rect: CanvasDirtyRect,
    pub bitmap: CanvasBitmap,
    pub composite: BitmapComposite,
}

impl BitmapEdit {
    /// 入力値を束ねた新しいインスタンスを生成する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn new(
        dirty_rect: CanvasDirtyRect,
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

/// 単一描画操作の種別と座標パラメータ。
///
/// `PaintInput` と 1 対 1 で対応する。Undo/Redo の replay 方式で使用する。
#[derive(Debug, Clone, PartialEq)]
pub enum BitmapEditOperation {
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

impl BitmapEditOperation {
    /// `PaintInput` から操作記録を生成する。
    pub fn from_paint_input(input: &PaintInput) -> Self {
        match input {
            PaintInput::Stamp { at, pressure } => Self::Stamp {
                at: *at,
                pressure: *pressure,
            },
            PaintInput::StrokeSegment { from, to, pressure } => Self::StrokeSegment {
                from: *from,
                to: *to,
                pressure: *pressure,
            },
            PaintInput::FloodFill { at } => Self::FloodFill { at: *at },
            PaintInput::LassoFill { points } => Self::LassoFill {
                points: points.clone(),
            },
        }
    }

    /// 操作記録を `PaintInput` へ変換する。
    pub fn to_paint_input(&self) -> PaintInput {
        match self {
            Self::Stamp { at, pressure } => PaintInput::Stamp {
                at: *at,
                pressure: *pressure,
            },
            Self::StrokeSegment { from, to, pressure } => PaintInput::StrokeSegment {
                from: *from,
                to: *to,
                pressure: *pressure,
            },
            Self::FloodFill { at } => PaintInput::FloodFill { at: *at },
            Self::LassoFill { points } => PaintInput::LassoFill {
                points: points.clone(),
            },
        }
    }
}

/// 描画操作の完全な記録。
///
/// panel・layer・操作パラメータ・ペン状態・色を保持し、
/// 同じ状態で replay できるようにする。
#[derive(Debug, Clone)]
pub struct BitmapEditRecord {
    pub panel_id: crate::PanelId,
    pub layer_index: usize,
    pub operation: BitmapEditOperation,
    pub pen_snapshot: crate::PenPreset,
    pub color_snapshot: ColorRgba8,
    pub tool_id: String,
}

/// ペン入力からビットマップ差分を生成する描画プラグイン契約。
pub trait PaintPlugin {
    /// ピクセル走査を行い、ID 用のビットマップ結果を生成する。
    fn id(&self) -> &'static str;

    /// ピクセル走査を行い、process 用のビットマップ結果を生成する。
    fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit>;
}

/// ピクセル走査を行い、ソース over ビットマップ 用のビットマップ結果を生成する。
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

/// ピクセル走査を行い、multiply ビットマップ 用のビットマップ結果を生成する。
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

/// ソース over ピクセル に対応するビットマップ処理を行う。
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

/// Multiply ピクセル に対応するビットマップ処理を行う。
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
