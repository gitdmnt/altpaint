//! 文字列キーを隠蔽する型付きコマンド生成 API を提供する。

/// ツール識別子を型として表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
    Bucket,
    LassoBucket,
    PanelRect,
}

impl Tool {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pen => "pen",
            Self::Eraser => "eraser",
            Self::Bucket => "bucket",
            Self::LassoBucket => "lasso_bucket",
            Self::PanelRect => "panel_rect",
        }
    }
}

/// RGB 色を 8bit 成分で表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl RgbColor {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// 現在の値を 16進文字列 string 形式へ変換する。
    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }
}

/// プロジェクト操作コマンド群。
pub mod project {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 新規 ドキュメント を計算して返す。
    pub fn new_document() -> CommandDescriptor {
        CommandDescriptor::new("project.new")
    }

    /// 現在の値を sized へ変換する。
    pub fn new_sized(width: usize, height: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("project.new_sized");
        descriptor
            .payload
            .insert("size".to_string(), json!(format!("{width}x{height}")));
        descriptor
    }

    /// 保存 を計算して返す。
    pub fn save() -> CommandDescriptor {
        CommandDescriptor::new("project.save")
    }

    /// As を保存先へ書き出す。
    pub fn save_as() -> CommandDescriptor {
        CommandDescriptor::new("project.save_as")
    }

    /// 現在の値を as パス へ変換する。
    pub fn save_as_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("project.save_as_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }

    /// 読込 を計算して返す。
    pub fn load() -> CommandDescriptor {
        CommandDescriptor::new("project.load")
    }

    /// 現在の値を パス へ変換する。
    pub fn load_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("project.load_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

/// ワークスペース操作コマンド群。
pub mod workspace {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 再読込 presets を計算して返す。
    pub fn reload_presets() -> CommandDescriptor {
        CommandDescriptor::new("workspace.reload_presets")
    }

    /// 現在の値を preset へ変換する。
    pub fn apply_preset(preset_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("workspace.apply_preset");
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
        let mut descriptor = CommandDescriptor::new("workspace.save_preset");
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
        let mut descriptor = CommandDescriptor::new("workspace.export_preset");
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
            .payload
            .insert("label".to_string(), json!(label.into()));
        descriptor
    }
}

/// ツール操作コマンド群。
pub mod tool {
    use super::{RgbColor, Tool};
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 現在の値を アクティブ へ変換する。
    pub fn set_active(tool: Tool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_active");
        descriptor
            .payload
            .insert("tool".to_string(), json!(tool.as_str()));
        descriptor
    }

    /// ツール を選択状態へ更新する。
    pub fn select_tool(tool_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.select");
        descriptor
            .payload
            .insert("tool_id".to_string(), json!(tool_id.into()));
        descriptor
    }

    /// 現在の値を 色 16進文字列 へ変換する。
    pub fn set_color_hex(color: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_color");
        descriptor
            .payload
            .insert("color".to_string(), json!(color.into()));
        descriptor
    }

    /// 色 RGB を設定する。
    pub fn set_color_rgb(color: RgbColor) -> CommandDescriptor {
        set_color_hex(color.to_hex_string())
    }

    /// 現在の値を サイズ へ変換する。
    pub fn set_size(size: u32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_size");
        descriptor.payload.insert("size".to_string(), json!(size));
        descriptor
    }

    /// 現在の値を pressure enabled へ変換する。
    pub fn set_pressure_enabled(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_pressure_enabled");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    /// 現在の値を アンチエイリアス へ変換する。
    pub fn set_antialias(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_antialias");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    /// 現在の値を stabilization へ変換する。
    pub fn set_stabilization(amount: u8) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_stabilization");
        descriptor
            .payload
            .insert("amount".to_string(), json!(amount.min(100)));
        descriptor
    }

    /// 次 ペン を選択状態へ更新する。
    pub fn select_next_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_next")
    }

    /// 前 ペン を選択状態へ更新する。
    pub fn select_previous_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_prev")
    }

    /// 再読込 ペン presets を計算して返す。
    pub fn reload_pen_presets() -> CommandDescriptor {
        CommandDescriptor::new("tool.reload_pen_presets")
    }

    /// ペン presets を読み込み、必要に応じて整形して返す。
    pub fn import_pen_presets() -> CommandDescriptor {
        CommandDescriptor::new("tool.import_pen_presets")
    }

    /// 現在の値を ペン パス へ変換する。
    pub fn import_pen_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.import_pen_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

/// ビュー操作コマンド群。
pub mod view {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 現在の値を output へ変換する。
    pub fn zoom(zoom: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.zoom");
        descriptor.payload.insert("zoom".to_string(), json!(zoom));
        descriptor
    }

    /// 現在の値を output へ変換する。
    pub fn pan(delta_x: f32, delta_y: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.pan");
        descriptor
            .payload
            .insert("delta_x".to_string(), json!(delta_x));
        descriptor
            .payload
            .insert("delta_y".to_string(), json!(delta_y));
        descriptor
    }

    /// 現在の値を pan へ変換する。
    pub fn set_pan(pan_x: f32, pan_y: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.set_pan");
        descriptor.payload.insert("pan_x".to_string(), json!(pan_x));
        descriptor.payload.insert("pan_y".to_string(), json!(pan_y));
        descriptor
    }

    /// 現在の値を output へ変換する。
    pub fn rotate(quarter_turns: i32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.rotate");
        descriptor
            .payload
            .insert("quarter_turns".to_string(), json!(quarter_turns));
        descriptor
    }

    /// 現在の値を 回転 degrees へ変換する。
    pub fn set_rotation_degrees(rotation_degrees: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.set_rotation");
        descriptor
            .payload
            .insert("rotation_degrees".to_string(), json!(rotation_degrees));
        descriptor
    }

    /// flip horizontal を計算して返す。
    pub fn flip_horizontal() -> CommandDescriptor {
        CommandDescriptor::new("view.flip_horizontal")
    }

    /// flip vertical を計算して返す。
    pub fn flip_vertical() -> CommandDescriptor {
        CommandDescriptor::new("view.flip_vertical")
    }

    /// 初期化 を計算して返す。
    pub fn reset() -> CommandDescriptor {
        CommandDescriptor::new("view.reset")
    }
}

/// コマ操作コマンド群。
pub mod panel {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 追加 を計算して返す。
    pub fn add() -> CommandDescriptor {
        CommandDescriptor::new("panel.add")
    }

    /// 削除 を計算して返す。
    pub fn remove() -> CommandDescriptor {
        CommandDescriptor::new("panel.remove")
    }

    /// 現在の値を output へ変換する。
    pub fn select(index: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("panel.select");
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    /// 次 を選択状態へ更新する。
    pub fn select_next() -> CommandDescriptor {
        CommandDescriptor::new("panel.select_next")
    }

    /// 前 を選択状態へ更新する。
    pub fn select_previous() -> CommandDescriptor {
        CommandDescriptor::new("panel.select_previous")
    }

    /// アクティブ へフォーカスを移す。
    pub fn focus_active() -> CommandDescriptor {
        CommandDescriptor::new("panel.focus_active")
    }
}

/// レイヤー操作コマンド群。
pub mod layer {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// レイヤーブレンドモードを型として表す。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BlendMode {
        Normal,
        Multiply,
        Screen,
        Add,
    }

    impl BlendMode {
        /// 現在の値を str 形式へ変換する。
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Normal => "normal",
                Self::Multiply => "multiply",
                Self::Screen => "screen",
                Self::Add => "add",
            }
        }
    }

    /// 追加 を計算して返す。
    pub fn add() -> CommandDescriptor {
        CommandDescriptor::new("layer.add")
    }

    /// 削除 を計算して返す。
    pub fn remove() -> CommandDescriptor {
        CommandDescriptor::new("layer.remove")
    }

    /// 現在の値を output へ変換する。
    pub fn select(index: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.select");
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    /// 現在の値を アクティブ へ変換する。
    pub fn rename_active(name: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.rename_active");
        descriptor
            .payload
            .insert("name".to_string(), json!(name.into()));
        descriptor
    }

    /// 現在の値を to へ変換する。
    pub fn move_to(from_index: usize, to_index: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.move");
        descriptor
            .payload
            .insert("from_index".to_string(), json!(from_index));
        descriptor
            .payload
            .insert("to_index".to_string(), json!(to_index));
        descriptor
    }

    /// 次 を選択状態へ更新する。
    pub fn select_next() -> CommandDescriptor {
        CommandDescriptor::new("layer.select_next")
    }

    /// ブレンド モード を順送りで切り替える。
    pub fn cycle_blend_mode() -> CommandDescriptor {
        CommandDescriptor::new("layer.cycle_blend_mode")
    }

    /// 現在の値を ブレンド モード へ変換する。
    pub fn set_blend_mode(mode: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.set_blend_mode");
        descriptor
            .payload
            .insert("mode".to_string(), json!(mode.into()));
        descriptor
    }

    /// ブレンド モード enum を設定する。
    pub fn set_blend_mode_enum(mode: BlendMode) -> CommandDescriptor {
        set_blend_mode(mode.as_str())
    }

    /// Visibility の有効状態を切り替える。
    pub fn toggle_visibility() -> CommandDescriptor {
        CommandDescriptor::new("layer.toggle_visibility")
    }

    /// マスク の有効状態を切り替える。
    pub fn toggle_mask() -> CommandDescriptor {
        CommandDescriptor::new("layer.toggle_mask")
    }
}
