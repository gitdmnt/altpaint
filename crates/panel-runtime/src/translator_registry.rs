//! 名前空間 prefix 単位の `RequestDescriptor` → [`TranslatedRequest`] 変換器レジストリ。
//!
//! BL-061: `translate_descriptor` の巨大 match を解体し、各 desktop feature が
//! 自分の名前空間 (`"tool."` / `"layer."` / `"view_service."` / `"koma_nav."` /
//! `"project_io."` / `"workspace_io."` / `"snapshot."` / `"export."` /
//! `"text_render."` / `"history."` / `"workspace_layout."` / `"tool_catalog."`)
//! の変換器を登録する仕組みにする。
//!
//! 巨大 match は廃止し、未登録 prefix/name は黙殺せず [`TranslationDiagnostic`] として
//! 呼び出し側へ返す (呼び出し側が log へ流す)。
//!
//! B4 時点では registry の物理的所属は panel-runtime に置き、登録は一箇所
//! (`register_default_translators`) で行う。B7 で各 feature クレートへ分散する。

use panel_protocol::RequestDescriptor;

use crate::request_translation::TranslatedRequest;

/// 単一名前空間の `RequestDescriptor` → [`TranslatedRequest`] 変換クロージャ。
///
/// `Ok(Some(_))`  : この名前を処理した (翻訳成功)。
/// `Ok(None)`     : この名前は名前空間内に存在しない (未登録 name)。
/// `Err(message)` : payload 欠落など翻訳に失敗した (黙殺禁止)。
pub type TranslatorFn =
    Box<dyn Fn(&RequestDescriptor) -> Result<Option<TranslatedRequest>, String> + Send + Sync>;

/// 翻訳に失敗した、または登録された変換器が存在しなかった理由。
///
/// 黙殺禁止 (BL-061): 呼び出し側はこれを必ず診断ログへ流す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationDiagnostic {
    /// `descriptor.name` の prefix に対応する変換器が登録されていない。
    UnregisteredNamespace { name: String },
    /// prefix の変換器は存在したが、その名前空間内に当該 name が無い。
    UnregisteredName { name: String },
    /// 変換器が翻訳に失敗した (payload 欠落・不正値など)。
    TranslationFailed { name: String, reason: String },
}

impl std::fmt::Display for TranslationDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnregisteredNamespace { name } => {
                write!(f, "no translator registered for namespace of `{name}`")
            }
            Self::UnregisteredName { name } => {
                write!(f, "unregistered request name `{name}`")
            }
            Self::TranslationFailed { name, reason } => {
                write!(f, "request translation failed for `{name}`: {reason}")
            }
        }
    }
}

/// 名前空間 prefix → 変換器のレジストリ。
///
/// `descriptor.name` の先頭 prefix にマッチした変換器へ振り分ける。
#[derive(Default)]
pub struct TranslatorRegistry {
    /// `(prefix, translator)` の登録順リスト。最長一致ではなく登録順で先頭一致。
    /// 名前空間 prefix は互いに排他 (例: `tool.` と `tool_catalog.`) なので順序は結果に影響しない。
    entries: Vec<(String, TranslatorFn)>,
}

impl TranslatorRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 名前空間 prefix の変換器を登録する。
    ///
    /// `prefix` は `"tool."` のように `.` を含む完全な名前空間とする。
    pub fn register(&mut self, prefix: impl Into<String>, translator: TranslatorFn) {
        self.entries.push((prefix.into(), translator));
    }

    /// 登録済み名前空間 prefix の一覧を返す (起動時 assert / 診断用)。
    pub fn registered_prefixes(&self) -> Vec<&str> {
        self.entries.iter().map(|(p, _)| p.as_str()).collect()
    }

    /// `RequestDescriptor` を翻訳する。
    ///
    /// prefix にマッチする変換器へ振り分け、結果を返す。マッチする変換器が無い、
    /// 名前空間内に name が無い、または翻訳に失敗した場合は黙殺せず
    /// [`TranslationDiagnostic`] を返す。
    pub fn translate(
        &self,
        descriptor: &RequestDescriptor,
    ) -> Result<TranslatedRequest, TranslationDiagnostic> {
        let name = descriptor.name.as_str();
        let Some((_, translator)) = self
            .entries
            .iter()
            .find(|(prefix, _)| name.starts_with(prefix.as_str()))
        else {
            return Err(TranslationDiagnostic::UnregisteredNamespace {
                name: name.to_string(),
            });
        };
        match translator(descriptor) {
            Ok(Some(translated)) => Ok(translated),
            Ok(None) => Err(TranslationDiagnostic::UnregisteredName {
                name: name.to_string(),
            }),
            Err(reason) => Err(TranslationDiagnostic::TranslationFailed {
                name: name.to_string(),
                reason,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_translation::register_default_translators;
    use document_model::DocumentCommand;
    use editor_state::SessionCommand;
    use panel_protocol::names::{koma_nav, layer, tool};
    use serde_json::json;

    fn descriptor(name: &str) -> RequestDescriptor {
        RequestDescriptor::new(name)
    }

    fn descriptor_with(name: &str, key: &str, value: serde_json::Value) -> RequestDescriptor {
        let mut d = RequestDescriptor::new(name);
        d.payload.insert(key.to_string(), value);
        d
    }

    fn registry() -> TranslatorRegistry {
        let mut registry = TranslatorRegistry::new();
        register_default_translators(&mut registry);
        registry
    }

    #[test]
    fn translates_tool_namespace_to_session_command() {
        let registry = registry();
        let translated = registry
            .translate(&descriptor_with(tool::SET_ACTIVE, "tool", json!("pen")))
            .expect("tool.set_active translates");
        assert!(matches!(
            translated,
            TranslatedRequest::Session(SessionCommand::SetActiveTool { .. })
        ));
    }

    #[test]
    fn translates_layer_namespace_to_document_command() {
        let registry = registry();
        let translated = registry
            .translate(&descriptor(layer::ADD))
            .expect("layer.add translates");
        assert_eq!(
            translated,
            TranslatedRequest::Document(DocumentCommand::AddRasterLayer)
        );
    }

    #[test]
    fn passes_through_service_namespace_verbatim() {
        let registry = registry();
        let translated = registry
            .translate(&descriptor_with(koma_nav::SELECT, "index", json!(2)))
            .expect("koma_nav.select passes through as service");
        match translated {
            TranslatedRequest::Service(request) => {
                assert_eq!(request.name, koma_nav::SELECT);
                assert_eq!(request.payload.get("index"), Some(&json!(2)));
            }
            other => panic!("expected service request, got {other:?}"),
        }
    }

    #[test]
    fn unregistered_namespace_yields_diagnostic_not_silent_swallow() {
        let registry = registry();
        let result = registry.translate(&descriptor("bogus_namespace.do_thing"));
        assert_eq!(
            result,
            Err(TranslationDiagnostic::UnregisteredNamespace {
                name: "bogus_namespace.do_thing".to_string(),
            })
        );
    }

    #[test]
    fn unregistered_name_in_known_namespace_yields_diagnostic() {
        let registry = registry();
        let result = registry.translate(&descriptor("tool.no_such_operation"));
        assert_eq!(
            result,
            Err(TranslationDiagnostic::UnregisteredName {
                name: "tool.no_such_operation".to_string(),
            })
        );
    }

    #[test]
    fn translation_failure_yields_diagnostic_with_reason() {
        let registry = registry();
        // tool.set_active without payload.tool は翻訳失敗。
        let result = registry.translate(&descriptor(tool::SET_ACTIVE));
        match result {
            Err(TranslationDiagnostic::TranslationFailed { name, .. }) => {
                assert_eq!(name, tool::SET_ACTIVE);
            }
            other => panic!("expected translation failure diagnostic, got {other:?}"),
        }
    }

    #[test]
    fn tool_namespace_does_not_capture_tool_catalog_names() {
        // `tool.` prefix が `tool_catalog.` を誤って捕捉しないこと。
        let registry = registry();
        let translated = registry
            .translate(&descriptor(tool::CATALOG_RELOAD_TOOLS))
            .expect("tool_catalog.reload_tools passes through");
        match translated {
            TranslatedRequest::Service(request) => {
                assert_eq!(request.name, tool::CATALOG_RELOAD_TOOLS);
            }
            other => panic!("expected service request, got {other:?}"),
        }
    }
}
