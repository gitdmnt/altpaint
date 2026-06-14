//! プロファイラが共有する集計型とステージ識別子を定義する。

use std::time::Duration;

/// 1 フレーム内で集計する主要計測区間を識別する。
///
/// 文字列ラベルではなく enum で識別することで、フレーム集計対象の区間を
/// 型レベルで固定し、`stats` マップの自由形式ラベルと混同しないようにする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameStage {
    /// フレーム全体の所要時間。
    FrameTotal,
    /// プレゼントフレーム準備区間。
    PrepareFrame,
    /// UI (パネル) 更新区間。
    UiUpdate,
    /// パネルサーフェス描画区間。
    PanelSurface,
    /// GPU 提示区間。
    PresentTotal,
}

impl FrameStage {
    /// レポート/統計マップ上で用いる正準ラベルを返す。
    pub const fn label(self) -> &'static str {
        match self {
            FrameStage::FrameTotal => "frame_total",
            FrameStage::PrepareFrame => "prepare_frame",
            FrameStage::UiUpdate => "ui_update",
            FrameStage::PanelSurface => "panel_surface",
            FrameStage::PresentTotal => "present_total",
        }
    }
}

/// GPU 提示の内訳時間を表す。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PresentTimings {
    pub upload: Duration,
    pub encode_and_submit: Duration,
    pub present: Duration,
    pub canvas_upload: Duration,
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

/// 窓内で集計した主要メトリクスのスナップショットを表す純データ。
///
/// タイトル文字列やレポート整形は保持せず、計測値のみを持つ。整形責務は
/// 呼び出し側 (desktop) にある。
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
