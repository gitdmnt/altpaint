//! 文字列キーを隠蔽する型付きコマンド生成 API を提供する。

/// ツール識別子を型として表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
    Bucket,
    LassoBucket,
}

impl Tool {
    /// SDK が使う文字列表現へ変換する。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pen => "pen",
            Self::Eraser => "eraser",
            Self::Bucket => "bucket",
            Self::LassoBucket => "lasso_bucket",
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
    /// RGB 色を構築する。
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// `#RRGGBB` 形式へ変換する。
    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }
}

/// プロジェクト操作コマンド群。
pub mod project {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// 新規ドキュメント作成コマンドを返す。
    pub fn new_document() -> CommandDescriptor {
        CommandDescriptor::new("project.new")
    }

    /// 指定サイズの新規ドキュメント作成コマンドを返す。
    pub fn new_sized(width: usize, height: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("project.new_sized");
        descriptor
            .payload
            .insert("size".to_string(), json!(format!("{width}x{height}")));
        descriptor
    }

    /// 保存コマンドを返す。
    pub fn save() -> CommandDescriptor {
        CommandDescriptor::new("project.save")
    }

    /// 名前を付けて保存コマンドを返す。
    pub fn save_as() -> CommandDescriptor {
        CommandDescriptor::new("project.save_as")
    }

    /// パス付き保存コマンドを返す。
    pub fn save_as_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("project.save_as_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }

    /// 読み込みコマンドを返す。
    pub fn load() -> CommandDescriptor {
        CommandDescriptor::new("project.load")
    }

    /// パス付き読み込みコマンドを返す。
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

    /// workspace preset カタログ再読込コマンドを返す。
    pub fn reload_presets() -> CommandDescriptor {
        CommandDescriptor::new("workspace.reload_presets")
    }

    /// 指定 workspace preset 適用コマンドを返す。
    pub fn apply_preset(preset_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("workspace.apply_preset");
        descriptor
            .payload
            .insert("preset_id".to_string(), json!(preset_id.into()));
        descriptor
    }
}

/// ツール操作コマンド群。
pub mod tool {
    use super::{RgbColor, Tool};
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// アクティブツール変更コマンドを返す。
    pub fn set_active(tool: Tool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_active");
        descriptor
            .payload
            .insert("tool".to_string(), json!(tool.as_str()));
        descriptor
    }

    /// 16 進カラー文字列の設定コマンドを返す。
    pub fn set_color_hex(color: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_color");
        descriptor
            .payload
            .insert("color".to_string(), json!(color.into()));
        descriptor
    }

    /// RGB カラーの設定コマンドを返す。
    pub fn set_color_rgb(color: RgbColor) -> CommandDescriptor {
        set_color_hex(color.to_hex_string())
    }

    /// ブラシサイズ設定コマンドを返す。
    pub fn set_size(size: u32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_size");
        descriptor.payload.insert("size".to_string(), json!(size));
        descriptor
    }

    /// 筆圧有効状態設定コマンドを返す。
    pub fn set_pressure_enabled(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_pressure_enabled");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    /// アンチエイリアス有効状態設定コマンドを返す。
    pub fn set_antialias(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_antialias");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    /// 手ぶれ補正強さ設定コマンドを返す。
    pub fn set_stabilization(amount: u8) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_stabilization");
        descriptor
            .payload
            .insert("amount".to_string(), json!(amount.min(100)));
        descriptor
    }

    /// 次のペンを選択するコマンドを返す。
    pub fn select_next_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_next")
    }

    /// 前のペンを選択するコマンドを返す。
    pub fn select_previous_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_prev")
    }

    /// ペンプリセット再読込コマンドを返す。
    pub fn reload_pen_presets() -> CommandDescriptor {
        CommandDescriptor::new("tool.reload_pen_presets")
    }
}

/// ビュー操作コマンド群。
pub mod view {
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    /// ズーム変更コマンドを返す。
    pub fn zoom(zoom: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.zoom");
        descriptor.payload.insert("zoom".to_string(), json!(zoom));
        descriptor
    }

    /// パン移動コマンドを返す。
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

    /// キャンバス表示パン位置を絶対値で設定する。
    pub fn set_pan(pan_x: f32, pan_y: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.set_pan");
        descriptor.payload.insert("pan_x".to_string(), json!(pan_x));
        descriptor.payload.insert("pan_y".to_string(), json!(pan_y));
        descriptor
    }

    /// 90 度単位の回転コマンドを返す。
    pub fn rotate(quarter_turns: i32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.rotate");
        descriptor
            .payload
            .insert("quarter_turns".to_string(), json!(quarter_turns));
        descriptor
    }

    /// キャンバス表示回転角を度単位で設定する。
    pub fn set_rotation_degrees(rotation_degrees: f32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("view.set_rotation");
        descriptor
            .payload
            .insert("rotation_degrees".to_string(), json!(rotation_degrees));
        descriptor
    }

    /// 左右反転コマンドを返す。
    pub fn flip_horizontal() -> CommandDescriptor {
        CommandDescriptor::new("view.flip_horizontal")
    }

    /// 上下反転コマンドを返す。
    pub fn flip_vertical() -> CommandDescriptor {
        CommandDescriptor::new("view.flip_vertical")
    }

    /// ビューリセットコマンドを返す。
    pub fn reset() -> CommandDescriptor {
        CommandDescriptor::new("view.reset")
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
        /// SDK が使う文字列表現へ変換する。
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Normal => "normal",
                Self::Multiply => "multiply",
                Self::Screen => "screen",
                Self::Add => "add",
            }
        }
    }

    /// レイヤー追加コマンドを返す。
    pub fn add() -> CommandDescriptor {
        CommandDescriptor::new("layer.add")
    }

    /// レイヤー削除コマンドを返す。
    pub fn remove() -> CommandDescriptor {
        CommandDescriptor::new("layer.remove")
    }

    /// レイヤー選択コマンドを返す。
    pub fn select(index: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.select");
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    /// アクティブレイヤー名変更コマンドを返す。
    pub fn rename_active(name: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.rename_active");
        descriptor
            .payload
            .insert("name".to_string(), json!(name.into()));
        descriptor
    }

    /// レイヤー移動コマンドを返す。
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

    /// 次レイヤー選択コマンドを返す。
    pub fn select_next() -> CommandDescriptor {
        CommandDescriptor::new("layer.select_next")
    }

    /// ブレンドモード循環コマンドを返す。
    pub fn cycle_blend_mode() -> CommandDescriptor {
        CommandDescriptor::new("layer.cycle_blend_mode")
    }

    /// ブレンドモード文字列設定コマンドを返す。
    pub fn set_blend_mode(mode: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.set_blend_mode");
        descriptor
            .payload
            .insert("mode".to_string(), json!(mode.into()));
        descriptor
    }

    /// 型付きブレンドモード設定コマンドを返す。
    pub fn set_blend_mode_enum(mode: BlendMode) -> CommandDescriptor {
        set_blend_mode(mode.as_str())
    }

    /// 可視性トグルコマンドを返す。
    pub fn toggle_visibility() -> CommandDescriptor {
        CommandDescriptor::new("layer.toggle_visibility")
    }

    /// マスクトグルコマンドを返す。
    pub fn toggle_mask() -> CommandDescriptor {
        CommandDescriptor::new("layer.toggle_mask")
    }
}
