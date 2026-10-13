//! 統計 (p50、p95、最大) と、合否の判定の単体テスト。
use spike_cpu_perf::stats::{summarize, verdict};

#[test]
fn summarize_uses_the_nearest_rank_percentile() {
    let samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
    let s = summarize(&samples);
    assert_eq!((s.p50, s.p95, s.max), (50.0, 95.0, 100.0));
}

#[test]
fn summarize_of_twenty_samples_takes_the_nineteenth_as_p95() {
    let samples: Vec<f64> = (1..=20).map(f64::from).collect();
    let s = summarize(&samples);
    assert_eq!((s.p50, s.p95, s.max), (10.0, 19.0, 20.0));
}

#[test]
fn verdict_passes_up_to_the_limits_of_recomposite_and_of_the_sum() {
    assert_eq!(verdict(0.2, 2.2), "合格");
    assert_eq!(verdict(8.7, 8.0), "合格");
    // ダブ描画が 8.7ms を超えても、再合成が 8ms 以内で、和が 16.7ms 以内なら合格である。
    assert_eq!(verdict(10.0, 2.0), "合格");
}

#[test]
fn verdict_classifies_which_side_exceeded() {
    assert_eq!(verdict(1.0, 8.01), "再合成の超過");
    assert_eq!(verdict(8.71, 8.0), "ダブ描画の超過");
    assert_eq!(verdict(15.0, 2.0), "ダブ描画の超過");
    assert_eq!(verdict(9.0, 8.5), "ダブ描画と再合成の超過");
}

#[test]
fn summarize_rounds_the_rank_up_when_it_is_not_whole() {
    let samples: Vec<f64> = (1..=30).map(f64::from).collect();
    let s = summarize(&samples);
    assert_eq!((s.p50, s.p95, s.max), (15.0, 29.0, 30.0));
}
