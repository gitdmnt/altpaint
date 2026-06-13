//! `frame-profiler` の計測データをタイトル/レポート文字列へ整形する。
//!
//! 整形責務は計測本体 (`frame-profiler`) から分離され、desktop 側が
//! `PerformanceSnapshot` / `FrameReport` を消費してウィンドウタイトルと
//! プロファイルログを組み立てる。表示用の閾値 (目標値) もここに置く。

use crate::theme::{INPUT_LATENCY_TARGET_MS, INPUT_SAMPLING_TARGET_HZ, WINDOW_TITLE};
use frame_profiler::{FrameReport, PerformanceSnapshot};

/// 最新スナップショットからウィンドウタイトル文字列を組み立てる。
///
/// スナップショットが無ければベースタイトルを返す。
pub(crate) fn window_title(snapshot: Option<PerformanceSnapshot>) -> String {
    snapshot.map_or_else(|| WINDOW_TITLE.to_string(), title_text)
}

/// スナップショットを 1 行のタイトル文字列へ整形する。
fn title_text(snapshot: PerformanceSnapshot) -> String {
    let latency_marker = if snapshot.canvas_latency_ms > 0.0
        && snapshot.canvas_latency_ms <= INPUT_LATENCY_TARGET_MS
    {
        "ok"
    } else {
        "ng"
    };
    let sample_marker = if snapshot.canvas_sample_hz >= INPUT_SAMPLING_TARGET_HZ {
        "ok"
    } else {
        "ng"
    };
    let present_marker = if snapshot.canvas_present_hz >= INPUT_SAMPLING_TARGET_HZ {
        "ok"
    } else {
        "ng"
    };
    format!(
        "{WINDOW_TITLE} | {:>5.1} fps | frame {:>5.2}ms | prep {:>5.2}ms | ui {:>5.2}ms | panel {:>5.2}ms | present {:>5.2}ms | ink {:>5.2}ms {} | motion {:>6.1}Hz {} | input {:>6.1}Hz {}",
        snapshot.fps,
        snapshot.frame_ms,
        snapshot.prepare_ms,
        snapshot.ui_update_ms,
        snapshot.panel_surface_ms,
        snapshot.present_ms,
        snapshot.canvas_latency_ms,
        latency_marker,
        snapshot.canvas_present_hz,
        present_marker,
        snapshot.canvas_sample_hz,
        sample_marker,
    )
}

/// レポート区間の計測データを `[profile]` 行群として標準エラーへ出力する。
pub(crate) fn print_frame_report(report: &FrameReport) {
    let interval_secs = report.interval_secs.max(f64::EPSILON);
    let snapshot = report.snapshot.unwrap_or_default();
    eprintln!(
        "[profile] ---- last {:.2}s | fps={:.1} frame={:.3}ms prep={:.3}ms ui={:.3}ms panel={:.3}ms present={:.3}ms ink={:.3}ms target<={:.1}ms motion={:.1}Hz target>={:.1}Hz input={:.1}Hz target>={:.1}Hz ----",
        report.interval_secs,
        report.frames as f64 / interval_secs,
        average_ms(report, "frame_total"),
        average_ms(report, "prepare_frame"),
        average_ms(report, "ui_update"),
        average_ms(report, "panel_surface"),
        average_ms(report, "present_total"),
        snapshot.canvas_latency_ms,
        INPUT_LATENCY_TARGET_MS,
        snapshot.canvas_present_hz,
        INPUT_SAMPLING_TARGET_HZ,
        snapshot.canvas_sample_hz,
        INPUT_SAMPLING_TARGET_HZ,
    );
    if let (Some(window_events), Some(raw_events), Some(dispatches)) = (
        report.stats.get("canvas_input_window_event"),
        report.stats.get("canvas_input_raw_event"),
        report.stats.get("canvas_input_dispatch"),
    ) {
        let wheel_events = report
            .stats
            .get("canvas_input_wheel_event")
            .map_or(0, |stat| stat.calls);
        eprintln!(
            "[profile] input sources window={} raw={} wheel={} dispatch={}",
            window_events.calls, raw_events.calls, wheel_events, dispatches.calls,
        );
    }
    for (label, stat) in &report.stats {
        let avg = if stat.calls == 0 {
            0.0
        } else {
            stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
        };
        eprintln!(
            "[profile] {:>18} calls={:>5} avg={:>8.3}ms max={:>8.3}ms total={:>8.3}ms",
            label,
            stat.calls,
            avg,
            stat.max.as_secs_f64() * 1000.0,
            stat.total.as_secs_f64() * 1000.0,
        );
    }
    for (label, stat) in &report.value_stats {
        let avg = if stat.samples == 0 {
            0.0
        } else {
            stat.total / stat.samples as f64
        };
        eprintln!(
            "[profile] {:>18} samples={:>5} avg={:>10.1} max={:>10.1}",
            label, stat.samples, avg, stat.max,
        );
    }
}

fn average_ms(report: &FrameReport, label: &'static str) -> f64 {
    report.stats.get(label).map_or(0.0, |stat| {
        if stat.calls == 0 {
            0.0
        } else {
            stat.total.as_secs_f64() * 1000.0 / stat.calls as f64
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_formats_window_title() {
        let title = title_text(PerformanceSnapshot {
            fps: 59.8,
            frame_ms: 16.72,
            prepare_ms: 3.11,
            ui_update_ms: 0.42,
            panel_surface_ms: 0.77,
            present_ms: 1.26,
            canvas_latency_ms: 8.40,
            canvas_present_hz: 144.0,
            canvas_sample_hz: 123.4,
        });

        assert!(title.contains("59.8 fps"));
        assert!(title.contains("prep  3.11ms"));
        assert!(title.contains("ui  0.42ms"));
        assert!(title.contains("ink  8.40ms ok"));
        assert!(title.contains("motion  144.0Hz ok"));
        assert!(title.contains("input  123.4Hz ok"));
    }

    #[test]
    fn window_title_without_snapshot_uses_base_title() {
        assert_eq!(window_title(None), WINDOW_TITLE);
    }
}
