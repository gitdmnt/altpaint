//! `.altp-panel` DSL の公開入口をまとめるクレートルート。
//!
//! パーサー、検証、正規化済み定義を責務ごとに分割し、呼び出し側には
//! 安定した関数と型だけを再公開する。

mod ast;
mod parser;
mod validation;

pub use ast::{
    AttrValue, PanelAst, PanelDefinition, PanelDslError, PanelHeaderAst, PanelManifest, RuntimeAst,
    RuntimeDefinition, StateField, StateFieldAst, StateType, ViewElement, ViewElementAst, ViewNode,
    ViewNodeAst,
};
pub use parser::parse_panel_source;
pub use validation::{load_panel_file, validate_panel_ast};

#[cfg(test)]
mod tests;
