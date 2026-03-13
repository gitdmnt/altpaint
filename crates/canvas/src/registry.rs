use std::collections::BTreeMap;

use app_core::PaintPlugin;

use crate::plugins::builtin_bitmap::BuiltinBitmapPaintPlugin;

pub type PaintPluginRegistry = BTreeMap<String, Box<dyn PaintPlugin>>;

pub const STANDARD_BITMAP_PLUGIN_ID: &str = "builtin.bitmap";

/// 既定の paint plugins を返す。
pub fn default_paint_plugins() -> PaintPluginRegistry {
    let mut plugins: PaintPluginRegistry = BTreeMap::new();
    plugins.insert(
        STANDARD_BITMAP_PLUGIN_ID.to_string(),
        Box::new(BuiltinBitmapPaintPlugin),
    );
    plugins
}
