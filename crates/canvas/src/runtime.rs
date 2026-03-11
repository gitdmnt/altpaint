use app_core::{BitmapEdit, Document, PaintInput};

use crate::{
    PaintPluginRegistry, STANDARD_BITMAP_PLUGIN_ID, build_paint_context, default_paint_plugins,
};

/// `Document` の読み取り状態から bitmap 差分を生成する描画ランタイムを表す。
pub struct CanvasRuntime {
    registry: PaintPluginRegistry,
}

impl Default for CanvasRuntime {
    fn default() -> Self {
        Self::new(default_paint_plugins())
    }
}

impl CanvasRuntime {
    pub fn new(registry: PaintPluginRegistry) -> Self {
        Self { registry }
    }

    pub fn execute_paint_input(
        &self,
        document: &Document,
        input: &PaintInput,
    ) -> Vec<BitmapEdit> {
        let Some(resolved) = build_paint_context(document, input) else {
            return Vec::new();
        };

        self.registry
            .get(resolved.plugin_id)
            .or_else(|| self.registry.get(STANDARD_BITMAP_PLUGIN_ID))
            .map(|plugin| plugin.process(input, &resolved.context))
            .unwrap_or_default()
    }
}
