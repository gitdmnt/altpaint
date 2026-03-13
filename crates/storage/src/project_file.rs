use std::fs;
use std::io::Cursor;
use std::path::Path;

use app_core::{Document, Page, PageId, Panel, PanelId, WorkId, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use workspace_persistence::{PluginConfigs, WorkspaceUiState};

use crate::project_sqlite::{
    DEFAULT_PROJECT_CHUNK_SIZE, PersistedPanelSnapshot, PersistedPanelSnapshotSummary,
    ProjectIndex, ProjectPageSummary, ProjectPanelSummary, ProjectSaveMode, ProjectSaveOptions,
    is_sqlite_project_path, load_page_from_sqlite_path, load_panel_from_sqlite_path,
    load_panel_snapshot_from_sqlite_path, load_project_from_sqlite_path,
    load_project_index_from_sqlite_path, save_project_to_sqlite_path,
};

/// ワークスペース レイアウト is empty を計算して返す。
fn workspace_layout_is_empty(layout: &WorkspaceLayout) -> bool {
    layout.panels.is_empty()
}

pub const CURRENT_FORMAT_VERSION: u32 = 7;
const BINARY_MAGIC: &[u8; 8] = b"ALTPBIN\0";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
#[cfg(test)]
const ZSTD_COMPRESSION_LEVEL: i32 = 3;

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: Document,
    pub ui_state: WorkspaceUiState,
}

impl LoadedProject {
    /// ワークスペース レイアウト を計算して返す。
    pub fn workspace_layout(&self) -> &WorkspaceLayout {
        &self.ui_state.workspace_layout
    }

    /// プラグイン configs を計算して返す。
    pub fn plugin_configs(&self) -> &PluginConfigs {
        &self.ui_state.plugin_configs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltpaintProjectFile {
    pub format_version: u32,
    pub document: Document,
    #[serde(default)]
    pub ui_state: WorkspaceUiState,
    #[serde(default, skip_serializing_if = "workspace_layout_is_empty")]
    pub workspace_layout: WorkspaceLayout,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_configs: PluginConfigs,
}

impl AltpaintProjectFile {
    /// 既定値を使って新しいインスタンスを生成する。
    pub fn new(
        document: &Document,
        workspace_layout: &WorkspaceLayout,
        plugin_configs: &BTreeMap<String, Value>,
    ) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            document: document.clone(),
            ui_state: WorkspaceUiState::new(workspace_layout.clone(), plugin_configs.clone()),
            workspace_layout: WorkspaceLayout::default(),
            plugin_configs: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("unsupported altpaint project format version: {0}")]
    UnsupportedFormatVersion(u32),
    #[error("failed to encode project file: {0}")]
    Encode(#[source] rmp_serde::encode::Error),
    #[error("failed to decode project file: {0}")]
    Decode(#[source] rmp_serde::decode::Error),
    #[error("failed to compress project file: {0}")]
    Compress(#[source] std::io::Error),
    #[error("failed to decompress project file: {0}")]
    Decompress(#[source] std::io::Error),
    #[error("sqlite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("failed to deserialize legacy json project file: {0}")]
    DeserializeLegacyJson(#[source] serde_json::Error),
    #[error("failed to serialize metadata json: {0}")]
    SerializeMetadataJson(#[source] serde_json::Error),
    #[error("failed to deserialize metadata json: {0}")]
    DeserializeMetadataJson(#[source] serde_json::Error),
    #[error("invalid project file: {0}")]
    InvalidProject(String),
    #[error("page not found in project: {0}")]
    PageNotFound(u64),
    #[error("panel not found in project: page={page_id}, panel={panel_id}")]
    PanelNotFound { page_id: u64, panel_id: u64 },
    #[error("failed to access project file: {0}")]
    Io(#[from] std::io::Error),
}

/// 現在の値を プロジェクト へ変換する。
///
/// 失敗時はエラーを返します。
#[cfg(test)]
fn serialize_project(project: &AltpaintProjectFile) -> Result<Vec<u8>, StorageError> {
    let encoded = rmp_serde::to_vec(project).map_err(StorageError::Encode)?;
    let compressed = zstd::stream::encode_all(Cursor::new(encoded), ZSTD_COMPRESSION_LEVEL)
        .map_err(StorageError::Compress)?;
    let mut bytes = Vec::with_capacity(BINARY_MAGIC.len() + compressed.len());
    bytes.extend_from_slice(BINARY_MAGIC);
    bytes.extend_from_slice(&compressed);
    Ok(bytes)
}

/// 入力を解析して プロジェクト に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
fn deserialize_project(bytes: &[u8]) -> Result<AltpaintProjectFile, StorageError> {
    if let Some(payload) = bytes.strip_prefix(BINARY_MAGIC) {
        let decoded_bytes = if payload.starts_with(&ZSTD_MAGIC) {
            zstd::stream::decode_all(Cursor::new(payload)).map_err(StorageError::Decompress)?
        } else {
            payload.to_vec()
        };
        rmp_serde::from_slice(&decoded_bytes).map_err(StorageError::Decode)
    } else {
        serde_json::from_slice(bytes).map_err(StorageError::DeserializeLegacyJson)
    }
}

/// ドキュメント to パス を保存先へ書き出す。
pub fn save_document_to_path(
    path: impl AsRef<Path>,
    document: &Document,
) -> Result<(), StorageError> {
    save_project_to_path(
        path,
        document,
        &WorkspaceLayout::default(),
        &BTreeMap::new(),
    )
}

/// プロジェクト to パス を保存先へ書き出す。
pub fn save_project_to_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    plugin_configs: &BTreeMap<String, Value>,
) -> Result<(), StorageError> {
    save_project_to_path_with_options(
        path,
        document,
        workspace_layout,
        plugin_configs,
        ProjectSaveOptions::default(),
    )
}

/// プロジェクト to パス with オプション を保存先へ書き出す。
pub fn save_project_to_path_with_options(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    plugin_configs: &BTreeMap<String, Value>,
    options: ProjectSaveOptions,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    save_project_to_sqlite_path(path, document, workspace_layout, plugin_configs, options)
}

/// ドキュメント from パス を読み込み、必要に応じて整形して返す。
pub fn load_document_from_path(path: impl AsRef<Path>) -> Result<Document, StorageError> {
    load_project_from_path(path).map(|project| project.document)
}

/// プロジェクト from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<LoadedProject, StorageError> {
    let path = path.as_ref();
    if is_sqlite_project_path(path)? {
        return load_project_from_sqlite_path(path);
    }

    let bytes = fs::read(path)?;
    let project = deserialize_project(&bytes)?;

    if !(1..=CURRENT_FORMAT_VERSION).contains(&project.format_version) {
        return Err(StorageError::UnsupportedFormatVersion(
            project.format_version,
        ));
    }

    let mut document = project.document;
    document.normalize_phase9_state();

    let ui_state = if project.ui_state != WorkspaceUiState::default() {
        project.ui_state
    } else {
        WorkspaceUiState::new(project.workspace_layout, project.plugin_configs)
    };

    Ok(LoadedProject { document, ui_state })
}

/// プロジェクト インデックス from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_project_index_from_path(path: impl AsRef<Path>) -> Result<ProjectIndex, StorageError> {
    let path = path.as_ref();
    if is_sqlite_project_path(path)? {
        return load_project_index_from_sqlite_path(path);
    }

    let bytes = fs::read(path)?;
    let project = deserialize_project(&bytes)?;
    if !(1..=CURRENT_FORMAT_VERSION).contains(&project.format_version) {
        return Err(StorageError::UnsupportedFormatVersion(
            project.format_version,
        ));
    }

    let loaded = load_project_from_path(path)?;
    Ok(derive_project_index(
        project.format_version,
        DEFAULT_PROJECT_CHUNK_SIZE,
        ProjectSaveMode::Full,
        &loaded,
        Vec::new(),
    ))
}

/// ページ from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_page_from_path(path: impl AsRef<Path>, page_id: PageId) -> Result<Page, StorageError> {
    let path = path.as_ref();
    if is_sqlite_project_path(path)? {
        return load_page_from_sqlite_path(path, page_id);
    }

    let project = load_project_from_path(path)?;
    project
        .document
        .work
        .pages
        .into_iter()
        .find(|page| page.id == page_id)
        .ok_or(StorageError::PageNotFound(page_id.0))
}

/// パネル from パス を読み込み、必要に応じて整形して返す。
pub fn load_panel_from_path(
    path: impl AsRef<Path>,
    page_id: PageId,
    panel_id: PanelId,
) -> Result<Panel, StorageError> {
    let path = path.as_ref();
    if is_sqlite_project_path(path)? {
        return load_panel_from_sqlite_path(path, page_id, panel_id);
    }

    let page = load_page_from_path(path, page_id)?;
    page.panels
        .into_iter()
        .find(|panel| panel.id == panel_id)
        .ok_or(StorageError::PanelNotFound {
            page_id: page_id.0,
            panel_id: panel_id.0,
        })
}

/// パネル スナップショット from パス を読み込み、必要に応じて整形して返す。
pub fn load_panel_snapshot_from_path(
    path: impl AsRef<Path>,
    snapshot_id: &str,
) -> Result<Option<PersistedPanelSnapshot>, StorageError> {
    let path = path.as_ref();
    if is_sqlite_project_path(path)? {
        return load_panel_snapshot_from_sqlite_path(path, snapshot_id);
    }
    Ok(None)
}

/// 現在の derive プロジェクト インデックス を返す。
fn derive_project_index(
    format_version: u32,
    chunk_size: usize,
    save_mode: ProjectSaveMode,
    loaded: &LoadedProject,
    snapshots: Vec<PersistedPanelSnapshotSummary>,
) -> ProjectIndex {
    let snapshot_map =
        snapshots
            .iter()
            .fold(BTreeMap::<u64, Vec<String>>::new(), |mut map, snapshot| {
                map.entry(snapshot.panel_id.0)
                    .or_default()
                    .push(snapshot.snapshot_id.clone());
                map
            });

    ProjectIndex {
        format_version,
        work_id: WorkId(loaded.document.work.id.0),
        title: loaded.document.work.title.clone(),
        save_mode,
        chunk_size,
        pages: loaded
            .document
            .work
            .pages
            .iter()
            .map(|page| ProjectPageSummary {
                id: page.id,
                panels: page
                    .panels
                    .iter()
                    .map(|panel| ProjectPanelSummary {
                        id: panel.id,
                        width: panel.bitmap.width,
                        height: panel.bitmap.height,
                        layer_count: panel.layers.len(),
                        snapshot_ids: snapshot_map.get(&panel.id.0).cloned().unwrap_or_default(),
                    })
                    .collect(),
            })
            .collect(),
        workspace_layout: loaded.ui_state.workspace_layout.clone(),
        plugin_configs: loaded.ui_state.plugin_configs.clone(),
        snapshots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{BlendMode, ColorRgba8, Document, LayerMask, Page, PageId, PanelId};
    use rusqlite::Connection;
    use std::time::Instant;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// small ドキュメント を計算して返す。
    fn small_document() -> Document {
        Document::new(64, 64)
    }

    /// 描画 test 点 に必要な描画内容を組み立てる。
    fn draw_test_point(document: &mut Document, x: usize, y: usize) {
        let color = document.active_color.to_rgba8();
        if let Some(panel) = document.active_panel_mut() {
            let _ = panel.layers[0]
                .bitmap
                .draw_point_sized_rgba(x, y, color, 1, true);
            panel.bitmap = panel.layers[0].bitmap.clone();
        }
    }

    /// Benchmark ドキュメント に必要な描画内容を組み立てる。
    fn benchmark_document(width: usize, height: usize) -> Document {
        let mut document = Document::new(width, height);
        document.set_active_color(ColorRgba8::new(0x11, 0x66, 0xcc, 0xff));

        for offset in (0..width.min(height)).step_by(25) {
            draw_test_point(&mut document, offset, offset);
        }

        document
    }

    /// 現在の temp パス を返す。
    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-{name}-{unique}.altp"))
    }

    /// 現在の値を ページ ドキュメント へ変換する。
    fn multi_page_document() -> Document {
        let mut document = Document::new(16, 16);
        document.work.title = "Phase 11 test".to_string();

        let mut second_panel = Document::new(8, 8).work.pages[0].panels[0].clone();
        second_panel.id = PanelId(2);
        second_panel.layers[0].name = "Blue layer".to_string();
        second_panel.layers[0]
            .bitmap
            .set_pixel_rgba(2, 3, [0x22, 0x44, 0xaa, 0xff]);
        second_panel.bitmap = second_panel.layers[0].bitmap.clone();

        let mut third_panel = Document::new(8, 8).work.pages[0].panels[0].clone();
        third_panel.id = PanelId(3);
        third_panel.layers[0]
            .bitmap
            .set_pixel_rgba(1, 1, [0x55, 0x99, 0x22, 0xff]);
        third_panel.bitmap = third_panel.layers[0].bitmap.clone();
        third_panel.layers.push(app_core::RasterLayer {
            id: app_core::LayerNodeId(99),
            name: "Overlay".to_string(),
            visible: true,
            blend_mode: BlendMode::Multiply,
            bitmap: {
                let mut bitmap = app_core::CanvasBitmap::transparent(8, 8);
                let _ = bitmap.set_pixel_rgba(1, 1, [0x33, 0x66, 0x11, 0x80]);
                bitmap
            },
            mask: Some(LayerMask {
                width: 8,
                height: 8,
                alpha: vec![255; 64],
            }),
        });

        document.work.pages[0].id = PageId(10);
        document.work.pages[0].panels.push(second_panel);
        document.work.pages.push(Page {
            id: PageId(20),
            width: 8,
            height: 8,
            panels: vec![third_panel],
        });
        document
    }

    /// 保存 and 読込 roundtrip preserves ドキュメント が期待どおりに動作することを検証する。
    #[test]
    fn save_and_load_roundtrip_preserves_document() {
        let path = temp_path("roundtrip");
        let mut document = small_document();
        document.set_active_color(ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff));
        draw_test_point(&mut document, 5, 6);

        save_document_to_path(&path, &document).expect("save should succeed");
        let loaded = load_document_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.work.title, document.work.title);
        assert_eq!(loaded.active_color, document.active_color);
        assert_eq!(
            loaded.work.pages[0].panels[0].bitmap.pixels,
            document.work.pages[0].panels[0].bitmap.pixels
        );

        let _ = fs::remove_file(path);
    }

    /// 保存 and 読込 roundtrip preserves ワークスペース レイアウト が期待どおりに動作することを検証する。
    #[test]
    fn save_and_load_roundtrip_preserves_workspace_layout() {
        let path = temp_path("workspace");
        let document = small_document();
        let workspace_layout = WorkspaceLayout {
            panels: vec![
                app_core::WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                    anchor: app_core::WorkspacePanelAnchor::TopLeft,
                    position: None,
                    size: None,
                },
                app_core::WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                    anchor: app_core::WorkspacePanelAnchor::TopLeft,
                    position: None,
                    size: None,
                },
            ],
        };

        save_project_to_path(&path, &document, &workspace_layout, &BTreeMap::new())
            .expect("save should succeed");
        let loaded = load_project_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.ui_state.workspace_layout, workspace_layout);

        let _ = fs::remove_file(path);
    }

    /// 保存 and 読込 roundtrip preserves プラグイン configs が期待どおりに動作することを検証する。
    #[test]
    fn save_and_load_roundtrip_preserves_plugin_configs() {
        let path = temp_path("plugin-configs");
        let document = small_document();
        let mut plugin_configs = BTreeMap::new();
        plugin_configs.insert(
            "builtin.tool-palette".to_string(),
            serde_json::json!({ "pen_shortcut": "P", "eraser_shortcut": "E" }),
        );

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &plugin_configs,
        )
        .expect("save should succeed");
        let loaded = load_project_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.ui_state.plugin_configs, plugin_configs);

        let _ = fs::remove_file(path);
    }

    /// 読込 version 1 file defaults ワークスペース レイアウト が期待どおりに動作することを検証する。
    #[test]
    fn load_version_1_file_defaults_workspace_layout() {
        let path = temp_path("version1");
        let project = serde_json::json!({
            "format_version": 1,
            "document": small_document(),
        });
        fs::write(
            &path,
            serde_json::to_vec(&project).expect("serialize should succeed"),
        )
        .expect("write should succeed");

        let loaded = load_project_from_path(&path).expect("load should succeed");
        assert!(loaded.ui_state.workspace_layout.panels.is_empty());
        assert!(loaded.ui_state.plugin_configs.is_empty());

        let _ = fs::remove_file(path);
    }

    /// 保存 プロジェクト writes sqlite header and チャンク tables が期待どおりに動作することを検証する。
    #[test]
    fn save_project_writes_sqlite_header_and_chunk_tables() {
        let path = temp_path("sqlite-format");
        let mut document = Document::new(256, 256);
        document.set_active_color(ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
        draw_test_point(&mut document, 32, 48);

        let project =
            AltpaintProjectFile::new(&document, &WorkspaceLayout::default(), &BTreeMap::new());
        let legacy_json =
            serde_json::to_vec_pretty(&project).expect("legacy json serialize should succeed");

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");
        let saved = fs::read(&path).expect("saved project should be readable");
        let connection = Connection::open(&path).expect("sqlite open should succeed");
        let chunk_count = connection
            .query_row("SELECT COUNT(*) FROM layer_chunks", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("chunk count query should succeed");

        assert!(saved.starts_with(crate::project_sqlite::SQLITE_HEADER));
        assert!(chunk_count > 0);
        assert!(saved.len() < legacy_json.len().saturating_mul(2));

        let _ = fs::remove_file(path);
    }

    /// 読込 プロジェクト インデックス reports pages panels and snapshots が期待どおりに動作することを検証する。
    #[test]
    fn load_project_index_reports_pages_panels_and_snapshots() {
        let path = temp_path("project-index");
        let document = multi_page_document();

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");

        let index = load_project_index_from_path(&path).expect("index load should succeed");

        assert_eq!(index.format_version, CURRENT_FORMAT_VERSION);
        assert_eq!(index.pages.len(), 2);
        assert_eq!(index.pages[0].id, PageId(10));
        assert_eq!(index.pages[0].panels.len(), 2);
        assert_eq!(index.pages[1].id, PageId(20));
        assert_eq!(index.pages[1].panels[0].layer_count, 2);
        assert_eq!(index.snapshots.len(), 3);
        assert!(
            index
                .snapshots
                .iter()
                .any(|snapshot| snapshot.snapshot_id == "page:10:panel:2:current")
        );

        let _ = fs::remove_file(path);
    }

    /// 読込 ページ from sqlite returns requested ページ only が期待どおりに動作することを検証する。
    #[test]
    fn load_page_from_sqlite_returns_requested_page_only() {
        let path = temp_path("partial-page");
        let document = multi_page_document();

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");

        let page = load_page_from_path(&path, PageId(20)).expect("page load should succeed");

        assert_eq!(page.id, PageId(20));
        assert_eq!(page.panels.len(), 1);
        assert_eq!(page.panels[0].id, PanelId(3));
        assert_eq!(
            page.panels[0].bitmap.pixel_rgba(1, 1),
            Some([0x55, 0x99, 0x22, 0xff])
        );

        let _ = fs::remove_file(path);
    }

    /// 読込 パネル スナップショット restores 現在 composited ビットマップ が期待どおりに動作することを検証する。
    #[test]
    fn load_panel_snapshot_restores_current_composited_bitmap() {
        let path = temp_path("snapshot");
        let document = multi_page_document();
        let expected = document.work.pages[0].panels[1].bitmap.pixel_rgba(2, 3);

        save_project_to_path_with_options(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
            ProjectSaveOptions::default(),
        )
        .expect("save should succeed");

        let snapshot = load_panel_snapshot_from_path(&path, "page:10:panel:2:current")
            .expect("snapshot load should succeed")
            .expect("snapshot should exist");

        assert_eq!(snapshot.summary.page_id, PageId(10));
        assert_eq!(snapshot.summary.panel_id, PanelId(2));
        assert_eq!(snapshot.bitmap.pixel_rgba(2, 3), expected);

        let _ = fs::remove_file(path);
    }

    /// 読込 プロジェクト supports 前 uncompressed binary 形式 が期待どおりに動作することを検証する。
    #[test]
    fn load_project_supports_previous_uncompressed_binary_format() {
        let path = temp_path("legacy-binary");
        let mut document = small_document();
        document.set_active_color(ColorRgba8::new(0x55, 0x66, 0x77, 0xff));
        let project = AltpaintProjectFile {
            format_version: 5,
            document: document.clone(),
            ui_state: WorkspaceUiState::default(),
            workspace_layout: WorkspaceLayout::default(),
            plugin_configs: BTreeMap::new(),
        };

        let mut serialized = Vec::new();
        serialized.extend_from_slice(BINARY_MAGIC);
        serialized.extend_from_slice(
            &rmp_serde::to_vec(&project).expect("legacy binary serialize should succeed"),
        );
        fs::write(&path, serialized).expect("write should succeed");

        let loaded = load_project_from_path(&path).expect("legacy binary load should succeed");

        assert_eq!(loaded.document.active_color, document.active_color);
        assert_eq!(loaded.document.work.title, document.work.title);

        let _ = fs::remove_file(path);
    }

    /// 読込 プロジェクト supports legacy JSON 形式 が期待どおりに動作することを検証する。
    #[test]
    fn load_project_supports_legacy_json_format() {
        let path = temp_path("legacy-json");
        let mut document = small_document();
        document.set_active_color(ColorRgba8::new(0xaa, 0xbb, 0xcc, 0xff));
        let project =
            AltpaintProjectFile::new(&document, &WorkspaceLayout::default(), &BTreeMap::new());
        let serialized = serde_json::to_vec_pretty(&project).expect("serialize should succeed");
        fs::write(&path, serialized).expect("write should succeed");

        let loaded = load_project_from_path(&path).expect("legacy json load should succeed");

        assert_eq!(loaded.document.active_color, document.active_color);
        assert_eq!(loaded.document.work.title, document.work.title);

        let _ = fs::remove_file(path);
    }

    /// 読込 rejects unknown 形式 version が期待どおりに動作することを検証する。
    #[test]
    fn load_rejects_unknown_format_version() {
        let path = temp_path("version");
        let project = AltpaintProjectFile {
            format_version: CURRENT_FORMAT_VERSION + 1,
            document: small_document(),
            ui_state: WorkspaceUiState::default(),
            workspace_layout: WorkspaceLayout::default(),
            plugin_configs: BTreeMap::new(),
        };
        let serialized = serialize_project(&project).expect("serialize should succeed");
        fs::write(&path, serialized).expect("write should succeed");

        let error = load_document_from_path(&path).expect_err("unknown version should fail");
        assert!(matches!(
            error,
            StorageError::UnsupportedFormatVersion(version) if version == CURRENT_FORMAT_VERSION + 1
        ));

        let _ = fs::remove_file(path);
    }

    /// サイズ difference benchmark for 1000 square ドキュメント が期待どおりに動作することを検証する。
    #[test]
    #[ignore = "manual benchmark: run with `cargo test -p storage size_difference_benchmark_for_1000_square_document -- --ignored --nocapture`"]
    fn size_difference_benchmark_for_1000_square_document() {
        let document = benchmark_document(1000, 1000);
        let project =
            AltpaintProjectFile::new(&document, &WorkspaceLayout::default(), &BTreeMap::new());

        let json_started = Instant::now();
        let json_bytes =
            serde_json::to_vec_pretty(&project).expect("json serialize should succeed");
        let json_elapsed = json_started.elapsed();

        let binary_started = Instant::now();
        let compressed_bytes =
            serialize_project(&project).expect("binary serialize should succeed");
        let binary_elapsed = binary_started.elapsed();

        let saved_bytes = json_bytes.len().saturating_sub(compressed_bytes.len());
        let reduction_ratio = if json_bytes.is_empty() {
            0.0
        } else {
            saved_bytes as f64 / json_bytes.len() as f64 * 100.0
        };

        println!(
            "1000x1000 project size benchmark\n  json:       {} bytes ({:?})\n  compressed: {} bytes ({:?})\n  saved:      {} bytes ({:.2}% smaller)",
            json_bytes.len(),
            json_elapsed,
            compressed_bytes.len(),
            binary_elapsed,
            saved_bytes,
            reduction_ratio,
        );

        assert!(compressed_bytes.starts_with(BINARY_MAGIC));
        assert!(compressed_bytes[BINARY_MAGIC.len()..].starts_with(&ZSTD_MAGIC));
        assert!(compressed_bytes.len() < json_bytes.len());
    }
}
