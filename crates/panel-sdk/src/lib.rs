//! `panel-sdk` は panel 作者向けの唯一の正面入口である。
//!
//! `panel-macros` は物理的には別 crate だが、作者はこの crate だけへ依存し、
//! `panel_sdk::panel_init` / `panel_sdk::panel_handler` / `panel_sdk::panel_sync_host`
//! を使う前提とする。

mod builder;
pub mod commands;
pub mod host;
pub mod runtime;
pub mod services;
pub mod state;

pub use builder::{CommandBuilder, command, handler_result};
pub use panel_macros::{panel_handler, panel_init, panel_sync_host};
pub use panel_schema::{
    CommandDescriptor, Diagnostic, DiagnosticLevel, HandlerResult, PanelEventRequest,
    PanelInitRequest, PanelInitResponse, StatePatch, StatePatchOp,
};

#[cfg(test)]
mod tests;
