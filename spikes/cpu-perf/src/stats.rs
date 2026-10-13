//! 計測値の要約。
#[derive(Clone, Copy, Debug)]
pub struct Summary {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

/// p50、p95、最大を求める。パーセンタイルは、順位を切り上げて取る (nearest-rank)。
pub fn summarize(samples: &[f64]) -> Summary {
    let sorted = {
        let mut values = samples.to_vec();
        values.sort_by(f64::total_cmp);
        values
    };
    let rank = |percent: usize| sorted[(percent * sorted.len()).div_ceil(100).max(1) - 1];
    Summary {
        p50: rank(50),
        p95: rank(95),
        max: rank(100),
    }
}

/// A-SPK-d の合否と、超過の分類。引数は、ダブ描画の p95 と再合成の p95 (ms)。
pub fn verdict(dab: f64, composite: f64) -> &'static str {
    let pass = composite <= 8.0 && dab + composite <= 16.7;
    match (pass, dab > 8.7, composite > 8.0) {
        (true, ..) => "合格",
        (false, true, true) => "ダブ描画と再合成の超過",
        (false, true, false) => "ダブ描画の超過",
        (false, false, _) => "再合成の超過",
    }
}
