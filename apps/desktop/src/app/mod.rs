//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod background_tasks;
mod bootstrap;
mod command_router;
mod commands;
mod drawing;
mod input;
mod io_state;
mod panel_config_sync;
mod panel_dispatch;
mod present;
mod present_state;
mod services;
mod state;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::BTreeSet;
use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::{CanvasPoint, Document};
use desktop_support::{
    DesktopDialogs, NativeDesktopDialogs, WorkspacePresetCatalog, default_workspace_preset_path,
};
use render::RenderFrame;
use ui_shell::{PanelSurface, UiShell};

use self::io_state::DesktopIoState;
#[cfg(test)]
pub(crate) use self::panel_dispatch::PanelDragState;
use self::panel_dispatch::PanelInteractionState;
use self::present_state::PresentFrameUpdate;
use canvas::CanvasInputState;
use crate::frame::DesktopLayout;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(super) const WORKSPACE_PRESET_PANEL_ID: &str = "builtin.workspace-presets";
pub(super) const TOOL_PALETTE_PANEL_ID: &str = "builtin.tool-palette";

/// ランタイムから利用されるデスクトップアプリ本体を表す。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) ui_shell: UiShell,
    pub(crate) io_state: DesktopIoState,
    workspace_presets: WorkspacePresetCatalog,
    active_workspace_preset_id: String,
    paint_runtime: drawing::CanvasRuntime,
    canvas_input: CanvasInputState,
    pub(crate) panel_surface: Option<PanelSurface>,
    pub(crate) layout: Option<DesktopLayout>,
    canvas_frame: Option<RenderFrame>,
    base_frame: Option<RenderFrame>,
    overlay_frame: Option<RenderFrame>,
    pending_canvas_dirty_rect: Option<app_core::CanvasDirtyRect>,
    pending_canvas_background_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_host_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_transform_update: bool,
    pub(crate) panel_interaction: PanelInteractionState,
    hover_canvas_position: Option<CanvasPoint>,
    needs_ui_sync: bool,
    ui_sync_panel_ids: BTreeSet<String>,
    deferred_view_panel_sync: bool,
    deferred_status_refresh: bool,
    needs_panel_surface_refresh: bool,
    needs_status_refresh: bool,
    needs_full_present_rebuild: bool,
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
        let bootstrap = Self::bootstrap_state(project_path, &session_path, &workspace_preset_path);

        let mut app = Self {
            document: bootstrap.document,
            ui_shell: bootstrap.ui_shell,
            io_state: DesktopIoState::new(
                bootstrap.project_path,
                session_path,
                workspace_preset_path,
                dialogs,
            ),
            workspace_presets: bootstrap.workspace_presets,
            active_workspace_preset_id: bootstrap.active_workspace_preset_id,
            paint_runtime: drawing::CanvasRuntime::default(),
            canvas_input: CanvasInputState::default(),
            panel_surface: None,
            layout: None,
            canvas_frame: None,
            base_frame: None,
            overlay_frame: None,
            pending_canvas_dirty_rect: None,
            pending_canvas_background_dirty_rect: None,
            pending_canvas_host_dirty_rect: None,
            pending_canvas_transform_update: false,
            panel_interaction: PanelInteractionState::default(),
            hover_canvas_position: None,
            needs_ui_sync: true,
            ui_sync_panel_ids: BTreeSet::new(),
            deferred_view_panel_sync: false,
            deferred_status_refresh: false,
            needs_panel_surface_refresh: true,
            needs_status_refresh: false,
            needs_full_present_rebuild: true,
        };
        app.refresh_canvas_frame();
        app.ensure_workspace_presets_file(&app.io_state.workspace_preset_path);
        app.ensure_canvas_templates_file();
        app.refresh_new_document_templates();
        app.refresh_workspace_presets();
        app
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
