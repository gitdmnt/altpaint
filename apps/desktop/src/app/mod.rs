//! デスクトップアプリケーションの状態遷移と副作用の窓口を定義する。
//!
//! `DesktopApp` はドキュメント、UI シェル、プロジェクト I/O を束ね、
//! ランタイムから見た「状態付きのアプリ本体」として振る舞う。

mod commands;
mod input;
mod present;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::thread::JoinHandle;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use app_core::{Command, DirtyRect, Document};
use desktop_support::{
    DEFAULT_PROJECT_PATH, DesktopDialogs, DesktopSessionState, NativeDesktopDialogs,
    default_panel_dir, default_pen_dir, load_session_state, save_session_state,
};
use render::RenderFrame;
use storage::{load_pen_directory, load_project_from_path};
use ui_shell::{PanelSurface, UiShell};

use crate::canvas_bridge::CanvasInputState;
use crate::frame::{
    DesktopLayout, Rect, TextureQuad, brush_preview_rect, canvas_texture_quad,
    exposed_canvas_background_rect,
};

/// 差分提示のために更新領域を集約した結果を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresentFrameUpdate {
    pub(crate) base_dirty_rect: Option<crate::frame::Rect>,
    pub(crate) overlay_dirty_rect: Option<crate::frame::Rect>,
    pub(crate) canvas_dirty_rect: Option<DirtyRect>,
    pub(crate) canvas_transform_changed: bool,
    pub(crate) canvas_updated: bool,
}

/// スライダードラッグ中のパネルノード情報を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelDragState {
    panel_id: String,
    node_id: String,
}

#[derive(Debug)]
struct PendingSaveTask {
    handle: JoinHandle<Result<(), String>>,
}

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
    pending_canvas_dirty_rect: Option<DirtyRect>,
    pending_canvas_background_dirty_rect: Option<Rect>,
    pending_canvas_host_dirty_rect: Option<Rect>,
    pending_canvas_transform_update: bool,
    active_panel_drag: Option<PanelDragState>,
    hover_canvas_position: Option<(usize, usize)>,
    needs_ui_sync: bool,
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
        Self::new_with_dialogs_and_session_path(project_path, dialogs, default_desktop_session_path())
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
        let loaded_project = load_project_from_path(&project_path).ok();
        let document = loaded_project
            .as_ref()
            .map(|project| project.document.clone())
            .unwrap_or_default();
        let mut ui_shell = UiShell::new();
        let _ = ui_shell.load_panel_directory(default_panel_dir());
        if let Some(project) = loaded_project {
            ui_shell.set_workspace_layout(project.workspace_layout);
            ui_shell.set_persistent_panel_configs(project.plugin_configs);
        }
        if let Some(session) = session.as_ref() {
            if !session.workspace_layout.panels.is_empty() {
                ui_shell.set_workspace_layout(session.workspace_layout.clone());
            }
            if !session.plugin_configs.is_empty() {
                ui_shell.set_persistent_panel_configs(session.plugin_configs.clone());
            }
        }
        let mut document = document;
        Self::reload_pen_presets_into_document(&mut document);
        ui_shell.update(&document);

        Self {
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
            hover_canvas_position: None,
            needs_ui_sync: true,
            needs_panel_surface_refresh: true,
            needs_status_refresh: false,
            needs_full_present_rebuild: true,
            pending_save_tasks: Vec::new(),
        }
    }

    fn session_state(&self) -> DesktopSessionState {
        DesktopSessionState {
            last_project_path: Some(self.project_path.clone()),
            workspace_layout: self.ui_shell.workspace_layout(),
            plugin_configs: self.ui_shell.persistent_panel_configs(),
        }
    }

    fn persist_session_state(&self) {
        if let Err(error) = save_session_state(&self.session_path, &self.session_state()) {
            eprintln!("failed to persist desktop session: {error}");
        }
    }

    fn poll_background_tasks(&mut self) {
        let mut remaining = Vec::new();
        let mut completed_any = false;

        for task in self.pending_save_tasks.drain(..) {
            if task.handle.is_finished() {
                completed_any = true;
                match task.handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        eprintln!("failed to save project: {error}");
                        self.dialogs.show_error("Save failed", &error);
                    }
                    Err(_) => {
                        self.dialogs
                            .show_error("Save failed", "background save task panicked");
                    }
                }
            } else {
                remaining.push(task);
            }
        }

        if completed_any {
            self.mark_status_dirty();
        }
        self.pending_save_tasks = remaining;
    }

    #[cfg(test)]
    pub(crate) fn wait_for_pending_save_tasks(&mut self) {
        let mut remaining = Vec::new();
        std::mem::swap(&mut remaining, &mut self.pending_save_tasks);
        for task in remaining {
            match task.handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => panic!("background save failed: {error}"),
                Err(_) => panic!("background save task panicked"),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_save_task_count(&self) -> usize {
        self.pending_save_tasks.len()
    }

    /// パネル面の再描画が必要であることを記録する。
    fn mark_panel_surface_dirty(&mut self) {
        self.needs_panel_surface_refresh = true;
    }

    /// ステータス表示の更新が必要であることを記録する。
    fn mark_status_dirty(&mut self) {
        self.needs_status_refresh = true;
    }

    /// ドキュメント変更後に UI シェル再同期を要求する。
    fn sync_ui_from_document(&mut self) {
        self.needs_ui_sync = true;
        self.mark_panel_surface_dirty();
    }

    /// 最終提示フレームの全面再構築を要求する。
    fn rebuild_present_frame(&mut self) {
        self.needs_full_present_rebuild = true;
    }

    /// 入力中状態を初期化して、読み込みや新規作成へ備える。
    fn reset_active_interactions(&mut self) {
        self.canvas_input = CanvasInputState::default();
        self.pending_canvas_dirty_rect = None;
        self.pending_canvas_background_dirty_rect = None;
        self.pending_canvas_background_dirty_rect = None;
        self.pending_canvas_host_dirty_rect = None;
        self.pending_canvas_transform_update = false;
        self.pending_canvas_transform_update = false;
        self.active_panel_drag = None;
        self.hover_canvas_position = None;
    }

    /// UI 変更の有無に応じてパネル面更新フラグを立てる。
    fn refresh_panel_surface_if_changed(&mut self, changed: bool) -> bool {
        if changed {
            self.mark_panel_surface_dirty();
        }
        changed
    }

    /// キャンバス dirty rect を次回提示用に集約する。
    fn append_canvas_dirty_rect(&mut self, dirty: DirtyRect) -> bool {
        self.pending_canvas_dirty_rect = Some(
            self.pending_canvas_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// キャンバス背景の dirty rect を次回提示用に集約する。
    fn append_canvas_background_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_canvas_background_dirty_rect = Some(
            self.pending_canvas_background_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// キャンバスホスト上の dirty rect を次回提示用に集約する。
    fn append_canvas_host_dirty_rect(&mut self, dirty: Rect) -> bool {
        self.pending_canvas_host_dirty_rect = Some(
            self.pending_canvas_host_dirty_rect
                .map_or(dirty, |existing| existing.union(dirty)),
        );
        true
    }

    /// GPU 側で再適用するキャンバス変換の更新を記録する。
    fn mark_canvas_transform_dirty(
        &mut self,
        previous_transform: app_core::CanvasViewTransform,
    ) -> bool {
        self.pending_canvas_transform_update = true;
        if let Some(canvas_viewport_rect) =
            self.layout.as_ref().map(|layout| layout.canvas_host_rect)
        {
            let (canvas_width, canvas_height) = self.canvas_dimensions();
            let background_dirty = exposed_canvas_background_rect(
                canvas_viewport_rect,
                canvas_width,
                canvas_height,
                previous_transform,
                self.document.view_transform,
            )
            .unwrap_or(canvas_viewport_rect);
            self.append_canvas_background_dirty_rect(background_dirty);
            if let Some(hover_position) = self.hover_canvas_position {
                let previous_preview = brush_preview_rect(
                    canvas_viewport_rect,
                    canvas_width,
                    canvas_height,
                    previous_transform,
                    hover_position,
                );
                let current_preview = brush_preview_rect(
                    canvas_viewport_rect,
                    canvas_width,
                    canvas_height,
                    self.document.view_transform,
                    hover_position,
                );

                match (previous_preview, current_preview) {
                    (Some(previous), Some(current)) => {
                        self.append_canvas_host_dirty_rect(previous.union(current));
                    }
                    (Some(previous), None) => {
                        self.append_canvas_host_dirty_rect(previous);
                    }
                    (None, Some(current)) => {
                        self.append_canvas_host_dirty_rect(current);
                    }
                    (None, None) => {}
                }
            }
        } else {
            self.rebuild_present_frame();
        }
        true
    }

    /// ドキュメント変更系コマンドを適用し、必要な更新フラグを立てる。
    fn execute_document_command(&mut self, command: Command) -> bool {
        let previous_transform = self.document.view_transform;
        let dirty = self.document.apply_command(&command);
        match command {
            Command::DrawPoint { .. }
            | Command::DrawStroke { .. }
            | Command::ErasePoint { .. }
            | Command::EraseStroke { .. } => {
                dirty.is_some_and(|dirty| self.append_canvas_dirty_rect(dirty))
            }
            Command::SetActiveTool { .. }
            | Command::SetActivePenSize { .. }
            | Command::SelectNextPenPreset
            | Command::SelectPreviousPenPreset
            | Command::SetActiveColor { .. } => {
                self.sync_ui_from_document();
                self.mark_status_dirty();
                true
            }
            Command::SetViewZoom { .. } | Command::ResetView => {
                self.mark_canvas_transform_dirty(previous_transform);
                self.mark_canvas_transform_dirty(previous_transform);
                self.mark_status_dirty();
                true
            }
            Command::PanView { .. } => self.mark_canvas_transform_dirty(previous_transform),
            Command::AddRasterLayer
            | Command::SelectLayer { .. }
            | Command::SelectNextLayer
            | Command::CycleActiveLayerBlendMode
            | Command::ToggleActiveLayerVisibility
            | Command::ToggleActiveLayerMask => {
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::NewDocumentSized { .. } => {
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Command::Noop
            | Command::NewDocument
            | Command::SaveProject
            | Command::SaveProjectAs
            | Command::SaveProjectToPath { .. }
            | Command::LoadProject
            | Command::LoadProjectFromPath { .. }
            | Command::ReloadPenPresets => false,
        }
    }

    /// 背景・パネル・ステータスを含むベースフレームを返す。
    pub(crate) fn base_frame(&self) -> Option<&RenderFrame> {
        self.base_frame.as_ref()
    }

    /// キャンバス上へ重ねるオーバーレイフレームを返す。
    pub(crate) fn overlay_frame(&self) -> Option<&RenderFrame> {
        self.overlay_frame.as_ref()
    }

    /// 現在のキャンバスを描画する GPU 四角形を返す。
    pub(crate) fn canvas_texture_quad(&self) -> Option<TextureQuad> {
        let layout = self.layout.as_ref()?;
        let bitmap = self.document.active_bitmap()?;
        canvas_texture_quad(
            layout.canvas_host_rect,
            bitmap.width,
            bitmap.height,
            self.document.view_transform,
        )
    }

    /// 現在のアクティブビットマップ寸法を返す。
    fn canvas_dimensions(&self) -> (usize, usize) {
        self.document
            .active_bitmap()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    pub(crate) fn reload_pen_presets(&mut self) -> bool {
        let changed = Self::reload_pen_presets_into_document(&mut self.document);
        if changed {
            self.sync_ui_from_document();
            self.mark_status_dirty();
            self.rebuild_present_frame();
        }
        changed
    }

    fn reload_pen_presets_into_document(document: &mut Document) -> bool {
        let (presets, diagnostics) = load_pen_directory(default_pen_dir());
        for diagnostic in diagnostics {
            eprintln!("pen preset load warning: {diagnostic}");
        }
        if presets.is_empty() {
            return false;
        }
        document.replace_pen_presets(presets);
        true
    }

    /// フッターへ表示する現在状態の概要文字列を生成する。
    fn status_text(&self) -> String {
        let file_name = self
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_PATH);
        let hidden_panels = self
            .ui_shell
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / pages={} / panels={} / hidden={}",
            file_name,
            self.document.active_tool,
            self.document
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.active_pen_size,
            self.document.active_color.hex_rgb(),
            self.document.view_transform.zoom,
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.panels.len())
                .sum::<usize>(),
            hidden_panels,
        )
    }

    /// キャンバス描画中かどうかを返す。
    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}

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
