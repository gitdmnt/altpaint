mod builtin_plugin;
mod commands;
mod config;
pub mod host_sync;
mod meta;
mod registry;

pub use builtin_plugin::{BuiltinPanelError, BuiltinPanelPlugin};
pub use commands::command_from_descriptor;
pub use host_sync::{
    EMPTY_WORKSPACE_PANELS_JSON, HostSnapshotCache, build_host_snapshot_cached,
};
pub use meta::{PanelMeta, PanelSizeMeta};
pub use panel_html_experiment::PanelSizeConstraints;
pub use registry::{PanelGpuFrame, PanelRuntime, RuntimeDispatchResult, RuntimeKeyboardResult};
