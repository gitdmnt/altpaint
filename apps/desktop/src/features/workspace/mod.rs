//! workspace feature スライス (provisional)。
//!
//! ワークスペースプリセット catalog を所有する。workspace_layout service の完全移行は
//! B7-part2。

mod workspace_presets;

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
