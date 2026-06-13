//! デスクトップアプリ内で保持する CPU 側のキャンバススナップショット表現。
//!
//! Phase 9F で `render::RenderFrame` / `render::RenderContext` を撤去した際に
//! `apps/desktop` 内へ移管した最小型。GPU キャンバスが標準経路だが、
//! `cpu_canvas_snapshot` の (width, height) は依然 viewport / 表示幾何 (CanvasViewGeometry) 計算で参照される。

use document_model::Document;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CpuCanvasSnapshot {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// アクティブページサイズの空バッファへアクティブコマの合成キャッシュを貼り付けて
/// CPU 側キャンバススナップショットを構築する。
pub(crate) fn build_cpu_canvas_snapshot(document: &Document) -> CpuCanvasSnapshot {
    let page = document.active_page().unwrap_or(&document.work.pages[0]);
    let koma = document.active_koma().unwrap_or(&page.komas[0]);
    let width = page.width.max(1);
    let height = page.height.max(1);
    let mut frame = CpuCanvasSnapshot {
        width,
        height,
        pixels: vec![255; width * height * 4],
    };

    let copy_width = koma
        .composite_cache
        .width
        .min(koma.bounds.width)
        .min(frame.width.saturating_sub(koma.bounds.x));
    let copy_height = koma
        .composite_cache
        .height
        .min(koma.bounds.height)
        .min(frame.height.saturating_sub(koma.bounds.y));
    for row in 0..copy_height {
        let src_row_start = row * koma.composite_cache.width * 4;
        let dst_row_start = ((koma.bounds.y + row) * frame.width + koma.bounds.x) * 4;
        let row_bytes = copy_width * 4;
        frame.pixels[dst_row_start..dst_row_start + row_bytes]
            .copy_from_slice(&koma.composite_cache.pixels[src_row_start..src_row_start + row_bytes]);
    }

    frame
}
