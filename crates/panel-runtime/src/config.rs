use panel_api::PanelPlugin;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn collect_persistent_panel_configs(
    panels: &[Box<dyn PanelPlugin>],
) -> BTreeMap<String, Value> {
    panels
        .iter()
        .filter_map(|panel| {
            panel
                .persistent_config()
                .map(|config| (panel.id().to_string(), config))
        })
        .collect()
}

pub(crate) fn restore_persistent_panel_configs(
    panels: &mut [Box<dyn PanelPlugin>],
    configs: &BTreeMap<String, Value>,
) {
    for panel in panels {
        if let Some(config) = configs.get(panel.id()) {
            panel.restore_persistent_config(config);
        }
    }
}
