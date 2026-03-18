use app_core::{
    BitmapEdit, BitmapEditOperation, BitmapEditRecord, Document, PaintInput, PaintPluginContext,
    PanelId, ToolKind,
};

use crate::{
    PaintPluginRegistry, STANDARD_BITMAP_PLUGIN_ID, build_paint_context, default_paint_plugins,
};

/// `execute_paint_input` が返す描画結果。
///
/// `edits` をドキュメントへ適用し、`record` を履歴スタックへ積む。
pub struct PaintResult {
    pub edits: Vec<BitmapEdit>,
    pub record: BitmapEditRecord,
}

/// `Document` の読み取り状態から bitmap 差分を生成する描画ランタイムを表す。
pub struct CanvasRuntime {
    registry: PaintPluginRegistry,
}

impl Default for CanvasRuntime {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new(default_paint_plugins())
    }
}

impl CanvasRuntime {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn new(registry: PaintPluginRegistry) -> Self {
        Self { registry }
    }

    /// 描画入力を実行し、レイヤーに適用するビットマップ差分とundo/redo用の操作記録を返す。
    ///
    /// コンテキスト解決に失敗した場合は `None` を返す。
    pub fn execute_paint_input(
        &self,
        document: &Document,
        input: &PaintInput,
    ) -> Option<(Vec<BitmapEdit>, BitmapEditRecord)> {
        // コンテキストを取得
        let resolved = build_paint_context(document, input)?;

        // レジストリから描画プラグインを取得する。プラグインを用いて入力操作からビットマップ差分のベクトルを生成する。
        let edits = self
            .registry
            .get(resolved.plugin_id)
            .or_else(|| self.registry.get(STANDARD_BITMAP_PLUGIN_ID))
            .map(|plugin| plugin.process(input, &resolved.context))
            .unwrap_or_default();

        // 書き込むためのレイヤーを取得。
        let panel_id = document
            .active_panel()
            .map(|panel| panel.id)
            .unwrap_or(PanelId(0));
        let layer_index = resolved.context.active_layer_index;

        // ペンの状態と色を取得。
        let pen_snapshot = document.active_pen_preset().cloned().unwrap_or_default();
        let color_snapshot = document.active_color;
        let tool_id = document.active_tool_id.clone();

        // 操作記録を生成。操作記録は履歴スタックに積まれ、undo/redo 時に再適用される。
        let record = BitmapEditRecord {
            panel_id,
            layer_index,
            operation: BitmapEditOperation::from_paint_input(input),
            pen_snapshot,
            color_snapshot,
            tool_id,
        };

        Some((edits, record))
    }

    /// 操作記録を replay してビットマップ差分を生成する。
    ///
    /// undo 時に過去操作を再適用するために使う。
    /// `page_index` / `panel_index` は `Document::find_panel_location` の結果を渡す。
    pub fn replay_paint_record(
        &self,
        document: &Document,
        page_index: usize,
        panel_index: usize,
        record: &BitmapEditRecord,
    ) -> Vec<BitmapEdit> {
        let Some(page) = document.work.pages.get(page_index) else {
            return Vec::new();
        };
        let Some(panel) = page.panels.get(panel_index) else {
            return Vec::new();
        };
        let layer_index = record.layer_index.min(panel.layers.len().saturating_sub(1));
        let Some(layer) = panel.layers.get(layer_index) else {
            return Vec::new();
        };
        let active_layer_bitmap = &layer.bitmap;
        let composited_bitmap = &panel.bitmap;

        let tool = document
            .tool_catalog
            .iter()
            .find(|t| t.id == record.tool_id);

        let drawing_plugin_id = tool
            .map(|t| t.drawing_plugin_id.as_str())
            .unwrap_or(STANDARD_BITMAP_PLUGIN_ID);

        let input = record.operation.to_paint_input();
        let resolved_size = resolved_size_from_record(record, &input);

        let context = PaintPluginContext {
            tool: tool.map(|t| t.kind).unwrap_or(ToolKind::Pen),
            tool_id: record.tool_id.as_str(),
            provider_plugin_id: tool.map(|t| t.provider_plugin_id.as_str()).unwrap_or(""),
            drawing_plugin_id,
            tool_settings: tool.map(|t| t.settings.as_slice()).unwrap_or(&[]),
            color: record.color_snapshot,
            pen: &record.pen_snapshot,
            resolved_size,
            active_layer_bitmap,
            composited_bitmap,
            active_layer_is_background: layer_index == 0,
            active_layer_index: layer_index,
            layer_count: panel.layers.len(),
        };

        self.registry
            .get(drawing_plugin_id)
            .or_else(|| self.registry.get(STANDARD_BITMAP_PLUGIN_ID))
            .map(|plugin| plugin.process(&input, &context))
            .unwrap_or_default()
    }
}

/// `BitmapEditRecord` の pen snapshot からブラシサイズを解決する。
fn resolved_size_from_record(record: &BitmapEditRecord, input: &PaintInput) -> u32 {
    match input {
        PaintInput::Stamp { pressure, .. } | PaintInput::StrokeSegment { pressure, .. } => {
            let base = record.pen_snapshot.size.max(1);
            if !record.pen_snapshot.pressure_enabled {
                return base;
            }
            let clamped = pressure.clamp(0.0, 1.0);
            let scaled = (base as f32 * (0.2 + clamped * 0.8)).round() as u32;
            scaled.max(1)
        }
        PaintInput::FloodFill { .. } | PaintInput::LassoFill { .. } => {
            record.pen_snapshot.size.max(1)
        }
    }
}
