use std::fs;
use std::path::Path;

use app_core::Document;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltpaintProjectFile {
    pub format_version: u32,
    pub document: Document,
}

impl AltpaintProjectFile {
    pub fn new(document: &Document) -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            document: document.clone(),
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
    let path = path.as_ref();
    let project = AltpaintProjectFile::new(document);
    let serialized = serde_json::to_vec_pretty(&project).map_err(StorageError::Serialize)?;
    fs::write(path, serialized)?;
    Ok(())
}

pub fn load_document_from_path(path: impl AsRef<Path>) -> Result<Document, StorageError> {
    let bytes = fs::read(path)?;
    let project: AltpaintProjectFile =
        serde_json::from_slice(&bytes).map_err(StorageError::Deserialize)?;

    if project.format_version != CURRENT_FORMAT_VERSION {
        return Err(StorageError::UnsupportedFormatVersion(
            project.format_version,
        ));
    }

    Ok(project.document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Document;
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
        let _ = document.draw_point(5, 6);

        save_document_to_path(&path, &document).expect("save should succeed");
        let loaded = load_document_from_path(&path).expect("load should succeed");

        assert_eq!(loaded.work.title, document.work.title);
        assert_eq!(
            loaded.work.pages[0].panels[0].bitmap.pixels,
            document.work.pages[0].panels[0].bitmap.pixels
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_rejects_unknown_format_version() {
        let path = temp_path("version");
        let project = AltpaintProjectFile {
            format_version: CURRENT_FORMAT_VERSION + 1,
            document: Document::default(),
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
