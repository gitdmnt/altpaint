//! `builtin-panels` — altpaint 同梱パネルの登録ハブ。
//!
//! Phase 10 で DSL (`.altp-panel`) を廃止し、各パネルは
//! `panel.html` / `panel.css` / Wasm ハンドラの組合せで構築される。
//! 本クレートは個別パネル定義 + ホスト側登録ロジックを集約する。
//!
//! `BuiltinPanelPlugin` 本体は `panel-runtime` 側に存在する (registry の downcast 都合)。
//! 本クレートからは convenience として再エクスポートする。

mod loader;

pub use loader::{BuiltinPanelDef, BuiltinPanelLoadError, register_builtin_panels};
pub use panel_runtime::{BuiltinPanelError, BuiltinPanelPlugin};
