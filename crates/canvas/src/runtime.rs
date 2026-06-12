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

    /// 描画入力を実行し、レイヤーに適用するビットマップ差分を返す。
    ///
    /// コンテキスト解決に失敗した場合は `None` を返す。
    pub fn execute_paint_input(
        &self,
        document: &Document,
        input: &PaintInput,
    ) -> Option<Vec<BitmapEdit>> {
        // コンテキストを取得
        let resolved = build_paint_context(document, input)?;

        // レジストリから描画プラグインを取得する。プラグインを用いて入力操作からビットマップ差分のベクトルを生成する。
        let edits = self
            .registry
            .get(resolved.plugin_id)
            .or_else(|| self.registry.get(STANDARD_BITMAP_PLUGIN_ID))
            .map(|plugin| plugin.process(input, &resolved.context))
            .unwrap_or_default();

        Some(edits)
    }
}
