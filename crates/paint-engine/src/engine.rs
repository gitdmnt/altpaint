use app_core::{BitmapEdit, Document, PaintInput};

use crate::{
    BUILTIN_BITMAP_BACKEND_ID, PaintPluginRegistry, build_paint_context, default_paint_plugins,
};

/// `Document` の読み取り状態から bitmap 差分を計算するペイントエンジンを表す。
pub struct PaintEngine {
    registry: PaintPluginRegistry,
}

impl Default for PaintEngine {
    fn default() -> Self {
        Self::new(default_paint_plugins())
    }
}

impl PaintEngine {
    pub fn new(registry: PaintPluginRegistry) -> Self {
        Self { registry }
    }

    /// 描画入力からレイヤーに適用するビットマップ差分を計算して返す (適用はしない)。
    ///
    /// コンテキスト解決に失敗した場合は `None` を返す。
    pub fn compute_paint_edits(
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
            .or_else(|| self.registry.get(BUILTIN_BITMAP_BACKEND_ID))
            .map(|plugin| plugin.process(input, &resolved.context))
            .unwrap_or_default();

        Some(edits)
    }
}
