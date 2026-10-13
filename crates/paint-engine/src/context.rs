use crate::painting::PaintPluginContext;

/// `Document` から解決した描画実行時コンテキストをまとめる。
pub struct ResolvedPaintContext<'a> {
    pub context: PaintPluginContext<'a>,
}
