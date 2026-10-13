//! フレーム時間とキャンバス入力レイテンシを集計する軽量プロファイラ。
//!
//! 実行時の計測責務を全層から参照できる水平土台として切り出す。タイトルや
//! レポートの整形責務は持たず、純粋な集計とインターバル判定のみを行う
//! (整形は呼び出し側で `FrameReport` / `PerformanceSnapshot` を消費する)。
//!
//! B8 の encoder 集約 (BL-133) 等の効果計測で gpu-paint / paint-engine の内部にも
//! 計測点を挿せるよう、下層から循環依存なしに参照できる位置に置く (§1.3)。

mod engine;
mod types;

pub use engine::{FrameProfiler, FrameReport, PERFORMANCE_SNAPSHOT_WINDOW};
pub use types::{FrameStage, PerformanceSnapshot, PresentTimings, StageStats, ValueStats};

#[cfg(test)]
mod tests;
