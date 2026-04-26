//! `render` クレートのパネル関連型 — 9E-3 で CPU ラスタライズ責務は撤去。
//!
//! 本ファイルに残るのは ui-shell の hit-test 互換型のみ:
//! - `PanelHitKind`
//! - `PanelHitRegion`
//!
//! `rasterize_panel_layer` / `measure_panel_size` / `draw_node` および 11 種の
//! private 描画関数群、さらに `RasterizedPanelLayer` / `FloatingPanel` /
//! `PanelFocusTarget` / `PanelTextInputState` / `PanelRenderState` /
//! `MeasuredPanelSize` 型は Phase 9E-3 で完全削除された。
//!
//! GPU 直描画 (`PanelRuntime::render_panels`) に統一されたため、CPU 経路は
//! 不要となった。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHitKind {
    MovePanel,
    Activate,
    Slider {
        min: i32,
        max: i32,
    },
    ColorWheel {
        hue_degrees: usize,
        saturation: usize,
        value: usize,
    },
    LayerListItem {
        value: i32,
    },
    DropdownOption {
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelHitRegion {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub panel_id: String,
    pub node_id: String,
    pub kind: PanelHitKind,
}
