//! workspace feature スライス (BL-111)。
//!
//! ワークスペースプリセット catalog (`workspace_presets`)、`workspace_io.*` service
//! ハンドラ (`service`)、`workspace_layout.*` service ハンドラ (`layout_service`) を
//! 所有する。
//!
//! B7 で `app/services/{workspace_io,workspace_layout}.rs` から service を移設した。

mod layout_service;
mod service;
mod workspace_presets;

pub(crate) use layout_service::handle_workspace_layout_service_request;
pub(crate) use service::handle_workspace_service_request;
pub(crate) use workspace_presets::{
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION, WorkspacePreset, WorkspacePresetCatalog,
    load_workspace_preset_catalog, save_workspace_preset_catalog,
};

/// workspace feature のサブ状態 (BL-110)。
///
/// DesktopApp に散在していたプリセットカタログと選択中プリセット ID をまとめる。
pub(crate) struct WorkspaceState {
    /// 永続化されるワークスペースプリセットカタログ。
    pub(crate) presets: WorkspacePresetCatalog,
    /// 現在選択中のワークスペースプリセット ID。
    pub(crate) active_preset_id: String,
}
