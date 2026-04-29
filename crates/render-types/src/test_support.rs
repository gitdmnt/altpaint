//! テスト用のピクセル検証ユーティリティ。
//!
//! Phase 9E-5 で導入。font8x8 → parley のフォント差分により
//! ピクセル一致比較系テストが破綻したため、実装非依存の
//! 弱検証 (色矩形検出 / 暗色ピクセル数) で代替する。
//!
//! - 純データ依存: `app-core` のドメイン型と本クレート内の `PixelRect` のみ。
//! - vello scene の glyph run 数カウントは `vello` 直接依存が必要となるため
//!   ここには置かず、各クレート (panel-html-experiment 等) のテスト側で
//!   `scene.encoding().resources` を直接参照する形を取る。

use crate::PixelRect;

/// RGBA8 ピクセル列のうち暗色 (R+G+B が `threshold` 以下) を数える。
///
/// パネル背景の白い領域上にテキスト・枠線などの暗色描画が「何かしら載った」
/// ことを弱検証する用途で使う。
pub fn find_dark_pixels(pixels: &[u8], threshold: u32) -> usize {
    debug_assert_eq!(pixels.len() % 4, 0, "pixels must be RGBA8");
    pixels
        .chunks_exact(4)
        .filter(|px| (px[0] as u32 + px[1] as u32 + px[2] as u32) <= threshold)
        .count()
}

/// `pixels` 全体 (幅 `width`) のうち、`rect` で指定した矩形内に
/// `target` 色 (RGB) が許容差 `tolerance` 以内で存在するピクセル数を返す。
///
/// 厳密一致ではなく "それらしき色矩形があるか" の弱検証用途。
pub fn find_color_in_rect(
    pixels: &[u8],
    width: usize,
    rect: PixelRect,
    target: (u8, u8, u8),
    tolerance: i32,
) -> usize {
    debug_assert_eq!(pixels.len() % 4, 0, "pixels must be RGBA8");
    let height = if width == 0 {
        0
    } else {
        pixels.len() / (width * 4)
    };
    let x_end = rect.x.saturating_add(rect.width).min(width);
    let y_end = rect.y.saturating_add(rect.height).min(height);
    let (tr, tg, tb) = (target.0 as i32, target.1 as i32, target.2 as i32);
    let mut count = 0usize;
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            let idx = (y * width + x) * 4;
            let dr = pixels[idx] as i32 - tr;
            let dg = pixels[idx + 1] as i32 - tg;
            let db = pixels[idx + 2] as i32 - tb;
            if dr.abs() <= tolerance && dg.abs() <= tolerance && db.abs() <= tolerance {
                count += 1;
            }
        }
    }
    count
}

/// vello scene 内の glyph run 数をカウントするための placeholder。
///
/// `render-types` は意図的に vello に依存しないため、ここでは関数を
/// 提供しない。glyph run 検証が必要なクレートは vello を直接依存に
/// 持つテスト側で次のように記述する:
///
/// ```ignore
/// // panel-html-experiment などの vello を直接持つクレートにて:
/// fn count_glyph_runs(scene: &vello::Scene) -> usize {
///     scene
///         .encoding()
///         .resources
///         .glyph_runs
///         .len()
/// }
/// ```
#[doc(hidden)]
pub const _GLYPH_RUN_NOTE: &str = "see module docs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_dark_pixels_counts_only_dark_rgb() {
        let pixels = vec![
            10, 10, 10, 255, // dark
            255, 255, 255, 255, // white
            0, 0, 0, 255, // dark
            200, 200, 200, 255, // bright
        ];
        assert_eq!(find_dark_pixels(&pixels, 64), 2);
    }

    #[test]
    fn find_color_in_rect_counts_within_tolerance() {
        // 3x2 image, target red at (1,0) and (2,1)
        let mut pixels = vec![255u8; 3 * 2 * 4];
        let set = |buf: &mut Vec<u8>, x: usize, y: usize, c: [u8; 4]| {
            let idx = (y * 3 + x) * 4;
            buf[idx..idx + 4].copy_from_slice(&c);
        };
        set(&mut pixels, 1, 0, [250, 5, 5, 255]);
        set(&mut pixels, 2, 1, [200, 0, 0, 255]);
        let rect = PixelRect {
            x: 0,
            y: 0,
            width: 3,
            height: 2,
        };
        // tolerance 10 → only (1,0) hits
        assert_eq!(find_color_in_rect(&pixels, 3, rect, (255, 0, 0), 10), 1);
        // tolerance 60 → both hit
        assert_eq!(find_color_in_rect(&pixels, 3, rect, (255, 0, 0), 60), 2);
    }
}
