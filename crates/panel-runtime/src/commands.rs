//! Wasm が emit する `CommandDescriptor` を `Command` enum に変換する。
//!
//! Phase 10 では BuiltinPanelPlugin がこのマッピングを使い、Wasm の戻り値を
//! HostAction::DispatchCommand(Command::*) に翻訳する。

use app_core::{Command, ToolKind};
use panel_schema::CommandDescriptor;
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

/// 現在の値を from 記述子 へ変換する。
///
/// 失敗時はエラーを返します。
pub fn command_from_descriptor(descriptor: &CommandDescriptor) -> Result<Command, String> {
    match descriptor.name.as_str() {
        "tool.set_active" => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_active is missing payload.tool".to_string())?;
            let tool = match tool {
                "pen" => ToolKind::Pen,
                "eraser" => ToolKind::Eraser,
                "bucket" => ToolKind::Bucket,
                "lasso_bucket" => ToolKind::LassoBucket,
                "panel_rect" => ToolKind::PanelRect,
                other => return Err(format!("unsupported tool kind: {other}")),
            };
            Ok(Command::SetActiveTool { tool })
        }
        "tool.select" => {
            let tool_id = descriptor
                .payload
                .get("tool_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.select is missing payload.tool_id".to_string())?;
            Ok(Command::SelectTool {
                tool_id: tool_id.to_string(),
            })
        }
        "tool.select_child" => {
            let child_id = descriptor
                .payload
                .get("child_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.select_child is missing payload.child_id".to_string())?;
            Ok(Command::SelectChildTool {
                child_id: child_id.to_string(),
            })
        }
        "tool.set_size" => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| "tool.set_size is missing payload.size".to_string())?;
            Ok(Command::SetActivePenSize { size: size as u32 })
        }
        "tool.set_pressure_enabled" => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    "tool.set_pressure_enabled is missing payload.enabled".to_string()
                })?;
            Ok(Command::SetActivePenPressureEnabled { enabled })
        }
        "tool.set_antialias" => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| "tool.set_antialias is missing payload.enabled".to_string())?;
            Ok(Command::SetActivePenAntialias { enabled })
        }
        "tool.set_stabilization" => {
            let amount = descriptor
                .payload
                .get("amount")
                .and_then(payload_u64)
                .ok_or_else(|| "tool.set_stabilization is missing payload.amount".to_string())?;
            Ok(Command::SetActivePenStabilization {
                amount: amount.min(100) as u8,
            })
        }
        "tool.pen_next" => Ok(Command::SelectNextPenPreset),
        "tool.pen_prev" => Ok(Command::SelectPreviousPenPreset),
        "tool.reload_pen_presets" => Ok(Command::ReloadPenPresets),
        "tool.import_pen_presets" => Ok(Command::ImportPenPresets),
        "tool.import_pen_path" => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.import_pen_path is missing payload.path".to_string())?;
            Ok(Command::ImportPenPresetsFromPath {
                path: path.to_string(),
            })
        }
        "tool.set_color" => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| "tool.set_color is missing payload.color".to_string())?;
            parse_hex_color(color)
                .map(|color| Command::SetActiveColor { color })
                .ok_or_else(|| format!("invalid color payload: {color}"))
        }
        "layer.add" => Ok(Command::AddRasterLayer),
        "layer.remove" => Ok(Command::RemoveActiveLayer),
        "layer.select" => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.select is missing payload.index".to_string())?;
            Ok(Command::SelectLayer {
                index: index as usize,
            })
        }
        "layer.rename_active" => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.rename_active is missing payload.name".to_string())?;
            Ok(Command::RenameActiveLayer {
                name: name.to_string(),
            })
        }
        "layer.move" => {
            let from_index = descriptor
                .payload
                .get("from_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.from_index".to_string())?;
            let to_index = descriptor
                .payload
                .get("to_index")
                .and_then(payload_u64)
                .ok_or_else(|| "layer.move is missing payload.to_index".to_string())?;
            Ok(Command::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            })
        }
        "layer.select_next" => Ok(Command::SelectNextLayer),
        "layer.cycle_blend_mode" => Ok(Command::CycleActiveLayerBlendMode),
        "layer.set_blend_mode" => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| "layer.set_blend_mode is missing payload.mode".to_string())?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(Command::SetActiveLayerBlendMode { mode })
        }
        "layer.toggle_visibility" => Ok(Command::ToggleActiveLayerVisibility),
        other => Err(format!("unsupported command descriptor: {other}")),
    }
}

/// 入力を解析して u64 に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

