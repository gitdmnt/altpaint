//! `render` は将来のキャンバス描画基盤になるクレート。
//!
//! フェーズ2では、`Document` 内の最初のコマにあるラスタビットマップを
//! フレームバッファとして取り出す最小描画経路を定義する。

use app_core::Document;

/// 画面へ転送するための最小フレームデータ。
/// フレームバッファとしての役割を果たす。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrame {
    /// フレームの横幅ピクセル数。
    pub width: usize,
    /// フレームの高さピクセル数。
    pub height: usize,
    /// RGBA8 のピクセル列。
    pub pixels: Vec<u8>,
}

/// キャンバス描画のための最小コンテキスト。
///
/// 将来的にはキャッシュ、カメラ、描画ターゲットなどを保持する。
#[derive(Debug, Default)]
pub struct RenderContext;

impl RenderContext {
    /// 空のレンダリングコンテキストを作成する。
    pub fn new() -> Self {
        Self
    }

    /// 現段階では描画対象 `Document` をそのまま返す。
    ///
    /// 将来的にはここで可視範囲の解決やレンダリング前処理を行う。
    pub fn document<'a>(&self, document: &'a Document) -> &'a Document {
        document
    }

    /// ドキュメントから最初のコマのビットマップをフレームへ変換する。
    pub fn render_frame(&self, document: &Document) -> RenderFrame {
        let panel = &document.work.pages[0].panels[0];
        RenderFrame {
            width: panel.bitmap.width,
            height: panel.bitmap.height,
            pixels: panel.bitmap.pixels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 描画フレームが最小キャンバスサイズを正しく反映することを確認する。
    #[test]
    fn render_frame_uses_first_panel_bitmap() {
        let mut document = Document::default();
        document.draw_stroke(1, 2, 4, 2);

        let context = RenderContext::new();
        let frame = context.render_frame(&document);

        // ドキュメントのデフォルトは幅64・高さ64のビットマップを持つため、フレームも同じサイズであること。
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);

        // ドキュメントのストローク描画結果がフレームへ反映されること。
        let index = (2 * frame.width + 1) * 4;
        assert_eq!(&frame.pixels[index..index + 4], &[0, 0, 0, 255]);
        let end_index = (2 * frame.width + 4) * 4;
        assert_eq!(&frame.pixels[end_index..end_index + 4], &[0, 0, 0, 255]);
    }
}
