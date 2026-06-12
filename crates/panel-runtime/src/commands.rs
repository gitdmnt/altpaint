//! Wasm が emit する `CommandDescriptor` を `Command` enum に変換する。
//!
//! Phase 10 では BuiltinPanelPlugin がこのマッピングを使い、Wasm の戻り値を
//! HostAction::DispatchCommand(Command::*) に翻訳する。

use app_core::{Command, ToolKind};
use panel_protocol::CommandDescriptor;
use panel_protocol::names::{layer, tool};
use serde_json::Value;

fn parse_hex_color(input: &str) -> Option<app_core::ColorRgba8> {
    let hex = input.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(app_core::ColorRgba8::new(r, g, b, 0xff))
}

pub fn command_from_descriptor(descriptor: &CommandDescriptor) -> Result<Command, String> {
    match descriptor.name.as_str() {
        tool::SET_ACTIVE => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool", tool::SET_ACTIVE))?;
            let tool = match tool {
                "pen" => ToolKind::Pen,
                "eraser" => ToolKind::Eraser,
                "bucket" => ToolKind::Bucket,
                "lasso_bucket" => ToolKind::LassoBucket,
                "koma_rect" => ToolKind::KomaRect,
                other => return Err(format!("unsupported tool kind: {other}")),
            };
            Ok(Command::SetActiveTool { tool })
        }
        tool::SELECT => {
            let tool_id = descriptor
                .payload
                .get("tool_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool_id", tool::SELECT))?;
            Ok(Command::SelectTool {
                tool_id: tool_id.to_string(),
            })
        }
        tool::SELECT_CHILD => {
            let child_id = descriptor
                .payload
                .get("child_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.child_id", tool::SELECT_CHILD))?;
            Ok(Command::SelectChildTool {
                child_id: child_id.to_string(),
            })
        }
        tool::SET_SIZE => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.size", tool::SET_SIZE))?;
            Ok(Command::SetActivePenSize { size: size as u32 })
        }
        tool::SET_PRESSURE_ENABLED => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    format!("{} is missing payload.enabled", tool::SET_PRESSURE_ENABLED)
                })?;
            Ok(Command::SetActivePenPressureEnabled { enabled })
        }
        tool::SET_ANTIALIAS => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("{} is missing payload.enabled", tool::SET_ANTIALIAS))?;
            Ok(Command::SetActivePenAntialias { enabled })
        }
        tool::SET_STABILIZATION => {
            let amount = descriptor
                .payload
                .get("amount")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.amount", tool::SET_STABILIZATION))?;
            Ok(Command::SetActivePenStabilization {
                amount: amount.min(100) as u8,
            })
        }
        tool::PEN_NEXT => Ok(Command::SelectNextPenPreset),
        tool::PEN_PREV => Ok(Command::SelectPreviousPenPreset),
        tool::RELOAD_PEN_PRESETS => Ok(Command::ReloadPenPresets),
        tool::IMPORT_PEN_PRESETS => Ok(Command::ImportPenPresets),
        tool::IMPORT_PEN_PATH => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.path", tool::IMPORT_PEN_PATH))?;
            Ok(Command::ImportPenPresetsFromPath {
                path: path.to_string(),
            })
        }
        tool::SET_COLOR => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.color", tool::SET_COLOR))?;
            parse_hex_color(color)
                .map(|color| Command::SetActiveColor { color })
                .ok_or_else(|| format!("invalid color payload: {color}"))
        }
        layer::ADD => Ok(Command::AddRasterLayer),
        layer::REMOVE => Ok(Command::RemoveActiveLayer),
        layer::SELECT => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.index", layer::SELECT))?;
            Ok(Command::SelectLayer {
                index: index as usize,
            })
        }
        layer::RENAME_ACTIVE => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.name", layer::RENAME_ACTIVE))?;
            Ok(Command::RenameActiveLayer {
                name: name.to_string(),
            })
        }
        layer::MOVE => {
            let from_index = descriptor
                .payload
                .get("from_index")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.from_index", layer::MOVE))?;
            let to_index = descriptor
                .payload
                .get("to_index")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.to_index", layer::MOVE))?;
            Ok(Command::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            })
        }
        layer::SELECT_NEXT => Ok(Command::SelectNextLayer),
        layer::CYCLE_BLEND_MODE => Ok(Command::CycleActiveLayerBlendMode),
        layer::SET_BLEND_MODE => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.mode", layer::SET_BLEND_MODE))?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(Command::SetActiveLayerBlendMode { mode })
        }
        layer::TOGGLE_VISIBILITY => Ok(Command::ToggleActiveLayerVisibility),
        other => Err(format!("unsupported command descriptor: {other}")),
    }
}

fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

