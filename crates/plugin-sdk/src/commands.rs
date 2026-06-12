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
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.red, self.green, self.blue)
    }
}

/// ツール操作コマンド群。
pub mod tool {
    use super::{RgbColor, Tool};
    use panel_schema::CommandDescriptor;
    use serde_json::json;

    pub fn set_active(tool: Tool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_active");
        descriptor
            .payload
            .insert("tool".to_string(), json!(tool.as_str()));
        descriptor
    }

    pub fn select_tool(tool_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.select");
        descriptor
            .payload
            .insert("tool_id".to_string(), json!(tool_id.into()));
        descriptor
    }

    pub fn set_color_hex(color: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_color");
        descriptor
            .payload
            .insert("color".to_string(), json!(color.into()));
        descriptor
    }

    pub fn set_color_rgb(color: RgbColor) -> CommandDescriptor {
        set_color_hex(color.to_hex_string())
    }

    pub fn set_size(size: u32) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_size");
        descriptor.payload.insert("size".to_string(), json!(size));
        descriptor
    }

    pub fn set_pressure_enabled(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_pressure_enabled");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    pub fn set_antialias(enabled: bool) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_antialias");
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    pub fn set_stabilization(amount: u8) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.set_stabilization");
        descriptor
            .payload
            .insert("amount".to_string(), json!(amount.min(100)));
        descriptor
    }

    pub fn select_next_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_next")
    }

    pub fn select_previous_pen() -> CommandDescriptor {
        CommandDescriptor::new("tool.pen_prev")
    }

    pub fn reload_pen_presets() -> CommandDescriptor {
        CommandDescriptor::new("tool.reload_pen_presets")
    }

    pub fn import_pen_presets() -> CommandDescriptor {
        CommandDescriptor::new("tool.import_pen_presets")
    }

    pub fn select_child_tool(child_id: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.select_child");
        descriptor
            .payload
            .insert("child_id".to_string(), json!(child_id.into()));
        descriptor
    }

    pub fn import_pen_path(path: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("tool.import_pen_path");
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
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
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Normal => "normal",
                Self::Multiply => "multiply",
                Self::Screen => "screen",
                Self::Add => "add",
            }
        }
    }

    pub fn add() -> CommandDescriptor {
        CommandDescriptor::new("layer.add")
    }

    pub fn remove() -> CommandDescriptor {
        CommandDescriptor::new("layer.remove")
    }

    pub fn select(index: usize) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.select");
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    pub fn rename_active(name: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.rename_active");
        descriptor
            .payload
            .insert("name".to_string(), json!(name.into()));
        descriptor
    }

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

    pub fn select_next() -> CommandDescriptor {
        CommandDescriptor::new("layer.select_next")
    }

    pub fn cycle_blend_mode() -> CommandDescriptor {
        CommandDescriptor::new("layer.cycle_blend_mode")
    }

    pub fn set_blend_mode(mode: impl Into<String>) -> CommandDescriptor {
        let mut descriptor = CommandDescriptor::new("layer.set_blend_mode");
        descriptor
            .payload
            .insert("mode".to_string(), json!(mode.into()));
        descriptor
    }

    pub fn set_blend_mode_enum(mode: BlendMode) -> CommandDescriptor {
        set_blend_mode(mode.as_str())
    }

    pub fn toggle_visibility() -> CommandDescriptor {
        CommandDescriptor::new("layer.toggle_visibility")
    }
}
