mod config;
mod dsl_loader;
mod dsl_panel;
pub mod dsl_to_html;
mod host_sync;
mod html_panel;
mod registry;

#[cfg(test)]
mod tests;

pub use dsl_panel::{altp_descriptor_to_panel_event, command_from_descriptor};
pub use html_panel::{HtmlPanelLoadError, HtmlPanelPlugin};
pub use registry::{PanelGpuFrame, PanelRuntime, RuntimeDispatchResult, RuntimeKeyboardResult};
