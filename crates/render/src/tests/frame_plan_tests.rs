use crate::RenderContext;

/// 描画 フレーム places アクティブ パネル ビットマップ inside ページ が期待どおりに動作することを検証する。
#[test]
fn render_frame_places_active_panel_bitmap_inside_page() {
    let mut document = app_core::Document::new(320, 240);
    let _ = document.apply_command(&app_core::Command::CreatePanel {
        x: 40,
        y: 32,
        width: 120,
        height: 80,
    });
    if let Some(panel) = document.active_panel_mut() {
        let _ = panel.layers[0]
            .bitmap
            .draw_line_sized_rgba(1, 2, 4, 2, [0, 0, 0, 255], 1, true);
        panel.bitmap = panel.layers[0].bitmap.clone();
    }

    let context = RenderContext::new();
    let frame = context.render_frame(&document);

    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 240);

    let index = ((32 + 2) * frame.width + (40 + 1)) * 4;
    assert_eq!(&frame.pixels[index..index + 4], &[0, 0, 0, 255]);
    let end_index = ((32 + 2) * frame.width + (40 + 4)) * 4;
    assert_eq!(&frame.pixels[end_index..end_index + 4], &[0, 0, 0, 255]);
    assert_eq!(&frame.pixels[0..4], &[255, 255, 255, 255]);
}
