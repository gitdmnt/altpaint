use std::fs;
use std::path::Path;

use app_core::{Document, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub document: Document,
    pub workspace_layout: WorkspaceLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltpaintProjectFile {
    pub format_version: u32,
    pub document: Document,
    #[serde(default)]
    pub workspace_layout: WorkspaceLayout,
}

impl AltpaintProjectFile {
    pub fn new(document: &Document, workspace_layout: &WorkspaceLayout) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            document: document.clone(),
            workspace_layout: workspace_layout.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("unsupported altpaint project format version: {0}")]
    UnsupportedFormatVersion(u32),
    #[error("failed to serialize project file: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to deserialize project file: {0}")]
    Deserialize(#[source] serde_json::Error),
    #[error("failed to access project file: {0}")]
    Io(#[from] std::io::Error),
}

pub fn save_document_to_path(
    path: impl AsRef<Path>,
    document: &Document,
) -> Result<(), StorageError> {
    save_project_to_path(path, document, &WorkspaceLayout::default())
}

pub fn save_project_to_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
) -> Result<(), StorageError> {
    let path = path.as_ref();
    let project = AltpaintProjectFile::new(document, workspace_layout);
    let serialized = serde_json::to_vec_pretty(&project).map_err(StorageError::Serialize)?;
    fs::write(path, serialized)?;
    Ok(())
}

pub fn load_document_from_path(path: impl AsRef<Path>) -> Result<Document, StorageError> {
    load_project_from_path(path).map(|project| project.document)
}

pub fn load_project_from_path(path: impl AsRef<Path>) -> Result<LoadedProject, StorageError> {
    let bytes = fs::read(path)?;
    let project: AltpaintProjectFile =
        serde_json::from_slice(&bytes).map_err(StorageError::Deserialize)?;

    if !(1..=CURRENT_FORMAT_VERSION).contains(&project.format_version) {
        return Err(StorageError::UnsupportedFormatVersion(
            project.format_version,
        ));
    }

    Ok(LoadedProject {
        document: project.document,
        workspace_layout: project.workspace_layout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::{ColorRgba8, Document};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("altpaint-{name}-{unique}.altp.json"))
    }

    #[test]
    fn save_and_load_roundtrip_preserves_document() {
        let path = temp_path("roundtrip");
        let mut document = Document::default();
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
        let document = Document::default();
        let workspace_layout = WorkspaceLayout {
            panels: vec![
                app_core::WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                },
                app_core::WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                },
            ],
        };

        save_project_to_path(&path, &document, &workspace_layout).expect("save should succeed");
        let loaded = load_project_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.workspace_layout, workspace_layout);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_version_1_file_defaults_workspace_layout() {
        let path = temp_path("version1");
        let project = serde_json::json!({
            "format_version": 1,
            "document": Document::default(),
        });
        fs::write(
            &path,
            serde_json::to_vec(&project).expect("serialize should succeed"),
        )
        .expect("write should succeed");

        let loaded = load_project_from_path(&path).expect("load should succeed");
        assert!(loaded.workspace_layout.panels.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_rejects_unknown_format_version() {
        let path = temp_path("version");
        let project = AltpaintProjectFile {
            format_version: CURRENT_FORMAT_VERSION + 1,
            document: Document::default(),
            workspace_layout: WorkspaceLayout::default(),
        };
        let serialized = serde_json::to_vec(&project).expect("serialize should succeed");
        fs::write(&path, serialized).expect("write should succeed");

        let error = load_document_from_path(&path).expect_err("unknown version should fail");
        assert!(matches!(
            error,
            StorageError::UnsupportedFormatVersion(version) if version == CURRENT_FORMAT_VERSION + 1
        ));

        let _ = fs::remove_file(path);
    }
}
