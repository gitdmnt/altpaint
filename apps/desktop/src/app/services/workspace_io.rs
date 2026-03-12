use std::path::PathBuf;

use desktop_support::{WorkspacePreset, WorkspacePresetCatalog, save_workspace_preset_catalog};
use panel_api::{ServiceRequest, services::names};

use super::DesktopApp;

impl DesktopApp {
    pub(super) fn handle_workspace_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::WORKSPACE_RELOAD_PRESETS => self.reload_workspace_presets(),
            names::WORKSPACE_APPLY_PRESET => {
                self.apply_workspace_preset(request.string("preset_id")?)
            }
            names::WORKSPACE_SAVE_PRESET => {
                self.save_workspace_preset(request.string("preset_id")?, request.string("label")?)
            }
            names::WORKSPACE_EXPORT_PRESET => {
                self.export_workspace_preset(request.string("preset_id")?, request.string("label")?)
            }
            names::WORKSPACE_EXPORT_PRESET_TO_PATH => self.export_workspace_preset_to_path(
                request.string("preset_id")?,
                request.string("label")?,
                PathBuf::from(request.string("path")?),
            ),
            _ => return None,
        };
        Some(changed)
    }

    pub(crate) fn apply_workspace_preset(&mut self, preset_id: &str) -> bool {
        let Some(preset) = self
            .workspace_presets
            .presets
            .iter()
            .find(|preset| preset.id == preset_id)
            .cloned()
        else {
            let message = format!("workspace preset not found: {preset_id}");
            eprintln!("{message}");
            self.io_state
                .dialogs
                .show_error("Workspace load failed", &message);
            return false;
        };

        self.active_workspace_preset_id = preset.id;
        self.workspace_presets.default_preset_id = self.active_workspace_preset_id.clone();
        self.apply_workspace_ui_state(preset.ui_state);
        self.persist_workspace_preset_catalog();
        true
    }

    pub(crate) fn save_workspace_preset(&mut self, preset_id: &str, label: &str) -> bool {
        let preset_id = preset_id.trim();
        let label = label.trim();
        if preset_id.is_empty() || label.is_empty() {
            self.io_state.dialogs.show_error(
                "Workspace save failed",
                "workspace preset id and label are required",
            );
            return false;
        }

        let ui_state = self.capture_workspace_ui_state();
        if let Some(existing) = self
            .workspace_presets
            .presets
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        {
            existing.label = label.to_string();
            existing.ui_state = ui_state;
        } else {
            self.workspace_presets.presets.push(WorkspacePreset {
                id: preset_id.to_string(),
                label: label.to_string(),
                ui_state,
            });
        }

        self.active_workspace_preset_id = preset_id.to_string();
        self.workspace_presets.default_preset_id = self.active_workspace_preset_id.clone();
        if let Err(error) = save_workspace_preset_catalog(
            &self.io_state.workspace_preset_path,
            &self.workspace_presets,
        ) {
            let message = format!("failed to save workspace preset catalog: {error}");
            eprintln!("{message}");
            self.io_state
                .dialogs
                .show_error("Workspace save failed", &message);
            return false;
        }

        self.refresh_workspace_presets();
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        self.persist_session_state();
        true
    }

    pub(crate) fn export_workspace_preset(&mut self, preset_id: &str, label: &str) -> bool {
        let suggested = self
            .io_state
            .workspace_preset_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{preset_id}.altp-workspace.json"));
        let Some(path) = self
            .io_state
            .dialogs
            .pick_save_workspace_preset_path(&suggested)
        else {
            return false;
        };
        self.export_workspace_preset_to_path(preset_id, label, path)
    }

    pub(crate) fn export_workspace_preset_to_path(
        &mut self,
        preset_id: &str,
        label: &str,
        path: PathBuf,
    ) -> bool {
        let preset_id = preset_id.trim();
        let label = label.trim();
        if preset_id.is_empty() || label.is_empty() {
            self.io_state.dialogs.show_error(
                "Workspace export failed",
                "workspace preset id and label are required",
            );
            return false;
        }

        let catalog = WorkspacePresetCatalog {
            format_version: self.workspace_presets.format_version,
            default_preset_id: preset_id.to_string(),
            presets: vec![WorkspacePreset {
                id: preset_id.to_string(),
                label: label.to_string(),
                ui_state: self.capture_workspace_ui_state(),
            }],
        };

        if let Err(error) = save_workspace_preset_catalog(&path, &catalog) {
            let message = format!("failed to export workspace preset: {error}");
            eprintln!("{message}");
            self.io_state
                .dialogs
                .show_error("Workspace export failed", &message);
            return false;
        }

        self.active_workspace_preset_id = preset_id.to_string();
        self.refresh_workspace_presets();
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        true
    }
}
