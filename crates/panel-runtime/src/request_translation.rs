//! Wasm が emit する `RequestDescriptor` を host 側の適用経路へ変換する。
//!
//! 変換結果は [`TranslatedRequest`] の 3 経路:
//! - `Document` — 純粋なドキュメント変異 (`DocumentCommand`)
//! - `Session` — エディタセッション変更 (`SessionCommand`)
//! - `Service` — I/O を伴うホスト副作用 (`ServiceRequest`)
//!
//! BL-061: 巨大 match は廃止し、名前空間 prefix 単位の変換器を
//! [`TranslatorRegistry`] へ登録する。`tool.` / `layer.` は command/session へ
//! 翻訳し、それ以外 (project_io / workspace / view / koma_nav / snapshot /
//! export / ...) は ServiceRequest としてそのまま搬送する。
//! 未登録 prefix/name は黙殺せず診断として呼び出し側へ返す。
//!
//! [`TranslatorRegistry`]: crate::translator_registry::TranslatorRegistry

use app_core::{DocumentCommand, SessionCommand, ToolKind};
use panel_protocol::RequestDescriptor;
use panel_protocol::names::{
    export, history, koma_nav, layer, project_io, snapshot, text_render, tool, view, workspace,
    workspace_layout,
};
use serde_json::Value;

use crate::ServiceRequest;
use crate::translator_registry::TranslatorRegistry;

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

/// 変換器が「この名前は名前空間内に存在しない」ことを表す結果。
type TranslateOutcome = Result<Option<TranslatedRequest>, String>;

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

fn payload_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
}

/// `tool.*` 名前空間: ツール/色/ペン操作の SessionCommand と、ペン import/reload の
/// I/O サービス要求。
fn translate_tool(descriptor: &RequestDescriptor) -> TranslateOutcome {
    let session = |command| Ok(Some(TranslatedRequest::Session(command)));
    let translated = match descriptor.name.as_str() {
        tool::SET_ACTIVE => {
            let tool = descriptor
                .payload
                .get("tool")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool", tool::SET_ACTIVE))?;
            let tool = ToolKind::from_wire(tool)
                .ok_or_else(|| format!("unsupported tool kind: {tool}"))?;
            return session(SessionCommand::SetActiveTool { tool });
        }
        tool::SELECT => {
            let tool_id = descriptor
                .payload
                .get("tool_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.tool_id", tool::SELECT))?;
            return session(SessionCommand::SelectTool {
                tool_id: tool_id.to_string(),
            });
        }
        tool::SELECT_CHILD => {
            let child_id = descriptor
                .payload
                .get("child_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.child_id", tool::SELECT_CHILD))?;
            return session(SessionCommand::SelectChildTool {
                child_id: child_id.to_string(),
            });
        }
        tool::SET_SIZE => {
            let size = descriptor
                .payload
                .get("size")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.size", tool::SET_SIZE))?;
            return session(SessionCommand::SetActivePenSize { size: size as u32 });
        }
        tool::SET_PRESSURE_ENABLED => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    format!("{} is missing payload.enabled", tool::SET_PRESSURE_ENABLED)
                })?;
            return session(SessionCommand::SetActivePenPressureEnabled { enabled });
        }
        tool::SET_ANTIALIAS => {
            let enabled = descriptor
                .payload
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("{} is missing payload.enabled", tool::SET_ANTIALIAS))?;
            return session(SessionCommand::SetActivePenAntialias { enabled });
        }
        tool::SET_STABILIZATION => {
            let amount = descriptor
                .payload
                .get("amount")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.amount", tool::SET_STABILIZATION))?;
            return session(SessionCommand::SetActivePenStabilization {
                amount: amount.min(100) as u8,
            });
        }
        tool::PEN_NEXT => return session(SessionCommand::SelectNextPenPreset),
        tool::PEN_PREV => return session(SessionCommand::SelectPreviousPenPreset),
        tool::SET_COLOR => {
            let color = descriptor
                .payload
                .get("color")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.color", tool::SET_COLOR))?;
            let color =
                parse_hex_color(color).ok_or_else(|| format!("invalid color payload: {color}"))?;
            return session(SessionCommand::SetActiveColor { color });
        }
        // ペンプリセットの reload/import は I/O。`tool.*` の旧コマンド wire 名は
        // 対応する `tool_catalog.*` サービスへ振り替える (挙動不変)。
        tool::RELOAD_PEN_PRESETS => {
            TranslatedRequest::Service(ServiceRequest::new(tool::CATALOG_RELOAD_PEN_PRESETS))
        }
        tool::IMPORT_PEN_PRESETS => {
            TranslatedRequest::Service(ServiceRequest::new(tool::CATALOG_IMPORT_PEN_PRESETS))
        }
        tool::IMPORT_PEN_PATH => {
            let path = descriptor
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.path", tool::IMPORT_PEN_PATH))?;
            TranslatedRequest::Service(
                ServiceRequest::new(tool::CATALOG_IMPORT_PEN_PATH).with_value("path", path),
            )
        }
        _ => return Ok(None),
    };
    Ok(Some(translated))
}

/// `layer.*` 名前空間: レイヤー操作の DocumentCommand。
fn translate_layer(descriptor: &RequestDescriptor) -> TranslateOutcome {
    let document = |command| Ok(Some(TranslatedRequest::Document(command)));
    match descriptor.name.as_str() {
        layer::ADD => document(DocumentCommand::AddRasterLayer),
        layer::REMOVE => document(DocumentCommand::RemoveActiveLayer),
        layer::SELECT => {
            let index = descriptor
                .payload
                .get("index")
                .and_then(payload_u64)
                .ok_or_else(|| format!("{} is missing payload.index", layer::SELECT))?;
            document(DocumentCommand::SelectLayer {
                index: index as usize,
            })
        }
        layer::RENAME_ACTIVE => {
            let name = descriptor
                .payload
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.name", layer::RENAME_ACTIVE))?;
            document(DocumentCommand::RenameActiveLayer {
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
            document(DocumentCommand::MoveLayer {
                from_index: from_index as usize,
                to_index: to_index as usize,
            })
        }
        layer::SELECT_NEXT => document(DocumentCommand::SelectNextLayer),
        layer::CYCLE_BLEND_MODE => document(DocumentCommand::CycleActiveLayerBlendMode),
        layer::SET_BLEND_MODE => {
            let mode = descriptor
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{} is missing payload.mode", layer::SET_BLEND_MODE))?;
            let mode = app_core::BlendMode::parse_name(mode)
                .ok_or_else(|| format!("unsupported layer blend mode: {mode}"))?;
            document(DocumentCommand::SetActiveLayerBlendMode { mode })
        }
        layer::TOGGLE_VISIBILITY => document(DocumentCommand::ToggleActiveLayerVisibility),
        _ => Ok(None),
    }
}

/// I/O サービス名前空間の pass-through 変換器を生成する。
///
/// その名前空間内の name 一覧 `known_names` を持ち、登録された名前のみを
/// `ServiceRequest` としてそのまま搬送する。未知の name は `Ok(None)` を返し、
/// registry が `UnregisteredName` 診断へ昇格させる (黙殺禁止)。
fn passthrough(known_names: &'static [&'static str]) -> crate::translator_registry::TranslatorFn {
    Box::new(move |descriptor: &RequestDescriptor| {
        if known_names.contains(&descriptor.name.as_str()) {
            Ok(Some(TranslatedRequest::Service(service_from_descriptor(
                descriptor,
            ))))
        } else {
            Ok(None)
        }
    })
}

/// B4 時点の既定の変換器を registry へ登録する。
///
/// registry の物理的所属は B7 で各 feature クレートへ分散する予定だが、B4 では
/// 一箇所 (本関数) で全名前空間を登録する。挙動は分割前の巨大 match と等価。
pub fn register_default_translators(registry: &mut TranslatorRegistry) {
    // command/session へ翻訳する名前空間。
    registry.register("tool.", Box::new(translate_tool));
    registry.register("layer.", Box::new(translate_layer));

    // I/O サービスとしてそのまま搬送する名前空間 (desktop feature が実処理を持つ)。
    registry.register(
        "project_io.",
        passthrough(&[
            project_io::NEW_DOCUMENT,
            project_io::NEW_DOCUMENT_SIZED,
            project_io::SAVE_CURRENT,
            project_io::SAVE_AS,
            project_io::SAVE_TO_PATH,
            project_io::LOAD_DIALOG,
            project_io::LOAD_FROM_PATH,
        ]),
    );
    registry.register(
        "workspace_io.",
        passthrough(&[
            workspace::RELOAD_PRESETS,
            workspace::APPLY_PRESET,
            workspace::SAVE_PRESET,
            workspace::EXPORT_PRESET,
            workspace::EXPORT_PRESET_TO_PATH,
        ]),
    );
    registry.register(
        "workspace_layout.",
        passthrough(&[
            workspace_layout::SET_PANEL_VISIBILITY,
            workspace_layout::MOVE_PANEL,
        ]),
    );
    registry.register(
        "tool_catalog.",
        passthrough(&[
            tool::CATALOG_RELOAD_TOOLS,
            tool::CATALOG_RELOAD_PEN_PRESETS,
            tool::CATALOG_IMPORT_PEN_PRESETS,
            tool::CATALOG_IMPORT_PEN_PATH,
        ]),
    );
    registry.register(
        "view_service.",
        passthrough(&[
            view::SET_ZOOM,
            view::SET_PAN,
            view::SET_ROTATION,
            view::FLIP_HORIZONTAL,
            view::FLIP_VERTICAL,
            view::RESET,
        ]),
    );
    registry.register(
        "koma_nav.",
        passthrough(&[
            koma_nav::ADD,
            koma_nav::REMOVE,
            koma_nav::SELECT,
            koma_nav::SELECT_NEXT,
            koma_nav::SELECT_PREVIOUS,
            koma_nav::FOCUS_ACTIVE,
        ]),
    );
    registry.register("history.", passthrough(&[history::UNDO, history::REDO]));
    registry.register(
        "snapshot.",
        passthrough(&[snapshot::CREATE, snapshot::RESTORE]),
    );
    registry.register("export.", passthrough(&[export::IMAGE]));
    registry.register("text_render.", passthrough(&[text_render::RENDER_TO_LAYER]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator_registry::{TranslationDiagnostic, TranslatorRegistry};
    use serde_json::json;

    fn registry() -> TranslatorRegistry {
        let mut registry = TranslatorRegistry::new();
        register_default_translators(&mut registry);
        registry
    }

    fn descriptor_with(name: &str, key: &str, value: Value) -> RequestDescriptor {
        let mut d = RequestDescriptor::new(name);
        d.payload.insert(key.to_string(), value);
        d
    }

    /// `tool.set_color` の hex 解釈が SessionCommand へ翻訳される。
    #[test]
    fn tool_set_color_parses_hex() {
        let registry = registry();
        let translated = registry
            .translate(&descriptor_with(tool::SET_COLOR, "color", json!("#112233")))
            .expect("tool.set_color translates");
        match translated {
            TranslatedRequest::Session(SessionCommand::SetActiveColor { color }) => {
                assert_eq!(color, app_core::ColorRgba8::new(0x11, 0x22, 0x33, 0xff));
            }
            other => panic!("expected SetActiveColor, got {other:?}"),
        }
    }

    /// `tool.reload_pen_presets` は I/O サービス `tool_catalog.reload_pen_presets` へ振り替わる。
    #[test]
    fn tool_reload_pen_presets_is_remapped_to_catalog_service() {
        let registry = registry();
        let translated = registry
            .translate(&RequestDescriptor::new(tool::RELOAD_PEN_PRESETS))
            .expect("tool.reload_pen_presets translates");
        match translated {
            TranslatedRequest::Service(request) => {
                assert_eq!(request.name, tool::CATALOG_RELOAD_PEN_PRESETS);
            }
            other => panic!("expected catalog service request, got {other:?}"),
        }
    }

    /// 既知の service 名前空間 (view_service) は ServiceRequest へ素通しされる。
    #[test]
    fn view_service_passes_through() {
        let registry = registry();
        let translated = registry
            .translate(&descriptor_with(view::SET_ZOOM, "zoom", json!(2.0)))
            .expect("view_service.set_zoom passes through");
        match translated {
            TranslatedRequest::Service(request) => {
                assert_eq!(request.name, view::SET_ZOOM);
                assert_eq!(request.payload.get("zoom"), Some(&json!(2.0)));
            }
            other => panic!("expected service request, got {other:?}"),
        }
    }

    /// 既知 service 名前空間でも未登録 name は黙殺せず診断を返す。
    #[test]
    fn unregistered_service_name_yields_diagnostic() {
        let registry = registry();
        let result = registry.translate(&RequestDescriptor::new("view_service.unknown_op"));
        assert_eq!(
            result,
            Err(TranslationDiagnostic::UnregisteredName {
                name: "view_service.unknown_op".to_string(),
            })
        );
    }

    /// 全 wire 名定数が registry に登録済みであること (silent no-op 防止)。
    ///
    /// payload 欠落による `TranslationFailed` は許容するが、`UnregisteredNamespace` /
    /// `UnregisteredName` は「登録漏れ」を意味するため許容しない。
    #[test]
    fn all_wire_names_are_registered() {
        let registry = registry();
        let all_names: &[&str] = &[
            project_io::NEW_DOCUMENT,
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
        for name in all_names {
            match registry.translate(&RequestDescriptor::new(*name)) {
                Ok(_) | Err(TranslationDiagnostic::TranslationFailed { .. }) => {}
                Err(other) => panic!("wire name `{name}` is not registered: {other:?}"),
            }
        }
    }
}
