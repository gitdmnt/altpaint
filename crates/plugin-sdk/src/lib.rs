//! `plugin-sdk` は plugin 作者向けの正面入口である。

mod builder;
pub mod commands;
pub mod host;
pub mod runtime;
pub mod services;
pub mod state;

pub use builder::{CommandBuilder, command, handler_result};
pub use panel_schema::{
    CommandDescriptor, Diagnostic, DiagnosticLevel, HandlerResult, PanelEventRequest,
    PanelInitRequest, PanelInitResponse, StatePatch, StatePatchOp,
};
pub use plugin_macros::{panel_handler, panel_init, panel_sync_host};

#[cfg(test)]
mod tests;
