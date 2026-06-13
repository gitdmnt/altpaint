mod json_store;
mod session;
mod canvas_size_presets;
mod workspace_presets;

pub use session::{DesktopSessionState, load_session_state, save_session_state};
pub use canvas_size_presets::{
    CanvasSizePreset, default_canvas_size_presets, load_canvas_size_presets,
    save_canvas_size_presets,
};
pub use workspace_presets::{
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION, WorkspacePreset, WorkspacePresetCatalog,
    load_workspace_preset_catalog, save_workspace_preset_catalog,
};
