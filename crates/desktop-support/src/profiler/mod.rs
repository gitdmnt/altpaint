//! フレーム時間とキャンバス入力レイテンシを集計する軽量プロファイラ。
//!
//! 実行時の計測責務をデスクトップバイナリから分離し、タイトル更新やテストが
//! 純粋な集計ロジックへ依存できるようにする。

mod engine;
mod snapshot;
mod types;

pub use engine::DesktopProfiler;
pub use types::{PerformanceSnapshot, PresentTimings, StageStats, ValueStats};

#[cfg(test)]
mod tests;
