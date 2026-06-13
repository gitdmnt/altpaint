//! OS 依存の境界 (ネイティブダイアログ・パス解決) を集約する。
//!
//! BL-112 / BL-113。`desktop-support::dialogs` をここへ移し、パス解決を
//! `dirs` ベース (配布対応) へ置き換える。OS 差異は `dirs` /
//! `tinyfiledialogs` が吸収する (OS 固有 cfg は書かない)。

pub(crate) mod dialogs;
pub(crate) mod paths;

pub(crate) use dialogs::{DesktopDialogs, NativeDesktopDialogs, normalize_project_path};
pub(crate) use paths::{
    DEFAULT_PROJECT_FILE_NAME, builtin_panels_dir, default_canvas_size_preset_path,
    default_project_path, default_session_path, default_workspace_preset_path, pen_dir, tool_dir,
};
