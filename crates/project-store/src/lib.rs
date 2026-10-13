//! SQLite プロジェクト永続化 (chunk codec / manifest)。
//!
//! PNG export とツールカタログ読み込みは BL-119 で desktop の `features/export` /
//! `features/tools` へ移管した。

mod project_file;
mod project_sqlite;
mod types;

pub use project_file::{
    load_koma_composite_from_path, load_page_from_path, load_project_from_path,
    load_project_manifest_from_path, save_project_to_path,
};
pub use project_sqlite::{
    PersistedKomaComposite, PersistedKomaCompositeSummary, ProjectKomaSummary, ProjectManifest,
    ProjectPageSummary, ProjectSaveMode,
};
pub use types::{CURRENT_PROJECT_FORMAT_VERSION, LoadedProject, ProjectStoreError};
