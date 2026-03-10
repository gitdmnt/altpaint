//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod commands;
mod input;
mod present;
mod state;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::Document;
use desktop_support::{
    DEFAULT_PROJECT_PATH, DesktopDialogs, NativeDesktopDialogs, default_canvas_template_path,
    default_canvas_templates, default_panel_dir, default_workspace_preset_catalog,
    default_workspace_preset_path, load_canvas_templates, load_session_state,
    load_workspace_preset_catalog, save_canvas_templates, save_workspace_preset_catalog,
};
use render::RenderFrame;
use serde_json::{Map, Value, json};
use ui_shell::{PanelSurface, UiShell};
use workspace_persistence::WorkspaceUiState;

use self::state::{PanelDragState, PanelPressState, PendingSaveTask, PresentFrameUpdate};
use crate::canvas_bridge::CanvasInputState;
use crate::frame::DesktopLayout;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// ランタイムから利用されるデスクトップアプリ本体を表す。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) ui_shell: UiShell,
    pub(crate) project_path: PathBuf,
    session_path: PathBuf,
    workspace_preset_path: PathBuf,
    workspace_presets: desktop_support::WorkspacePresetCatalog,
    active_workspace_preset_id: String,
    dialogs: Box<dyn DesktopDialogs>,
    canvas_input: CanvasInputState,
    pub(crate) panel_surface: Option<PanelSurface>,
    pub(crate) layout: Option<DesktopLayout>,
    base_frame: Option<RenderFrame>,
    overlay_frame: Option<RenderFrame>,
    pending_canvas_dirty_rect: Option<app_core::DirtyRect>,
    pending_canvas_background_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_host_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_transform_update: bool,
    active_panel_drag: Option<PanelDragState>,
    pending_panel_press: Option<PanelPressState>,
    hover_canvas_position: Option<(usize, usize)>,
    needs_ui_sync: bool,
    ui_sync_panel_ids: BTreeSet<String>,
    needs_panel_surface_refresh: bool,
    needs_status_refresh: bool,
    needs_full_present_rebuild: bool,
    pending_save_tasks: Vec<PendingSaveTask>,
}

impl DesktopApp {
    /// 既定ダイアログ実装付きのアプリ本体を生成する。
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            Box::new(NativeDesktopDialogs),
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    /// ダイアログ実装を差し替えてアプリ本体を生成する。
    #[allow(dead_code)]
    pub(crate) fn new_with_dialogs(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
    ) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            dialogs,
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    /// ダイアログ実装・セッション保存先・workspace preset を差し替えて生成する。
    pub(crate) fn new_with_dialogs_session_path_and_workspace_preset_path(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
        session_path: PathBuf,
        workspace_preset_path: PathBuf,
    ) -> Self {
        let session = load_session_state(&session_path);
        let project_path = session
            .as_ref()
            .and_then(|state| {
                (project_path == std::path::Path::new(DEFAULT_PROJECT_PATH))
                    .then(|| state.last_project_path.clone())
                    .flatten()
            })
            .unwrap_or(project_path);
        let loaded_project = storage::load_project_from_path(&project_path).ok();
        let document = loaded_project
            .as_ref()
            .map(|project| project.document.clone())
            .unwrap_or_default();
        let mut ui_shell = UiShell::new();
        let _ = ui_shell.load_panel_directory(default_panel_dir());
        let preset_catalog = load_workspace_preset_catalog(&workspace_preset_path);
        let active_workspace_preset_id = preset_catalog.default_preset_id.clone();
        if let Some(default_preset) = preset_catalog
            .presets
            .iter()
            .find(|preset| preset.id == preset_catalog.default_preset_id)
        {
            if !default_preset.ui_state.workspace_layout.panels.is_empty() {
                ui_shell.set_workspace_layout(default_preset.ui_state.workspace_layout.clone());
            }
            if !default_preset.ui_state.plugin_configs.is_empty() {
                ui_shell
                    .set_persistent_panel_configs(default_preset.ui_state.plugin_configs.clone());
            }
        }
        if let Some(project) = loaded_project {
            if !project.ui_state.workspace_layout.panels.is_empty() {
                ui_shell.set_workspace_layout(project.ui_state.workspace_layout);
            }
            if !project.ui_state.plugin_configs.is_empty() {
                ui_shell.set_persistent_panel_configs(project.ui_state.plugin_configs);
            }
        }
        if let Some(session) = session.as_ref() {
            if !session.workspace_layout().panels.is_empty() {
                ui_shell.set_workspace_layout(session.workspace_layout().clone());
            }
            if !session.plugin_configs().is_empty() {
                ui_shell.set_persistent_panel_configs(session.plugin_configs().clone());
            }
        }
        let mut document = document;
        Self::reload_pen_presets_into_document(&mut document);
        ui_shell.update(&document);

        let mut app = Self {
            document,
            ui_shell,
            project_path,
            session_path,
            workspace_preset_path,
            workspace_presets: preset_catalog,
            active_workspace_preset_id,
            dialogs,
            canvas_input: CanvasInputState::default(),
            panel_surface: None,
            layout: None,
            base_frame: None,
            overlay_frame: None,
            pending_canvas_dirty_rect: None,
            pending_canvas_background_dirty_rect: None,
            pending_canvas_host_dirty_rect: None,
            pending_canvas_transform_update: false,
            active_panel_drag: None,
            pending_panel_press: None,
            hover_canvas_position: None,
            needs_ui_sync: true,
            ui_sync_panel_ids: BTreeSet::new(),
            needs_panel_surface_refresh: true,
            needs_status_refresh: false,
            needs_full_present_rebuild: true,
            pending_save_tasks: Vec::new(),
        };
        app.ensure_workspace_presets_file(&app.workspace_preset_path);
        app.ensure_canvas_templates_file();
        app.refresh_new_document_templates();
        app.refresh_workspace_presets();
        app
    }

    pub(crate) fn refresh_new_document_templates(&mut self) {
        let templates = load_canvas_templates(default_canvas_template_path());
        let default_template = templates
            .first()
            .cloned()
            .or_else(|| default_canvas_templates().into_iter().next());
        let options = templates
            .iter()
            .map(|template| template.dropdown_option())
            .collect::<Vec<_>>()
            .join("|");

        let mut configs = self.ui_shell.persistent_panel_configs();
        let entry = configs
            .entry("builtin.app-actions".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        object.insert("template_options".to_string(), json!(options));
        object.insert(
            "default_template_size".to_string(),
            json!(
                default_template
                    .as_ref()
                    .map(|template| template.size_string())
                    .unwrap_or_else(|| "2894x4093".to_string())
            ),
        );
        self.ui_shell.set_persistent_panel_configs(configs);
    }

    pub(crate) fn refresh_workspace_presets(&mut self) {
        let options = self
            .workspace_presets
            .presets
            .iter()
            .map(|preset| format!("{}:{}", preset.id, preset.label))
            .collect::<Vec<_>>()
            .join("|");
        let selected_workspace = self.selected_workspace_preset_id();

        let mut configs = self.ui_shell.persistent_panel_configs();
        let entry = configs
            .entry("builtin.app-actions".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        object.insert("workspace_options".to_string(), json!(options));
        object.insert(
            "selected_workspace".to_string(),
            json!(selected_workspace.clone()),
        );
        self.active_workspace_preset_id = selected_workspace;
        self.ui_shell.set_persistent_panel_configs(configs);
    }

    pub(crate) fn reload_workspace_presets(&mut self) -> bool {
        self.workspace_presets = load_workspace_preset_catalog(&self.workspace_preset_path);
        self.refresh_workspace_presets();
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        self.persist_session_state();
        true
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
            self.dialogs.show_error("Workspace load failed", &message);
            return false;
        };

        self.active_workspace_preset_id = preset.id;
        self.apply_workspace_ui_state(preset.ui_state);
        true
    }

    fn apply_workspace_ui_state(&mut self, ui_state: WorkspaceUiState) {
        let (workspace_layout, plugin_configs) = ui_state.into_parts();
        self.ui_shell.set_workspace_layout(workspace_layout);
        self.ui_shell.set_persistent_panel_configs(plugin_configs);
        self.refresh_new_document_templates();
        self.refresh_workspace_presets();
        self.reset_active_interactions();
        self.mark_panel_surface_dirty();
        self.mark_status_dirty();
        self.rebuild_present_frame();
        self.persist_session_state();
    }

    fn selected_workspace_preset_id(&self) -> String {
        if self
            .workspace_presets
            .presets
            .iter()
            .any(|preset| preset.id == self.active_workspace_preset_id)
        {
            return self.active_workspace_preset_id.clone();
        }

        if self
            .workspace_presets
            .presets
            .iter()
            .any(|preset| preset.id == self.workspace_presets.default_preset_id)
        {
            return self.workspace_presets.default_preset_id.clone();
        }

        self.workspace_presets
            .presets
            .first()
            .map(|preset| preset.id.clone())
            .unwrap_or_default()
    }

    fn ensure_canvas_templates_file(&self) {
        let path = default_canvas_template_path();
        if path.exists() {
            return;
        }

        if let Err(error) = save_canvas_templates(&path, &default_canvas_templates()) {
            eprintln!("failed to create canvas templates file: {error}");
        }
    }

    fn ensure_workspace_presets_file(&self, path: &std::path::Path) {
        if path.exists() {
            return;
        }

        if let Err(error) = save_workspace_preset_catalog(path, &default_workspace_preset_catalog())
        {
            eprintln!("failed to create workspace presets file: {error}");
        }
    }
}

/// テスト時はセッションファイルを一意パスへ逃がし、本番時は既定位置を使う。
fn default_desktop_session_path() -> PathBuf {
    #[cfg(test)]
    {
        let unique = TEST_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-test-session-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[cfg(not(test))]
    {
        desktop_support::default_session_path()
    }
}
