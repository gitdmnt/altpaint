//! キャンバス表示 (ビュー) の相対操作ポリシー。
//!
//! ホイール 1 ノッチあたりのズーム倍率・ズーム上下限・1 line あたりのパン量を
//! ドメイン側の単一定義として持つ。入力層 (desktop) は相対量 (lines) のみを発行し、
//! ポリシーの適用は本モジュールと [`crate::document::Document::apply_session_command`]
//! が担う (BL-064)。
//!
//! B5 (BL-073) で `editor-state` クレートへ `SessionCommand` とともに移設予定。

/// ズーム下限。これより小さくはならない。
pub const ZOOM_MIN: f32 = 0.25;

/// ズーム上限。これより大きくはならない。
pub const ZOOM_MAX: f32 = 16.0;

/// ホイール 1 line あたりのズーム倍率の底。`zoom *= ZOOM_LINE_BASE.powf(lines)`。
pub const ZOOM_LINE_BASE: f32 = 1.1;

/// パン 1 line あたりのピクセル量。
pub const PAN_PIXELS_PER_LINE: f32 = 32.0;

/// 現在のズーム値に対し `lines` ノッチ分の相対ズームを適用し、上下限でクランプした値を返す。
///
/// 純粋関数。入力層はこの関数でクランプ後の値を求め、飽和 (変化なし) を検出してよい。
#[must_use]
pub fn zoom_after_lines(current_zoom: f32, lines: f32) -> f32 {
    (current_zoom * ZOOM_LINE_BASE.powf(lines)).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// ズーム値を上下限でクランプする。絶対値設定 (`SetViewZoom`) でも用いる。
#[must_use]
pub fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_notch_applies_base_multiplier() {
        assert!((zoom_after_lines(1.0, 1.0) - ZOOM_LINE_BASE).abs() < 1e-6);
    }

    #[test]
    fn saturates_at_upper_and_lower_bounds() {
        assert_eq!(zoom_after_lines(1.0, 1000.0), ZOOM_MAX);
        assert_eq!(zoom_after_lines(1.0, -1000.0), ZOOM_MIN);
    }

    #[test]
    fn clamp_zoom_matches_bounds() {
        assert_eq!(clamp_zoom(100.0), ZOOM_MAX);
        assert_eq!(clamp_zoom(0.0), ZOOM_MIN);
        assert_eq!(clamp_zoom(2.0), 2.0);
    }
}
