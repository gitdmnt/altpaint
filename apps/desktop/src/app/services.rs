//! ツールカタログ、ペン preset、補助的なサービス処理を扱う。

use std::path::PathBuf;

use app_core::Document;
use desktop_support::{DEFAULT_PROJECT_PATH, default_pen_dir, default_tool_dir, default_panel_dir};
use serde_json::{Map, Value, json};
use storage::{ImportedPenSet, load_pen_directory, load_tool_directory, parse_pen_file};
use workspace_persistence::WorkspaceUiState;

use super::{DesktopApp, TOOL_PALETTE_PANEL_ID};

impl DesktopApp {
    pub(super) fn capture_workspace_ui_state(&self) -> WorkspaceUiState {
        WorkspaceUiState::new(
            self.ui_shell.workspace_layout(),
            self.ui_shell.persistent_panel_configs(),
        )
    }

    pub(crate) fn import_pen_presets(&mut self) -> bool {
        let suggested = default_panel_dir()
            .parent()
            .map(|_| desktop_support::default_pen_dir())
            .unwrap_or_else(desktop_support::default_pen_dir);
        let Some(path) = self.io_state.dialogs.pick_open_pen_path(&suggested) else {
            return false;
        };
        self.import_pen_presets_from_path(path)
    }

    pub(crate) fn import_pen_presets_from_path(&mut self, path: PathBuf) -> bool {
        match parse_pen_file(&path) {
            Ok(imported) => {
                let imported_names = imported
                    .pens
                    .iter()
                    .map(|pen| pen.name.clone())
                    .collect::<Vec<_>>();
                let runtime_presets = imported
                    .pens
                    .iter()
                    .map(|pen| pen.to_runtime_preset())
                    .collect::<Vec<_>>();
                if self.document.merge_pen_presets(runtime_presets) == 0 {
                    self.io_state.dialogs.show_error(
                        "Pen import failed",
                        "no importable pen presets were found",
                    );
                    return false;
                }

                self.update_pen_import_report(&path, &imported, imported_names.as_slice());
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                true
            }
            Err(error) => {
                let message = format!("failed to import pen preset: {error}");
                eprintln!("{message}");
                self.io_state.dialogs.show_error("Pen import failed", &message);
                false
            }
        }
    }

    fn update_pen_import_report(
        &mut self,
        path: &std::path::Path,
        imported: &ImportedPenSet,
        imported_names: &[String],
    ) {
        let source_label = match imported.report.source {
            storage::PenSourceKind::AltPaint => "AltPaint",
            storage::PenSourceKind::PhotoshopAbr => "Photoshop ABR",
            storage::PenSourceKind::ClipStudioSut => "Clip Studio SUT",
            storage::PenSourceKind::GimpGbr => "GIMP GBR",
            storage::PenSourceKind::Unknown => "Unknown",
        };
        let preview = imported_names
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let issues = imported
            .report
            .issues
            .iter()
            .map(|issue| format!("{}: {}", issue.code, issue.message))
            .collect::<Vec<_>>()
            .join(" / ");

        let mut configs = self.ui_shell.persistent_panel_configs();
        let entry = configs
            .entry(TOOL_PALETTE_PANEL_ID.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let object = entry.as_object_mut().expect("config object created");
        object.insert(
            "last_import_summary".to_string(),
            json!(format!(
                "{} / imported={} / skipped={} / file={}",
                source_label,
                imported.report.imported_count,
                imported.report.skipped_count,
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("<unknown>")
            )),
        );
        object.insert("last_import_preview".to_string(), json!(preview));
        object.insert("last_import_issues".to_string(), json!(issues));
        self.ui_shell.set_persistent_panel_configs(configs);
    }

    /// 既定ペンディレクトリからプリセットを再読込する。
    pub(crate) fn reload_pen_presets(&mut self) -> bool {
        let changed = Self::reload_pen_presets_into_document(&mut self.document);
        if changed {
            self.sync_ui_from_document();
            self.mark_status_dirty();
            self.rebuild_present_frame();
        }
        changed
    }

    /// ドキュメントへ読み込んだペンプリセット群を適用する。
    pub(super) fn reload_pen_presets_into_document(document: &mut Document) -> bool {
        let (presets, diagnostics) = load_pen_directory(default_pen_dir());
        for diagnostic in diagnostics {
            eprintln!("pen preset load warning: {diagnostic}");
        }
        if presets.is_empty() {
            return false;
        }
        document.replace_pen_presets(presets);
        true
    }

    /// 既定ツールディレクトリからツールカタログを再読込する。
    pub(super) fn reload_tool_catalog_into_document(document: &mut Document) -> bool {
        let (tools, diagnostics) = load_tool_directory(default_tool_dir());
        for diagnostic in diagnostics {
            eprintln!("tool catalog load warning: {diagnostic}");
        }
        if tools.is_empty() {
            return false;
        }
        document.replace_tool_catalog(tools);
        true
    }

    /// フッターへ表示する現在状態の概要文字列を生成する。
    pub(crate) fn status_text(&self) -> String {
        let file_name = self
            .io_state
            .project_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_PROJECT_PATH);
        let hidden_panels = self
            .ui_shell
            .workspace_layout()
            .panels
            .iter()
            .filter(|entry| !entry.visible)
            .count();
        format!(
            "file={} / tool={:?} / pen={} {}px / color={} / zoom={:.2}x / page={} / panel={}/{} / pages={} / panels={} / hidden={}",
            file_name,
            self.document.active_tool,
            self.document
                .active_pen_preset()
                .map(|preset| preset.name.as_str())
                .unwrap_or("Round Pen"),
            self.document.active_pen_size,
            self.document.active_color.hex_rgb(),
            self.document.view_transform.zoom,
            self.document.active_page_index() + 1,
            self.document.active_panel_index() + 1,
            self.document.active_page_panel_count().max(1),
            self.document.work.pages.len(),
            self.document
                .work
                .pages
                .iter()
                .map(|page| page.panels.len())
                .sum::<usize>(),
            hidden_panels,
        )
    }
}
