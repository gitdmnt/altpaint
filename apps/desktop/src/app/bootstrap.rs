//! `DesktopApp` 構築時の復元と初期化 orchestration を扱う。

use std::path::{Path, PathBuf};

use document_model::Document;
use desktop_support::{
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION, DEFAULT_PROJECT_FILE_NAME, DesktopSessionState,
    WorkspacePreset, WorkspacePresetCatalog, default_canvas_size_preset_path,
    default_canvas_size_presets, builtin_panels_dir, load_session_state,
    load_workspace_preset_catalog, save_canvas_size_presets, save_workspace_preset_catalog,
};
use std::collections::BTreeMap;

use panel_runtime::{PanelRuntime, register_builtin_panels};
use panel_workspace::{
    PanelLayoutDefaults, PanelWorkspace, WorkspaceLayout, WorkspacePanelAnchor,
    WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};

use super::{DesktopApp, panel_config_sync::selected_workspace_preset_id_from_configs};

pub(super) struct BootstrapState {
    pub(super) document: Document,
    pub(super) panel_runtime: PanelRuntime,
    pub(super) panel_workspace: PanelWorkspace,
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
        let loaded_project = project_store::load_project_from_path(&project_path).ok();
        let document = loaded_project
            .as_ref()
            .map(|project| project.document.clone())
            .unwrap_or_default();

        // BL-095: パネルを登録し、meta 既定配置を panel-workspace へ bridge してから
        // meta 由来の既定カタログを構築する (ビルトイン ID は meta だけが知る)。
        let (mut panel_runtime, mut panel_workspace) = Self::build_panel_system();
        let workspace_presets = load_workspace_preset_catalog(
            workspace_preset_path,
            default_workspace_preset_catalog_from(&panel_runtime),
        );
        let mut active_workspace_preset_id = workspace_presets.default_preset_id.clone();
        Self::apply_initial_workspace_state(
            &mut panel_runtime,
            &mut panel_workspace,
            &workspace_presets,
            loaded_project.as_ref().map(|project| &project.ui_state),
            session.as_ref().map(|state| &state.ui_state),
        );
        if let Some(selected_preset_id) =
            selected_workspace_preset_id_from_configs(&panel_runtime.persistent_panel_configs())
                .filter(|preset_id| {
                    workspace_presets
                        .presets
                        .iter()
                        .any(|preset| preset.id == *preset_id)
                })
        {
            active_workspace_preset_id = selected_preset_id;
        }

        let mut document = document;
        // BL-079: エディタセッション (ツール/色/ペン/ビュー) は project ファイルではなく
        // session 永続化から復元する。on-disk カタログ再ロードの前に適用し、復元された
        // active_tool_id / active_pen_preset_id をカタログ整合で温存する。
        if let Some(session_state) = session.as_ref() {
            document.session = session_state.editor_session.clone();
        }
        Self::reload_tool_catalog_into_document(&mut document);
        Self::reload_pen_presets_into_document(&mut document);
        panel_runtime.mark_all_dirty();
        let _changed_panels =
            panel_runtime.sync_dirty_panels(&document, panel_runtime::HostState::default());
        panel_workspace.reconcile_panels(panel_runtime.panel_ids());

        BootstrapState {
            document,
            panel_runtime,
            panel_workspace,
            project_path,
            workspace_presets,
            active_workspace_preset_id,
        }
    }

    /// パネルを登録し、meta 既定配置を panel-workspace へ bridge する (BL-095)。
    /// プリセット適用前の素の状態を返す。
    fn build_panel_system() -> (PanelRuntime, PanelWorkspace) {
        let mut panel_runtime = PanelRuntime::new();
        let mut panel_workspace = PanelWorkspace::new();
        let diags = register_builtin_panels(&mut panel_runtime, &builtin_panels_dir());
        for diag in &diags {
            eprintln!("register_builtin_panels: {diag}");
        }
        // BL-095: パネルが meta.json で宣言した既定配置を panel-workspace へ bridge する。
        // panel-workspace はビルトイン ID をハードコードせず、この既定値で配置を解決する。
        panel_workspace.set_panel_layout_defaults(panel_layout_defaults_from(&panel_runtime));
        panel_workspace.reconcile_panels(panel_runtime.panel_ids());
        (panel_runtime, panel_workspace)
    }

    /// 既定プリセット → project → session の順で UI 状態を適用する。
    fn apply_initial_workspace_state(
        panel_runtime: &mut PanelRuntime,
        panel_workspace: &mut PanelWorkspace,
        workspace_presets: &WorkspacePresetCatalog,
        project_ui_state: Option<&WorkspaceUiState>,
        session_ui_state: Option<&WorkspaceUiState>,
    ) {
        if let Some(default_preset) = workspace_presets
            .presets
            .iter()
            .find(|preset| preset.id == workspace_presets.default_preset_id)
        {
            apply_ui_state_to_panel_system(
                panel_runtime,
                panel_workspace,
                &default_preset.ui_state,
            );
        }
        if let Some(project_ui_state) = project_ui_state {
            apply_ui_state_to_panel_system(panel_runtime, panel_workspace, project_ui_state);
        }
        if let Some(session_ui_state) = session_ui_state {
            apply_ui_state_to_panel_system(panel_runtime, panel_workspace, session_ui_state);
        }
    }

    pub(super) fn apply_workspace_ui_state(&mut self, ui_state: WorkspaceUiState) {
        let (workspace_layout, panel_configs) = ui_state.into_parts();
        self.panel_workspace
            .replace_workspace_layout(workspace_layout);
        self.panel_runtime
            .replace_persistent_panel_configs(panel_configs);
        self.panel_workspace
            .reconcile_panels(self.panel_runtime.panel_ids());
        self.refresh_new_document_size_presets();
        self.refresh_workspace_presets();
        self.reset_active_interactions();
        self.request_panel_reconcile();
        self.mark_status_dirty();
        self.rebuild_present_frame();
        self.persist_session_state();
    }

    pub(super) fn ensure_canvas_size_presets_file(&self) {
        let path = default_canvas_size_preset_path();
        if path.exists() {
            return;
        }

        if let Err(error) = save_canvas_size_presets(&path, &default_canvas_size_presets()) {
            eprintln!("failed to create canvas size presets file: {error}");
        }
    }

    /// 現在登録されているパネルの meta から同梱 default-floating プリセットカタログを
    /// 構築する (BL-095)。on-disk カタログ欠落/破損時のフォールバックに使う。
    pub(super) fn default_workspace_preset_catalog(&self) -> WorkspacePresetCatalog {
        default_workspace_preset_catalog_from(&self.panel_runtime)
    }

    pub(super) fn ensure_workspace_presets_file(&self, path: &Path) {
        if path.exists() {
            return;
        }

        let default_catalog = default_workspace_preset_catalog_from(&self.panel_runtime);
        if let Err(error) = save_workspace_preset_catalog(path, &default_catalog) {
            eprintln!("failed to create workspace presets file: {error}");
        }
    }

    pub(super) fn persist_workspace_preset_catalog(&self) {
        if let Err(error) = save_workspace_preset_catalog(
            &self.io_state.workspace_preset_path,
            &self.workspace_presets,
        ) {
            let message = format!("failed to persist workspace preset catalog: {error}");
            eprintln!("{message}");
            self.io_state
                .dialogs
                .show_error("Workspace save failed", &message);
        }
    }
}

/// panel-runtime の meta から panel-workspace 用の既定配置マップを構築する (BL-095)。
///
/// ビルトイン ID の知識を持つのは「パネル自身 (meta.json)」だけになり、水平層の
/// panel-workspace は ID 非依存のまま配置を解決できる。
fn panel_layout_defaults_from(
    panel_runtime: &PanelRuntime,
) -> BTreeMap<String, PanelLayoutDefaults> {
    panel_runtime
        .panel_layout_metas()
        .into_iter()
        .map(|(id, meta)| {
            let defaults = PanelLayoutDefaults {
                anchor: meta
                    .anchor
                    .as_deref()
                    .and_then(WorkspacePanelAnchor::from_kebab),
                position: meta
                    .position
                    .map(|p| WorkspacePanelPosition { x: p.x, y: p.y }),
                hidden_by_default: meta.hidden_by_default,
                always_visible: meta.always_visible,
            };
            (id.to_string(), defaults)
        })
        .collect()
}

/// panel-runtime の meta から同梱 default-floating プリセットカタログを構築する (BL-095)。
///
/// `desktop_support::default_workspace_preset_catalog` のビルトイン ID ハードコードを
/// 置換する。`preset` を宣言したパネルのみが既定プリセットに含まれ、宣言された
/// anchor/position/size/visible で配置される。パネル登録順を保つ。
fn default_workspace_preset_catalog_from(
    panel_runtime: &PanelRuntime,
) -> WorkspacePresetCatalog {
    let panels = panel_runtime
        .panel_preset_metas()
        .into_iter()
        .map(|(id, preset)| WorkspacePanelState {
            id: id.to_string(),
            visible: preset.visible,
            anchor: WorkspacePanelAnchor::from_kebab(&preset.anchor)
                .unwrap_or(WorkspacePanelAnchor::TopLeft),
            position: Some(WorkspacePanelPosition {
                x: preset.position.x,
                y: preset.position.y,
            }),
            size: Some(WorkspacePanelSize {
                width: preset.size.width as usize,
                height: preset.size.height as usize,
            }),
        })
        .collect();
    WorkspacePresetCatalog {
        format_version: CURRENT_WORKSPACE_PRESET_FORMAT_VERSION,
        default_preset_id: "default-floating".to_string(),
        presets: vec![WorkspacePreset {
            id: "default-floating".to_string(),
            label: "Default floating workspace".to_string(),
            ui_state: WorkspaceUiState::new(WorkspaceLayout { panels }, Default::default()),
        }],
    }
}

fn resolve_startup_project_path(
    project_path: PathBuf,
    session: Option<&DesktopSessionState>,
) -> PathBuf {
    session
        .and_then(|state| {
            (project_path == Path::new(DEFAULT_PROJECT_FILE_NAME))
                .then(|| state.last_project_path.clone())
                .flatten()
        })
        .unwrap_or(project_path)
}

fn apply_ui_state_to_panel_system(
    panel_runtime: &mut PanelRuntime,
    panel_workspace: &mut PanelWorkspace,
    ui_state: &WorkspaceUiState,
) {
    if !ui_state.workspace_layout.panels.is_empty() {
        panel_workspace.replace_workspace_layout(ui_state.workspace_layout.clone());
    }
    if !ui_state.panel_configs.is_empty() {
        panel_runtime.replace_persistent_panel_configs(ui_state.panel_configs.clone());
    }
    panel_workspace.reconcile_panels(panel_runtime.panel_ids());

    // Phase 11: GPU パネル (HTML) の size を確定する。
    // 1. workspace_layout に永続値があればそれを使う。
    // 2. 無ければ panel.meta.json の default_size を使う。
    // 3. どちらも無ければ (1, 1) を最終 fallback (実質的に到達しない経路)。
    // 確定値を panel_workspace.set_panel_size で workspace に書き戻し、
    // panel_runtime.restore_panel_size で view の panel_size にも反映する。
    let panel_ids = panel_runtime.panel_ids_with_gpu();
    let layout_snapshot = panel_workspace.workspace_layout();
    for panel_id in panel_ids {
        let persisted = layout_snapshot
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)
            .and_then(|entry| entry.size)
            .map(|s| (s.width as u32, s.height as u32));
        let size = persisted
            .or_else(|| panel_runtime.panel_default_size(&panel_id))
            .unwrap_or((1, 1));
        let _ = panel_workspace.set_panel_size(&panel_id, size.0 as usize, size.1 as usize);
        let _ = panel_runtime.restore_panel_size(&panel_id, size);
    }
}
