//! workspace feature スライス (provisional)。
//!
//! ワークスペースプリセット catalog を所有する。workspace_layout service の完全移行は
//! B7-part2。

mod workspace_presets;

pub(crate) use workspace_presets::{
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION, WorkspacePreset, WorkspacePresetCatalog,
    load_workspace_preset_catalog, save_workspace_preset_catalog,
};
