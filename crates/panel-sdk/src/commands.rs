//! 文字列キーを隠蔽する型付きコマンド生成 API を提供する。

/// ツール識別子を型として表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser,
    Bucket,
    LassoBucket,
    KomaRect,
}

impl Tool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pen => "pen",
            Self::Eraser => "eraser",
            Self::Bucket => "bucket",
            Self::LassoBucket => "lasso_bucket",
            Self::KomaRect => "koma_rect",
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
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::tool as wire;
    use serde_json::json;

    pub fn set_active(tool: Tool) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_ACTIVE);
        descriptor
            .payload
            .insert("tool".to_string(), json!(tool.as_str()));
        descriptor
    }

    pub fn select_tool(tool_id: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SELECT);
        descriptor
            .payload
            .insert("tool_id".to_string(), json!(tool_id.into()));
        descriptor
    }

    pub fn set_color_hex(color: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_COLOR);
        descriptor
            .payload
            .insert("color".to_string(), json!(color.into()));
        descriptor
    }

    pub fn set_color_rgb(color: RgbColor) -> RequestDescriptor {
        set_color_hex(color.to_hex_string())
    }

    pub fn set_size(size: u32) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_SIZE);
        descriptor.payload.insert("size".to_string(), json!(size));
        descriptor
    }

    pub fn set_pressure_enabled(enabled: bool) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_PRESSURE_ENABLED);
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    pub fn set_antialias(enabled: bool) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_ANTIALIAS);
        descriptor
            .payload
            .insert("enabled".to_string(), json!(enabled));
        descriptor
    }

    pub fn set_stabilization(amount: u8) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_STABILIZATION);
        descriptor
            .payload
            .insert("amount".to_string(), json!(amount.min(100)));
        descriptor
    }

    pub fn select_next_pen() -> RequestDescriptor {
        RequestDescriptor::new(wire::PEN_NEXT)
    }

    pub fn select_previous_pen() -> RequestDescriptor {
        RequestDescriptor::new(wire::PEN_PREV)
    }

    pub fn reload_pen_presets() -> RequestDescriptor {
        RequestDescriptor::new(wire::RELOAD_PEN_PRESETS)
    }

    pub fn import_pen_presets() -> RequestDescriptor {
        RequestDescriptor::new(wire::IMPORT_PEN_PRESETS)
    }

    pub fn select_child_tool(child_id: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SELECT_CHILD);
        descriptor
            .payload
            .insert("child_id".to_string(), json!(child_id.into()));
        descriptor
    }

    pub fn import_pen_path(path: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::IMPORT_PEN_PATH);
        descriptor
            .payload
            .insert("path".to_string(), json!(path.into()));
        descriptor
    }
}

/// レイヤー操作コマンド群。
pub mod layer {
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::layer as wire;
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

    pub fn add() -> RequestDescriptor {
        RequestDescriptor::new(wire::ADD)
    }

    pub fn remove() -> RequestDescriptor {
        RequestDescriptor::new(wire::REMOVE)
    }

    pub fn select(index: usize) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SELECT);
        descriptor.payload.insert("index".to_string(), json!(index));
        descriptor
    }

    pub fn rename_active(name: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::RENAME_ACTIVE);
        descriptor
            .payload
            .insert("name".to_string(), json!(name.into()));
        descriptor
    }

    pub fn move_to(from_index: usize, to_index: usize) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::MOVE);
        descriptor
            .payload
            .insert("from_index".to_string(), json!(from_index));
        descriptor
            .payload
            .insert("to_index".to_string(), json!(to_index));
        descriptor
    }

    pub fn select_next() -> RequestDescriptor {
        RequestDescriptor::new(wire::SELECT_NEXT)
    }

    pub fn cycle_blend_mode() -> RequestDescriptor {
        RequestDescriptor::new(wire::CYCLE_BLEND_MODE)
    }

    pub fn set_blend_mode(mode: impl Into<String>) -> RequestDescriptor {
        let mut descriptor = RequestDescriptor::new(wire::SET_BLEND_MODE);
        descriptor
            .payload
            .insert("mode".to_string(), json!(mode.into()));
        descriptor
    }

    pub fn set_blend_mode_enum(mode: BlendMode) -> RequestDescriptor {
        set_blend_mode(mode.as_str())
    }

    pub fn toggle_visibility() -> RequestDescriptor {
        RequestDescriptor::new(wire::TOGGLE_VISIBILITY)
    }
}
