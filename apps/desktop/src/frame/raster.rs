//! `frame` の低レベル raster 処理をまとめる。
//!
//! RGBA fill・scale blit・スクロールコピー・ブラシプレビュー描画のような
//! 画素操作をここへ閉じ込め、上位の合成ロジックから分離する。

use app_core::CanvasViewTransform;
use ui_shell::{draw_text_rgba, measure_text_width};

use super::geometry::{brush_preview_rect, canvas_scene, map_canvas_point_to_display};
use super::{CanvasCompositeSource, Rect};

/// ビットマップ文字描画を `RenderFrame` 向けに薄くラップする。
pub(super) fn draw_text(
    frame: &mut render::RenderFrame,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
) {
    draw_text_rgba(
        frame.pixels.as_mut_slice(),
        frame.width,
        frame.height,
        x,
        y,
        text,
        color,
    );
}

/// 単色矩形をフレームへ塗り込む。
pub(super) fn fill_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    let max_x = (rect.x + rect.width).min(frame.width);
    let max_y = (rect.y + rect.height).min(frame.height);
    if rect.x >= max_x || rect.y >= max_y {
        return;
    }

    let row_start_x = rect.x * 4;
    let row_len = (max_x - rect.x) * 4;
    for yy in rect.y..max_y {
        let row_start = yy * frame.width * 4 + row_start_x;
        fill_rgba_slice(&mut frame.pixels[row_start..row_start + row_len], color);
    }
}

/// 1 行分の RGBA バッファを倍々コピーで高速に塗り潰す。
fn fill_rgba_slice(target: &mut [u8], color: [u8; 4]) {
    if target.is_empty() {
        return;
    }

    target[..4].copy_from_slice(&color);
    let mut filled = 4;
    while filled < target.len() {
        let copy_len = filled.min(target.len() - filled);
        let (head, tail) = target.split_at_mut(filled);
        tail[..copy_len].copy_from_slice(&head[..copy_len]);
        filled += copy_len;
    }
}

/// 単色枠線をフレームへ描画する。
pub(super) fn stroke_rect(frame: &mut render::RenderFrame, rect: Rect, color: [u8; 4]) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
    fill_rect(
        frame,
        Rect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        color,
    );
}

/// dirty rect と交差する枠線だけを再描画する。
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn stroke_rect_region(
    frame: &mut render::RenderFrame,
    rect: Rect,
    dirty_rect: Rect,
    color: [u8; 4],
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let edges = [
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        },
        Rect {
            x: rect.x,
            y: rect.y + rect.height.saturating_sub(1),
            width: rect.width,
            height: 1,
        },
        Rect {
            x: rect.x,
            y: rect.y,
            width: 1,
            height: rect.height,
        },
        Rect {
            x: rect.x + rect.width.saturating_sub(1),
            y: rect.y,
            width: 1,
            height: rect.height,
        },
    ];

    for edge in edges {
        if let Some(region) = edge.intersect(dirty_rect) {
            fill_rect(frame, region, color);
        }
    }
}

/// RGBA ソースをスケーリングしつつ dirty rect 範囲だけ転送する。
pub(crate) fn blit_scaled_rgba_region(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<Rect>,
) {
    if destination.width == 0 || destination.height == 0 || source_width == 0 || source_height == 0
    {
        return;
    }

    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination);

    if target.width == 0 || target.height == 0 {
        return;
    }

    if destination.width == source_width && destination.height == source_height {
        let src_start_x = target.x.saturating_sub(destination.x);
        let src_start_y = target.y.saturating_sub(destination.y);
        let row_bytes = target.width * 4;
        for row in 0..target.height {
            let src_y = src_start_y + row;
            let dst_y = target.y + row;
            let src_row_start = (src_y * source_width + src_start_x) * 4;
            let dst_row_start = (dst_y * frame.width + target.x) * 4;
            frame.pixels[dst_row_start..dst_row_start + row_bytes]
                .copy_from_slice(&source_pixels[src_row_start..src_row_start + row_bytes]);
        }
        return;
    }

    for dst_y in target.y..target.y + target.height {
        let local_y = dst_y - destination.y;
        let src_y = ((local_y * source_height) / destination.height).min(source_height - 1);
        let dst_row_start = dst_y * frame.width * 4;
        for dst_x in target.x..target.x + target.width {
            let local_x = dst_x - destination.x;
            let src_x = ((local_x * source_width) / destination.width).min(source_width - 1);
            let src_index = (src_y * source_width + src_x) * 4;
            let dst_index = dst_row_start + dst_x * 4;
            frame.pixels[dst_index..dst_index + 4]
                .copy_from_slice(&source_pixels[src_index..src_index + 4]);
        }
    }
}

/// キャンバスをビュー変換込みでソフトウェア描画する。
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn blit_canvas_with_transform(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    dirty_rect: Option<Rect>,
) {
    if destination.width == 0
        || destination.height == 0
        || source.width == 0
        || source.height == 0
        || source.pixels.len() < source.width * source.height * 4
    {
        return;
    }

    let Some(scene) = canvas_scene(destination, source.width, source.height, transform) else {
        return;
    };
    let Some(drawn_rect) = scene.drawn_rect().map(super::geometry::from_render_rect) else {
        return;
    };
    let target = dirty_rect
        .and_then(|dirty| destination.intersect(dirty))
        .unwrap_or(destination)
        .intersect(drawn_rect)
        .unwrap_or(Rect {
            x: destination.x,
            y: destination.y,
            width: 0,
            height: 0,
        });

    if target.width == 0 || target.height == 0 {
        return;
    }

    let (offset_x, offset_y) = scene.offset();
    let src_x_runs = build_source_axis_runs(
        target.x,
        target.width,
        offset_x,
        scene.scale(),
        source.width,
    );
    let src_y_runs = build_source_axis_runs(
        target.y,
        target.height,
        offset_y,
        scene.scale(),
        source.height,
    );

    for y_run in &src_y_runs {
        let src_row_start = y_run.src_index * source.width * 4;
        for x_run in &src_x_runs {
            let src_offset = src_row_start + x_run.src_index * 4;
            let pixel = [
                source.pixels[src_offset],
                source.pixels[src_offset + 1],
                source.pixels[src_offset + 2],
                source.pixels[src_offset + 3],
            ];
            fill_rgba_block(
                frame,
                target.x + x_run.dst_offset,
                target.y + y_run.dst_offset,
                x_run.len,
                y_run.len,
                pixel,
            );
        }
    }
}

/// 1 軸ぶんの destination 連続範囲と source index の対応を表す。
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceAxisRun {
    pub(super) dst_offset: usize,
    pub(super) len: usize,
    pub(super) src_index: usize,
}

/// destination 連続区間ごとの source index 対応を前計算する。
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn build_source_axis_runs(
    destination_start: usize,
    destination_len: usize,
    offset: f32,
    scale: f32,
    source_len: usize,
) -> Vec<SourceAxisRun> {
    let mut runs: Vec<SourceAxisRun> = Vec::new();

    for index in 0..destination_len {
        let src = {
            let dst = destination_start + index;
            let src = ((dst as f32 + 0.5 - offset) / scale).floor();
            (0.0..source_len as f32)
                .contains(&src)
                .then_some(src as usize)
        };

        let Some(src_index) = src else {
            continue;
        };

        if let Some(last) = runs.last_mut()
            && last.src_index == src_index
            && last.dst_offset + last.len == index
        {
            last.len += 1;
            continue;
        }

        runs.push(SourceAxisRun {
            dst_offset: index,
            len: 1,
            src_index,
        });
    }

    runs
}

/// 単色ブロックを矩形範囲へ高速に書き込む。
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn fill_rgba_block(
    frame: &mut render::RenderFrame,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    if width == 0 || height == 0 || x >= frame.width || y >= frame.height {
        return;
    }

    let row_start_x = x * 4;
    let row_len = width.min(frame.width - x) * 4;
    let max_y = (y + height).min(frame.height);
    for yy in y..max_y {
        let row_start = yy * frame.width * 4 + row_start_x;
        fill_rgba_slice(&mut frame.pixels[row_start..row_start + row_len], color);
    }
}

/// 既存 frame 上の矩形領域をスクロールし、露出領域を返す。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn scroll_canvas_region(
    frame: &mut render::RenderFrame,
    region: Rect,
    delta_x: i32,
    delta_y: i32,
) -> Rect {
    if region.width == 0 || region.height == 0 {
        return region;
    }

    let shift_x = delta_x.clamp(-(region.width as i32), region.width as i32);
    let shift_y = delta_y.clamp(-(region.height as i32), region.height as i32);
    let overlap_width = region.width.saturating_sub(shift_x.unsigned_abs() as usize);
    let overlap_height = region
        .height
        .saturating_sub(shift_y.unsigned_abs() as usize);

    if overlap_width == 0 || overlap_height == 0 {
        return region;
    }

    let src_x = if shift_x >= 0 {
        region.x
    } else {
        region.x + shift_x.unsigned_abs() as usize
    };
    let src_y = if shift_y >= 0 {
        region.y
    } else {
        region.y + shift_y.unsigned_abs() as usize
    };
    let dst_x = if shift_x >= 0 {
        region.x + shift_x as usize
    } else {
        region.x
    };
    let dst_y = if shift_y >= 0 {
        region.y + shift_y as usize
    } else {
        region.y
    };

    let mut copied = vec![0; overlap_width * overlap_height * 4];
    for row in 0..overlap_height {
        let src_row_start = ((src_y + row) * frame.width + src_x) * 4;
        let src_row_end = src_row_start + overlap_width * 4;
        let dst_row_start = row * overlap_width * 4;
        copied[dst_row_start..dst_row_start + overlap_width * 4]
            .copy_from_slice(&frame.pixels[src_row_start..src_row_end]);
    }

    for row in 0..overlap_height {
        let dst_row_start = ((dst_y + row) * frame.width + dst_x) * 4;
        let src_row_start = row * overlap_width * 4;
        frame.pixels[dst_row_start..dst_row_start + overlap_width * 4]
            .copy_from_slice(&copied[src_row_start..src_row_start + overlap_width * 4]);
    }

    exposed_scroll_rect(region, shift_x, shift_y)
}

/// スクロール後に新たに露出した矩形を返す。
#[cfg_attr(not(test), allow(dead_code))]
fn exposed_scroll_rect(region: Rect, shift_x: i32, shift_y: i32) -> Rect {
    if shift_x.unsigned_abs() as usize >= region.width
        || shift_y.unsigned_abs() as usize >= region.height
    {
        return region;
    }

    let mut exposed = None;
    if shift_x > 0 {
        exposed = Some(Rect {
            x: region.x,
            y: region.y,
            width: shift_x as usize,
            height: region.height,
        });
    } else if shift_x < 0 {
        let width = shift_x.unsigned_abs() as usize;
        exposed = Some(Rect {
            x: region.x + region.width - width,
            y: region.y,
            width,
            height: region.height,
        });
    }

    if shift_y > 0 {
        let rect = Rect {
            x: region.x,
            y: region.y,
            width: region.width,
            height: shift_y as usize,
        };
        exposed = Some(exposed.map_or(rect, |existing| existing.union(rect)));
    } else if shift_y < 0 {
        let height = shift_y.unsigned_abs() as usize;
        let rect = Rect {
            x: region.x,
            y: region.y + region.height - height,
            width: region.width,
            height,
        };
        exposed = Some(exposed.map_or(rect, |existing| existing.union(rect)));
    }

    exposed.unwrap_or(region)
}

/// ブラシプレビューの輪郭だけを overlay frame に描く。
pub(super) fn draw_brush_preview(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    canvas_position: (usize, usize),
    brush_size: u32,
    dirty_rect: Option<Rect>,
) {
    if source.width == 0 || source.height == 0 {
        return;
    }
    let Some(scene) = canvas_scene(destination, source.width, source.height, transform) else {
        return;
    };
    let Some((center_x, center_y)) = scene.map_canvas_point_to_display(canvas_position) else {
        return;
    };
    let radius = ((brush_size.max(1) as f32 * scene.scale()) * 0.5).max(4.0);
    let Some(target) = brush_preview_rect(
        destination,
        source.width,
        source.height,
        transform,
        canvas_position,
        brush_size,
    )
    .and_then(|rect| match dirty_rect {
        Some(dirty) => rect.intersect(dirty),
        None => Some(rect),
    }) else {
        return;
    };
    for y in target.y..target.y + target.height {
        let row_start = y * frame.width * 4;
        for x in target.x..target.x + target.width {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            if (distance - radius).abs() <= 1.0 {
                let index = row_start + x * 4;
                frame.pixels[index..index + 4].copy_from_slice(&[0x9f, 0xb7, 0xff, 0xff]);
            }
        }
    }
}

/// 投げ縄のプレビュー線を overlay frame に描く。
pub(super) fn draw_lasso_preview(
    frame: &mut render::RenderFrame,
    destination: Rect,
    source: CanvasCompositeSource<'_>,
    transform: CanvasViewTransform,
    points: &[(usize, usize)],
    dirty_rect: Option<Rect>,
) {
    if points.len() < 2 || source.width == 0 || source.height == 0 {
        return;
    }

    for window in points.windows(2) {
        let Some(start) = map_canvas_point_to_display(
            destination,
            source.width,
            source.height,
            transform,
            window[0],
        ) else {
            continue;
        };
        let Some(end) = map_canvas_point_to_display(
            destination,
            source.width,
            source.height,
            transform,
            window[1],
        ) else {
            continue;
        };
        draw_overlay_line(frame, start, end, dirty_rect, [0xff, 0xc1, 0x07, 0xff]);
    }
}

fn draw_overlay_line(
    frame: &mut render::RenderFrame,
    start: (f32, f32),
    end: (f32, f32),
    dirty_rect: Option<Rect>,
    color: [u8; 4],
) {
    let min_x = start.0.min(end.0).floor().max(0.0) as usize;
    let min_y = start.1.min(end.1).floor().max(0.0) as usize;
    let max_x = start.0.max(end.0).ceil().min(frame.width as f32) as usize;
    let max_y = start.1.max(end.1).ceil().min(frame.height as f32) as usize;
    let bounds = Rect {
        x: min_x.saturating_sub(2),
        y: min_y.saturating_sub(2),
        width: max_x.saturating_sub(min_x).saturating_add(4),
        height: max_y.saturating_sub(min_y).saturating_add(4),
    };
    let Some(target) = (match dirty_rect {
        Some(dirty) => bounds.intersect(dirty),
        None => Some(bounds),
    }) else {
        return;
    };

    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f32::EPSILON {
        return;
    }

    for y in target.y..target.y + target.height {
        let row_start = y * frame.width * 4;
        for x in target.x..target.x + target.width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let t = (((px - start.0) * dx + (py - start.1) * dy) / length_sq).clamp(0.0, 1.0);
            let closest_x = start.0 + dx * t;
            let closest_y = start.1 + dy * t;
            let distance = ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt();
            if distance <= 1.25 {
                let index = row_start + x * 4;
                frame.pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    }
}

/// ステータス文字列に必要な横幅を計測する。
pub(super) fn measured_status_width(status_text: &str) -> usize {
    measure_text_width(status_text).saturating_add(16).max(1)
}
