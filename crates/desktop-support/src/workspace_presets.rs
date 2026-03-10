use std::path::{Path, PathBuf};

use app_core::{
    WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
    WorkspacePanelState,
};
use serde::{Deserialize, Serialize};
use workspace_persistence::WorkspaceUiState;

const CURRENT_WORKSPACE_PRESET_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspacePreset {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub ui_state: WorkspaceUiState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspacePresetCatalog {
    #[serde(default = "default_workspace_preset_format_version")]
    pub format_version: u32,
    #[serde(default)]
    pub default_preset_id: String,
    #[serde(default)]
    pub presets: Vec<WorkspacePreset>,
}

fn default_workspace_preset_format_version() -> u32 {
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION
}

pub fn default_workspace_preset_path() -> PathBuf {
    PathBuf::from("workspace-presets.json")
}

pub fn default_workspace_preset_catalog() -> WorkspacePresetCatalog {
    WorkspacePresetCatalog {
        format_version: CURRENT_WORKSPACE_PRESET_FORMAT_VERSION,
        default_preset_id: "default-floating".to_string(),
        presets: vec![WorkspacePreset {
            id: "default-floating".to_string(),
            label: "Default floating workspace".to_string(),
            ui_state: WorkspaceUiState::new(
                WorkspaceLayout {
                    panels: vec![
                        panel_state(
                            "builtin.workspace-layout",
                            true,
                            WorkspacePanelAnchor::TopLeft,
                            24,
                            72,
                            320,
                            280,
                        ),
                        panel_state(
                            "builtin.tool-palette",
                            true,
                            WorkspacePanelAnchor::TopLeft,
                            24,
                            384,
                            300,
                            280,
                        ),
                        panel_state(
                            "builtin.app-actions",
                            true,
                            WorkspacePanelAnchor::TopLeft,
                            356,
                            72,
                            320,
                            240,
                        ),
                        panel_state(
                            "builtin.workspace-presets",
                            true,
                            WorkspacePanelAnchor::TopRight,
                            24,
                            616,
                            320,
                            180,
                        ),
                        panel_state(
                            "builtin.layers-panel",
                            true,
                            WorkspacePanelAnchor::TopRight,
                            24,
                            72,
                            320,
                            320,
                        ),
                        panel_state(
                            "builtin.color-palette",
                            true,
                            WorkspacePanelAnchor::BottomLeft,
                            24,
                            24,
                            320,
                            320,
                        ),
                        panel_state(
                            "builtin.pen-settings",
                            true,
                            WorkspacePanelAnchor::BottomRight,
                            24,
                            24,
                            320,
                            260,
                        ),
                        panel_state(
                            "builtin.view-controls",
                            true,
                            WorkspacePanelAnchor::BottomRight,
                            376,
                            24,
                            320,
                            260,
                        ),
                        panel_state(
                            "builtin.job-progress",
                            true,
                            WorkspacePanelAnchor::BottomLeft,
                            376,
                            24,
                            280,
                            180,
                        ),
                        panel_state(
                            "builtin.snapshot-panel",
                            true,
                            WorkspacePanelAnchor::TopRight,
                            24,
                            424,
                            280,
                            180,
                        ),
                    ],
                },
                Default::default(),
            ),
        }],
    }
}

fn panel_state(
    id: &str,
    visible: bool,
    anchor: WorkspacePanelAnchor,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> WorkspacePanelState {
    WorkspacePanelState {
        id: id.to_string(),
        visible,
        anchor,
        position: Some(WorkspacePanelPosition { x, y }),
        size: Some(WorkspacePanelSize { width, height }),
    }
}

pub fn load_workspace_preset_catalog(path: impl AsRef<Path>) -> WorkspacePresetCatalog {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return default_workspace_preset_catalog(),
    };
    serde_json::from_slice::<WorkspacePresetCatalog>(&bytes)
        .ok()
        .filter(|catalog| {
            catalog.format_version == CURRENT_WORKSPACE_PRESET_FORMAT_VERSION
                && !catalog.presets.is_empty()
        })
        .unwrap_or_else(default_workspace_preset_catalog)
}

pub fn save_workspace_preset_catalog(
    path: impl AsRef<Path>,
    catalog: &WorkspacePresetCatalog,
) -> std::io::Result<()> {
    let serialized = serde_json::to_vec_pretty(catalog)?;
    std::fs::write(path, serialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn default_workspace_preset_catalog_contains_anchor_based_layout() {
        let catalog = default_workspace_preset_catalog();
        let default = catalog
            .presets
            .iter()
            .find(|preset| preset.id == catalog.default_preset_id)
            .expect("default preset exists");
        let layers = default
            .ui_state
            .workspace_layout
            .panels
            .iter()
            .find(|panel| panel.id == "builtin.layers-panel")
            .expect("layers preset exists");

        assert_eq!(layers.anchor, WorkspacePanelAnchor::TopRight);
        assert_eq!(layers.position, Some(WorkspacePanelPosition { x: 24, y: 72 }));
    }

    #[test]
    fn workspace_preset_catalog_roundtrip_preserves_default_preset() {
        let path = unique_test_path("workspace-presets");
        let catalog = default_workspace_preset_catalog();

        save_workspace_preset_catalog(&path, &catalog).expect("save should succeed");
        let loaded = load_workspace_preset_catalog(&path);

        assert_eq!(loaded, catalog);

        let _ = std::fs::remove_file(path);
    }
}