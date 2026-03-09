//! タイトル表示向けスナップショット整形ロジックを保持する。

use crate::config::{INPUT_LATENCY_TARGET_MS, INPUT_SAMPLING_TARGET_HZ, WINDOW_TITLE};

use super::types::PerformanceSnapshot;

impl PerformanceSnapshot {
    /// ウィンドウタイトル向けの短い表示文字列を生成する。
    pub fn title_text(&self) -> String {
        let latency_marker =
            if self.canvas_latency_ms > 0.0 && self.canvas_latency_ms <= INPUT_LATENCY_TARGET_MS {
                "ok"
            } else {
                "ng"
            };
        let sample_marker = if self.canvas_sample_hz >= INPUT_SAMPLING_TARGET_HZ {
            "ok"
        } else {
            "ng"
        };
        let present_marker = if self.canvas_present_hz >= INPUT_SAMPLING_TARGET_HZ {
            "ok"
        } else {
            "ng"
        };
        format!(
            "{WINDOW_TITLE} | {:>5.1} fps | frame {:>5.2}ms | prep {:>5.2}ms | ui {:>5.2}ms | panel {:>5.2}ms | present {:>5.2}ms | ink {:>5.2}ms {} | motion {:>6.1}Hz {} | input {:>6.1}Hz {}",
            self.fps,
            self.frame_ms,
            self.prepare_ms,
            self.ui_update_ms,
            self.panel_surface_ms,
            self.present_ms,
            self.canvas_latency_ms,
            latency_marker,
            self.canvas_present_hz,
            present_marker,
            self.canvas_sample_hz,
            sample_marker,
        )
    }
}
