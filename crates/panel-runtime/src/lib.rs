mod builtin_plugin;
mod commands;
mod config;
pub mod host_sync;
mod loader;
mod meta;
mod runtime;

pub use builtin_plugin::{BuiltinPanelError, BuiltinPanelPlugin};
pub use commands::command_from_descriptor;
pub use host_sync::{
    EMPTY_WORKSPACE_PANELS_JSON, HostSnapshotCache, build_host_snapshot_cached,
};
pub use loader::{BuiltinPanelDef, BuiltinPanelLoadError, register_builtin_panels};
pub use meta::{PanelMeta, PanelSizeMeta};
pub use runtime::{PanelGpuFrame, PanelRuntime, RuntimeDispatchResult, RuntimeKeyboardResult};

// パネルサブシステムの facade 再公開。
// desktop はパネル関連の型を panel-runtime 経由でのみ参照する (Phase 15 / ADR 017)。
pub use panel_api::{
    HostAction, PanelEvent, PanelMoveDirection, ResizeEdge, ServiceRequest, services,
};
pub use panel_html as html;
pub use panel_html::PanelSizeConstraints;
