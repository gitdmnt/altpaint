//! GPU の単色クワッド描画用 DTO とピクセル→NDC 変換ヘルパ。
//!
//! `SolidQuad` は `WgpuPresenter` の `solid_quad_pipeline` に渡される
//! 描画リクエストの最小単位。`pixel_rect_to_ndc` は wgpu の Y 軸 (上=+1) を
//! 考慮した変換式を一箇所に集約する。

use desktop_support::{
    ACTIVE_UI_PANEL_BORDER, APP_BACKGROUND, CANVAS_BACKGROUND, CANVAS_FRAME_BACKGROUND,
    CANVAS_FRAME_BORDER,
};

use super::Rect;

/// 1px 線幅の枠線分解で使用する固定線幅。
const BORDER_THICKNESS: usize = 1;

/// GPU で描画する単色矩形の最小 DTO。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SolidQuad {
    pub(crate) rect: Rect,
    /// RGBA 各 1 バイト (sRGB)。
    pub(crate) color: [u8; 4],
}

/// ピクセル矩形を wgpu NDC 座標 `[left, top, right, bottom]` に変換する。
///
/// wgpu の NDC は左 = -1.0, 右 = +1.0, 上 = +1.0, 下 = -1.0。
/// ピクセル Y は下方向が増加するため、Y は反転して NDC 化する。
pub(crate) fn pixel_rect_to_ndc(
    rect: Rect,
    surface_width: u32,
    surface_height: u32,
) -> [f32; 4] {
    let surface_width = surface_width.max(1) as f32;
    let surface_height = surface_height.max(1) as f32;
    let left = rect.x as f32 / surface_width * 2.0 - 1.0;
    let top = 1.0 - rect.y as f32 / surface_height * 2.0;
    let right = (rect.x + rect.width) as f32 / surface_width * 2.0 - 1.0;
    let bottom = 1.0 - (rect.y + rect.height) as f32 / surface_height * 2.0;
    [left, top, right, bottom]
}

/// 矩形を 4 本の 1px 枠線（top/bottom/left/right）に分解する。
///
/// 矩形の幅・高さが 0 の場合は何も追加しない。
pub(crate) fn push_border_quads(out: &mut Vec<SolidQuad>, rect: Rect, color: [u8; 4]) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let t = BORDER_THICKNESS;
    out.push(SolidQuad {
        rect: Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: t.min(rect.height),
        },
        color,
    });
    if rect.height > t {
        out.push(SolidQuad {
            rect: Rect {
                x: rect.x,
                y: rect.y + rect.height - t,
                width: rect.width,
                height: t,
            },
            color,
        });
    }
    if rect.height > t * 2 {
        let inner_h = rect.height - t * 2;
        out.push(SolidQuad {
            rect: Rect {
                x: rect.x,
                y: rect.y + t,
                width: t.min(rect.width),
                height: inner_h,
            },
            color,
        });
        if rect.width > t {
            out.push(SolidQuad {
                rect: Rect {
                    x: rect.x + rect.width - t,
                    y: rect.y + t,
                    width: t,
                    height: inner_h,
                },
                color,
            });
        }
    }
}

/// `host` 矩形のうち `display` 矩形に覆われない 4 つのマージン領域を返す。
fn host_margins(host: Rect, display: Rect) -> [Rect; 4] {
    let display_y = display.y.max(host.y);
    let display_y_end = (display.y + display.height).min(host.y + host.height);
    [
        Rect {
            x: host.x,
            y: host.y,
            width: host.width,
            height: display_y.saturating_sub(host.y),
        },
        Rect {
            x: host.x,
            y: display_y_end,
            width: host.width,
            height: (host.y + host.height).saturating_sub(display_y_end),
        },
        Rect {
            x: host.x,
            y: display_y,
            width: display.x.saturating_sub(host.x),
            height: display_y_end.saturating_sub(display_y),
        },
        Rect {
            x: display.x + display.width,
            y: display_y,
            width: (host.x + host.width).saturating_sub(display.x + display.width),
            height: display_y_end.saturating_sub(display_y),
        },
    ]
}

/// 背景レイヤー (L0) に積む solid quads を組み立てる。
///
/// 含む内容:
/// - ウィンドウ全面の `APP_BACKGROUND`
/// - キャンバスホストの `display` 領域 (`CANVAS_BACKGROUND`)
/// - キャンバスホストの 4 マージン領域 (`CANVAS_FRAME_BACKGROUND`)
/// - キャンバスホスト枠線 (`CANVAS_FRAME_BORDER`、4 矩形分解)
pub(crate) fn build_background_solid_quads(window: Rect, host: Rect, display: Rect) -> Vec<SolidQuad> {
    let mut quads = Vec::with_capacity(10);
    quads.push(SolidQuad {
        rect: window,
        color: APP_BACKGROUND,
    });
    if display.width > 0 && display.height > 0 {
        quads.push(SolidQuad {
            rect: display,
            color: CANVAS_BACKGROUND,
        });
    }
    for margin in host_margins(host, display) {
        if margin.width > 0 && margin.height > 0 {
            quads.push(SolidQuad {
                rect: margin,
                color: CANVAS_FRAME_BACKGROUND,
            });
        }
    }
    push_border_quads(&mut quads, host, CANVAS_FRAME_BORDER);
    quads
}

/// 前景レイヤー (L6) に積む solid quads を組み立てる。
///
/// 現状はアクティブ UI パネル枠線のみ。`active_panel_rect` が `None` または
/// 0 サイズなら空の `Vec` を返す。
pub(crate) fn build_foreground_solid_quads(active_panel_rect: Option<Rect>) -> Vec<SolidQuad> {
    let Some(rect) = active_panel_rect else {
        return Vec::new();
    };
    if rect.width == 0 || rect.height == 0 {
        return Vec::new();
    }
    let mut quads = Vec::with_capacity(4);
    push_border_quads(&mut quads, rect, ACTIVE_UI_PANEL_BORDER);
    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: usize, y: usize, w: usize, h: usize) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn pixel_rect_to_ndc_maps_fullscreen() {
        let ndc = pixel_rect_to_ndc(rect(0, 0, 640, 480), 640, 480);
        assert_eq!(ndc, [-1.0, 1.0, 1.0, -1.0]);
    }

    #[test]
    fn pixel_rect_to_ndc_maps_quadrant() {
        let ndc = pixel_rect_to_ndc(rect(0, 0, 320, 240), 640, 480);
        assert_eq!(ndc, [-1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn pixel_rect_to_ndc_clamps_zero_surface() {
        let ndc = pixel_rect_to_ndc(rect(0, 0, 0, 0), 0, 0);
        assert_eq!(ndc, [-1.0, 1.0, -1.0, 1.0]);
    }

    #[test]
    fn build_background_emits_app_background_first() {
        let quads = build_background_solid_quads(
            rect(0, 0, 800, 600),
            rect(8, 40, 600, 500),
            rect(8, 40, 600, 500),
        );
        assert_eq!(quads[0].rect, rect(0, 0, 800, 600));
        assert_eq!(quads[0].color, APP_BACKGROUND);
    }

    #[test]
    fn build_background_emits_four_border_quads_for_host() {
        let quads = build_background_solid_quads(
            rect(0, 0, 800, 600),
            rect(8, 40, 600, 500),
            rect(8, 40, 600, 500),
        );
        let border_count = quads
            .iter()
            .filter(|q| q.color == CANVAS_FRAME_BORDER)
            .count();
        assert_eq!(border_count, 4);
    }

    #[test]
    fn build_background_emits_canvas_background_for_display_rect() {
        let quads = build_background_solid_quads(
            rect(0, 0, 800, 600),
            rect(8, 40, 600, 500),
            rect(58, 90, 500, 400),
        );
        let canvas = quads
            .iter()
            .find(|q| q.color == CANVAS_BACKGROUND)
            .expect("CANVAS_BACKGROUND quad");
        assert_eq!(canvas.rect, rect(58, 90, 500, 400));
    }

    #[test]
    fn build_background_fills_margins_when_display_is_smaller_than_host() {
        let quads = build_background_solid_quads(
            rect(0, 0, 800, 600),
            rect(8, 40, 600, 500),
            rect(58, 90, 500, 400),
        );
        let margin_count = quads
            .iter()
            .filter(|q| q.color == CANVAS_FRAME_BACKGROUND)
            .count();
        assert_eq!(margin_count, 4);
    }

    #[test]
    fn build_background_skips_margins_when_display_equals_host() {
        let host = rect(8, 40, 600, 500);
        let quads = build_background_solid_quads(rect(0, 0, 800, 600), host, host);
        let margin_count = quads
            .iter()
            .filter(|q| q.color == CANVAS_FRAME_BACKGROUND)
            .count();
        assert_eq!(margin_count, 0);
    }

    #[test]
    fn build_foreground_returns_empty_when_active_rect_is_none() {
        assert!(build_foreground_solid_quads(None).is_empty());
    }

    #[test]
    fn build_foreground_returns_empty_for_zero_sized_rect() {
        assert!(build_foreground_solid_quads(Some(rect(10, 10, 0, 0))).is_empty());
    }

    #[test]
    fn build_foreground_emits_four_quads_for_active_panel() {
        let quads = build_foreground_solid_quads(Some(rect(100, 100, 200, 150)));
        assert_eq!(quads.len(), 4);
        for quad in &quads {
            assert_eq!(quad.color, ACTIVE_UI_PANEL_BORDER);
        }
    }
}
