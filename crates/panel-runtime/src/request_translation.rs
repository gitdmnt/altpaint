//! Wasm が emit する `RequestDescriptor` を host 側の適用経路へ変換する。
//!
//! 変換結果は [`TranslatedRequest`] の 3 経路:
//! - `Document` — 純粋なドキュメント変異 (`DocumentCommand`)
//! - `Session` — エディタセッション変更 (`SessionCommand`)
//! - `Service` — I/O を伴うホスト副作用 (`ServiceRequest`)
//!
//! `tool.*` / `layer.*` の名前空間は command/session へ翻訳し、それ以外
//! (project_io / workspace / view / koma_nav / snapshot / export / ...) は
//! ServiceRequest としてそのまま搬送する。registry 化は BL-061 で実施する。

use app_core::{DocumentCommand, SessionCommand, ToolKind};
use panel_protocol::RequestDescriptor;
use panel_protocol::names::{layer, tool};
use serde_json::Value;

use crate::ServiceRequest;

/// `RequestDescriptor` の翻訳結果。host 側の 3 経路のいずれかへ振り分ける。
#[derive(Debug, Clone, PartialEq)]
pub enum TranslatedRequest {
    /// 純粋なドキュメント変異。
    Document(DocumentCommand),
    /// エディタセッション (ツール/色/ペン/ビュー) 変更。
    Session(SessionCommand),
    /// I/O を伴うホストサービス要求。
    Service(ServiceRequest),
}

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

fn service_from_descriptor(descriptor: &RequestDescriptor) -> ServiceRequest {
    let mut request = ServiceRequest::new(descriptor.name.clone());
    for (key, value) in &descriptor.payload {
        request = request.with_value(key.clone(), value.clone());
    }
    request
}

/// `RequestDescriptor` を host 側の適用経路へ翻訳する。
///
/// `tool.*` / `layer.*` の既知コマンド名は `DocumentCommand` / `SessionCommand`
/// へ翻訳する。それ以外の名前は I/O サービス要求として `ServiceRequest` に包む。
/// payload 欠落など翻訳に失敗した場合は `Err` を返す (黙殺禁止: 呼び出し側が
/// diagnostics へ流す)。
pub fn translate_descriptor(
    descriptor: &RequestDescriptor,
) -> Result<TranslatedRequest, String> {
    match descriptor.name.as_str() {
        tool::SET_ACTIVE => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool", tool::SET_ACTIVE))?;
            let tool = ToolKind::from_wire(tool)
                .ok_or_else(|| format!("unsupported tool kind: {tool}"))?;
            Ok(TranslatedRequest::Session(SessionCommand::SetActiveTool {
                tool,
            }))
        }
        tool::SELECT => {
            let tool_id = descriptor
                .payload
                .get("tool_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool_id", tool::SELECT))?;
            Ok(TranslatedRequest::Session(SessionCommand::SelectTool {
                tool_id: tool_id.to_string(),
            }))
        }
        tool::SELECT_CHILD => {
            let child_id = descriptor
                .payload
                .get("child_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.child_id", tool::SELECT_CHILD))?;
            Ok(TranslatedRequest::Session(SessionCommand::SelectChildTool {
                child_id: child_id.to_string(),
            }))
        }
        tool::SET_SIZE => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.size", tool::SET_SIZE))?;
            Ok(TranslatedRequest::Session(SessionCommand::SetActivePenSize {
                size: size as u32,
            }))
        }
        tool::SET_PRESSURE_ENABLED => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    format!("{} is missing payload.enabled", tool::SET_PRESSURE_ENABLED)
                })?;
            Ok(TranslatedRequest::Session(
                SessionCommand::SetActivePenPressureEnabled { enabled },
            ))
        }
        tool::SET_ANTIALIAS => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("{} is missing payload.enabled", tool::SET_ANTIALIAS))?;
            Ok(TranslatedRequest::Session(
                SessionCommand::SetActivePenAntialias { enabled },
            ))
        }
        tool::SET_STABILIZATION => {
            let amount = descriptor
                .payload
                .get("amount")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.amount", tool::SET_STABILIZATION))?;
            Ok(TranslatedRequest::Session(
                SessionCommand::SetActivePenStabilization {
                    amount: amount.min(100) as u8,
                },
            ))
        }
        tool::PEN_NEXT => Ok(TranslatedRequest::Session(
            SessionCommand::SelectNextPenPreset,
        )),
        tool::PEN_PREV => Ok(TranslatedRequest::Session(
            SessionCommand::SelectPreviousPenPreset,
        )),
        tool::SET_COLOR => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.color", tool::SET_COLOR))?;
            parse_hex_color(color)
                .map(|color| TranslatedRequest::Session(SessionCommand::SetActiveColor { color }))
                .ok_or_else(|| format!("invalid color payload: {color}"))
        }
        layer::ADD => Ok(TranslatedRequest::Document(DocumentCommand::AddRasterLayer)),
        layer::REMOVE => Ok(TranslatedRequest::Document(
            DocumentCommand::RemoveActiveLayer,
        )),
        layer::SELECT => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.index", layer::SELECT))?;
            Ok(TranslatedRequest::Document(DocumentCommand::SelectLayer {
                index: index as usize,
            }))
        }
        layer::RENAME_ACTIVE => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.name", layer::RENAME_ACTIVE))?;
            Ok(TranslatedRequest::Document(
                DocumentCommand::RenameActiveLayer {
                    name: name.to_string(),
                },
            ))
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
            Ok(TranslatedRequest::Document(DocumentCommand::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            }))
        }
        layer::SELECT_NEXT => Ok(TranslatedRequest::Document(
            DocumentCommand::SelectNextLayer,
        )),
        layer::CYCLE_BLEND_MODE => Ok(TranslatedRequest::Document(
            DocumentCommand::CycleActiveLayerBlendMode,
        )),
        layer::SET_BLEND_MODE => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.mode", layer::SET_BLEND_MODE))?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            Ok(TranslatedRequest::Document(
                DocumentCommand::SetActiveLayerBlendMode { mode },
            ))
        }
        layer::TOGGLE_VISIBILITY => Ok(TranslatedRequest::Document(
            DocumentCommand::ToggleActiveLayerVisibility,
        )),
        // ペンプリセットの reload/import は I/O。`tool.*` の旧コマンド wire 名は
        // 対応する `tool_catalog.*` サービスへ振り替える (挙動不変)。
        tool::RELOAD_PEN_PRESETS => Ok(TranslatedRequest::Service(ServiceRequest::new(
            tool::CATALOG_RELOAD_PEN_PRESETS,
        ))),
        tool::IMPORT_PEN_PRESETS => Ok(TranslatedRequest::Service(ServiceRequest::new(
            tool::CATALOG_IMPORT_PEN_PRESETS,
        ))),
        tool::IMPORT_PEN_PATH => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.path", tool::IMPORT_PEN_PATH))?;
            Ok(TranslatedRequest::Service(
                ServiceRequest::new(tool::CATALOG_IMPORT_PEN_PATH).with_value("path", path),
            ))
        }
        // それ以外の全名前空間 (project_io / workspace / view / koma_nav /
        // snapshot / export / tool_catalog / ...) はホストサービス要求として
        // そのまま搬送する。
        _ => Ok(TranslatedRequest::Service(service_from_descriptor(
            descriptor,
        ))),
    }
}

fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}
