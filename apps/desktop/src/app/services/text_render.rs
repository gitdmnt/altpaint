//! テキスト描画 service request のハンドラ。

use canvas::ops::text::render_text_to_bitmap_edit;
use panel_api::{ServiceRequest, services::names};

use super::DesktopApp;

impl DesktopApp {
    /// text_render service request を処理する。
    pub(super) fn handle_text_render_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        match request.name.as_str() {
            names::TEXT_RENDER_TO_LAYER => {
                let text = request.string("text").unwrap_or("").to_string();
                let font_size = request
                    .payload
                    .get("font_size")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(32) as u32;
                let color_hex = request
                    .string("color_hex")
                    .unwrap_or("#000000")
                    .to_string();
                let x = request
                    .payload
                    .get("x")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let y = request
                    .payload
                    .get("y")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                Some(self.render_text_to_active_layer(text, font_size, color_hex, x, y))
            }
            _ => None,
        }
    }

    /// テキストをアクティブレイヤーへ描画する。
    pub(super) fn render_text_to_active_layer(
        &mut self,
        text: String,
        font_size: u32,
        color_hex: String,
        x: usize,
        y: usize,
    ) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let color = parse_color_hex(&color_hex).unwrap_or([0, 0, 0, 255]);
        let Some(edit) = render_text_to_bitmap_edit(&text, font_size, color, x, y) else {
            return false;
        };
        let Some(dirty) = self
            .document
            .apply_bitmap_edits_to_active_layer(&[edit])
        else {
            return false;
        };
        self.append_canvas_dirty_rect(dirty);
        true
    }
}

/// 16進文字列を RGBA へ変換する。
///
/// 値を生成できない場合は `None` を返します。
fn parse_color_hex(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b, 255])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app::DesktopApp;
    use crate::app::tests::unique_test_path;
    use desktop_support::NativeDesktopDialogs;

    fn make_app() -> DesktopApp {
        DesktopApp::new_with_dialogs_session_path_and_workspace_preset_path(
            PathBuf::from("/tmp/altpaint-text-render-test.altp.json"),
            Box::new(NativeDesktopDialogs),
            unique_test_path("text-render-session"),
            unique_test_path("text-render-workspace"),
        )
    }

    /// テキスト描画 on empty text returns false が期待どおりに動作することを検証する。
    #[test]
    fn render_empty_text_returns_false() {
        let mut app = make_app();
        let changed = app.render_text_to_active_layer(
            "".to_string(),
            32,
            "#000000".to_string(),
            0,
            0,
        );
        assert!(!changed);
    }

    /// テキスト描画 with valid text returns true が期待どおりに動作することを検証する。
    #[test]
    fn render_valid_text_returns_true() {
        let mut app = make_app();
        let changed = app.render_text_to_active_layer(
            "Hello".to_string(),
            8,
            "#000000".to_string(),
            0,
            0,
        );
        assert!(changed);
    }
}
