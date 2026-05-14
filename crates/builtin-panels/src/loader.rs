//! 同梱パネルの登録ロジック。
//!
//! Phase 10 着地後、本クレートが `crates/builtin-panels/<name>/` 以下の
//! `panel.html` / `panel.css` / `panel.meta.json` / `<wasm>` を順に読み、
//! `BuiltinPanelPlugin` として `PanelRuntime` に登録する。

use std::path::{Path, PathBuf};
use thiserror::Error;

use panel_runtime::{BuiltinPanelPlugin, PanelRuntime};

#[derive(Debug, Error)]
pub enum BuiltinPanelLoadError {
    #[error("builtin panel directory missing: {0}")]
    MissingDirectory(PathBuf),
    #[error("panel.meta.json missing: {0}")]
    MissingMeta(PathBuf),
    #[error("panel.html missing: {0}")]
    MissingHtml(PathBuf),
    #[error("wasm module missing: {0}")]
    MissingWasm(PathBuf),
    #[error("io: {0}")]
    Io(String),
}

/// 同梱パネル 1 件の定義。
pub struct BuiltinPanelDef {
    pub directory_name: &'static str,
    pub wasm_filename: &'static str,
}

/// 同梱パネル一覧。各エントリは依存少→多 順に並べる。
const BUILTIN_PANELS: &[BuiltinPanelDef] = &[
    BuiltinPanelDef {
        directory_name: "view-controls",
        wasm_filename: "builtin_panel_view_controls.wasm",
    },
    BuiltinPanelDef {
        directory_name: "job-progress",
        wasm_filename: "builtin_panel_job_progress.wasm",
    },
    BuiltinPanelDef {
        directory_name: "panel-list",
        wasm_filename: "builtin_panel_panel_list.wasm",
    },
    BuiltinPanelDef {
        directory_name: "tool-palette",
        wasm_filename: "builtin_panel_tool_palette.wasm",
    },
    BuiltinPanelDef {
        directory_name: "snapshot-panel",
        wasm_filename: "builtin_panel_snapshot_panel.wasm",
    },
    BuiltinPanelDef {
        directory_name: "pen-settings",
        wasm_filename: "builtin_panel_pen_settings.wasm",
    },
    BuiltinPanelDef {
        directory_name: "workspace-presets",
        wasm_filename: "builtin_panel_workspace_presets.wasm",
    },
    BuiltinPanelDef {
        directory_name: "app-actions",
        wasm_filename: "builtin_panel_app_actions.wasm",
    },
    BuiltinPanelDef {
        directory_name: "layers-panel",
        wasm_filename: "builtin_panel_layers_panel.wasm",
    },
    BuiltinPanelDef {
        directory_name: "text-flow",
        wasm_filename: "builtin_panel_text_flow.wasm",
    },
    BuiltinPanelDef {
        directory_name: "color-palette",
        wasm_filename: "builtin_panel_color_palette.wasm",
    },
];

/// 同梱パネルを `PanelRuntime` に登録する。
///
/// `assets_root` は同梱アセットのルート (リポジトリ内 `crates/builtin-panels/`)。
/// 戻り値はロードに失敗したパネルの診断メッセージ列 (空なら全成功)。
pub fn register_builtin_panels(
    runtime: &mut PanelRuntime,
    assets_root: &Path,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !assets_root.is_dir() {
        diagnostics.push(format!(
            "builtin panel assets root not found: {}",
            assets_root.display()
        ));
        return diagnostics;
    }
    for def in BUILTIN_PANELS {
        let directory = assets_root.join(def.directory_name);
        match BuiltinPanelPlugin::load(&directory, def.wasm_filename, None) {
            Ok(panel) => runtime.register_panel(Box::new(panel)),
            Err(error) => diagnostics.push(format!("{}: {error}", directory.display())),
        }
    }
    diagnostics
}
