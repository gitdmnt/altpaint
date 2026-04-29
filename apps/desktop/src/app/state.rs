//! `DesktopApp` のキャンバスフレーム状態と overlay 補助を定義する。

use super::DesktopApp;
use super::canvas_frame::build_canvas_frame;
use render_types::{PanelNavigatorEntry, PanelNavigatorOverlay};

impl DesktopApp {
    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// キャンバス フレーム を更新する。
    pub(super) fn refresh_canvas_frame(&mut self) {
        self.canvas_frame = Some(build_canvas_frame(&self.document));
    }

    /// アクティブな パネル マスク オーバーレイ を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub(super) fn active_panel_mask_overlay(&self) -> Option<app_core::PanelBounds> {
        let page = self.document.active_page()?;
        let bounds = self.document.active_panel_bounds()?;
        (page.panels.len() > 1
            || bounds != app_core::PanelBounds::full_page(page.width, page.height))
        .then_some(bounds)
    }

    /// 現在の パネル 生成 プレビュー 範囲 を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub(super) fn panel_creation_preview_bounds(&self) -> Option<app_core::PanelBounds> {
        let (page_width, page_height) = self.document.active_page_dimensions();
        canvas::panel_creation_preview_bounds(&self.canvas_input, page_width, page_height)
    }

    /// パネル navigator オーバーレイ に必要な描画内容を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
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

    /// 既存データを走査して キャンバス dimensions を組み立てる。
    pub(super) fn canvas_dimensions(&self) -> (usize, usize) {
        self.canvas_frame
            .as_ref()
            .map(|bitmap| (bitmap.width, bitmap.height))
            .unwrap_or((1, 1))
    }

    /// Is キャンバス interacting かどうかを返す。
    pub(crate) fn is_canvas_interacting(&self) -> bool {
        self.canvas_input.is_drawing
    }
}
