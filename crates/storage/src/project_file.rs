use std::path::Path;

use app_core::{Document, Page, PageId, WorkspaceLayout};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use app_core::WorkspaceUiState;

use crate::project_sqlite::{
    PersistedKomaComposite, ProjectManifest, ProjectSaveOptions, file_has_sqlite_header,
    load_page_from_sqlite_path, load_koma_composite_from_sqlite_path,
    load_project_from_sqlite_path, load_project_manifest_from_sqlite_path,
    save_project_to_sqlite_path,
};

pub const CURRENT_PROJECT_FORMAT_VERSION: u32 = 7;

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: Document,
    pub ui_state: WorkspaceUiState,
}

#[derive(Debug, Error)]
pub enum ProjectStoreError {
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
    #[error("koma not found in project: page={page_id}, koma={koma_id}")]
    KomaNotFound { page_id: u64, koma_id: u64 },
    #[error("failed to access project file: {0}")]
    Io(#[from] std::io::Error),
}

/// 指定パスが altpaint の sqlite プロジェクトファイルであることを確認する。
///
/// sqlite ヘッダーを持たないファイル (旧 JSON / ALTPBIN 形式を含む) はエラーになる。
fn ensure_sqlite_project(path: &Path) -> Result<(), ProjectStoreError> {
    if file_has_sqlite_header(path)? {
        return Ok(());
    }
    Err(ProjectStoreError::InvalidProject(format!(
        "not an altpaint sqlite project file: {}",
        path.display()
    )))
}

pub fn save_project_to_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    panel_configs: &BTreeMap<String, Value>,
) -> Result<(), ProjectStoreError> {
    save_project_to_path_with_options(
        path,
        document,
        workspace_layout,
        panel_configs,
        ProjectSaveOptions::default(),
    )
}

pub(crate) fn save_project_to_path_with_options(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    panel_configs: &BTreeMap<String, Value>,
    options: ProjectSaveOptions,
) -> Result<(), ProjectStoreError> {
    let path = path.as_ref();
    save_project_to_sqlite_path(path, document, workspace_layout, panel_configs, options)
}

pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<LoadedProject, ProjectStoreError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_project_from_sqlite_path(path)
}

pub fn load_project_manifest_from_path(path: impl AsRef<Path>) -> Result<ProjectManifest, ProjectStoreError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_project_manifest_from_sqlite_path(path)
}

pub fn load_page_from_path(path: impl AsRef<Path>, page_id: PageId) -> Result<Page, ProjectStoreError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_page_from_sqlite_path(path, page_id)
}

pub fn load_koma_composite_from_path(
    path: impl AsRef<Path>,
    composite_id: &str,
) -> Result<Option<PersistedKomaComposite>, ProjectStoreError> {
    let path = path.as_ref();
    ensure_sqlite_project(path)?;
    load_koma_composite_from_sqlite_path(path, composite_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{Document, LayerMask, Page, PageId, KomaId};
    use editor_state::ColorRgba8;
    use raster::BlendMode;
    use rusqlite::Connection;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn small_document() -> Document {
        Document::new(64, 64)
    }

    fn draw_test_point(document: &mut Document, x: usize, y: usize) {
        let color = document.session.active_color.to_rgba8();
        if let Some(koma) = document.active_koma_mut() {
            let _ = koma.layers[0]
                .bitmap
                .draw_point_sized_rgba(x, y, color, 1, true);
            koma.composite_cache = koma.layers[0].bitmap.clone();
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-{name}-{unique}.altp"))
    }

    fn multi_page_document() -> Document {
        let mut document = Document::new(16, 16);
        document.work.title = "Phase 11 test".to_string();

        let mut second_koma = Document::new(8, 8).work.pages[0].komas[0].clone();
        second_koma.id = KomaId(2);
        second_koma.layers[0].name = "Blue layer".to_string();
        second_koma.layers[0]
            .bitmap
            .set_pixel_rgba(2, 3, [0x22, 0x44, 0xaa, 0xff]);
        second_koma.composite_cache = second_koma.layers[0].bitmap.clone();

        let mut third_koma = Document::new(8, 8).work.pages[0].komas[0].clone();
        third_koma.id = KomaId(3);
        third_koma.layers[0]
            .bitmap
            .set_pixel_rgba(1, 1, [0x55, 0x99, 0x22, 0xff]);
        third_koma.composite_cache = third_koma.layers[0].bitmap.clone();
        third_koma.layers.push(app_core::RasterLayer {
            id: app_core::LayerNodeId(99),
            name: "Overlay".to_string(),
            visible: true,
            blend_mode: BlendMode::Multiply,
            bitmap: {
                let mut bitmap = raster::RgbaBitmap::transparent(8, 8);
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
        document.work.pages[0].komas.push(second_koma);
        document.work.pages.push(Page {
            id: PageId(20),
            width: 8,
            height: 8,
            komas: vec![third_koma],
        });
        document
    }

    #[test]
    fn save_and_load_roundtrip_preserves_document() {
        let path = temp_path("roundtrip");
        let mut document = small_document();
        document
            .session
            .set_active_color(ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff));
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
        assert_eq!(loaded.session.active_color, document.session.active_color);
        assert_eq!(
            loaded.work.pages[0].komas[0].composite_cache.pixels,
            document.work.pages[0].komas[0].composite_cache.pixels
        );

        let _ = fs::remove_file(path);
    }

    /// 保存→読込ラウンドトリップでレイヤー構造 (名前・可視・ブレンド・マスク・選択 index) が保全されることを検証する。
    #[test]
    fn save_and_load_roundtrip_preserves_layer_structure() {
        let path = temp_path("layer-structure");
        let mut document = multi_page_document();
        document.normalize_after_load();

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
            assert_eq!(loaded_page.komas.len(), page.komas.len());
            for (koma, loaded_koma) in page.komas.iter().zip(loaded_page.komas.iter()) {
                assert_eq!(loaded_koma.id, koma.id);
                assert_eq!(loaded_koma.bounds, koma.bounds);
                assert_eq!(loaded_koma.active_layer_index, koma.active_layer_index);
                assert_eq!(loaded_koma.created_layer_count, koma.created_layer_count);
                assert_eq!(loaded_koma.composite_cache.pixels, koma.composite_cache.pixels);
                assert_eq!(loaded_koma.layers.len(), koma.layers.len());
                for (layer, loaded_layer) in koma.layers.iter().zip(loaded_koma.layers.iter()) {
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

    #[test]
    fn save_and_load_roundtrip_preserves_workspace_layout() {
        let path = temp_path("workspace");
        let document = small_document();
        let workspace_layout = WorkspaceLayout {
            panels: vec![
                app_core::WorkspacePanelState {
                    id: "builtin.layers".to_string(),
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

    #[test]
    fn save_and_load_roundtrip_preserves_panel_configs() {
        let path = temp_path("plugin-configs");
        let document = small_document();
        let mut panel_configs = BTreeMap::new();
        panel_configs.insert(
            "builtin.tool-palette".to_string(),
            serde_json::json!({ "pen_shortcut": "P", "eraser_shortcut": "E" }),
        );

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &panel_configs,
        )
        .expect("save should succeed");
        let loaded = load_project_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.ui_state.panel_configs, panel_configs);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_project_writes_sqlite_header_and_chunk_tables() {
        let path = temp_path("sqlite-format");
        let mut document = Document::new(256, 256);
        document
            .session
            .set_active_color(ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
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

    #[test]
    fn load_project_manifest_reports_pages_komas_and_composites() {
        let path = temp_path("project-manifest");
        let document = multi_page_document();

        save_project_to_path(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
        )
        .expect("save should succeed");

        let manifest =
            load_project_manifest_from_path(&path).expect("manifest load should succeed");

        assert_eq!(manifest.format_version, CURRENT_PROJECT_FORMAT_VERSION);
        assert_eq!(manifest.pages.len(), 2);
        assert_eq!(manifest.pages[0].id, PageId(10));
        assert_eq!(manifest.pages[0].komas.len(), 2);
        assert_eq!(manifest.pages[1].id, PageId(20));
        assert_eq!(manifest.pages[1].komas[0].layer_count, 2);
        assert_eq!(manifest.composites.len(), 3);
        assert!(
            manifest
                .composites
                .iter()
                .any(|composite| composite.composite_id == "page:10:koma:2:current")
        );

        let _ = fs::remove_file(path);
    }

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
        assert_eq!(page.komas.len(), 1);
        assert_eq!(page.komas[0].id, KomaId(3));
        assert_eq!(
            page.komas[0].composite_cache.pixel_rgba(1, 1),
            Some([0x55, 0x99, 0x22, 0xff])
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_koma_composite_restores_current_composited_bitmap() {
        let path = temp_path("composite");
        let document = multi_page_document();
        let expected = document.work.pages[0].komas[1].composite_cache.pixel_rgba(2, 3);

        save_project_to_path_with_options(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
            ProjectSaveOptions::default(),
        )
        .expect("save should succeed");

        let composite = load_koma_composite_from_path(&path, "page:10:koma:2:current")
            .expect("composite load should succeed")
            .expect("composite should exist");

        assert_eq!(composite.summary.page_id, PageId(10));
        assert_eq!(composite.summary.koma_id, KomaId(2));
        assert_eq!(composite.bitmap.pixel_rgba(2, 3), expected);

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
        overwrite_format_version(&path, CURRENT_PROJECT_FORMAT_VERSION + 1);

        let error = load_project_from_path(&path).expect_err("unknown version should fail");
        assert!(matches!(
            error,
            ProjectStoreError::UnsupportedFormatVersion(version) if version == CURRENT_PROJECT_FORMAT_VERSION + 1
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
        overwrite_format_version(&path, CURRENT_PROJECT_FORMAT_VERSION - 1);

        let error = load_project_from_path(&path).expect_err("outdated version should fail");
        assert!(matches!(
            error,
            ProjectStoreError::UnsupportedFormatVersion(version) if version == CURRENT_PROJECT_FORMAT_VERSION - 1
        ));

        let _ = fs::remove_file(path);
    }

    /// BL-032 ゴールデン: composite を永続化しない保存からの読込が、
    /// app-core のレイヤー合成と同一の composite を再計算することを検証する。
    #[test]
    fn load_recomputes_composite_equal_to_app_core_compositing() {
        let path = temp_path("recompute-composite");
        let mut document = Document::new(8, 8);
        {
            let koma = document.active_koma_mut().expect("koma should exist");
            let _ = koma.layers[0].bitmap.set_pixel_rgba(1, 1, [255, 0, 0, 255]);
            let _ = koma.layers[0].bitmap.set_pixel_rgba(2, 1, [0, 255, 0, 128]);
        }
        document.add_raster_layer();
        {
            let koma = document.active_koma_mut().expect("koma should exist");
            let top = &mut koma.layers[1];
            let _ = top.bitmap.set_pixel_rgba(1, 1, [50, 80, 200, 128]);
            let _ = top.bitmap.set_pixel_rgba(2, 1, [50, 80, 200, 128]);
            top.mask = Some(LayerMask {
                width: 8,
                height: 8,
                alpha: vec![128; 64],
            });
        }
        // set_active_layer_blend_mode が app-core 側の合成で composite_cache を再計算する。
        document.set_active_layer_blend_mode(BlendMode::Multiply);

        save_project_to_path_with_options(
            &path,
            &document,
            &WorkspaceLayout::default(),
            &BTreeMap::new(),
            ProjectSaveOptions {
                persist_current_composites: false,
                ..ProjectSaveOptions::default()
            },
        )
        .expect("save should succeed");
        let loaded = load_project_from_path(&path)
            .expect("load should succeed")
            .document;

        let expected = &document.work.pages[0].komas[0].composite_cache;
        let recomputed = &loaded.work.pages[0].komas[0].composite_cache;
        assert_eq!(recomputed.pixels, expected.pixels);
        // 固定アンカー: Multiply + mask(128) の代表画素 (opaque dst / 半透明 dst)。
        assert_eq!(recomputed.pixel_rgba(1, 1), Some([204, 0, 0, 255]));
        assert_eq!(recomputed.pixel_rgba(2, 1), Some([0, 106, 0, 160]));

        let _ = fs::remove_file(path);
    }

    /// sqlite 形式でないレガシー JSON プロジェクトファイルを拒否することを検証する。
    #[test]
    fn load_rejects_legacy_json_project_file() {
        let path = temp_path("legacy-json-rejected");
        let legacy = serde_json::json!({
            "format_version": CURRENT_PROJECT_FORMAT_VERSION,
            "document": small_document(),
        });
        fs::write(
            &path,
            serde_json::to_vec(&legacy).expect("serialize should succeed"),
        )
        .expect("write should succeed");

        let error = load_project_from_path(&path).expect_err("legacy json should fail");
        assert!(matches!(error, ProjectStoreError::InvalidProject(_)));

        let _ = fs::remove_file(path);
    }

}
