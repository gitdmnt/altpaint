//! `desktop` クレートの薄いエントリポイントを定義する。
//!
//! 実際の責務は `app`・`runtime`・描画補助モジュールへ分割し、
//! このファイルは起動順序の宣言だけを担う。

mod app;
mod canvas_bridge;
mod config;
mod dialogs;
mod frame;
mod pens;
mod profiler;
mod runtime;
mod session;
mod wgpu_canvas;

use anyhow::Result;

use crate::config::DEFAULT_PROJECT_PATH;
use crate::runtime::DesktopRuntime;
use crate::session::startup_project_path;

/// デスクトップアプリケーションを既定プロジェクトパスで起動する。
fn main() -> Result<()> {
    DesktopRuntime::run(startup_project_path(DEFAULT_PROJECT_PATH))
}
