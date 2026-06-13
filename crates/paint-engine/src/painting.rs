//! ペイント入力イベントと描画コンテキスト。
//!
//! 旧 `app-core::painting` から `paint-engine` へ移設した (BL-074/B5)。R5 で
//! `PaintPlugin` trait + registry を撤去し、ペイント実行は `PaintBackend` 体系
//! (desktop features/paint) と CPU 参照実装 (`ops::compute_bitmap_edits`) に分離した。

use editor_state::{ColorRgba8, PenPreset, ToolKind, ToolSettingDefinition};
use geometry::KomaLocalPoint;
use raster::RgbaBitmap as CanvasBitmap;

/// 唯一の組み込み描画バックエンド id (R6: 旧 `STANDARD_BITMAP_PLUGIN_ID`)。
///
/// ツール定義の `drawing_plugin_id` がこの値を指す。registry lookup は撤去済み
/// (R5) で、現状は単一バックエンドの識別子としてのみ存在する。
pub const BUILTIN_BITMAP_BACKEND_ID: &str = "builtin.bitmap";

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
