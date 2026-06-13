//! `panel-sdk` はパネル作者向けの正面入口である。

pub mod commands;
pub mod dom;
pub mod host;
pub mod runtime;
pub mod services;
pub mod state;

pub use panel_protocol::names;
pub use panel_protocol::{
    RequestDescriptor, Diagnostic, DiagnosticLevel, HandlerEffects, StatePatch, StatePatchOp,
};
pub use panel_macros::{panel_handler, panel_init, panel_sync_host};

#[cfg(test)]
mod tests;
