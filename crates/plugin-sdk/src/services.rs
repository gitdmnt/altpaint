//! host service request を型付きで組み立てる API を提供する。

use panel_schema::CommandDescriptor;
use serde_json::json;

/// 記述子 を計算して返す。
fn descriptor(name: impl Into<String>) -> CommandDescriptor {
    CommandDescriptor::new(name)
}

pub mod project_io {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 新規 ドキュメント を計算して返す。
    pub fn new_document() -> CommandDescriptor {
        descriptor("project_io.new_document")
    }

    /// 現在の値を ドキュメント sized へ変換する。
    pub fn new_document_sized(width: usize, height: usize) -> CommandDescriptor {
        let mut descriptor = descriptor("project_io.new_document_sized");
        descriptor.payload.insert("width".to_string(), json!(width));
        descriptor
            .payload
            .insert("height".to_string(), json!(height));
        descriptor
    }

    /// 現在 を保存先へ書き出す。
    pub fn save_current() -> CommandDescriptor {
        descriptor("project_io.save_current")
    }

    /// As を保存先へ書き出す。
    pub fn save_as() -> CommandDescriptor {
        descriptor("project_io.save_as")
    }

    /// 現在の値を to パス へ変換する。
    pub fn save_to_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("project_io.save_to_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }

    /// ダイアログ を読み込み、必要に応じて整形して返す。
    pub fn load_dialog() -> CommandDescriptor {
        descriptor("project_io.load_dialog")
    }

    /// 現在の値を from パス へ変換する。
    pub fn load_from_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("project_io.load_from_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod workspace_io {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 再読込 presets を計算して返す。
    pub fn reload_presets() -> CommandDescriptor {
        descriptor("workspace_io.reload_presets")
    }

    /// 現在の値を preset へ変換する。
    pub fn apply_preset(preset_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("workspace_io.apply_preset");
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
    }

    /// 現在の値を preset へ変換する。
    pub fn save_preset(
        preset_id: impl Into<String>,
        label: impl Into<String>,
    ) -> CommandDescriptor {
        let mut descriptor = descriptor("workspace_io.save_preset");
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    /// 現在の値を preset へ変換する。
    pub fn export_preset(
        preset_id: impl Into<String>,
        label: impl Into<String>,
    ) -> CommandDescriptor {
        let mut descriptor = descriptor("workspace_io.export_preset");
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    /// 現在の値を preset to パス へ変換する。
    pub fn export_preset_to_path(
        preset_id: impl Into<String>,
        label: impl Into<String>,
        path: impl Into<String>,
    ) -> CommandDescriptor {
        let mut descriptor = export_preset(preset_id, label);
        descriptor.name = "workspace_io.export_preset_to_path".to_string();
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod tool_catalog {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 再読込 tools を計算して返す。
    pub fn reload_tools() -> CommandDescriptor {
        descriptor("tool_catalog.reload_tools")
    }

    /// 再読込 ペン presets を計算して返す。
    pub fn reload_pen_presets() -> CommandDescriptor {
        descriptor("tool_catalog.reload_pen_presets")
    }

    /// ペン presets を読み込み、必要に応じて整形して返す。
    pub fn import_pen_presets() -> CommandDescriptor {
        descriptor("tool_catalog.import_pen_presets")
    }

    /// 現在の値を ペン パス へ変換する。
    pub fn import_pen_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("tool_catalog.import_pen_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod view {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 現在の値を ズーム へ変換する。
    pub fn set_zoom(zoom: f32) -> CommandDescriptor {
        let mut descriptor = descriptor("view_service.set_zoom");
        descriptor.payload.insert("zoom".to_string(), json!(zoom));
        descriptor
    }

    /// 現在の値を pan へ変換する。
    pub fn set_pan(pan_x: f32, pan_y: f32) -> CommandDescriptor {
        let mut descriptor = descriptor("view_service.set_pan");
        descriptor.payload.insert("pan_x".to_string(), json!(pan_x));
        descriptor.payload.insert("pan_y".to_string(), json!(pan_y));
        descriptor
    }

    /// 現在の値を 回転 へ変換する。
    pub fn set_rotation(rotation_degrees: f32) -> CommandDescriptor {
        let mut descriptor = descriptor("view_service.set_rotation");
        descriptor
            .payload
            .insert("rotation_degrees".to_string(), json!(rotation_degrees));
        descriptor
    }

    /// flip horizontal を計算して返す。
    pub fn flip_horizontal() -> CommandDescriptor {
        descriptor("view_service.flip_horizontal")
    }

    /// flip vertical を計算して返す。
    pub fn flip_vertical() -> CommandDescriptor {
        descriptor("view_service.flip_vertical")
    }

    /// 初期化 を計算して返す。
    pub fn reset() -> CommandDescriptor {
        descriptor("view_service.reset")
    }
}

pub mod panel_nav {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 追加 を計算して返す。
    pub fn add() -> CommandDescriptor {
        descriptor("panel_nav.add")
    }

    /// 削除 を計算して返す。
    pub fn remove() -> CommandDescriptor {
        descriptor("panel_nav.remove")
    }

    /// 現在の値を output へ変換する。
    pub fn select(index: usize) -> CommandDescriptor {
        let mut descriptor = descriptor("panel_nav.select");
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    /// 次 を選択状態へ更新する。
    pub fn select_next() -> CommandDescriptor {
        descriptor("panel_nav.select_next")
    }

    /// 前 を選択状態へ更新する。
    pub fn select_previous() -> CommandDescriptor {
        descriptor("panel_nav.select_previous")
    }

    /// アクティブ へフォーカスを移す。
    pub fn focus_active() -> CommandDescriptor {
        descriptor("panel_nav.focus_active")
    }
}

pub mod history {
    use super::descriptor;
    use panel_schema::CommandDescriptor;

    /// 直前の操作を元に戻す。
    pub fn undo() -> CommandDescriptor {
        descriptor("history.undo")
    }

    /// 元に戻した操作をやり直す。
    pub fn redo() -> CommandDescriptor {
        descriptor("history.redo")
    }
}

pub mod snapshot {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// スナップショットを作成する。handler は 7-4 で登録する。
    pub fn create(label: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("snapshot.create");
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    /// スナップショットを復元する。handler は 7-4 で登録する。
    pub fn restore(snapshot_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("snapshot.restore");
        descriptor
            .payload
            .insert("snapshot_id".to_string(), json!(snapshot_id.into()));
        descriptor
    }
}

pub mod export_image {
    use super::{descriptor, json};
    use panel_schema::CommandDescriptor;

    /// 画像として書き出す。handler は 7-3 で登録する。
    pub fn export(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = descriptor("export.image");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}
