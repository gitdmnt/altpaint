use std::collections::BTreeMap;

use app_core::PanelSurfaceRect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusTarget {
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextInputEditorState {
    pub(crate) cursor_chars: usize,
    pub(crate) preedit: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) struct PanelRenderState<'a> {
    pub(crate) focused_target: Option<&'a FocusTarget>,
    pub(crate) expanded_dropdown: Option<&'a FocusTarget>,
    pub(crate) text_input_states: &'a BTreeMap<(String, String), TextInputEditorState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSurface {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl PanelSurface {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn from_pixels(x: usize, y: usize, width: usize, height: usize, pixels: Vec<u8>) -> Self {
        Self {
            x,
            y,
            width,
            height,
            pixels,
        }
    }

    /// 現在の global 範囲 を返す。
    pub fn global_bounds(&self) -> PanelSurfaceRect {
        PanelSurfaceRect::new(self.x, self.y, self.width, self.height)
    }
}
