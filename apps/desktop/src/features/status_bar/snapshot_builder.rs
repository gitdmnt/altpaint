//! ステータスバー用スナップショットの組み立て (BL-111)。
//!
//! ツール名・ズーム % ・status text を `DesktopApp` の状態から集約する。
//! B7 で `app/services/mod.rs` から status_bar feature へ移設した。

use crate::app::DesktopApp;
use crate::platform::DEFAULT_PROJECT_FILE_NAME;

use super::StatusSnapshot;

impl DesktopApp {
    /// 9E-4: HtmlPanelView ステータスバー用のスナップショットを組み立てる。
    /// ツール名・ズーム % ・status text を集約して返す。
    pub(crate) fn build_status_snapshot(&self) -> StatusSnapshot {
        let tool_name = self.document.session.active_tool().display_label();
        let zoom_percent = (self.document.session.view_transform.zoom * 100.0)
            .round()
            .clamp(1.0, 100_000.0) as u32;
        let status_text = self.status_text();
        StatusSnapshot::new(tool_name, zoom_percent, status_text)
    }

    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .paths
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
