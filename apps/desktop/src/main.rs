//! `desktop` クレートの薄いエントリポイントを定義する。
//!
//! 実際の責務は `app`・`runtime`・描画補助モジュールへ分割し、
//! このファイルは起動順序の宣言だけを担う。

mod app;
mod canvas_bridge;
mod config;
mod dialogs;
mod frame;
mod profiler;
mod runtime;
mod wgpu_canvas;

use anyhow::Result;

use crate::config::DEFAULT_PROJECT_PATH;
use crate::runtime::DesktopRuntime;

/// デスクトップアプリケーションを既定プロジェクトパスで起動する。
fn main() -> Result<()> {
    DesktopRuntime::run(std::path::PathBuf::from(DEFAULT_PROJECT_PATH))
}
