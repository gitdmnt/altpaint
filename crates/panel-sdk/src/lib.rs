//! `panel-sdk` はパネル作者向けの正面入口である。

// panel_handler マクロが生成する typed payload 取り出し (`::panel_sdk::runtime::
// event_payload`) を、panel-sdk 自身のテスト内でも解決できるようにする自己エイリアス。
extern crate self as panel_sdk;

pub mod commands;
pub mod dom;
pub mod host;
pub mod runtime;
pub mod services;
pub mod state;

pub use panel_protocol::host_state;
pub use panel_protocol::keyboard;
pub use panel_protocol::names;
pub use panel_protocol::{
    RequestDescriptor, Diagnostic, DiagnosticLevel, HandlerEffects, StatePatch, StatePatchOp,
};
pub use panel_macros::{panel_handler, panel_init, panel_sync_host};

#[cfg(test)]
mod tests;
