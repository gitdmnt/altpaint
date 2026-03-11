use app_core::PaintPluginContext;

/// `Document` から解決した描画実行時コンテキストをまとめる。
pub struct ResolvedPaintContext<'a> {
    pub plugin_id: &'a str,
    pub context: PaintPluginContext<'a>,
}
