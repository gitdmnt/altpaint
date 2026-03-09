//! プロファイラが共有する集計型を定義する。

use std::time::Duration;

/// GPU 提示の内訳時間を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PresentTimings {
    pub upload: Duration,
    pub encode_and_submit: Duration,
    pub present: Duration,
    pub base_upload: Duration,
    pub overlay_upload: Duration,
    pub canvas_upload: Duration,
    pub base_upload_bytes: u64,
    pub overlay_upload_bytes: u64,
    pub canvas_upload_bytes: u64,
}

/// 計測ラベルごとの回数・合計・最大値を保持する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StageStats {
    pub calls: u64,
    pub total: Duration,
    pub max: Duration,
}

/// 数値メトリクスのサンプル数・合計・最大値を保持する。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ValueStats {
    pub samples: u64,
    pub total: f64,
    pub max: f64,
}

/// ウィンドウタイトルへ表示する集計済みスナップショットを表す。
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PerformanceSnapshot {
    pub fps: f64,
    pub frame_ms: f64,
    pub prepare_ms: f64,
    pub ui_update_ms: f64,
    pub panel_surface_ms: f64,
    pub present_ms: f64,
    pub canvas_latency_ms: f64,
    pub canvas_present_hz: f64,
    pub canvas_sample_hz: f64,
}
