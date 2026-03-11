mod config;
mod dsl_loader;
mod dsl_panel;
mod host_sync;
mod registry;

#[cfg(test)]
mod tests;

pub use dsl_panel::command_from_descriptor;
pub use registry::{PanelRuntime, RuntimeDispatchResult, RuntimeKeyboardResult};
