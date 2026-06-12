//! `DesktopApp` のキャンバスフレーム状態 (寸法・ブラシプレビュー・overlay 構築) を集約する。
//!
//! present 系 (`present.rs` / `present_state.rs`) と入力系 (`input.rs`) の両方から参照される
//! 読み取り中心の補助メソッド群。

use super::DesktopApp;
use super::canvas_frame::build_canvas_frame;
use render_types::{PanelNavigatorEntry, PanelNavigatorOverlay};

impl DesktopApp {
    pub(super) fn brush_preview_size(&self) -> Option<u32> {
        match self.document.active_tool {
            app_core::ToolKind::Pen | app_core::ToolKind::Eraser => {
                Some(self.document.active_pen_size.max(1))
            }
            app_core::ToolKind::Bucket
            | app_core::ToolKind::LassoBucket
            | app_core::ToolKind::PanelRect => None,
        }
    }

    pub(super) fn refresh_canvas_frame(&mut self) {
        self.canvas_frame = Some(build_canvas_frame(&self.document));
    }

    pub(super) fn active_panel_mask_overlay(&self) -> Option<app_core::KomaBounds> {
        let page = self.document.active_page()?;
        let bounds = self.document.active_panel_bounds()?;
        (page.panels.len() > 1
            || bounds != app_core::KomaBounds::full_page(page.width, page.height))
        .then_some(bounds)
    }

    pub(super) fn panel_creation_preview_bounds(&self) -> Option<app_core::KomaBounds> {
        let (page_width, page_height) = self.document.active_page_dimensions();
        canvas::panel_creation_preview_bounds(&self.canvas_input, page_width, page_height)
    }

    pub(super) fn panel_navigator_overlay(&self) -> Option<PanelNavigatorOverlay> {
        let page = self.document.active_page()?;
        (page.panels.len() > 1).then(|| PanelNavigatorOverlay {
            page_width: page.width,
            page_height: page.height,
            panels: page
                .panels
                .iter()
                .enumerate()
                .map(|(index, panel)| PanelNavigatorEntry {
                    bounds: panel.bounds,
                    active: index == self.document.active_panel_index(),
                })
                .collect(),
        })
    }

    pub(super) fn canvas_dimensions(&self) -> (usize, usize) {
        self.canvas_frame
            .as_ref()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}
