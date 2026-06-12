//! host service request を型付きで組み立てる API を提供する。
//!
//! wire 名は `panel_protocol::names` の定数のみを参照する (BL-036)。

use panel_protocol::RequestDescriptor;
use serde_json::json;

fn descriptor(name: impl Into<String>) -> RequestDescriptor {
    RequestDescriptor::new(name)
}

pub mod project_io {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::project_io as wire;

    pub fn new_document() -> RequestDescriptor {
        descriptor(wire::NEW_DOCUMENT)
    }

    pub fn new_document_sized(width: usize, height: usize) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::NEW_DOCUMENT_SIZED);
        descriptor.payload.insert("width".to_string(), json!(width));
        descriptor
            .payload
            .insert("height".to_string(), json!(height));
        descriptor
    }

    pub fn save_current() -> RequestDescriptor {
        descriptor(wire::SAVE_CURRENT)
    }

    pub fn save_as() -> RequestDescriptor {
        descriptor(wire::SAVE_AS)
    }

    pub fn save_to_path(path: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SAVE_TO_PATH);
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }

    pub fn load_dialog() -> RequestDescriptor {
        descriptor(wire::LOAD_DIALOG)
    }

    pub fn load_from_path(path: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::LOAD_FROM_PATH);
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod workspace_io {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::workspace as wire;

    pub fn reload_presets() -> RequestDescriptor {
        descriptor(wire::RELOAD_PRESETS)
    }

    pub fn apply_preset(preset_id: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::APPLY_PRESET);
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
    }

    pub fn save_preset(
        preset_id: impl Into<String>,
        label: impl Into<String>,
    ) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SAVE_PRESET);
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    pub fn export_preset(
        preset_id: impl Into<String>,
        label: impl Into<String>,
    ) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::EXPORT_PRESET);
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    pub fn export_preset_to_path(
        preset_id: impl Into<String>,
        label: impl Into<String>,
        path: impl Into<String>,
    ) -> RequestDescriptor {
        let mut descriptor = export_preset(preset_id, label);
        descriptor.name = wire::EXPORT_PRESET_TO_PATH.to_string();
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod tool_catalog {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::tool as wire;

    pub fn reload_tools() -> RequestDescriptor {
        descriptor(wire::CATALOG_RELOAD_TOOLS)
    }

    pub fn reload_pen_presets() -> RequestDescriptor {
        descriptor(wire::CATALOG_RELOAD_PEN_PRESETS)
    }

    pub fn import_pen_presets() -> RequestDescriptor {
        descriptor(wire::CATALOG_IMPORT_PEN_PRESETS)
    }

    pub fn import_pen_path(path: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::CATALOG_IMPORT_PEN_PATH);
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

pub mod view {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::view as wire;

    pub fn set_zoom(zoom: f32) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SET_ZOOM);
        descriptor.payload.insert("zoom".to_string(), json!(zoom));
        descriptor
    }

    pub fn set_pan(pan_x: f32, pan_y: f32) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SET_PAN);
        descriptor.payload.insert("pan_x".to_string(), json!(pan_x));
        descriptor.payload.insert("pan_y".to_string(), json!(pan_y));
        descriptor
    }

    pub fn set_rotation(rotation_degrees: f32) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SET_ROTATION);
        descriptor
            .payload
            .insert("rotation_degrees".to_string(), json!(rotation_degrees));
        descriptor
    }

    pub fn flip_horizontal() -> RequestDescriptor {
        descriptor(wire::FLIP_HORIZONTAL)
    }

    pub fn flip_vertical() -> RequestDescriptor {
        descriptor(wire::FLIP_VERTICAL)
    }

    pub fn reset() -> RequestDescriptor {
        descriptor(wire::RESET)
    }
}

pub mod koma_nav {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::koma_nav as wire;

    pub fn add() -> RequestDescriptor {
        descriptor(wire::ADD)
    }

    pub fn remove() -> RequestDescriptor {
        descriptor(wire::REMOVE)
    }

    pub fn select(index: usize) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SELECT);
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    pub fn select_next() -> RequestDescriptor {
        descriptor(wire::SELECT_NEXT)
    }

    pub fn select_previous() -> RequestDescriptor {
        descriptor(wire::SELECT_PREVIOUS)
    }

    pub fn focus_active() -> RequestDescriptor {
        descriptor(wire::FOCUS_ACTIVE)
    }
}

pub mod history {
    use super::descriptor;
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::history as wire;

    /// 直前の操作を元に戻す。
    pub fn undo() -> RequestDescriptor {
        descriptor(wire::UNDO)
    }

    /// 元に戻した操作をやり直す。
    pub fn redo() -> RequestDescriptor {
        descriptor(wire::REDO)
    }
}

pub mod snapshot {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::snapshot as wire;

    /// スナップショットを作成する。handler は 7-4 で登録する。
    pub fn create(label: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::CREATE);
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }

    /// スナップショットを復元する。handler は 7-4 で登録する。
    pub fn restore(snapshot_id: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::RESTORE);
        descriptor
            .payload
            .insert("snapshot_id".to_string(), json!(snapshot_id.into()));
        descriptor
    }
}

pub mod export_image {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::export as wire;

    /// 画像として書き出す。handler は 7-3 で登録する。
    pub fn export(path: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::IMAGE);
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

/// ワークスペース パネル管理 (workspace-layout パネル) 用サービス。
pub mod workspace_layout {
    use super::{descriptor, json};
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::workspace_layout as wire;

    /// 指定パネルの表示/非表示を切り替える。
    pub fn set_panel_visibility(
        panel_id: impl Into<String>,
        visible: bool,
    ) -> RequestDescriptor {
        let mut descriptor = descriptor(wire::SET_PANEL_VISIBILITY);
        descriptor
            .payload
            .insert("panel_id".to_string(), json!(panel_id.into()));
        descriptor
            .payload
            .insert("visible".to_string(), json!(visible));
        descriptor
    }
}

/// テキスト描画サービス。
pub mod text_render {
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::text_render as wire;
    use serde_json::json;

    /// テキストをアクティブレイヤーへ描画するサービス要求を構築する。
    pub fn render_to_layer(
        text: impl Into<String>,
        font_size: u32,
        color_hex: impl Into<String>,
        x: usize,
        y: usize,
    ) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::RENDER_TO_LAYER);
        descriptor.payload.insert("text".to_string(), json!(text.into()));
        descriptor.payload.insert("font_size".to_string(), json!(font_size));
        descriptor.payload.insert("color_hex".to_string(), json!(color_hex.into()));
        descriptor.payload.insert("x".to_string(), json!(x));
        descriptor.payload.insert("y".to_string(), json!(y));
        descriptor
    }
}
