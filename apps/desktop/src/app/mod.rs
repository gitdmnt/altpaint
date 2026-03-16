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
mod snapshot_store;
mod state;
#[cfg(test)]
pub(crate) mod tests;

use std::path::PathBuf;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::{CanvasBitmap, CanvasDirtyRect, CanvasPoint, CommandHistory, Document, PanelId};
use desktop_support::{
    DesktopDialogs, NativeDesktopDialogs, WorkspacePresetCatalog, default_workspace_preset_path,
};
use panel_runtime::PanelRuntime;
use render::RenderFrame;
use ui_shell::{PanelPresentation, PanelSurface};

use self::io_state::DesktopIoState;
use self::snapshot_store::SnapshotStore;
#[cfg(test)]
pub(crate) use self::panel_dispatch::PanelDragState;
use self::panel_dispatch::PanelInteractionState;
use self::present_state::PresentFrameUpdate;
use crate::frame::DesktopLayout;
use canvas::CanvasInputState;

#[cfg(test)]
static TEST_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// ストローク中のビットマップ差分追跡状態。
struct PendingStroke {
    panel_id: PanelId,
    layer_index: usize,
    /// ストローク開始前のレイヤービットマップ全体。
    before_layer: CanvasBitmap,
    /// ストローク中に蓄積したパネルローカル dirty rect の合計。
    dirty: Option<CanvasDirtyRect>,
}

pub(super) const WORKSPACE_PRESET_PANEL_ID: &str = "builtin.workspace-presets";
pub(super) const TOOL_PALETTE_PANEL_ID: &str = "builtin.tool-palette";

/// ランタイムから利用されるデスクトップアプリ本体を表す。
pub(crate) struct DesktopApp {
    pub(crate) document: Document,
    pub(crate) panel_runtime: PanelRuntime,
    pub(crate) panel_presentation: PanelPresentation,
    pub(crate) io_state: DesktopIoState,
    workspace_presets: WorkspacePresetCatalog,
    active_workspace_preset_id: String,
    paint_runtime: drawing::CanvasRuntime,
    canvas_input: CanvasInputState,
    pub(crate) panel_surface: Option<PanelSurface>,
    pub(crate) layout: Option<DesktopLayout>,
    canvas_frame: Option<RenderFrame>,
    background_frame: Option<RenderFrame>,
    temp_overlay_frame: Option<RenderFrame>,
    ui_panel_frame: Option<RenderFrame>,
    pending_canvas_dirty_rect: Option<app_core::CanvasDirtyRect>,
    pending_background_dirty_rect: Option<crate::frame::Rect>,
    pending_temp_overlay_dirty_rect: Option<crate::frame::Rect>,
    pending_ui_panel_dirty_rect: Option<crate::frame::Rect>,
    pending_canvas_transform_update: bool,
    pub(crate) history: CommandHistory,
    pub(crate) snapshots: SnapshotStore,
    pub(crate) panel_interaction: PanelInteractionState,
    hover_canvas_position: Option<CanvasPoint>,
    pending_stroke: Option<PendingStroke>,
    deferred_view_panel_sync: bool,
    deferred_status_refresh: bool,
    needs_panel_surface_refresh: bool,
    needs_status_refresh: bool,
    needs_full_present_rebuild: bool,
}

impl DesktopApp {
    /// 既定値を使って新しいインスタンスを生成する。
    pub(crate) fn new(project_path: PathBuf) -> Self {
        Self::new_with_dialogs_session_path_and_workspace_preset_path(
            project_path,
            Box::new(NativeDesktopDialogs),
            default_desktop_session_path(),
            default_workspace_preset_path(),
        )
    }

    /// 既定値を使って新しいインスタンスを生成する。
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

    /// 既定値を使って新しいインスタンスを生成する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(crate) fn new_with_dialogs_session_path_and_workspace_preset_path(
        project_path: PathBuf,
        dialogs: Box<dyn DesktopDialogs>,
        session_path: PathBuf,
        workspace_preset_path: PathBuf,
    ) -> Self {
        let bootstrap = Self::bootstrap_state(project_path, &session_path, &workspace_preset_path);

        let mut app = Self {
            document: bootstrap.document,
            panel_runtime: bootstrap.panel_runtime,
            panel_presentation: bootstrap.panel_presentation,
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
            background_frame: None,
            temp_overlay_frame: None,
            ui_panel_frame: None,
            pending_canvas_dirty_rect: None,
            pending_background_dirty_rect: None,
            pending_temp_overlay_dirty_rect: None,
            pending_ui_panel_dirty_rect: None,
            pending_canvas_transform_update: false,
            history: CommandHistory::new(),
            snapshots: SnapshotStore::default(),
            panel_interaction: PanelInteractionState::default(),
            hover_canvas_position: None,
            pending_stroke: None,
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

/// 既定の desktop セッション パス を返す。
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
