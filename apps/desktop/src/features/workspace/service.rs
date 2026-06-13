use std::path::PathBuf;

use super::{WorkspacePreset, WorkspacePresetCatalog, save_workspace_preset_catalog};
use panel_runtime::{ServiceRequest, services::names};

use crate::app::DesktopApp;

/// workspace preset service request を処理する。
pub(crate) fn handle_workspace_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::WORKSPACE_RELOAD_PRESETS => app.reload_workspace_presets(),
        names::WORKSPACE_APPLY_PRESET => app.apply_workspace_preset(request.string("preset_id")?),
        names::WORKSPACE_SAVE_PRESET => {
            app.save_workspace_preset(request.string("preset_id")?, request.string("label")?)
        }
        names::WORKSPACE_EXPORT_PRESET => {
            app.export_workspace_preset(request.string("preset_id")?, request.string("label")?)
        }
        names::WORKSPACE_EXPORT_PRESET_TO_PATH => app.export_workspace_preset_to_path(
            request.string("preset_id")?,
            request.string("label")?,
            PathBuf::from(request.string("path")?),
        ),
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
    /// 現在のワークスペース UI 状態 (レイアウト + パネル config) を採取する。
    pub(crate) fn capture_workspace_ui_state(&self) -> panel_workspace::WorkspaceUiState {
        panel_workspace::WorkspaceUiState::new(
            self.panel_workspace.workspace_layout(),
            self.panel_runtime.persistent_panel_configs(),
        )
    }

    pub(crate) fn apply_workspace_preset(&mut self, preset_id: &str) -> bool {
        let Some(preset) = self
            .workspace
            .presets
            .presets
            .iter()
            .find(|preset| preset.id == preset_id)
            .cloned()
        else {
            let message = format!("workspace preset not found: {preset_id}");
            eprintln!("{message}");
            self.dialogs
                .show_error("Workspace load failed", &message);
            return false;
        };

        self.workspace.active_preset_id = preset.id;
        self.workspace.presets.default_preset_id = self.workspace.active_preset_id.clone();
        self.apply_workspace_ui_state(preset.ui_state);
        self.persist_workspace_preset_catalog();
        true
    }

    pub(crate) fn save_workspace_preset(&mut self, preset_id: &str, label: &str) -> bool {
        let preset_id = preset_id.trim();
        let label = label.trim();
        if preset_id.is_empty() || label.is_empty() {
            self.dialogs.show_error(
                "Workspace save failed",
                "workspace preset id and label are required",
            );
            return false;
        }

        let ui_state = self.capture_workspace_ui_state();
        if let Some(existing) = self
            .workspace
            .presets
            .presets
            .iter_mut()
            .find(|preset| preset.id == preset_id)
        {
            existing.label = label.to_string();
            existing.ui_state = ui_state;
        } else {
            self.workspace.presets.presets.push(WorkspacePreset {
                id: preset_id.to_string(),
                label: label.to_string(),
                ui_state,
            });
        }

        self.workspace.active_preset_id = preset_id.to_string();
        self.workspace.presets.default_preset_id = self.workspace.active_preset_id.clone();
        if let Err(error) = save_workspace_preset_catalog(
            &self.paths.workspace_preset_path,
            &self.workspace.presets,
        ) {
            let message = format!("failed to save workspace preset catalog: {error}");
            eprintln!("{message}");
            self.dialogs
                .show_error("Workspace save failed", &message);
            return false;
        }

        self.refresh_workspace_presets();
        self.request_panel_reconcile();
        self.mark_status_dirty();
        self.persist_session_state();
        true
    }

    pub(crate) fn export_workspace_preset(&mut self, preset_id: &str, label: &str) -> bool {
        let suggested = self
            .paths.workspace_preset_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{preset_id}.altp-workspace.json"));
        let Some(path) = self
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
            self.dialogs.show_error(
                "Workspace export failed",
                "workspace preset id and label are required",
            );
            return false;
        }

        let catalog = WorkspacePresetCatalog {
            format_version: self.workspace.presets.format_version,
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
            self.dialogs
                .show_error("Workspace export failed", &message);
            return false;
        }

        self.workspace.active_preset_id = preset_id.to_string();
        self.refresh_workspace_presets();
        self.request_panel_reconcile();
        self.mark_status_dirty();
        true
    }
}
