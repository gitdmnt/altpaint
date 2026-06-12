//! host↔panel 間で交換される wire 名 (command / service 名) の単一定義点。
//!
//! 全クレートは本モジュールの定数を参照する。wire 名のリテラル直書きは禁止 (BL-036)。
//! wire 名の変更は、本モジュールの定数値 (とピン留めテスト) の書換えとして行う。

/// プロジェクト入出力サービス。
pub mod project_io {
    pub const NEW_DOCUMENT_SIZED: &str = "project_io.new_document_sized";
    pub const SAVE_CURRENT: &str = "project_io.save_current";
    pub const SAVE_AS: &str = "project_io.save_as";
    pub const SAVE_TO_PATH: &str = "project_io.save_to_path";
    pub const LOAD_DIALOG: &str = "project_io.load_dialog";
    pub const LOAD_FROM_PATH: &str = "project_io.load_from_path";
}

/// ワークスペースプリセットサービス。
pub mod workspace {
    pub const RELOAD_PRESETS: &str = "workspace_io.reload_presets";
    pub const APPLY_PRESET: &str = "workspace_io.apply_preset";
    pub const SAVE_PRESET: &str = "workspace_io.save_preset";
    pub const EXPORT_PRESET: &str = "workspace_io.export_preset";
    pub const EXPORT_PRESET_TO_PATH: &str = "workspace_io.export_preset_to_path";
}

/// ツール操作コマンド (`tool.*`) とツールカタログサービス (`tool_catalog.*`)。
pub mod tool {
    pub const SET_ACTIVE: &str = "tool.set_active";
    pub const SELECT: &str = "tool.select";
    pub const SELECT_CHILD: &str = "tool.select_child";
    pub const SET_SIZE: &str = "tool.set_size";
    pub const SET_PRESSURE_ENABLED: &str = "tool.set_pressure_enabled";
    pub const SET_ANTIALIAS: &str = "tool.set_antialias";
    pub const SET_STABILIZATION: &str = "tool.set_stabilization";
    pub const PEN_NEXT: &str = "tool.pen_next";
    pub const PEN_PREV: &str = "tool.pen_prev";
    pub const RELOAD_PEN_PRESETS: &str = "tool.reload_pen_presets";
    pub const IMPORT_PEN_PRESETS: &str = "tool.import_pen_presets";
    pub const IMPORT_PEN_PATH: &str = "tool.import_pen_path";
    pub const SET_COLOR: &str = "tool.set_color";

    pub const CATALOG_RELOAD_TOOLS: &str = "tool_catalog.reload_tools";
    pub const CATALOG_RELOAD_PEN_PRESETS: &str = "tool_catalog.reload_pen_presets";
    pub const CATALOG_IMPORT_PEN_PRESETS: &str = "tool_catalog.import_pen_presets";
    pub const CATALOG_IMPORT_PEN_PATH: &str = "tool_catalog.import_pen_path";
}

/// レイヤー操作コマンド。
pub mod layer {
    pub const ADD: &str = "layer.add";
    pub const REMOVE: &str = "layer.remove";
    pub const SELECT: &str = "layer.select";
    pub const RENAME_ACTIVE: &str = "layer.rename_active";
    pub const MOVE: &str = "layer.move";
    pub const SELECT_NEXT: &str = "layer.select_next";
    pub const CYCLE_BLEND_MODE: &str = "layer.cycle_blend_mode";
    pub const SET_BLEND_MODE: &str = "layer.set_blend_mode";
    pub const TOGGLE_VISIBILITY: &str = "layer.toggle_visibility";
}

/// ビュー操作サービス。
pub mod view {
    pub const SET_ZOOM: &str = "view_service.set_zoom";
    pub const SET_PAN: &str = "view_service.set_pan";
    pub const SET_ROTATION: &str = "view_service.set_rotation";
    pub const FLIP_HORIZONTAL: &str = "view_service.flip_horizontal";
    pub const FLIP_VERTICAL: &str = "view_service.flip_vertical";
    pub const RESET: &str = "view_service.reset";
}

/// コマナビゲーションサービス。
pub mod koma_nav {
    pub const ADD: &str = "koma_nav.add";
    pub const REMOVE: &str = "koma_nav.remove";
    pub const SELECT: &str = "koma_nav.select";
    pub const SELECT_NEXT: &str = "koma_nav.select_next";
    pub const SELECT_PREVIOUS: &str = "koma_nav.select_previous";
    pub const FOCUS_ACTIVE: &str = "koma_nav.focus_active";
}

/// 編集履歴サービス。
pub mod history {
    pub const UNDO: &str = "history.undo";
    pub const REDO: &str = "history.redo";
}

/// ドキュメントスナップショットサービス。
pub mod snapshot {
    pub const CREATE: &str = "snapshot.create";
    pub const RESTORE: &str = "snapshot.restore";
}

/// 画像書き出しサービス。
pub mod export {
    pub const IMAGE: &str = "export.image";
}

/// テキスト描画サービス。
pub mod text_render {
    pub const RENDER_TO_LAYER: &str = "text_render.render_to_layer";
}

/// ワークスペースレイアウト (UI パネル可視性・並び順) サービス。
pub mod workspace_layout {
    pub const SET_PANEL_VISIBILITY: &str = "workspace_layout.set_panel_visibility";
    pub const MOVE_PANEL: &str = "workspace_layout.move_panel";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 全 wire 名定数の一覧。新規追加時はここにも追加する。
    const ALL_WIRE_NAMES: &[&str] = &[
        project_io::NEW_DOCUMENT_SIZED,
        project_io::SAVE_CURRENT,
        project_io::SAVE_AS,
        project_io::SAVE_TO_PATH,
        project_io::LOAD_DIALOG,
        project_io::LOAD_FROM_PATH,
        workspace::RELOAD_PRESETS,
        workspace::APPLY_PRESET,
        workspace::SAVE_PRESET,
        workspace::EXPORT_PRESET,
        workspace::EXPORT_PRESET_TO_PATH,
        tool::SET_ACTIVE,
        tool::SELECT,
        tool::SELECT_CHILD,
        tool::SET_SIZE,
        tool::SET_PRESSURE_ENABLED,
        tool::SET_ANTIALIAS,
        tool::SET_STABILIZATION,
        tool::PEN_NEXT,
        tool::PEN_PREV,
        tool::RELOAD_PEN_PRESETS,
        tool::IMPORT_PEN_PRESETS,
        tool::IMPORT_PEN_PATH,
        tool::SET_COLOR,
        tool::CATALOG_RELOAD_TOOLS,
        tool::CATALOG_RELOAD_PEN_PRESETS,
        tool::CATALOG_IMPORT_PEN_PRESETS,
        tool::CATALOG_IMPORT_PEN_PATH,
        layer::ADD,
        layer::REMOVE,
        layer::SELECT,
        layer::RENAME_ACTIVE,
        layer::MOVE,
        layer::SELECT_NEXT,
        layer::CYCLE_BLEND_MODE,
        layer::SET_BLEND_MODE,
        layer::TOGGLE_VISIBILITY,
        view::SET_ZOOM,
        view::SET_PAN,
        view::SET_ROTATION,
        view::FLIP_HORIZONTAL,
        view::FLIP_VERTICAL,
        view::RESET,
        koma_nav::ADD,
        koma_nav::REMOVE,
        koma_nav::SELECT,
        koma_nav::SELECT_NEXT,
        koma_nav::SELECT_PREVIOUS,
        koma_nav::FOCUS_ACTIVE,
        history::UNDO,
        history::REDO,
        snapshot::CREATE,
        snapshot::RESTORE,
        export::IMAGE,
        text_render::RENDER_TO_LAYER,
        workspace_layout::SET_PANEL_VISIBILITY,
        workspace_layout::MOVE_PANEL,
    ];

    #[test]
    fn wire_names_are_unique_and_namespaced() {
        let unique: BTreeSet<&str> = ALL_WIRE_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), ALL_WIRE_NAMES.len(), "wire 名が重複している");
        for name in ALL_WIRE_NAMES {
            let (namespace, operation) = name
                .split_once('.')
                .unwrap_or_else(|| panic!("wire 名 {name} が namespace.operation 形式でない"));
            assert!(!namespace.is_empty(), "wire 名 {name} の namespace が空");
            assert!(!operation.is_empty(), "wire 名 {name} の operation が空");
        }
    }

    /// wire 値のピン留め (プロトコル互換の回帰網)。
    /// 意図的な wire 改名時は定数定義と本テストを同時に書き換える。
    #[test]
    fn wire_values_are_pinned() {
        assert_eq!(project_io::NEW_DOCUMENT_SIZED, "project_io.new_document_sized");
        assert_eq!(project_io::SAVE_CURRENT, "project_io.save_current");
        assert_eq!(project_io::SAVE_AS, "project_io.save_as");
        assert_eq!(project_io::SAVE_TO_PATH, "project_io.save_to_path");
        assert_eq!(project_io::LOAD_DIALOG, "project_io.load_dialog");
        assert_eq!(project_io::LOAD_FROM_PATH, "project_io.load_from_path");
        assert_eq!(workspace::RELOAD_PRESETS, "workspace_io.reload_presets");
        assert_eq!(workspace::APPLY_PRESET, "workspace_io.apply_preset");
        assert_eq!(workspace::SAVE_PRESET, "workspace_io.save_preset");
        assert_eq!(workspace::EXPORT_PRESET, "workspace_io.export_preset");
        assert_eq!(
            workspace::EXPORT_PRESET_TO_PATH,
            "workspace_io.export_preset_to_path"
        );
        assert_eq!(tool::SET_ACTIVE, "tool.set_active");
        assert_eq!(tool::SELECT, "tool.select");
        assert_eq!(tool::SELECT_CHILD, "tool.select_child");
        assert_eq!(tool::SET_SIZE, "tool.set_size");
        assert_eq!(tool::SET_PRESSURE_ENABLED, "tool.set_pressure_enabled");
        assert_eq!(tool::SET_ANTIALIAS, "tool.set_antialias");
        assert_eq!(tool::SET_STABILIZATION, "tool.set_stabilization");
        assert_eq!(tool::PEN_NEXT, "tool.pen_next");
        assert_eq!(tool::PEN_PREV, "tool.pen_prev");
        assert_eq!(tool::RELOAD_PEN_PRESETS, "tool.reload_pen_presets");
        assert_eq!(tool::IMPORT_PEN_PRESETS, "tool.import_pen_presets");
        assert_eq!(tool::IMPORT_PEN_PATH, "tool.import_pen_path");
        assert_eq!(tool::SET_COLOR, "tool.set_color");
        assert_eq!(tool::CATALOG_RELOAD_TOOLS, "tool_catalog.reload_tools");
        assert_eq!(
            tool::CATALOG_RELOAD_PEN_PRESETS,
            "tool_catalog.reload_pen_presets"
        );
        assert_eq!(
            tool::CATALOG_IMPORT_PEN_PRESETS,
            "tool_catalog.import_pen_presets"
        );
        assert_eq!(tool::CATALOG_IMPORT_PEN_PATH, "tool_catalog.import_pen_path");
        assert_eq!(layer::ADD, "layer.add");
        assert_eq!(layer::REMOVE, "layer.remove");
        assert_eq!(layer::SELECT, "layer.select");
        assert_eq!(layer::RENAME_ACTIVE, "layer.rename_active");
        assert_eq!(layer::MOVE, "layer.move");
        assert_eq!(layer::SELECT_NEXT, "layer.select_next");
        assert_eq!(layer::CYCLE_BLEND_MODE, "layer.cycle_blend_mode");
        assert_eq!(layer::SET_BLEND_MODE, "layer.set_blend_mode");
        assert_eq!(layer::TOGGLE_VISIBILITY, "layer.toggle_visibility");
        assert_eq!(view::SET_ZOOM, "view_service.set_zoom");
        assert_eq!(view::SET_PAN, "view_service.set_pan");
        assert_eq!(view::SET_ROTATION, "view_service.set_rotation");
        assert_eq!(view::FLIP_HORIZONTAL, "view_service.flip_horizontal");
        assert_eq!(view::FLIP_VERTICAL, "view_service.flip_vertical");
        assert_eq!(view::RESET, "view_service.reset");
        assert_eq!(koma_nav::ADD, "koma_nav.add");
        assert_eq!(koma_nav::REMOVE, "koma_nav.remove");
        assert_eq!(koma_nav::SELECT, "koma_nav.select");
        assert_eq!(koma_nav::SELECT_NEXT, "koma_nav.select_next");
        assert_eq!(koma_nav::SELECT_PREVIOUS, "koma_nav.select_previous");
        assert_eq!(koma_nav::FOCUS_ACTIVE, "koma_nav.focus_active");
        assert_eq!(history::UNDO, "history.undo");
        assert_eq!(history::REDO, "history.redo");
        assert_eq!(snapshot::CREATE, "snapshot.create");
        assert_eq!(snapshot::RESTORE, "snapshot.restore");
        assert_eq!(export::IMAGE, "export.image");
        assert_eq!(text_render::RENDER_TO_LAYER, "text_render.render_to_layer");
        assert_eq!(
            workspace_layout::SET_PANEL_VISIBILITY,
            "workspace_layout.set_panel_visibility"
        );
        assert_eq!(workspace_layout::MOVE_PANEL, "workspace_layout.move_panel");
    }
}
