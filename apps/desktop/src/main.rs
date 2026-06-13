//! `desktop` クレートの薄いエントリポイントを定義する。
//!
//! 実際の責務は `app`・`event_loop`・描画補助モジュールへ分割し、
//! このファイルは起動順序の宣言だけを担う。

mod app;
mod features;
mod platform;
mod present_quads;
mod event_loop;
mod profiling;
mod theme;
mod wgpu_canvas;

use anyhow::Result;

use crate::event_loop::DesktopEventLoop;
use crate::platform::{default_project_path, default_session_path};

/// メインスレッドのスタックサイズは `.cargo/config.toml` の linker フラグ
/// (`-C link-arg=/STACK:...`) で 32MB に拡張している。
/// Phase 10 で Blitz/stylo の selector 解決が deep recursion で
/// 1MB 既定スタックを溢れさせる事象への対処。
fn main() -> Result<()> {
    DesktopEventLoop::run(startup_project_path())
}

/// 起動時に開くプロジェクトパスを解決する。
///
/// 直近のセッションに保存された `last_project_path` があればそれを、無ければ
/// 既定プロジェクトパスを返す。パス解決は `dirs` ベース (BL-113)。
fn startup_project_path() -> std::path::PathBuf {
    desktop_support::load_session_state(default_session_path())
        .and_then(|state| state.last_project_path)
        .unwrap_or_else(default_project_path)
}
