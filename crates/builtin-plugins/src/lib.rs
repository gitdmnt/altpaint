mod app_actions;
mod layers_panel;
mod tool_palette;

pub use app_actions::AppActionsPlugin;
pub use layers_panel::{LayersPanelPlugin, LayersPanelSnapshot};
pub use tool_palette::{ToolPalettePlugin, ToolPaletteSnapshot};

use plugin_api::PanelPlugin;

pub fn default_builtin_panels() -> Vec<Box<dyn PanelPlugin>> {
    let mut panels: Vec<Box<dyn PanelPlugin>> = Vec::new();
    let app_actions: Box<dyn PanelPlugin> = Box::new(AppActionsPlugin::default());
    let tool_palette: Box<dyn PanelPlugin> = Box::new(ToolPalettePlugin::default());
    let layers_panel: Box<dyn PanelPlugin> = Box::new(LayersPanelPlugin::default());
    panels.push(app_actions);
    panels.push(tool_palette);
    panels.push(layers_panel);
    panels
}
