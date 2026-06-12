pub mod builtin_bitmap;

use std::collections::BTreeMap;

use app_core::PaintPlugin;

use builtin_bitmap::BuiltinBitmapPaintPlugin;

pub type PaintPluginRegistry = BTreeMap<String, Box<dyn PaintPlugin>>;

pub const BUILTIN_BITMAP_BACKEND_ID: &str = "builtin.bitmap";

pub fn default_paint_plugins() -> PaintPluginRegistry {
    let mut plugins: PaintPluginRegistry = BTreeMap::new();
    plugins.insert(
        BUILTIN_BITMAP_BACKEND_ID.to_string(),
        Box::new(BuiltinBitmapPaintPlugin),
    );
    plugins
}
