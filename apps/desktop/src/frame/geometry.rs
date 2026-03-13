//! `frame` 用の固定レイアウト計算と座標変換をまとめる。

use app_core::{PanelSurfacePoint, WindowPoint, WindowRect};

use super::Rect;

/// 矩形 が収まるように矩形を計算する。
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

/// ウィンドウ to パネル サーフェス を別座標系へ変換する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn map_window_to_panel_surface(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    point: WindowPoint,
) -> Option<PanelSurfacePoint> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }

    let window_rect = WindowRect::new(rect.x, rect.y, rect.width, rect.height);
    let local = window_rect.to_panel_surface_point(point)?;
    Some(PanelSurfacePoint::new(
        (((local.x as f32 / rect.width as f32) * surface_width as f32).floor() as usize)
            .min(surface_width.saturating_sub(1)),
        (((local.y as f32 / rect.height as f32) * surface_height as f32).floor() as usize)
            .min(surface_height.saturating_sub(1)),
    ))
}

/// ウィンドウ to パネル サーフェス clamped を別座標系へ変換する。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn map_window_to_panel_surface_clamped(
    surface_width: usize,
    surface_height: usize,
    rect: Rect,
    point: WindowPoint,
) -> Option<PanelSurfacePoint> {
    if surface_width == 0 || surface_height == 0 || rect.width == 0 || rect.height == 0 {
        return None;
    }

    let window_rect = WindowRect::new(rect.x, rect.y, rect.width, rect.height);
    let clamped_point = window_rect.clamp_point(point)?;
    map_window_to_panel_surface(surface_width, surface_height, rect, clamped_point)
}
