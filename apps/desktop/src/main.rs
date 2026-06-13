//! `desktop` クレートの薄いエントリポイントを定義する。
//!
//! 実際の責務は `app`・`event_loop`・描画補助モジュールへ分割し、
//! このファイルは起動順序の宣言だけを担う。

mod app;
mod present_quads;
mod event_loop;
mod profiling;
mod wgpu_canvas;

use anyhow::Result;
use desktop_support::{DEFAULT_PROJECT_FILE_NAME, startup_project_path};

use crate::event_loop::DesktopEventLoop;

/// メインスレッドのスタックサイズは `.cargo/config.toml` の linker フラグ
/// (`-C link-arg=/STACK:...`) で 32MB に拡張している。
/// Phase 10 で Blitz/stylo の selector 解決が deep recursion で
/// 1MB 既定スタックを溢れさせる事象への対処。
fn main() -> Result<()> {
    DesktopEventLoop::run(startup_project_path(DEFAULT_PROJECT_FILE_NAME))
}
