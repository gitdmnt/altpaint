use std::path::PathBuf;

use document_model::Document;
use crate::platform::{builtin_panels_dir, pen_dir};
use panel_runtime::{
    ServiceRequest,
    services::names::{self, config_keys, panel_ids},
};
use serde_json::json;
use pen_io::{ImportedPenSet, load_pen_directory, parse_pen_file};

use super::DesktopApp;

/// tool_catalog service request を処理する。
pub(crate) fn handle_tool_catalog_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::TOOL_CATALOG_RELOAD_TOOLS => app.reload_tool_catalog(),
        names::TOOL_CATALOG_RELOAD_PEN_PRESETS => app.reload_pen_presets(),
        names::TOOL_CATALOG_IMPORT_PEN_PRESETS => app.import_pen_presets(),
        names::TOOL_CATALOG_IMPORT_PEN_PATH => {
            app.import_pen_presets_from_path(PathBuf::from(request.string("path")?))
        }
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
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
        let suggested = builtin_panels_dir()
            .parent()
            .map(|_| pen_dir())
            .unwrap_or_else(pen_dir);
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
                if self.document.session.merge_pen_presets(runtime_presets) == 0 {
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
            pen_io::PenSourceKind::AltPaint => "AltPaint",
            pen_io::PenSourceKind::PhotoshopAbr => "Photoshop ABR",
            pen_io::PenSourceKind::ClipStudioSut => "Clip Studio SUT",
            pen_io::PenSourceKind::GimpGbr => "GIMP GBR",
            pen_io::PenSourceKind::Unknown => "Unknown",
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

        let summary = format!(
            "{} / imported={} / skipped={} / file={}",
            source_label,
            imported.report.imported_count,
            imported.report.skipped_count,
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("<unknown>")
        );
        self.update_panel_config(panel_ids::TOOL_PALETTE, |object| {
            object.insert(config_keys::LAST_IMPORT_SUMMARY.to_string(), json!(summary));
            object.insert(config_keys::LAST_IMPORT_PREVIEW.to_string(), json!(preview));
            object.insert(config_keys::LAST_IMPORT_ISSUES.to_string(), json!(issues));
        });
    }

    pub(crate) fn reload_pen_presets(&mut self) -> bool {
        let changed = Self::reload_pen_presets_into_document(&mut self.document);
        if changed {
            self.sync_ui_from_document();
            self.mark_status_dirty();
            self.rebuild_present_frame();
        }
        changed
    }

    pub(crate) fn reload_pen_presets_into_document(document: &mut Document) -> bool {
        let (presets, diagnostics) = load_pen_directory(pen_dir());
        for diagnostic in diagnostics {
            eprintln!("pen preset load warning: {diagnostic}");
        }
        if presets.is_empty() {
            return false;
        }
        document.session.replace_pen_presets(presets);
        true
    }
}
