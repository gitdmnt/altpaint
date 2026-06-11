use std::path::Path;

use app_core::{Document, Page, PageId, WorkspaceLayout};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use app_core::WorkspaceUiState;

use crate::project_sqlite::{
    PersistedPanelSnapshot, ProjectIndex, ProjectSaveOptions, is_sqlite_project_path,
    load_page_from_sqlite_path, load_panel_snapshot_from_sqlite_path,
    load_project_from_sqlite_path, load_project_index_from_sqlite_path,
    save_project_to_sqlite_path,
};

pub const CURRENT_FORMAT_VERSION: u32 = 7;

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: Document,
    pub ui_state: WorkspaceUiState,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("unsupported altpaint project format version: {0}")]
    UnsupportedFormatVersion(u32),
    #[error("failed to compress project file: {0}")]
    Compress(#[source] std::io::Error),
    #[error("failed to decompress project file: {0}")]
    Decompress(#[source] std::io::Error),
    #[error("sqlite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
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

/// 指定パスが altpaint の sqlite プロジェクトファイルであることを確認する。
///
/// sqlite ヘッダーを持たないファイル (旧 JSON / ALTPBIN 形式を含む) はエラーになる。
fn ensure_sqlite_project(path: &Path) -> Result<(), StorageError> {
    if is_sqlite_project_path(path)? {
        return Ok(());
    }
    Err(StorageError::InvalidProject(format!(
        "not an altpaint sqlite project file: {}",
        path.display()
    )))
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
pub(crate) fn save_project_to_path_with_options(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    plugin_configs: &BTreeMap<String, Value>,
    options: ProjectSaveOptions,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    save_project_to_sqlite_path(path, document, workspace_layout, plugin_configs, options)
}

/// プロジェクト from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<LoadedProject, StorageError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_project_from_sqlite_path(path)
}

/// プロジェクト インデックス from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_project_index_from_path(path: impl AsRef<Path>) -> Result<ProjectIndex, StorageError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_project_index_from_sqlite_path(path)
}

/// ページ from パス を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
pub fn load_page_from_path(path: impl AsRef<Path>, page_id: PageId) -> Result<Page, StorageError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_page_from_sqlite_path(path, page_id)
}

/// パネル スナップショット from パス を読み込み、必要に応じて整形して返す。
pub fn load_panel_snapshot_from_path(
    path: impl AsRef<Path>,
    snapshot_id: &str,
) -> Result<Option<PersistedPanelSnapshot>, StorageError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_panel_snapshot_from_sqlite_path(path, snapshot_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{BlendMode, ColorRgba8, Document, LayerMask, Page, PageId, PanelId};
    use rusqlite::Connection;
    use std::fs;
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

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");
        let loaded = load_project_from_path(&path)
            .expect("load should succeed")
            .document;

        assert_eq!(loaded.work.title, document.work.title);
        assert_eq!(loaded.active_color, document.active_color);
        assert_eq!(
            loaded.work.pages[0].panels[0].bitmap.pixels,
            document.work.pages[0].panels[0].bitmap.pixels
        );

        let _ = fs::remove_file(path);
    }

    /// 保存→読込ラウンドトリップでレイヤー構造 (名前・可視・ブレンド・マスク・選択 index) が保全されることを検証する。
    #[test]
    fn save_and_load_roundtrip_preserves_layer_structure() {
        let path = temp_path("layer-structure");
        let mut document = multi_page_document();
        document.normalize_phase9_state();

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");
        let loaded = load_project_from_path(&path)
            .expect("load should succeed")
            .document;

        assert_eq!(loaded.work.pages.len(), document.work.pages.len());
        for (page, loaded_page) in document.work.pages.iter().zip(loaded.work.pages.iter()) {
            assert_eq!(loaded_page.panels.len(), page.panels.len());
            for (panel, loaded_panel) in page.panels.iter().zip(loaded_page.panels.iter()) {
                assert_eq!(loaded_panel.id, panel.id);
                assert_eq!(loaded_panel.bounds, panel.bounds);
                assert_eq!(loaded_panel.active_layer_index, panel.active_layer_index);
                assert_eq!(loaded_panel.created_layer_count, panel.created_layer_count);
                assert_eq!(loaded_panel.bitmap.pixels, panel.bitmap.pixels);
                assert_eq!(loaded_panel.layers.len(), panel.layers.len());
                for (layer, loaded_layer) in panel.layers.iter().zip(loaded_panel.layers.iter()) {
                    assert_eq!(loaded_layer.id, layer.id);
                    assert_eq!(loaded_layer.name, layer.name);
                    assert_eq!(loaded_layer.visible, layer.visible);
                    assert_eq!(loaded_layer.blend_mode, layer.blend_mode);
                    assert_eq!(loaded_layer.bitmap.pixels, layer.bitmap.pixels);
                    assert_eq!(
                        loaded_layer
                            .mask
                            .as_ref()
                            .map(|mask| (mask.width, mask.height, mask.alpha.clone())),
                        layer
                            .mask
                            .as_ref()
                            .map(|mask| (mask.width, mask.height, mask.alpha.clone())),
                    );
                }
            }
        }

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

    /// 保存 プロジェクト writes sqlite header and チャンク tables が期待どおりに動作することを検証する。
    #[test]
    fn save_project_writes_sqlite_header_and_chunk_tables() {
        let path = temp_path("sqlite-format");
        let mut document = Document::new(256, 256);
        document.set_active_color(ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
        draw_test_point(&mut document, 32, 48);

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

    /// 保存済み sqlite プロジェクトの format_version を書き換えるテストヘルパー。
    fn overwrite_format_version(path: &std::path::Path, version: u32) {
        let connection = Connection::open(path).expect("sqlite open should succeed");
        connection
            .execute(
                "UPDATE metadata SET value_json = ?1 WHERE key = 'format_version'",
                [version.to_string()],
            )
            .expect("format_version update should succeed");
    }

    /// 現行より新しい format_version の sqlite プロジェクトを拒否することを検証する。
    #[test]
    fn load_rejects_unknown_format_version() {
        let path = temp_path("version-unknown");
        save_project_to_path(
            &path,
            &small_document(),
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");
        overwrite_format_version(&path, CURRENT_FORMAT_VERSION + 1);

        let error = load_project_from_path(&path).expect_err("unknown version should fail");
        assert!(matches!(
            error,
            StorageError::UnsupportedFormatVersion(version) if version == CURRENT_FORMAT_VERSION + 1
        ));

        let _ = fs::remove_file(path);
    }

    /// 現行より古い format_version の sqlite プロジェクトを拒否することを検証する (旧版受理の全廃)。
    #[test]
    fn load_rejects_outdated_format_version() {
        let path = temp_path("version-outdated");
        save_project_to_path(
            &path,
            &small_document(),
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");
        overwrite_format_version(&path, CURRENT_FORMAT_VERSION - 1);

        let error = load_project_from_path(&path).expect_err("outdated version should fail");
        assert!(matches!(
            error,
            StorageError::UnsupportedFormatVersion(version) if version == CURRENT_FORMAT_VERSION - 1
        ));

        let _ = fs::remove_file(path);
    }

    /// sqlite 形式でないレガシー JSON プロジェクトファイルを拒否することを検証する。
    #[test]
    fn load_rejects_legacy_json_project_file() {
        let path = temp_path("legacy-json-rejected");
        let legacy = serde_json::json!({
            "format_version": CURRENT_FORMAT_VERSION,
            "document": small_document(),
        });
        fs::write(
            &path,
            serde_json::to_vec(&legacy).expect("serialize should succeed"),
        )
        .expect("write should succeed");

        let error = load_project_from_path(&path).expect_err("legacy json should fail");
        assert!(matches!(error, StorageError::InvalidProject(_)));

        let _ = fs::remove_file(path);
    }

}
