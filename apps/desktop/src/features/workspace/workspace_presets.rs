//! ワークスペースプリセットの永続化カタログ (BL-095)。
//!
//! B7 で `desktop-support::workspace_presets` から features/workspace へ移管した。

use std::path::Path;

use panel_workspace::WorkspaceUiState;
use serde::{Deserialize, Serialize};

use crate::features::json_store::{JsonLoad, load_json};

pub(crate) const CURRENT_WORKSPACE_PRESET_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct WorkspacePreset {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) ui_state: WorkspaceUiState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct WorkspacePresetCatalog {
    #[serde(default = "default_workspace_preset_format_version")]
    pub(crate) format_version: u32,
    #[serde(default)]
    pub(crate) default_preset_id: String,
    #[serde(default)]
    pub(crate) presets: Vec<WorkspacePreset>,
}

fn default_workspace_preset_format_version() -> u32 {
    CURRENT_WORKSPACE_PRESET_FORMAT_VERSION
}

/// 既定ワークスペースプリセットを `path` から読み込む (BL-095)。
///
/// Missing / Corrupt はいずれも `default_catalog` (呼び出し側が panel meta から構築) で
/// 動作する。Corrupt は load_json が診断を出力済みで、元ファイルはこの層では温存される
/// (書き込まない)。ビルトイン ID をこの層が知らないよう、既定カタログは引数で受け取る。
pub(crate) fn load_workspace_preset_catalog(
    path: impl AsRef<Path>,
    default_catalog: WorkspacePresetCatalog,
) -> WorkspacePresetCatalog {
    match load_json::<WorkspacePresetCatalog>(path, "workspace presets") {
        JsonLoad::Loaded(catalog)
            if catalog.format_version == CURRENT_WORKSPACE_PRESET_FORMAT_VERSION
                && !catalog.presets.is_empty() =>
        {
            catalog
        }
        _ => default_catalog,
    }
}

pub(crate) fn save_workspace_preset_catalog(
    path: impl AsRef<Path>,
    catalog: &WorkspacePresetCatalog,
) -> std::io::Result<()> {
    let serialized = serde_json::to_vec_pretty(catalog)?;
    std::fs::write(path, serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "altpaint-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        ))
    }

    /// テスト用の最小カタログ (BL-095: ビルトイン ID 既定値はこの層が持たない)。
    fn fixture_catalog() -> WorkspacePresetCatalog {
        WorkspacePresetCatalog {
            format_version: CURRENT_WORKSPACE_PRESET_FORMAT_VERSION,
            default_preset_id: "fixture".to_string(),
            presets: vec![WorkspacePreset {
                id: "fixture".to_string(),
                label: "Fixture".to_string(),
                ui_state: WorkspaceUiState::default(),
            }],
        }
    }

    #[test]
    fn workspace_preset_catalog_roundtrip_preserves_default_preset() {
        let path = unique_test_path("workspace-presets");
        let catalog = fixture_catalog();

        save_workspace_preset_catalog(&path, &catalog).expect("save should succeed");
        let loaded = load_workspace_preset_catalog(&path, fixture_catalog());

        assert_eq!(loaded, catalog);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn corrupt_catalog_falls_back_to_provided_default_without_overwriting() {
        let path = unique_test_path("corrupt-workspace-presets");
        let raw = b"{ user was hand-editing and left it broken";
        std::fs::write(&path, raw).expect("write corrupt");

        // 破損カタログでも引数の既定カタログで動作する。
        let loaded = load_workspace_preset_catalog(&path, fixture_catalog());
        assert_eq!(loaded, fixture_catalog());

        // 破損ファイルは温存される (黙って既定値で上書きしない)。
        let after = std::fs::read(&path).expect("read back");
        assert_eq!(after, raw);
        let _ = std::fs::remove_file(path);
    }
}
