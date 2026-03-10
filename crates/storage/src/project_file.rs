use std::fs;
use std::io::Cursor;
use std::path::Path;

use app_core::{Document, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use workspace_persistence::{PluginConfigs, WorkspaceUiState};

fn workspace_layout_is_empty(layout: &WorkspaceLayout) -> bool {
    layout.panels.is_empty()
}

pub const CURRENT_FORMAT_VERSION: u32 = 6;
const BINARY_MAGIC: &[u8; 8] = b"ALTPBIN\0";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];
const ZSTD_COMPRESSION_LEVEL: i32 = 3;

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: Document,
    pub ui_state: WorkspaceUiState,
}

impl LoadedProject {
    pub fn workspace_layout(&self) -> &WorkspaceLayout {
        &self.ui_state.workspace_layout
    }

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
    #[error("failed to deserialize legacy json project file: {0}")]
    DeserializeLegacyJson(#[source] serde_json::Error),
    #[error("failed to access project file: {0}")]
    Io(#[from] std::io::Error),
}

fn serialize_project(project: &AltpaintProjectFile) -> Result<Vec<u8>, StorageError> {
    let encoded = rmp_serde::to_vec(project).map_err(StorageError::Encode)?;
    let compressed = zstd::stream::encode_all(Cursor::new(encoded), ZSTD_COMPRESSION_LEVEL)
        .map_err(StorageError::Compress)?;
    let mut bytes = Vec::with_capacity(BINARY_MAGIC.len() + compressed.len());
    bytes.extend_from_slice(BINARY_MAGIC);
    bytes.extend_from_slice(&compressed);
    Ok(bytes)
}

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

pub fn save_project_to_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    plugin_configs: &BTreeMap<String, Value>,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    let project = AltpaintProjectFile::new(document, workspace_layout, plugin_configs);
    let serialized = serialize_project(&project)?;
    fs::write(path, serialized)?;
    Ok(())
}

pub fn load_document_from_path(path: impl AsRef<Path>) -> Result<Document, StorageError> {
    load_project_from_path(path).map(|project| project.document)
}

pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<LoadedProject, StorageError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{ColorRgba8, Document};
    use std::time::Instant;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn small_document() -> Document {
        Document::new(64, 64)
    }

    fn benchmark_document(width: usize, height: usize) -> Document {
        let mut document = Document::new(width, height);
        document.set_active_color(ColorRgba8::new(0x11, 0x66, 0xcc, 0xff));

        for offset in (0..width.min(height)).step_by(25) {
            let _ = document.draw_point(offset, offset);
        }

        document
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-{name}-{unique}.altp"))
    }

    #[test]
    fn save_and_load_roundtrip_preserves_document() {
        let path = temp_path("roundtrip");
        let mut document = small_document();
        document.set_active_color(ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff));
        let _ = document.draw_point(5, 6);

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

    #[test]
    fn save_and_load_roundtrip_preserves_workspace_layout() {
        let path = temp_path("workspace");
        let document = small_document();
        let workspace_layout = WorkspaceLayout {
            panels: vec![
                app_core::WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                    position: None,
                    size: None,
                },
                app_core::WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
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

    #[test]
    fn save_project_writes_binary_header_and_reduces_size_vs_json() {
        let path = temp_path("binary-format");
        let mut document = Document::new(256, 256);
        document.set_active_color(ColorRgba8::new(0x12, 0x34, 0x56, 0xff));
        let _ = document.draw_point(32, 48);

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

        assert!(saved.starts_with(BINARY_MAGIC));
        assert!(saved[BINARY_MAGIC.len()..].starts_with(&ZSTD_MAGIC));
        assert!(saved.len() < legacy_json.len());

        let _ = fs::remove_file(path);
    }

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
