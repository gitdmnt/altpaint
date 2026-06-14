//! ピクセルブレンドの単一実装 (BL-032)。
//!
//! CPU 側のレイヤー合成・`BitmapEdit` 合成・ブラシ被覆ブレンドはすべて
//! 本モジュールの関数を使う。
//!
//! # BlendMode 対応表 (単一定義)
//!
//! | BlendMode | gpu_code | チャンネル混合式      |
//! |-----------|----------|-----------------------|
//! | Normal    | 0        | s                     |
//! | Multiply  | 1        | s * d                 |
//! | Screen    | 2        | 1 - (1 - s) * (1 - d) |
//! | Add       | 3        | min(s + d, 1)         |
//!
//! GPU 側 (`crates/gpu-paint/src/shaders/layer_composite.wgsl` の `blend_channel`)
//! はこの表を複製している。変更時は必ず両者を同期すること。

use serde::{Deserialize, Serialize};

/// レイヤー・ビットマップ合成のブレンドモード。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Add,
}

impl BlendMode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Add => "add",
        }
    }

    /// 空文字列は `None` を返す。未知の文字列は後方互換として `Normal` にフォールバック
    /// する（旧 `Custom(String)` variant の保存値はここで破棄される）。
    pub fn parse_name(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        match trimmed.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "add" => Some(Self::Add),
            _ => Some(Self::Normal),
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Normal => Self::Multiply,
            Self::Multiply => Self::Screen,
            Self::Screen => Self::Add,
            Self::Add => Self::Normal,
        }
    }

    /// GPU compute shader に渡す blend code。
    ///
    /// モジュール冒頭の対応表が単一定義。`layer_composite.wgsl` の switch と対応する。
    pub fn gpu_code(&self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
            Self::Add => 3,
        }
    }
}

/// `BlendMode` のチャンネル混合 (0.0..=1.0 正規化値)。
///
/// モジュール冒頭の対応表と 1:1。`layer_composite.wgsl` の `blend_channel` と同一式。
fn blend_channel(dst: f32, src: f32, mode: &BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => src,
        BlendMode::Multiply => src * dst,
        BlendMode::Screen => 1.0 - (1.0 - src) * (1.0 - dst),
        BlendMode::Add => (src + dst).min(1.0),
    }
}

/// straight-alpha source-over (`BlendMode` つき)。
///
/// レイヤー合成と `BitmapEdit` 合成の標準式で、GPU `layer_composite.wgsl` と同一。
/// dst 色を dst alpha で重み付けせずそのまま補間するため、透明地に半透明 src を
/// 置くと色値が src alpha 倍へ縮む (premultiply 風) 挙動を持つ。
pub fn composite_pixel(dst: [u8; 4], src: [u8; 4], mode: &BlendMode) -> [u8; 4] {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return dst;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let d = dst[channel] as f32 / 255.0;
        let s = src[channel] as f32 / 255.0;
        let mixed = blend_channel(d, s, mode);
        let out_c = mixed * src_a + d * (1.0 - src_a);
        out[channel] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}

/// 被覆率つき source-over。ブラシ描画経路の標準式。
///
/// `composite_pixel` と異なり dst 色を dst alpha で重み付けし、結果を out alpha で
/// 正規化する (straight alpha を厳密に扱う)。透明地に AA ブラシを置いても
/// dst 色 (0,0,0) に引きずられない。
pub fn source_over_coverage_pixel(dst: [u8; 4], src: [u8; 4], coverage: f32) -> [u8; 4] {
    let dst_f = [
        dst[0] as f32 / 255.0,
        dst[1] as f32 / 255.0,
        dst[2] as f32 / 255.0,
        dst[3] as f32 / 255.0,
    ];
    let src_alpha = (src[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    let out_alpha = src_alpha + dst_f[3] * (1.0 - src_alpha);

    let (out_r, out_g, out_b) = if out_alpha <= f32::EPSILON {
        (0.0, 0.0, 0.0)
    } else {
        let src_f = [
            src[0] as f32 / 255.0,
            src[1] as f32 / 255.0,
            src[2] as f32 / 255.0,
        ];
        (
            (src_f[0] * src_alpha + dst_f[0] * dst_f[3] * (1.0 - src_alpha)) / out_alpha,
            (src_f[1] * src_alpha + dst_f[1] * dst_f[3] * (1.0 - src_alpha)) / out_alpha,
            (src_f[2] * src_alpha + dst_f[2] * dst_f[3] * (1.0 - src_alpha)) / out_alpha,
        )
    };

    [
        (out_r * 255.0).round().clamp(0.0, 255.0) as u8,
        (out_g * 255.0).round().clamp(0.0, 255.0) as u8,
        (out_b * 255.0).round().clamp(0.0, 255.0) as u8,
        (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BlendMode::gpu_code` はモジュール冒頭の対応表 (= GPU shader の switch コード) と
    /// 1:1 対応する。
    #[test]
    fn gpu_code_matches_shader_switch_codes() {
        assert_eq!(BlendMode::Normal.gpu_code(), 0);
        assert_eq!(BlendMode::Multiply.gpu_code(), 1);
        assert_eq!(BlendMode::Screen.gpu_code(), 2);
        assert_eq!(BlendMode::Add.gpu_code(), 3);
    }
}
