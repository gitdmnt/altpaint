mod config;
mod dialogs;
mod profiler;
mod session;
mod templates;
mod workspace_presets;

pub use config::{
    APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND, CANVAS_FRAME_BORDER,
    DEFAULT_PROJECT_PATH, FOOTER_HEIGHT, HEADER_HEIGHT, INPUT_LATENCY_TARGET_MS,
    INPUT_SAMPLING_TARGET_HZ, PANEL_FRAME_BACKGROUND, PANEL_FRAME_BORDER,
    PERFORMANCE_SNAPSHOT_WINDOW, SIDEBAR_BACKGROUND, SIDEBAR_WIDTH, TEXT_PRIMARY, TEXT_SECONDARY,
    WINDOW_HEIGHT, WINDOW_PADDING, WINDOW_TITLE, WINDOW_WIDTH, default_panel_dir, default_pen_dir,
    default_tool_dir, parse_document_size,
};
pub use dialogs::{
    DesktopDialogs, NativeDesktopDialogs, normalize_project_path, normalize_workspace_preset_path,
};
pub use profiler::{DesktopProfiler, PerformanceSnapshot, PresentTimings, StageStats, ValueStats};
pub use session::{
    DesktopSessionState, default_session_path, load_session_state, save_session_state,
    startup_project_path,
};
pub use templates::{
    CanvasTemplate, default_canvas_template_path, default_canvas_templates,
    load_canvas_templates, save_canvas_templates,
};
pub use workspace_presets::{
    WorkspacePreset, WorkspacePresetCatalog, default_workspace_preset_path,
    default_workspace_preset_catalog, load_workspace_preset_catalog, save_workspace_preset_catalog,
};
