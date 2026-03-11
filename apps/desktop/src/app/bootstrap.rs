//! `DesktopApp` 構築時の復元と初期化 orchestration を扱う。

use std::path::{Path, PathBuf};

use app_core::Document;
use desktop_support::{
    DEFAULT_PROJECT_PATH, DesktopSessionState, WorkspacePresetCatalog, default_canvas_template_path,
    default_canvas_templates, default_panel_dir, default_workspace_preset_catalog,
    load_session_state, load_workspace_preset_catalog, save_canvas_templates,
    save_workspace_preset_catalog,
};
use panel_runtime::PanelRuntime;
use ui_shell::PanelPresentation;
use workspace_persistence::WorkspaceUiState;

use super::{DesktopApp, panel_config_sync::selected_workspace_preset_id_from_configs};

pub(super) struct BootstrapState {
    pub(super) document: Document,
    pub(super) panel_runtime: PanelRuntime,
    pub(super) panel_presentation: PanelPresentation,
    pub(super) project_path: PathBuf,
    pub(super) workspace_presets: WorkspacePresetCatalog,
    pub(super) active_workspace_preset_id: String,
}

impl DesktopApp {
    pub(super) fn bootstrap_state(
        project_path: PathBuf,
        session_path: &Path,
        workspace_preset_path: &Path,
    ) -> BootstrapState {
        let session = load_session_state(session_path);
        let project_path = resolve_startup_project_path(project_path, session.as_ref());
        let loaded_project = storage::load_project_from_path(&project_path).ok();
        let document = loaded_project
            .as_ref()
            .map(|project| project.document.clone())
            .unwrap_or_default();
        let workspace_presets = load_workspace_preset_catalog(workspace_preset_path);
        let mut active_workspace_preset_id = workspace_presets.default_preset_id.clone();
        let (mut panel_runtime, mut panel_presentation) = Self::load_panel_system(
            &workspace_presets,
            loaded_project.as_ref().map(|project| &project.ui_state),
            session.as_ref().map(|state| &state.ui_state),
        );
        if let Some(selected_preset_id) = selected_workspace_preset_id_from_configs(
            &panel_runtime.persistent_panel_configs(),
        )
        .filter(|preset_id| {
            workspace_presets
                .presets
                .iter()
                .any(|preset| preset.id == *preset_id)
        }) {
            active_workspace_preset_id = selected_preset_id;
        }

        let mut document = document;
        Self::reload_tool_catalog_into_document(&mut document);
        Self::reload_pen_presets_into_document(&mut document);
        let _changed_panels = panel_runtime.sync_document(&document);
        panel_presentation.reconcile_runtime_panels(&panel_runtime);

        BootstrapState {
            document,
            panel_runtime,
            panel_presentation,
            project_path,
            workspace_presets,
            active_workspace_preset_id,
        }
    }

    fn load_panel_system(
        workspace_presets: &WorkspacePresetCatalog,
        project_ui_state: Option<&WorkspaceUiState>,
        session_ui_state: Option<&WorkspaceUiState>,
    ) -> (PanelRuntime, PanelPresentation) {
        let mut panel_runtime = PanelRuntime::new();
        let mut panel_presentation = PanelPresentation::new();
        let _ = panel_runtime.load_panel_directory(default_panel_dir());
        panel_presentation.reconcile_runtime_panels(&panel_runtime);

        if let Some(default_preset) = workspace_presets
            .presets
            .iter()
            .find(|preset| preset.id == workspace_presets.default_preset_id)
        {
            apply_ui_state_to_panel_system(&mut panel_runtime, &mut panel_presentation, &default_preset.ui_state);
        }
        if let Some(project_ui_state) = project_ui_state {
            apply_ui_state_to_panel_system(&mut panel_runtime, &mut panel_presentation, project_ui_state);
        }
        if let Some(session_ui_state) = session_ui_state {
            apply_ui_state_to_panel_system(&mut panel_runtime, &mut panel_presentation, session_ui_state);
        }
        (panel_runtime, panel_presentation)
    }

    pub(super) fn apply_workspace_ui_state(&mut self, ui_state: WorkspaceUiState) {
        let (workspace_layout, plugin_configs) = ui_state.into_parts();
        self.panel_presentation.replace_workspace_layout(workspace_layout);
        self.panel_runtime.replace_persistent_panel_configs(plugin_configs);
        self.panel_presentation.reconcile_runtime_panels(&self.panel_runtime);
        self.refresh_new_document_templates();
        self.refresh_workspace_presets();
        self.reset_active_interactions();
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        self.rebuild_present_frame();
        self.persist_session_state();
    }

    pub(super) fn ensure_canvas_templates_file(&self) {
        let path = default_canvas_template_path();
        if path.exists() {
            return;
        }

        if let Err(error) = save_canvas_templates(&path, &default_canvas_templates()) {
            eprintln!("failed to create canvas templates file: {error}");
        }
    }

    pub(super) fn ensure_workspace_presets_file(&self, path: &Path) {
        if path.exists() {
            return;
        }

        if let Err(error) = save_workspace_preset_catalog(path, &default_workspace_preset_catalog()) {
            eprintln!("failed to create workspace presets file: {error}");
        }
    }

    pub(super) fn persist_workspace_preset_catalog(&self) {
        if let Err(error) =
            save_workspace_preset_catalog(&self.io_state.workspace_preset_path, &self.workspace_presets)
        {
            let message = format!("failed to persist workspace preset catalog: {error}");
            eprintln!("{message}");
            self.io_state
                .dialogs
                .show_error("Workspace save failed", &message);
        }
    }
}

fn resolve_startup_project_path(
    project_path: PathBuf,
    session: Option<&DesktopSessionState>,
) -> PathBuf {
    session
        .and_then(|state| {
            (project_path == Path::new(DEFAULT_PROJECT_PATH))
                .then(|| state.last_project_path.clone())
                .flatten()
        })
        .unwrap_or(project_path)
}

fn apply_ui_state_to_panel_system(
    panel_runtime: &mut PanelRuntime,
    panel_presentation: &mut PanelPresentation,
    ui_state: &WorkspaceUiState,
) {
    if !ui_state.workspace_layout.panels.is_empty() {
        panel_presentation.replace_workspace_layout(ui_state.workspace_layout.clone());
    }
    if !ui_state.plugin_configs.is_empty() {
        panel_runtime.replace_persistent_panel_configs(ui_state.plugin_configs.clone());
    }
    panel_presentation.reconcile_runtime_panels(panel_runtime);
}
