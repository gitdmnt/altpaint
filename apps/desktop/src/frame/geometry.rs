//! `frame` 用の純粋な幾何計算と `render` 連携 helper をまとめる。
//!
//! 表示矩形・dirty rect・ビュー座標変換の責務を合成処理から分離し、
//! 計算ロジックを副作用のない関数として再利用しやすくする。

use app_core::{CanvasViewTransform, DirtyRect};

use super::{Rect, TextureQuad};

/// `frame::Rect` を `render::PixelRect` へ変換する。
pub(super) fn to_render_rect(rect: Rect) -> render::PixelRect {
    render::PixelRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// `render::PixelRect` を `frame::Rect` へ変換する。
pub(super) fn from_render_rect(rect: render::PixelRect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// `render::TextureQuad` を desktop 側の軽量 quad へ変換する。
fn from_render_quad(quad: render::TextureQuad) -> TextureQuad {
    TextureQuad {
        destination: from_render_rect(quad.destination),
        uv_min: quad.uv_min,
        uv_max: quad.uv_max,
        rotation_turns: quad.rotation_turns,
        flip_x: quad.flip_x,
        flip_y: quad.flip_y,
    }
}

/// desktop 側の表示矩形から `render::CanvasScene` を構築する。
pub(super) fn canvas_scene(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<render::CanvasScene> {
    render::prepare_canvas_scene(
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
    )
}

/// 元画像を target 内へアスペクト比維持で収めた矩形を返す。
pub(crate) fn fit_rect(source_width: usize, source_height: usize, target: Rect) -> Rect {
    if source_width == 0 || source_height == 0 || target.width == 0 || target.height == 0 {
        return Rect {
            x: target.x,
            y: target.y,
            width: 0,
            height: 0,
        };
    }

    let scale_x = target.width as f32 / source_width as f32;
    let scale_y = target.height as f32 / source_height as f32;
    let scale = scale_x.min(scale_y);
    let fitted_width = ((source_width as f32 * scale).floor() as usize).max(1);
    let fitted_height = ((source_height as f32 * scale).floor() as usize).max(1);

    Rect {
        x: target.x + (target.width.saturating_sub(fitted_width)) / 2,
        y: target.y + (target.height.saturating_sub(fitted_height)) / 2,
        width: fitted_width,
        height: fitted_height,
    }
}

/// ビットマップ dirty rect を表示先の矩形へ写像する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn map_canvas_dirty_to_display(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
) -> Rect {
    map_canvas_dirty_to_display_with_transform(
        dirty,
        destination,
        source_width,
        source_height,
        CanvasViewTransform::default(),
    )
}

/// ビュー変換を考慮して dirty rect を表示先へ写像する。
pub(crate) fn map_canvas_dirty_to_display_with_transform(
    dirty: DirtyRect,
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Rect {
    from_render_rect(render::map_canvas_dirty_to_display_with_transform(
        dirty,
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
    ))
}

/// ブラシプレビューの表示範囲を算出する。
pub(crate) fn brush_preview_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: (usize, usize),
) -> Option<Rect> {
    render::brush_preview_rect(
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
        canvas_position,
    )
    .map(from_render_rect)
}

/// キャンバス座標を表示座標へ変換する。
pub(crate) fn map_canvas_point_to_display(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
    canvas_position: (usize, usize),
) -> Option<(f32, f32)> {
    render::map_canvas_point_to_display(
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
        canvas_position,
    )
}

/// ビュー変換後に実際に描かれるキャンバス領域を返す。
#[allow(dead_code)]
pub(crate) fn canvas_drawn_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<Rect> {
    render::canvas_drawn_rect(
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
    )
    .map(from_render_rect)
}

/// 前回表示との差分で露出する背景領域を返す。
#[allow(dead_code)]
pub(crate) fn exposed_canvas_background_rect(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    previous_transform: CanvasViewTransform,
    current_transform: CanvasViewTransform,
) -> Option<Rect> {
    render::exposed_canvas_background_rect(
        to_render_rect(destination),
        source_width,
        source_height,
        previous_transform,
        current_transform,
    )
    .map(from_render_rect)
}

/// GPU 提示用の texture quad を desktop 側型へ変換する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn canvas_texture_quad(
    destination: Rect,
    source_width: usize,
    source_height: usize,
    transform: CanvasViewTransform,
) -> Option<TextureQuad> {
    render::canvas_texture_quad(
        to_render_rect(destination),
        source_width,
        source_height,
        transform,
    )
    .map(from_render_quad)
}

/// ビュー座標をパネルサーフェス座標へ変換する。
pub(crate) fn map_view_to_surface(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }
    if !rect.contains(x, y) {
        return None;
    }

    let local_x = (x - rect.x as i32) as f32;
    let local_y = (y - rect.y as i32) as f32;
    Some((
        (((local_x / rect.width as f32) * surface_width as f32).floor() as usize)
            .min(surface_width.saturating_sub(1)),
        (((local_y / rect.height as f32) * surface_height as f32).floor() as usize)
            .min(surface_height.saturating_sub(1)),
    ))
}

/// ビュー外座標もクランプしたうえでサーフェス座標へ変換する。
pub(crate) fn map_view_to_surface_clamped(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    x: i32,
    y: i32,
) -> Option<(usize, usize)> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }

    let clamped_x = x.clamp(
        rect.x as i32,
        (rect.x + rect.width.saturating_sub(1)) as i32,
    );
    let clamped_y = y.clamp(
        rect.y as i32,
        (rect.y + rect.height.saturating_sub(1)) as i32,
    );
    map_view_to_surface(surface_width, surface_height, rect, clamped_x, clamped_y)
}
