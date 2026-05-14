//! `desktop` クレートの薄いエントリポイントを定義する。
//!
//! 実際の責務は `app`・`runtime`・描画補助モジュールへ分割し、
//! このファイルは起動順序の宣言だけを担う。

mod app;
mod frame;
mod runtime;
mod wgpu_canvas;

use anyhow::Result;
use desktop_support::{DEFAULT_PROJECT_PATH, startup_project_path};

use crate::runtime::DesktopRuntime;

/// アプリケーションのエントリーポイントとしてランタイムを起動する。
///
/// メインスレッドのスタックサイズは `.cargo/config.toml` の linker フラグ
/// (`-C link-arg=/STACK:...`) で 32MB に拡張している。
/// Phase 10 で Blitz/stylo の selector 解決が deep recursion で
/// 1MB 既定スタックを溢れさせる事象への対処。
fn main() -> Result<()> {
    DesktopRuntime::run(startup_project_path(DEFAULT_PROJECT_PATH))
}
