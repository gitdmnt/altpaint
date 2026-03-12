use std::path::PathBuf;

use app_core::Document;
use desktop_support::{default_panel_dir, default_pen_dir};
use plugin_api::{ServiceRequest, services::names};
use serde_json::{Map, Value, json};
use storage::{ImportedPenSet, load_pen_directory, parse_pen_file};

use super::DesktopApp;
use crate::app::TOOL_PALETTE_PANEL_ID;

impl DesktopApp {
    pub(super) fn handle_tool_catalog_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::TOOL_CATALOG_RELOAD_TOOLS => self.reload_tool_catalog(),
            names::TOOL_CATALOG_RELOAD_PEN_PRESETS => self.reload_pen_presets(),
            names::TOOL_CATALOG_IMPORT_PEN_PRESETS => self.import_pen_presets(),
            names::TOOL_CATALOG_IMPORT_PEN_PATH => {
                self.import_pen_presets_from_path(PathBuf::from(request.string("path")?))
            }
            _ => return None,
        };
        Some(changed)
    }

    pub(crate) fn reload_tool_catalog(&mut self) -> bool {
        let changed = Self::reload_tool_catalog_into_document(&mut self.document);
        if changed {
            self.sync_ui_from_document();
            self.mark_status_dirty();
            self.rebuild_present_frame();
        }
        changed
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
                    self.io_state
                        .dialogs
                        .show_error("Pen import failed", "no importable pen presets were found");
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
                self.io_state
                    .dialogs
                    .show_error("Pen import failed", &message);
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

        let mut configs = self.panel_runtime.persistent_panel_configs();
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
        self.panel_runtime.replace_persistent_panel_configs(configs);
        self.panel_presentation
            .reconcile_runtime_panels(&self.panel_runtime);
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
    pub(crate) fn reload_pen_presets_into_document(document: &mut Document) -> bool {
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
}
