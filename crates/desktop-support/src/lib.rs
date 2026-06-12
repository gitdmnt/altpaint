mod config;
mod dialogs;
mod json_store;
mod profiler;
mod session;
mod canvas_size_presets;
mod workspace_presets;

pub use app_core::parse_document_size;
pub use config::{
    ACTIVE_KOMA_BORDER, ACTIVE_KOMA_FILL, ACTIVE_KOMA_MASK, ACTIVE_PANEL_BORDER,
    APP_BACKGROUND, BRUSH_PREVIEW_RING, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND,
    CANVAS_FRAME_BORDER, DEFAULT_PROJECT_FILE_NAME, FOOTER_HEIGHT, HEADER_HEIGHT, LASSO_LINE,
    KOMA_NAVIGATOR_ACTIVE, KOMA_NAVIGATOR_BACKGROUND, KOMA_NAVIGATOR_BORDER,
    KOMA_NAVIGATOR_KOMA, KOMA_PREVIEW_BORDER, KOMA_PREVIEW_FILL, WINDOW_HEIGHT, WINDOW_PADDING,
    WINDOW_TITLE, WINDOW_WIDTH, builtin_panels_dir, default_pen_dir, default_tool_dir,
};
pub use dialogs::{DesktopDialogs, NativeDesktopDialogs, normalize_project_path};
pub use profiler::{FrameProfiler, PresentTimings, StageStats, ValueStats};
pub use session::{
    DesktopSessionState, default_session_path, load_session_state, save_session_state,
    startup_project_path,
};
pub use canvas_size_presets::{
    CanvasSizePreset, default_canvas_size_preset_path, default_canvas_size_presets, load_canvas_size_presets,
    save_canvas_size_presets,
};
pub use workspace_presets::{
    WorkspacePreset, WorkspacePresetCatalog, default_workspace_preset_catalog,
    default_workspace_preset_path, load_workspace_preset_catalog, save_workspace_preset_catalog,
};
