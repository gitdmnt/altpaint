//! テキストラスタライズと BitmapEdit 生成。
//!
//! `TextRenderer` trait により実装を差し替え可能にする。
//! 既定実装 `Font8x8Renderer` は `font8x8` クレートを使ったビットマップフォントレンダラ。

use app_core::{BitmapComposite, BitmapEdit, CanvasBitmap, CanvasDirtyRect};

// ─── 公開 trait ──────────────────────────────────────────────────────────────

/// テキストをピクセルバッファへラスタライズする抽象インターフェース。
///
/// 実装を差し替えることで、font8x8 / ab_glyph / fontdue など複数の
/// バックエンドへの切り替えを可能にする。
pub trait TextRenderer: Send + Sync {
    /// テキストをラスタライズして RGBA ビットマップを返す。
    ///
    /// - `text`: 描画する文字列（単一行）
    /// - `font_size`: フォントサイズ（ピクセル高さの目安、最小 8）
    /// - `color`: RGBA 色（アルファは前景不透明度として使用）
    fn render(&self, text: &str, font_size: u32, color: [u8; 4]) -> TextRenderOutput;
}

/// `TextRenderer::render` の結果。
pub struct TextRenderOutput {
    /// RGBA row-major ピクセル列（アルファ事前乗算なし）。
    pub pixels: Vec<u8>,
    /// ビットマップ幅（ピクセル）。
    pub width: usize,
    /// ビットマップ高さ（ピクセル）。
    pub height: usize,
}

// ─── Font8x8Renderer ─────────────────────────────────────────────────────────

/// `font8x8` を使った 8×8 ビットマップフォントレンダラ。
///
/// `font_size / 8` をスケール係数として各文字ブロックを拡大する。
/// 単一行 ASCII / 基本 Unicode のみを対象とする。
#[derive(Debug, Clone, Default)]
pub struct Font8x8Renderer;

impl TextRenderer for Font8x8Renderer {
    fn render(&self, text: &str, font_size: u32, color: [u8; 4]) -> TextRenderOutput {
        let scale = ((font_size / 8) as usize).max(1);
        let glyph_w = 8 * scale;
        let glyph_h = 8 * scale;

        let chars: Vec<char> = text.chars().collect();
        let total_width = glyph_w * chars.len().max(1);
        let total_height = glyph_h;
        let mut pixels = vec![0u8; total_width * total_height * 4];

        for (char_index, ch) in chars.iter().enumerate() {
            let rows = glyph_rows_for(*ch);
            let base_x = char_index * glyph_w;

            for (row_index, row_mask) in rows.iter().enumerate() {
                for bit in 0..8usize {
                    let on = (row_mask >> (7 - bit)) & 1 == 1;
                    if !on {
                        continue;
                    }
                    // スケーリング: 1ビット → scale×scale ブロック
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = base_x + bit * scale + dx;
                            let py = row_index * scale + dy;
                            let idx = (py * total_width + px) * 4;
                            if idx + 3 < pixels.len() {
                                pixels[idx] = color[0];
                                pixels[idx + 1] = color[1];
                                pixels[idx + 2] = color[2];
                                pixels[idx + 3] = color[3];
                            }
                        }
                    }
                }
            }
        }

        TextRenderOutput {
            pixels,
            width: total_width,
            height: total_height,
        }
    }
}

/// 文字に対応する 8 行分のビットマスクを返す。
///
/// ASCII 範囲外は空白として扱う。
fn glyph_rows_for(ch: char) -> [u8; 8] {
    use font8x8::UnicodeFonts;
    font8x8::BASIC_FONTS
        .get(ch)
        .unwrap_or([0u8; 8])
}

// ─── Canvas Op ───────────────────────────────────────────────────────────────

/// テキストを RGBA ビットマップとしてラスタライズし、`BitmapEdit` を返す。
///
/// `x`, `y` はキャンバス左上からの配置座標（ピクセル）。
/// テキストが空または幅 0 の場合は `None` を返す。
pub fn render_text_to_bitmap_edit(
    text: &str,
    font_size: u32,
    color: [u8; 4],
    x: usize,
    y: usize,
) -> Option<BitmapEdit> {
    render_text_to_bitmap_edit_with(text, font_size, color, x, y, &Font8x8Renderer)
}

/// `renderer` を指定して `BitmapEdit` を生成する（テスト・差し替え用）。
pub fn render_text_to_bitmap_edit_with(
    text: &str,
    font_size: u32,
    color: [u8; 4],
    x: usize,
    y: usize,
    renderer: &dyn TextRenderer,
) -> Option<BitmapEdit> {
    if text.trim().is_empty() {
        return None;
    }
    let output = renderer.render(text, font_size, color);
    if output.width == 0 || output.height == 0 {
        return None;
    }
    let dirty_rect = CanvasDirtyRect {
        x,
        y,
        width: output.width,
        height: output.height,
    };
    let bitmap = CanvasBitmap {
        width: output.width,
        height: output.height,
        pixels: output.pixels,
    };
    Some(BitmapEdit::new(dirty_rect, bitmap, BitmapComposite::SourceOver))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Font8x8Renderer は空でない出力を生成する が期待どおりに動作することを検証する。
    #[test]
    fn font8x8_renderer_produces_nonempty_output() {
        let renderer = Font8x8Renderer;
        let out = renderer.render("A", 8, [0, 0, 0, 255]);
        assert_eq!(out.width, 8);
        assert_eq!(out.height, 8);
        assert_eq!(out.pixels.len(), 8 * 8 * 4);
    }

    /// スケール 2 では幅が 2 倍 が期待どおりに動作することを検証する。
    #[test]
    fn font8x8_renderer_scales_with_font_size() {
        let renderer = Font8x8Renderer;
        let out = renderer.render("AB", 16, [255, 0, 0, 255]);
        assert_eq!(out.width, 32);   // 2文字 × 8px × scale(2)
        assert_eq!(out.height, 16);  // 8px × scale(2)
    }

    /// render_text_to_bitmap_edit は有効テキストで Some を返す が期待どおりに動作することを検証する。
    #[test]
    fn render_text_to_bitmap_edit_returns_some_for_valid_text() {
        let edit = render_text_to_bitmap_edit("Hello", 8, [0, 0, 0, 255], 10, 20);
        let edit = edit.expect("should return Some");
        assert_eq!(edit.dirty_rect.x, 10);
        assert_eq!(edit.dirty_rect.y, 20);
        assert_eq!(edit.dirty_rect.width, 8 * 5);   // 5 chars × 8px
        assert_eq!(edit.dirty_rect.height, 8);
    }

    /// 空テキストは None を返す が期待どおりに動作することを検証する。
    #[test]
    fn render_text_to_bitmap_edit_returns_none_for_empty_text() {
        assert!(render_text_to_bitmap_edit("", 8, [0, 0, 0, 255], 0, 0).is_none());
        assert!(render_text_to_bitmap_edit("   ", 8, [0, 0, 0, 255], 0, 0).is_none());
    }

    /// カスタム renderer の差し替えが可能 が期待どおりに動作することを検証する。
    #[test]
    fn render_text_with_custom_renderer() {
        struct StubRenderer;
        impl TextRenderer for StubRenderer {
            fn render(&self, text: &str, _font_size: u32, color: [u8; 4]) -> TextRenderOutput {
                let w = text.len();
                let h = 1;
                let mut pixels = vec![0u8; w * h * 4];
                for i in 0..w {
                    let idx = i * 4;
                    pixels[idx..idx + 4].copy_from_slice(&color);
                }
                TextRenderOutput { pixels, width: w, height: h }
            }
        }
        let edit = render_text_to_bitmap_edit_with("Hi", 8, [1, 2, 3, 4], 0, 0, &StubRenderer);
        let edit = edit.expect("should return Some");
        assert_eq!(edit.bitmap.width, 2);
        assert_eq!(edit.bitmap.height, 1);
    }
}
