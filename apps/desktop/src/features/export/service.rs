//! 画像 export サービス要求のハンドラ。

use std::path::PathBuf;

use panel_runtime::{ServiceRequest, services::names};

use crate::app::DesktopApp;

/// export service request を処理する。
pub(crate) fn handle_export_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::EXPORT_IMAGE => app.export_image(request.string("path").map(PathBuf::from)),
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
    /// アクティブパネルを PNG として書き出す。
    ///
    /// `path` が `None` の場合はダイアログでパスを選択する。
    fn export_image(&mut self, path: Option<PathBuf>) -> bool {
        let path = match path {
            Some(p) => p,
            None => {
                let current = self.paths.project_path.with_extension("png");
                match self.dialogs.pick_save_image_path(&current) {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        let path = normalize_image_path(path);
        self.enqueue_export_png(path)
    }
}

/// PNG 拡張子が付いていなければ付与する。
fn normalize_image_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("png")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_image_path_adds_png_extension() {
        assert_eq!(
            normalize_image_path(PathBuf::from("output")),
            PathBuf::from("output.png")
        );
    }

    #[test]
    fn normalize_image_path_preserves_existing_extension() {
        assert_eq!(
            normalize_image_path(PathBuf::from("output.png")),
            PathBuf::from("output.png")
        );
    }
}
