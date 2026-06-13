//! host service request と補助的な状態同期処理を扱う。

mod gpu_sync;
mod history;
mod registry;

use crate::platform::DEFAULT_PROJECT_FILE_NAME;

use super::DesktopApp;

impl DesktopApp {
    /// 9E-4: HtmlPanelView ステータスバー用のスナップショットを組み立てる。
    /// ツール名・ズーム % ・status text を集約して返す。
    pub(crate) fn build_status_snapshot(&self) -> crate::features::status_bar::StatusSnapshot {
        let tool_name = self.document.session.active_tool().display_label();
        let zoom_percent =
            (self.document.session.view_transform.zoom * 100.0).round().clamp(1.0, 100_000.0) as u32;
        let status_text = self.status_text();
        crate::features::status_bar::StatusSnapshot::new(tool_name, zoom_percent, status_text)
    }

    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .io_state
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_FILE_NAME);
        let hidden_panels = self
            .panel_workspace
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / page={} / koma={}/{} / pages={} / komas={} / hidden={}",
            file_name,
            self.document.session.active_tool(),
            self.document
                .session
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.session.active_pen_size,
            self.document.session.active_color.hex_rgb(),
            self.document.session.view_transform.zoom,
            self.document.active_page_index() + 1,
            self.document.active_koma_index() + 1,
            self.document.active_page_koma_count().max(1),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.komas.len())
                .sum::<usize>(),
            hidden_panels,
        )
    }
}
