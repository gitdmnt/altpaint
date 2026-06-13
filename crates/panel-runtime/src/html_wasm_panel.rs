//! `HtmlWasmPanel` — Phase 10 同梱パネルの統一実装型。
//!
//! 構成要素:
//! - `HtmlPanelView` (Blitz HTML/CSS + parley + vello)
//! - `PanelWasmInstance` (wasmtime, panel_init / panel_handle_* / panel_sync_host export を呼ぶ)
//! - 各 Wasm 呼出は `PanelWasmInstance::call_with_dom` で view の document を context にし、
//!   Wasm 内 DOM mutation host function (`set_attribute` / `set_inner_html` 等) で直接 DOM を書換える
//!
//! `update` (host state 同期) と `handle_event` (UI イベント) のいずれでも DOM mutation を
//! 行う可能性があるため、両経路で `call_with_dom` を必ず通すこと。

use std::path::Path;
use std::sync::Arc;

use document_model::DocumentCommand;
use crate::host_request::{HostRequest, PanelEvent};
use crate::services::ServiceRequest;
use panel_html::{
    ActionDescriptor, HtmlPanelView, blitz_dom::LocalName, blitz_dom::node::NodeData,
    parse_data_action,
};
use crate::request_translation::TranslatedRequest;
use crate::translator_registry::TranslatorRegistry;
use crate::host_state::HostStateBuild;
use crate::meta::{PanelLayoutMeta, PanelMeta, PanelPresetMeta};
use panel_wasm_host::{PanelWasmHostError, PanelWasmInstance};
use panel_protocol::{Diagnostic, DiagnosticLevel, HandlerEffects};
use serde_json::{Value, json};

pub struct HtmlWasmPanel {
    id: String,
    title: String,
    default_size: (u32, u32),
    view: HtmlPanelView,
    wasm: PanelWasmInstance,
    /// Wasm 側が保持する state (panel_init で初期化、handler 戻り値の patch を蓄積)。
    state: Value,
    /// 最新の host state (handler 内 host_get_* で利用される)。
    /// `update` が呼ばれるたびに最新値へ更新される (DOM 再 render の有無に関わらず)。
    last_host_state: Value,
    /// meta.json で宣言された購読 host state セクション (BL-093)。
    /// 空の場合は全セクション購読 (どれか変われば再 render)。
    subscribes: Vec<String>,
    /// meta.json で宣言された既定ワークスペース配置 (BL-095)。
    layout: PanelLayoutMeta,
    /// meta.json で宣言された default-floating プリセット配置 (BL-095)。
    preset: Option<PanelPresetMeta>,
    /// 初回 `update` を購読 delta に関わらず必ず render させるフラグ (BL-093)。
    ///
    /// section registry の revision キャッシュはパネル横断で共有されるため、
    /// 後から登録されたパネルでは「購読セクションに変化なし」と判定され得る。
    /// 初回だけは last_host_state が空 (`{}`) で DOM が未同期のため、必ず render する。
    needs_initial_render: bool,
    /// Wasm が `panel_handle_keyboard` を export しているか (load 時に確定)。
    /// `handles_keyboard_event` は `&self` のため、`PanelWasmInstance::has_handler`
    /// (`&mut self`) を毎回呼べずキャッシュする。
    has_keyboard_handler: bool,
    /// `RequestDescriptor` → host 経路の翻訳に使う registry (BL-061)。
    /// `PanelRuntime::register_panel` が共有 registry を注入する。注入前は
    /// 既定の変換器を登録した単独 registry を使う (テスト等の単体 load 経路)。
    translator_registry: Arc<TranslatorRegistry>,
}

#[derive(Debug, thiserror::Error)]
pub enum HtmlWasmPanelError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid panel.meta.json: {0}")]
    Meta(#[from] serde_json::Error),
    #[error("panel wasm host: {0}")]
    Host(#[from] PanelWasmHostError),
}

impl HtmlWasmPanel {
    /// パネルディレクトリを読み込み、HTML/CSS/Wasm を初期化する。
    ///
    /// ディレクトリ構成:
    /// - `panel.html`: 初期 DOM (必須)
    /// - `panel.css`: ユーザー CSS (任意)
    /// - `panel.meta.json`: `{ "id", "title", "default_size": { "width", "height" } }` (必須)
    /// - `<wasm_filename>`: コンパイル済み Wasm モジュール (必須)
    pub fn load(
        directory: &Path,
        wasm_filename: &str,
        restored_size: Option<(u32, u32)>,
    ) -> Result<Self, HtmlWasmPanelError> {
        let html = std::fs::read_to_string(directory.join("panel.html"))?;
        let css = directory.join("panel.css");
        let css = if css.exists() {
            std::fs::read_to_string(&css)?
        } else {
            String::new()
        };
        let meta_raw = std::fs::read_to_string(directory.join("panel.meta.json"))?;
        let meta: PanelMeta = serde_json::from_str(&meta_raw)?;
        let default_size = meta.default_size.as_tuple();

        let mut view = HtmlPanelView::new(&html, &css);
        view.set_panel_size(restored_size.unwrap_or(default_size));
        let mut wasm = PanelWasmInstance::load(directory.join(wasm_filename))?;

        // panel_init は DOM context 必須 (Wasm が初期 DOM を mutate する可能性)。
        let init = wasm.call_with_dom(view.document_mut(), |rt| rt.panel_init())?;
        view.mark_mutated();
        // BL-102: panel_init が積んだ診断を黙殺せず流す。
        emit_handler_diagnostics(&meta.id, "panel_init", &init);
        let has_keyboard_handler = wasm.has_handler("keyboard");

        // panel_init が返した state_patch を空 state に適用して初期 state を確定する。
        let mut state = json!({});
        panel_protocol::apply_patches(&mut state, &init.state_patch);

        Ok(Self {
            id: meta.id,
            title: meta.title,
            default_size,
            view,
            wasm,
            state,
            last_host_state: json!({}),
            subscribes: meta.subscribes,
            layout: meta.layout,
            preset: meta.preset,
            needs_initial_render: true,
            has_keyboard_handler,
            translator_registry: Arc::new(default_translator_registry()),
        })
    }

    /// `PanelRuntime` が保持する共有 translator registry を注入する (BL-061)。
    ///
    /// 登録は一箇所 (runtime 構築時) で行い、各パネルは同じ registry を共有する。
    pub(crate) fn set_translator_registry(&mut self, registry: Arc<TranslatorRegistry>) {
        self.translator_registry = registry;
    }

    #[cfg(test)]
    pub(crate) fn translator_registry_ptr(&self) -> *const TranslatorRegistry {
        Arc::as_ptr(&self.translator_registry)
    }

    /// meta.json で宣言された購読セクション (BL-093)。空 = 全セクション購読。
    pub fn subscribes(&self) -> &[String] {
        &self.subscribes
    }

    /// meta.json で宣言された既定ワークスペース配置 (BL-095)。
    pub fn layout(&self) -> &PanelLayoutMeta {
        &self.layout
    }

    /// meta.json で宣言された default-floating プリセット配置 (BL-095)。
    pub fn preset(&self) -> Option<&PanelPresetMeta> {
        self.preset.as_ref()
    }

    /// panel.meta.json の `default_size` を返す。
    /// 起動時に workspace に未記録のパネルへ初期サイズとして注入される。
    pub fn default_size(&self) -> (u32, u32) {
        self.default_size
    }

    pub fn view(&self) -> &HtmlPanelView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut HtmlPanelView {
        &mut self.view
    }

    /// Wasm が `data-action="command:..."` ボタンをクリックされた等のイベントを処理する。
    ///
    /// Wasm export 名は `panel_handle_<sanitized_handler_name>`。
    fn dispatch_to_wasm(
        &mut self,
        handler_name: &str,
        event_payload: Value,
    ) -> Result<Vec<HostRequest>, PanelWasmHostError> {
        if !self.wasm.has_handler(handler_name) {
            return Ok(Vec::new());
        }
        let input = panel_host_call_input(event_payload, &self.state, &self.last_host_state);
        let result = self
            .wasm
            .call_with_dom(self.view.document_mut(), |rt| {
                rt.handle_event(handler_name, &input)
            })?;
        self.view.mark_mutated();
        // BL-102: handler が積んだ診断を黙殺せず流す。
        emit_handler_diagnostics(&self.id, handler_name, &result);
        panel_protocol::apply_patches(&mut self.state, &result.state_patch);
        let registry = &self.translator_registry;
        Ok(result
            .commands
            .into_iter()
            .filter_map(|descriptor| request_descriptor_to_host_request(registry, descriptor))
            .collect())
    }
}

/// 既定の変換器を登録した単独 registry を構築する。
///
/// `PanelRuntime` 注入前の単体 load 経路 (テスト等) で使う。本番では runtime が
/// 構築した共有 registry が `set_translator_registry` で上書きする。
fn default_translator_registry() -> TranslatorRegistry {
    let mut registry = TranslatorRegistry::new();
    crate::request_translation::register_default_translators(&mut registry);
    registry
}

fn panel_host_call_input(
    event_payload: Value,
    state: &Value,
    host_state: &Value,
) -> panel_protocol::HostCallInput {
    panel_protocol::HostCallInput {
        event_payload,
        state: state.clone(),
        host_state: host_state.clone(),
    }
}

/// パネル (Wasm) が `diagnostic` host function で積んだ診断を 1 件 1 行へ整形する。
///
/// BL-102: パネル内エラーの黙殺を廃止するため、`HandlerEffects::diagnostics` を
/// この形式で診断ログへ流す。
fn format_panel_diagnostic(panel_id: &str, handler: &str, diagnostic: &Diagnostic) -> String {
    let level = match diagnostic.level {
        DiagnosticLevel::Info => "INFO",
        DiagnosticLevel::Warning => "WARN",
        DiagnosticLevel::Error => "ERROR",
    };
    format!(
        "panel diagnostic [{level}] {panel_id}::{handler}: {}",
        diagnostic.message
    )
}

/// `HandlerEffects::diagnostics` を診断ログ (stderr) へ流し、消費件数を返す (BL-102)。
///
/// 黙殺禁止: panel_init / handler / sync_host のいずれの戻り値の診断も
/// 必ずこの経路を通す。戻り値はテストで「診断が消費されたか」を検証するために使う。
fn emit_handler_diagnostics(
    panel_id: &str,
    handler: &str,
    effects: &HandlerEffects,
) -> usize {
    for diagnostic in &effects.diagnostics {
        eprintln!("{}", format_panel_diagnostic(panel_id, handler, diagnostic));
    }
    effects.diagnostics.len()
}

fn request_descriptor_to_host_request(
    registry: &TranslatorRegistry,
    descriptor: panel_protocol::RequestDescriptor,
) -> Option<HostRequest> {
    match registry.translate(&descriptor) {
        Ok(TranslatedRequest::Document(command)) => {
            Some(HostRequest::DispatchDocumentCommand(command))
        }
        Ok(TranslatedRequest::Session(command)) => {
            Some(HostRequest::DispatchSessionCommand(command))
        }
        Ok(TranslatedRequest::Service(request)) => Some(HostRequest::RequestService(request)),
        Err(diagnostic) => {
            // 黙殺禁止 (BL-061): 未登録名・翻訳失敗いずれも診断ログへ流す。
            eprintln!("{diagnostic}");
            None
        }
    }
}

/// パネルのライフサイクルメソッド (旧 `PanelPlugin` trait。P1 で trait を撤去し
/// `HtmlWasmPanel` の inherent メソッドに統合した)。
impl HtmlWasmPanel {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// 合成済み host state を受け取り、購読セクションが変化していれば再 render する。
    ///
    /// `last_host_state` は再 render の有無に関わらず常に最新値へ更新する
    /// (handler 内 `host_get_*` が最新データを読めるように)。DOM mutation
    /// (`sync_host`) は購読セクション (`subscribes`) のいずれかが今回変化した
    /// 時のみ実行する (BL-093: revision ベース購読)。
    ///
    /// 戻り値: DOM を再 render した場合 true。
    pub fn update(&mut self, host_state: &HostStateBuild) -> bool {
        self.last_host_state = host_state.value.clone();
        let force = self.needs_initial_render;
        self.needs_initial_render = false;
        if !force && !host_state.affects(&self.subscribes) {
            return false;
        }
        if !self.wasm.supports_sync_host() {
            return false;
        }
        let state = &self.state;
        let host_state_value = &self.last_host_state;
        let outcome = self
            .wasm
            .call_with_dom(self.view.document_mut(), |rt| {
                rt.sync_host(state, host_state_value)
            });
        if let Ok(result) = outcome {
            // BL-102: sync_host が積んだ診断を黙殺せず流す。
            emit_handler_diagnostics(&self.id, "sync_host", &result);
            panel_protocol::apply_patches(&mut self.state, &result.state_patch);
            self.view.mark_mutated();
            true
        } else {
            false
        }
    }

    pub fn persistent_config(&self) -> Option<Value> {
        self.state.get("config").cloned()
    }

    pub fn restore_persistent_config(&mut self, config: &Value) {
        if !self.state.is_object() {
            self.state = json!({});
        }
        if let Some(obj) = self.state.as_object_mut() {
            obj.insert("config".to_string(), config.clone());
        }
    }

    pub fn handles_keyboard_event(&self) -> bool {
        self.has_keyboard_handler
    }

    pub fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostRequest> {
        match event {
            PanelEvent::Keyboard {
                panel_id,
                shortcut,
                key,
                repeat,
            } if *panel_id == self.id => self
                .dispatch_to_wasm(
                    "keyboard",
                    json!({ "shortcut": shortcut, "key": key, "repeat": repeat }),
                )
                .unwrap_or_default(),
            PanelEvent::Activate { panel_id, node_id } if *panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({}))
            }
            PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            } if *panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({ "value": value }))
            }
            PanelEvent::DragValue {
                panel_id,
                node_id,
                from,
                to,
            } if *panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(
                    descriptor,
                    json!({ "from": from, "to": to, "value": to }),
                )
            }
            PanelEvent::SetText {
                panel_id,
                node_id,
                value,
            } if *panel_id == self.id => {
                let descriptor = self.lookup_action_descriptor(node_id);
                self.descriptor_to_actions(descriptor, json!({ "value": value.clone() }))
            }
            _ => Vec::new(),
        }
    }
}

impl HtmlWasmPanel {
    fn lookup_action_descriptor(&self, node_id: &str) -> Option<ActionDescriptor> {
        let document = self.view.document();
        let id_selector = format!("#{}", css_escape_id(node_id));
        let id = document.query_selector(&id_selector).ok().flatten()?;
        let node = document.get_node(id)?;
        let NodeData::Element(element) = &node.data else {
            return None;
        };
        let raw_action = element.attr(LocalName::from("data-action"))?;
        let raw_args = element.attr(LocalName::from("data-args"));
        parse_data_action(raw_action, raw_args).ok()
    }

    fn descriptor_to_actions(
        &mut self,
        descriptor: Option<ActionDescriptor>,
        extra_payload: Value,
    ) -> Vec<HostRequest> {
        match descriptor {
            Some(ActionDescriptor::Command { id, .. }) => {
                command_id_to_host_request(&id).map(|a| vec![a]).unwrap_or_default()
            }
            Some(ActionDescriptor::Service { name, mut payload }) => {
                if let Some(extra_obj) = extra_payload.as_object() {
                    for (k, v) in extra_obj {
                        payload.insert(k.clone(), v.clone());
                    }
                }
                let mut request = ServiceRequest::new(name);
                for (k, v) in payload {
                    request = request.with_value(k, v);
                }
                vec![HostRequest::RequestService(request)]
            }
            Some(ActionDescriptor::Altp { node_id: handler, mut payload }) => {
                if let Some(extra_obj) = extra_payload.as_object() {
                    for (k, v) in extra_obj {
                        payload.insert(k.clone(), v.clone());
                    }
                }
                let event_payload = Value::Object(payload);
                self.dispatch_to_wasm(&handler, event_payload)
                    .unwrap_or_default()
            }
            None => Vec::new(),
        }
    }
}

fn command_id_to_host_request(command_id: &str) -> Option<HostRequest> {
    match command_id {
        "noop" => Some(HostRequest::DispatchDocumentCommand(DocumentCommand::Noop)),
        _ => None,
    }
}

/// CSS セレクタ用に id をエスケープする (`.` や `:` を含む id 対応)。
fn css_escape_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('\\');
            out.push(ch);
        }
    }
    out
}

/// テスト用パネルディレクトリ生成 (registry テストとも共有)。
#[cfg(test)]
pub(crate) mod test_fixture {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// keyboard handler を持つパネル: event payload の "shortcut" を
    /// state "config.last_shortcut" へコピーする。
    pub(crate) const KEYBOARD_WAT: &str = r#"(module
    (import "host" "event_get_string_len" (func $event_get_string_len (param i32 i32) (result i32)))
    (import "host" "event_get_string_copy" (func $event_get_string_copy (param i32 i32 i32 i32)))
    (import "host" "state_set_string" (func $state_set_string (param i32 i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "shortcut")
    (data (i32.const 16) "config.last_shortcut")
    (func (export "panel_init"))
    (func (export "panel_handle_keyboard")
        (local $len i32)
        i32.const 0
        i32.const 8
        call $event_get_string_len
        local.set $len
        i32.const 0
        i32.const 8
        i32.const 64
        local.get $len
        call $event_get_string_copy
        i32.const 16
        i32.const 20
        i32.const 64
        local.get $len
        call $state_set_string))"#;

    /// keyboard handler を持たないパネル。
    pub(crate) const NO_KEYBOARD_WAT: &str = r#"(module
    (memory (export "memory") 1)
    (func (export "panel_init")))"#;

    /// `panel_init` で error 診断 (level=2) を 1 件積むパネル (BL-102 検証用)。
    pub(crate) const DIAGNOSTIC_WAT: &str = r#"(module
    (import "host" "diagnostic" (func $diagnostic (param i32 i32 i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "boom")
    (func (export "panel_init")
        i32.const 2
        i32.const 0
        i32.const 4
        call $diagnostic))"#;

    pub(crate) fn write_panel_fixture(name: &str, wat: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time available")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("altpaint-builtin-{name}-{suffix}"));
        std::fs::create_dir_all(&directory).expect("temp directory created");
        std::fs::write(
            directory.join("panel.html"),
            r#"<div class="panel"><button id="kb.test" data-action="command:noop">x</button></div>"#,
        )
        .expect("panel.html written");
        std::fs::write(
            directory.join("panel.meta.json"),
            r#"{ "id": "builtin.test-kb", "title": "KB", "default_size": { "width": 200, "height": 120 } }"#,
        )
        .expect("panel.meta.json written");
        std::fs::write(directory.join("panel.wasm"), wat).expect("panel.wasm written");
        directory
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixture::{DIAGNOSTIC_WAT, KEYBOARD_WAT, NO_KEYBOARD_WAT, write_panel_fixture};
    use super::*;
    use panel_protocol::RequestDescriptor;
    use panel_protocol::names::{layer, tool};

    /// BL-102: 診断は黙殺されず、level/panel/handler/message を含む 1 行へ整形される。
    #[test]
    fn diagnostic_is_formatted_with_level_panel_and_handler() {
        let line = format_panel_diagnostic(
            "builtin.example",
            "panel_init",
            &Diagnostic::error("boom"),
        );
        assert!(line.contains("ERROR"), "level present: {line}");
        assert!(line.contains("builtin.example"), "panel id present: {line}");
        assert!(line.contains("panel_init"), "handler present: {line}");
        assert!(line.contains("boom"), "message present: {line}");
    }

    /// BL-102: emit_handler_diagnostics は積まれた診断件数を消費して返す (黙殺しない)。
    #[test]
    fn emit_handler_diagnostics_consumes_all_diagnostics() {
        let effects = HandlerEffects {
            diagnostics: vec![Diagnostic::warning("a"), Diagnostic::error("b")],
            ..HandlerEffects::default()
        };
        let consumed = emit_handler_diagnostics("builtin.x", "save", &effects);
        assert_eq!(consumed, 2, "all diagnostics must be consumed, not dropped");
    }

    /// BL-102: 診断が無ければ消費件数は 0。
    #[test]
    fn emit_handler_diagnostics_returns_zero_when_empty() {
        let consumed =
            emit_handler_diagnostics("builtin.x", "noop", &HandlerEffects::default());
        assert_eq!(consumed, 0);
    }

    /// BL-102: panel_init が診断を積むパネルもエラーなくロードでき (診断は消費経路へ流れる)。
    #[test]
    fn panel_with_init_diagnostic_loads_without_swallowing_error() {
        let dir = write_panel_fixture("diag-init", DIAGNOSTIC_WAT);
        // panel_init が error 診断を 1 件積むが、ロード自体は成功する。
        // 診断は emit_handler_diagnostics 経由で流れる (黙殺されない)。
        let panel = HtmlWasmPanel::load(&dir, "panel.wasm", None);
        assert!(panel.is_ok(), "panel with init diagnostic should load");
    }

    /// 既知の command 名は registry 経由で HostRequest へ翻訳される。
    #[test]
    fn known_command_translates_to_host_request() {
        let registry = default_translator_registry();
        let action =
            request_descriptor_to_host_request(&registry, RequestDescriptor::new(layer::ADD));
        assert!(matches!(
            action,
            Some(HostRequest::DispatchDocumentCommand(
                DocumentCommand::AddRasterLayer
            ))
        ));
    }

    /// 未登録名は黙殺せず None を返す (diagnostics は registry 側でテスト済み)。
    #[test]
    fn unregistered_name_yields_no_host_request() {
        let registry = default_translator_registry();
        let action = request_descriptor_to_host_request(
            &registry,
            RequestDescriptor::new("totally.unknown_request"),
        );
        assert!(action.is_none());
    }

    /// payload 欠落の翻訳失敗も None を返す (diagnostics へ流れる)。
    #[test]
    fn translation_failure_yields_no_host_request() {
        let registry = default_translator_registry();
        // tool.set_active without payload.tool は翻訳失敗。
        let action = request_descriptor_to_host_request(
            &registry,
            RequestDescriptor::new(tool::SET_ACTIVE),
        );
        assert!(action.is_none());
    }

    /// Wasm が panel_handle_keyboard を export していれば handles_keyboard_event は true。
    #[test]
    fn handles_keyboard_event_true_when_wasm_exports_keyboard_handler() {
        let dir = write_panel_fixture("kb-true", KEYBOARD_WAT);
        let panel = HtmlWasmPanel::load(&dir, "panel.wasm", None).expect("panel loads");
        assert!(panel.handles_keyboard_event());
    }

    /// keyboard handler が無ければ handles_keyboard_event は false。
    #[test]
    fn handles_keyboard_event_false_without_keyboard_handler() {
        let dir = write_panel_fixture("kb-false", NO_KEYBOARD_WAT);
        let panel = HtmlWasmPanel::load(&dir, "panel.wasm", None).expect("panel loads");
        assert!(!panel.handles_keyboard_event());
    }

    /// Keyboard イベントが Wasm keyboard handler へ届き、state patch 経由で
    /// persistent_config に反映される。
    #[test]
    fn keyboard_event_dispatches_to_wasm_and_updates_persistent_config() {
        let dir = write_panel_fixture("kb-dispatch", KEYBOARD_WAT);
        let mut panel = HtmlWasmPanel::load(&dir, "panel.wasm", None).expect("panel loads");

        let actions = panel.handle_event(&PanelEvent::Keyboard {
            panel_id: "builtin.test-kb".to_string(),
            shortcut: "Ctrl+Alt+N".to_string(),
            key: "N".to_string(),
            repeat: false,
        });

        assert!(actions.is_empty());
        assert_eq!(
            panel.persistent_config(),
            Some(json!({ "last_shortcut": "Ctrl+Alt+N" }))
        );
    }

    /// panel_id が一致しない Keyboard イベントは無視される。
    #[test]
    fn keyboard_event_for_other_panel_is_ignored() {
        let dir = write_panel_fixture("kb-other", KEYBOARD_WAT);
        let mut panel = HtmlWasmPanel::load(&dir, "panel.wasm", None).expect("panel loads");

        let actions = panel.handle_event(&PanelEvent::Keyboard {
            panel_id: "builtin.other".to_string(),
            shortcut: "Ctrl+Alt+N".to_string(),
            key: "N".to_string(),
            repeat: false,
        });

        assert!(actions.is_empty());
        assert_eq!(panel.persistent_config(), None);
    }
}

