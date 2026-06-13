//! `project_file` と `project_sqlite` が共有する型・定数。
//!
//! 両モジュール間の相互依存を断ち、依存を一方向
//! (`project_file` → `project_sqlite` → `types`) に保つための共有点。

use document_model::Document;
use panel_workspace::WorkspaceUiState;
use thiserror::Error;

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
