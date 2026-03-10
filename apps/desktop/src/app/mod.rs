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
    default_canvas_templates, default_panel_dir, load_canvas_templates, load_session_state,
    save_canvas_templates,
};
use render::RenderFrame;
use serde_json::{Map, Value, json};
use ui_shell::{PanelSurface, UiShell};

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
        Self::new_with_dialogs_and_session_path(
            project_path,
            Box::new(NativeDesktopDialogs),
            default_desktop_session_path(),
        )
    }

    /// ダイアログ実装を差し替えてアプリ本体を生成する。
    #[allow(dead_code)]
    pub(crate) fn new_with_dialogs(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
    ) -> Self {
        Self::new_with_dialogs_and_session_path(
            project_path,
            dialogs,
            default_desktop_session_path(),
        )
    }

    /// ダイアログ実装とセッション保存先を差し替えてアプリ本体を生成する。
    pub(crate) fn new_with_dialogs_and_session_path(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
        session_path: PathBuf,
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
        if let Some(project) = loaded_project {
            ui_shell.set_workspace_layout(project.ui_state.workspace_layout);
            ui_shell.set_persistent_panel_configs(project.ui_state.plugin_configs);
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
        app.ensure_canvas_templates_file();
        app.refresh_new_document_templates();
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
            json!(default_template
                .as_ref()
                .map(|template| template.size_string())
                .unwrap_or_else(|| "2894x4093".to_string())),
        );
        self.ui_shell.set_persistent_panel_configs(configs);
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
