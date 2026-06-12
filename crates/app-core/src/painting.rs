use crate::{ColorRgba8, PenPreset, ToolKind, ToolSettingDefinition};
use geometry::KomaLocalPoint;
use raster::{BitmapEdit, RgbaBitmap as CanvasBitmap};

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

/// ペン入力からビットマップ差分を生成する描画プラグイン契約。
pub trait PaintPlugin {
    fn id(&self) -> &'static str;

    fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit>;
}
