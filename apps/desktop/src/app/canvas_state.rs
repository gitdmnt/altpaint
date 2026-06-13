//! `DesktopApp` のキャンバス表示状態 (寸法・ブラシプレビュー・overlay 構築) を集約する。
//!
//! present 系 (`present.rs` / `invalidation.rs`) と入力系 (`input.rs`) の両方から参照される
//! 読み取り中心の補助メソッド群。

use super::DesktopApp;
use super::cpu_canvas_snapshot::build_cpu_canvas_snapshot;
use crate::present_quads::{KomaNavigatorEntry, KomaNavigatorOverlay};

impl DesktopApp {
    pub(super) fn brush_preview_size(&self) -> Option<u32> {
        match self.document.session.active_tool() {
            editor_state::ToolKind::Pen | editor_state::ToolKind::Eraser => {
                Some(self.document.session.active_pen_size.max(1))
            }
            editor_state::ToolKind::Bucket
            | editor_state::ToolKind::LassoBucket
            | editor_state::ToolKind::KomaRect => None,
        }
    }

    pub(super) fn refresh_cpu_canvas_snapshot(&mut self) {
        self.paint.cpu_canvas_snapshot = Some(build_cpu_canvas_snapshot(&self.document));
    }

    pub(super) fn active_koma_mask_overlay(&self) -> Option<document_model::KomaBounds> {
        let page = self.document.active_page()?;
        let bounds = self.document.active_koma_bounds()?;
        (page.komas.len() > 1
            || bounds != document_model::KomaBounds::full_page(page.width, page.height))
        .then_some(bounds)
    }

    pub(super) fn koma_creation_preview_bounds(&self) -> Option<document_model::KomaBounds> {
        let (page_width, page_height) = self.document.active_page_dimensions();
        super::koma_gesture::koma_creation_preview_bounds(
            &self.koma_gesture,
            page_width,
            page_height,
        )
    }

    pub(super) fn koma_navigator_overlay(&self) -> Option<KomaNavigatorOverlay> {
        let page = self.document.active_page()?;
        (page.komas.len() > 1).then(|| KomaNavigatorOverlay {
            page_width: page.width,
            page_height: page.height,
            panels: page
                .komas
                .iter()
                .enumerate()
                .map(|(index, koma)| KomaNavigatorEntry {
                    bounds: koma.bounds,
                    active: index == self.document.active_koma_index(),
                })
                .collect(),
        })
    }

    pub(super) fn canvas_dimensions(&self) -> (usize, usize) {
        self.paint.cpu_canvas_snapshot
            .as_ref()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.paint.canvas_input.is_drawing || self.koma_gesture.is_drawing
    }
}
