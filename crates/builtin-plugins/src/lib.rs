mod app_actions;
mod job_progress;
mod layers_panel;
mod snapshot_panel;
mod tool_palette;

pub use app_actions::AppActionsPlugin;
pub use job_progress::{JobProgressPanelPlugin, JobProgressSnapshot};
pub use layers_panel::{LayersPanelPlugin, LayersPanelSnapshot};
pub use snapshot_panel::{SnapshotPanelPlugin, SnapshotPanelSnapshot};
pub use tool_palette::{ToolPalettePlugin, ToolPaletteSnapshot};

use plugin_api::PanelPlugin;

pub fn default_builtin_panels() -> Vec<Box<dyn PanelPlugin>> {
    let mut panels: Vec<Box<dyn PanelPlugin>> = Vec::new();
    let app_actions: Box<dyn PanelPlugin> = Box::new(AppActionsPlugin);
    let tool_palette: Box<dyn PanelPlugin> = Box::new(ToolPalettePlugin::default());
    let layers_panel: Box<dyn PanelPlugin> = Box::new(LayersPanelPlugin::default());
    let job_progress: Box<dyn PanelPlugin> = Box::new(JobProgressPanelPlugin::default());
    let snapshot_panel: Box<dyn PanelPlugin> = Box::new(SnapshotPanelPlugin::default());
    panels.push(app_actions);
    panels.push(tool_palette);
    panels.push(layers_panel);
    panels.push(job_progress);
    panels.push(snapshot_panel);
    panels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_builtin_panels_include_phase_five_panels() {
        let panels = default_builtin_panels();
        let ids: Vec<_> = panels.iter().map(|panel| panel.id()).collect();

        assert!(ids.contains(&"builtin.job-progress"));
        assert!(ids.contains(&"builtin.snapshot-panel"));
    }
}
