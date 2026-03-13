use app_core::{
    BitmapEdit, BitmapEditOperation, BitmapEditRecord, Document, PaintInput, PanelId,
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

    /// 描画入力を実行し、ビットマップ差分と操作記録を返す。
    ///
    /// コンテキスト解決に失敗した場合は `None` を返す。
    pub fn execute_paint_input(
        &self,
        document: &Document,
        input: &PaintInput,
    ) -> Option<PaintResult> {
        let resolved = build_paint_context(document, input)?;

        let edits = self
            .registry
            .get(resolved.plugin_id)
            .or_else(|| self.registry.get(STANDARD_BITMAP_PLUGIN_ID))
            .map(|plugin| plugin.process(input, &resolved.context))
            .unwrap_or_default();

        let panel_id = document
            .active_panel()
            .map(|panel| panel.id)
            .unwrap_or(PanelId(0));
        let layer_index = resolved.context.active_layer_index;
        let pen_snapshot = document
            .active_pen_preset()
            .cloned()
            .unwrap_or_default();
        let color_snapshot = document.active_color;
        let tool_id = document.active_tool_id.clone();

        let record = BitmapEditRecord {
            panel_id,
            layer_index,
            operation: BitmapEditOperation::from_paint_input(input),
            pen_snapshot,
            color_snapshot,
            tool_id,
        };

        Some(PaintResult { edits, record })
    }
}
