//! `panel-sdk` はパネル作者向けの正面入口である。

// panel_handler マクロが生成する typed payload 取り出し (`::panel_sdk::runtime::
// event_payload`) を、panel-sdk 自身のテスト内でも解決できるようにする自己エイリアス。
extern crate self as panel_sdk;

pub mod commands;
pub mod dom;
pub mod runtime;
pub mod services;
pub mod shortcut;
pub mod state;
pub mod test_macros;

pub use panel_protocol::host_state;
pub use panel_protocol::keyboard;
pub use panel_protocol::names;

// typed handler payload (BL-141) / host_state DTO (BL-142) の derive を、パネルが
// 個別に serde / serde_json を Cargo 依存へ加えずに使えるよう再公開する。
// パネルは `#[derive(panel_sdk::serde::Deserialize)]` ではなく、`use panel_sdk::serde;`
// 後に `#[derive(serde::Deserialize)]` と書ける (derive マクロが `::serde` ではなく
// スコープ内 `serde` を解決するため)。
pub use serde;
pub use serde_json;
pub use panel_protocol::{
    RequestDescriptor, Diagnostic, DiagnosticLevel, HandlerEffects, StatePatch, StatePatchOp,
};
pub use panel_macros::{panel_handler, panel_init, panel_on_host_change};

#[cfg(test)]
mod tests;
