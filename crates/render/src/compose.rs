use desktop_support::{FOOTER_HEIGHT, TEXT_PRIMARY, TEXT_SECONDARY, WINDOW_PADDING};

use render_types::{FramePlan, PanelSurfaceSource, PixelRect};

use crate::status::status_text_bounds;
use crate::{RenderFrame, draw_text_rgba};
/// 合成 background フレーム を生成する。
///
/// 9C-1 時点では透明な L1 バッファを返し、ステータステキストとデバッグラベルだけを CPU で描画する。
/// 単色矩形・枠線・キャンバスホスト背景は GPU の L0 solid quad パイプラインが担当する。
/// 9C-2 でテキスト描画を GPU 化したらこの関数自体が削除される。
pub fn compose_background_frame(plan: &FramePlan<'_>) -> RenderFrame {
    let mut frame = RenderFrame {
        width: plan.window_width,
        height: plan.window_height,
        pixels: vec![0; plan.window_width * plan.window_height * 4],
    };

    if std::env::var_os("ALTPAINT_DEBUG_LABELS").is_some() {
        draw_text(
            &mut frame,
            WINDOW_PADDING,
            WINDOW_PADDING + 4,
            "Background layer",
            TEXT_PRIMARY,
        );
        draw_text(
            &mut frame,
            plan.canvas.host_rect.x,
            WINDOW_PADDING + 4,
            "Canvas layer (winit + wgpu canvas texture)",
            TEXT_PRIMARY,
        );
        draw_text(
            &mut frame,
            WINDOW_PADDING,
            plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
            "UI panels are rendered into an independent floating UI layer.",
            TEXT_SECONDARY,
        );
    }
    draw_text(
        &mut frame,
        plan.canvas.host_rect.x,
        plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
        plan.status_text,
        TEXT_SECONDARY,
    );

    frame
}

/// 合成 UI パネル フレーム に必要な差分領域だけを描画または合成する。
pub fn compose_ui_panel_frame(plan: &FramePlan<'_>) -> RenderFrame {
    let mut frame = RenderFrame {
        width: plan.window_width,
        height: plan.window_height,
        pixels: vec![0; plan.window_width * plan.window_height * 4],
    };
    compose_ui_panel_region(&mut frame, plan, None);
    frame
}

/// 合成 UI パネル 領域 に必要な差分領域だけを描画または合成する。
pub fn compose_ui_panel_region(
    frame: &mut RenderFrame,
    plan: &FramePlan<'_>,
    dirty_rect: Option<PixelRect>,
) {
    let clear_rect = dirty_rect.unwrap_or(PixelRect {
        x: 0,
        y: 0,
        width: frame.width,
        height: frame.height,
    });
    fill_rect(frame, clear_rect, [0, 0, 0, 0]);
    compose_panel_host_region(frame, plan.panel_surface, dirty_rect);
}

/// 合成 パネル ホスト 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub fn compose_panel_host_region(
    frame: &mut RenderFrame,
    panel_surface: PanelSurfaceSource<'_>,
    dirty_rect: Option<PixelRect>,
) {
    blit_rgba_region_at(
        frame,
        panel_surface.rect(),
        panel_surface.width,
        panel_surface.height,
        panel_surface.pixels,
        dirty_rect,
    );
}

/// ステータス領域の差分更新。
///
/// 9C-1 時点では L1 はテキスト専用バッファのため、領域を透明にクリアしてから
/// 新しい status text を描画し直す。GPU L0 が背景を毎フレーム塗るため fill_rect は不要。
pub fn compose_status_region(frame: &mut RenderFrame, plan: &FramePlan<'_>) {
    let status_rect = status_text_bounds(
        plan.window_width,
        plan.window_height,
        plan.canvas.host_rect,
        plan.status_text,
    );
    fill_rect(frame, status_rect, [0, 0, 0, 0]);
    draw_text(
        frame,
        plan.canvas.host_rect.x,
        plan.window_height.saturating_sub(FOOTER_HEIGHT) + 6,
        plan.status_text,
        TEXT_SECONDARY,
    );
}

/// 描画 テキスト に必要な描画内容を組み立てる。
fn draw_text(frame: &mut RenderFrame, x: usize, y: usize, text: &str, color: [u8; 4]) {
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

/// 塗りつぶし 矩形 に必要な描画内容を組み立てる。
fn fill_rect(frame: &mut RenderFrame, rect: PixelRect, color: [u8; 4]) {
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

/// 塗りつぶし RGBA slice に必要な描画内容を組み立てる。
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

/// Blit scaled RGBA 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub fn blit_scaled_rgba_region(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<PixelRect>,
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

    if destination.width < source_width || destination.height < source_height {
        // 縮小時は bilinear 補間でアンチエイリアスを適用する
        for dst_y in target.y..target.y + target.height {
            let local_y = dst_y - destination.y;
            let src_y_f =
                (local_y as f32 + 0.5) * source_height as f32 / destination.height as f32 - 0.5;
            let y0 = src_y_f.floor() as i64;
            let y1 = y0 + 1;
            let wy = src_y_f - y0 as f32;
            let y0c = y0.clamp(0, source_height as i64 - 1) as usize;
            let y1c = y1.clamp(0, source_height as i64 - 1) as usize;
            let dst_row_start = dst_y * frame.width * 4;
            for dst_x in target.x..target.x + target.width {
                let local_x = dst_x - destination.x;
                let src_x_f = (local_x as f32 + 0.5) * source_width as f32
                    / destination.width as f32
                    - 0.5;
                let x0 = src_x_f.floor() as i64;
                let x1 = x0 + 1;
                let wx = src_x_f - x0 as f32;
                let x0c = x0.clamp(0, source_width as i64 - 1) as usize;
                let x1c = x1.clamp(0, source_width as i64 - 1) as usize;

                let sample = |sx: usize, sy: usize| -> [f32; 4] {
                    let i = (sy * source_width + sx) * 4;
                    [
                        source_pixels[i] as f32,
                        source_pixels[i + 1] as f32,
                        source_pixels[i + 2] as f32,
                        source_pixels[i + 3] as f32,
                    ]
                };
                let p00 = sample(x0c, y0c);
                let p10 = sample(x1c, y0c);
                let p01 = sample(x0c, y1c);
                let p11 = sample(x1c, y1c);

                let lerp = |a: f32, b: f32, t: f32| -> f32 { a + (b - a) * t };
                let to_u8 = |v: f32| -> u8 { v.round().clamp(0.0, 255.0) as u8 };
                let pixel = [
                    to_u8(lerp(lerp(p00[0], p10[0], wx), lerp(p01[0], p11[0], wx), wy)),
                    to_u8(lerp(lerp(p00[1], p10[1], wx), lerp(p01[1], p11[1], wx), wy)),
                    to_u8(lerp(lerp(p00[2], p10[2], wx), lerp(p01[2], p11[2], wx), wy)),
                    to_u8(lerp(lerp(p00[3], p10[3], wx), lerp(p01[3], p11[3], wx), wy)),
                ];
                let dst_index = dst_row_start + dst_x * 4;
                frame.pixels[dst_index..dst_index + 4].copy_from_slice(&pixel);
            }
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

/// Blit RGBA 領域 at に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn blit_rgba_region_at(
    frame: &mut RenderFrame,
    destination: PixelRect,
    source_width: usize,
    source_height: usize,
    source_pixels: &[u8],
    dirty_rect: Option<PixelRect>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceAxisRun {
    pub dst_offset: usize,
    pub len: usize,
    pub src_index: usize,
}

/// ソース axis runs を構築する。
pub fn build_source_axis_runs(
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

/// 塗りつぶし RGBA block に必要な描画内容を組み立てる。
pub fn fill_rgba_block(
    frame: &mut RenderFrame,
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

/// スクロール キャンバス 領域 に必要な描画内容を組み立てる。
pub fn scroll_canvas_region(
    frame: &mut RenderFrame,
    region: PixelRect,
    delta_x: i32,
    delta_y: i32,
) -> PixelRect {
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

/// exposed スクロール 矩形 を計算して返す。
fn exposed_scroll_rect(region: PixelRect, shift_x: i32, shift_y: i32) -> PixelRect {
    if shift_x.unsigned_abs() as usize >= region.width
        || shift_y.unsigned_abs() as usize >= region.height
    {
        return region;
    }

    let mut exposed = None;
    if shift_x > 0 {
        exposed = Some(PixelRect {
            x: region.x,
            y: region.y,
            width: shift_x as usize,
            height: region.height,
        });
    } else if shift_x < 0 {
        let width = shift_x.unsigned_abs() as usize;
        exposed = Some(PixelRect {
            x: region.x + region.width - width,
            y: region.y,
            width,
            height: region.height,
        });
    }

    if shift_y > 0 {
        let rect = PixelRect {
            x: region.x,
            y: region.y,
            width: region.width,
            height: shift_y as usize,
        };
        exposed = Some(exposed.map_or(rect, |existing: PixelRect| existing.union(rect)));
    } else if shift_y < 0 {
        let height = shift_y.unsigned_abs() as usize;
        let rect = PixelRect {
            x: region.x,
            y: region.y + region.height - height,
            width: region.width,
            height,
        };
        exposed = Some(exposed.map_or(rect, |existing: PixelRect| existing.union(rect)));
    }

    exposed.unwrap_or(region)
}

