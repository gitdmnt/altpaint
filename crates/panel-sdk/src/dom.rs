//! Wasm 著者向け DOM mutation API。
//!
//! 関数名は Blitz `DocumentMutator` / `BaseDocument` と一致させる (合成 API は提供しない)。
//! Wasm ABI 上必要な ptr/len 変換だけを吸収する free function 群。
//!
//! 使用例:
//! ```ignore
//! use panel_sdk::dom::{query_selector, set_attribute, set_inner_html, html_escape};
//!
//! // Undo ボタンを disable に
//! if let Some(btn) = query_selector("#btn-undo") {
//!     set_attribute(btn, "disabled", "");
//! }
//!
//! // 動的リスト構築
//! let list = query_selector("#layer-list").unwrap();
//! let mut html = String::new();
//! for layer in layers {
//!     html.push_str(&format!(
//!         r#"<li class="layer">{}</li>"#,
//!         html_escape(&layer.name),
//!     ));
//! }
//! set_inner_html(list, &html);
//! ```

/// Wasm から見た Blitz NodeId の不透明ハンドル (P25)。
///
/// ABI 上は host 側 NodeId に `+1` した `i64` で運ばれる (0 は `None` を意味するため
/// 避ける)。この `+1` シフトを誤読しないよう、生の `i64` を直接扱う型 alias ではなく
/// newtype で包む。中身 (`raw`) は host が組み立てた ABI 値そのものであり、パネル
/// コードは内部表現を解釈しない。`set_attribute` 等の DOM API へはこのハンドルを
/// そのまま渡す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle(i64);

// from_abi / to_abi は ABI 境界 (`#[cfg(target_arch = "wasm32")]`) と単体テストでのみ
// 使うため、native 非テストビルドでは到達しない。ABI 契約として常に保持する。
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
impl NodeHandle {
    /// ABI から受け取った生の `i64` をハンドルへ包む。
    ///
    /// `raw` が `0` 以下 (= `None` 番兵 / 無効値) なら `None` を返す。
    pub(crate) fn from_abi(raw: i64) -> Option<Self> {
        if raw <= 0 { None } else { Some(Self(raw)) }
    }

    /// DOM host function へ渡す ABI 値 (host NodeId + 1)。
    pub(crate) fn to_abi(self) -> i64 {
        self.0
    }
}

#[cfg(target_arch = "wasm32")]
mod imports {
    #[link(wasm_import_module = "dom")]
    unsafe extern "C" {
        pub fn query_selector(ptr: *const u8, len: i32) -> i64;
        pub fn set_attribute(
            node: i64,
            name_ptr: *const u8,
            name_len: i32,
            value_ptr: *const u8,
            value_len: i32,
        );
        pub fn clear_attribute(node: i64, name_ptr: *const u8, name_len: i32);
        pub fn set_inner_html(node: i64, html_ptr: *const u8, html_len: i32);
    }
}

/// CSS セレクタにマッチする最初の要素を返す。マッチなしなら `None`。
pub fn query_selector(selector: &str) -> Option<NodeHandle> {
    let bytes = selector.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let raw = imports::query_selector(bytes.as_ptr(), bytes.len() as i32);
        NodeHandle::from_abi(raw)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = bytes;
        None
    }
}

/// 属性をセットする。
pub fn set_attribute(node: NodeHandle, name: &str, value: &str) {
    let nb = name.as_bytes();
    let vb = value.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::set_attribute(
            node.to_abi(),
            nb.as_ptr(),
            nb.len() as i32,
            vb.as_ptr(),
            vb.len() as i32,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (node, nb, vb);
    }
}

/// 属性を削除する。
pub fn clear_attribute(node: NodeHandle, name: &str) {
    let nb = name.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::clear_attribute(node.to_abi(), nb.as_ptr(), nb.len() as i32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (node, nb);
    }
}

/// 要素の inner HTML を置き換える (HTML 断片を Blitz パーサに通す)。
///
/// **信頼境界**: `html` 引数は Blitz の HTML パーサに直接流される。
/// host state 由来の文字列を埋め込む場合は必ず `html_escape` を経由すること。
pub fn set_inner_html(node: NodeHandle, html: &str) {
    let hb = html.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::set_inner_html(node.to_abi(), hb.as_ptr(), hb.len() as i32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (node, hb);
    }
}

/// ホストから供給される構造化 JSON 配列 (BL-105) を `(value, label)` の組へパースする。
///
/// `raw` は `[{"<value_key>": "...", "<label_key>": "..."}, ...]` の JSON 文字列。
/// 旧 "WxH:Label" / "id:label" パイプ区切り独自形式を置換し、dropdown 構築用の
/// (option value, 表示ラベル) 列を返す。空文字列・不正 JSON は空列を返す。
pub fn parse_option_list(raw: &str, value_key: &str, label_key: &str) -> Vec<(String, String)> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let value = item.get(value_key)?.as_str()?.to_string();
            let label = item.get(label_key)?.as_str()?.to_string();
            Some((value, label))
        })
        .collect()
}

/// HTML 特殊文字をエスケープする。`set_inner_html` に流す動的文字列で必須。
pub fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

// ===========================================================================
// 水平 DOM ヘルパ (BL-143)
//
// 12 パネルに散在していた `set_text` / `set_visible` / `set_button_active` /
// `set_slider` / `render_options` / `render_action_list` のコピペを SDK に集約する。
// テキスト・属性値・ラベルは **すべて `html_escape` を内蔵** し、XSS を構造的に
// 防ぐ (escape 安全性テストは本モジュールに集約)。`set_*` は native では
// `query_selector` が `None` を返すため副作用なしの no-op になる。
// ===========================================================================

/// セレクタにマッチする要素のテキスト内容を設定する (escape 内蔵)。
///
/// `text` は `html_escape` を通して `set_inner_html` へ流す。host state 由来の
/// 文字列をそのまま渡してよい (生 HTML として解釈されない)。
pub fn set_text(selector: &str, text: &str) {
    if let Some(node) = query_selector(selector) {
        set_inner_html(node, &html_escape(text));
    }
}

/// セレクタにマッチする要素の表示/非表示を `hidden` 属性で切り替える。
pub fn set_visible(selector: &str, visible: bool) {
    if let Some(node) = query_selector(selector) {
        if visible {
            clear_attribute(node, "hidden");
        } else {
            set_attribute(node, "hidden", "");
        }
    }
}

/// ボタン要素の `class` を `active` 状態に応じて `"btn active"` / `"btn"` へ設定する。
pub fn set_button_active(selector: &str, active: bool) {
    if let Some(node) = query_selector(selector) {
        set_attribute(node, "class", if active { "btn active" } else { "btn" });
    }
}

/// スライダーの `value` 属性を設定し、対応する表示要素のテキストも更新する。
///
/// `display_selector` が空文字列なら表示要素の更新は行わない (スライダーのみ更新)。
pub fn set_slider(selector: &str, value: i32, display_selector: &str) {
    if let Some(node) = query_selector(selector) {
        set_attribute(node, "value", &value.to_string());
    }
    if !display_selector.is_empty() {
        set_text(display_selector, &value.to_string());
    }
}

/// `(value, label)` の組から `<option>` 列の HTML 断片を組み立てる (escape 内蔵)。
///
/// `selected` と一致する value の option に ` selected` を付与する。value / label は
/// `html_escape` を通すため、host state 由来の文字列を安全に渡せる。返値は
/// `set_inner_html` へ流す前提の HTML 断片 (要素自体の探索・設定は呼出側が行う)。
pub fn render_options<'a, I>(options: I, selected: &str) -> String
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut html = String::new();
    for (value, label) in options {
        let mark = if value == selected { " selected" } else { "" };
        html.push_str(r#"<option value=""#);
        html.push_str(&html_escape(value));
        html.push('"');
        html.push_str(mark);
        html.push('>');
        html.push_str(&html_escape(label));
        html.push_str("</option>");
    }
    html
}

/// `render_action_list` の 1 行を表す。data-action 付き `<li>` を生成する素材。
///
/// - `handler`: `data-action="altp:activate:<handler>"` に埋める handler 名。
/// - `args`: `data-args` の JSON オブジェクト (`serde_json::Value`)。`Value::Null` なら
///   `data-args` 属性を出力しない。
/// - `active`: true なら `<li>` に `class="active"` を付与する。
/// - `body`: `<li>` の内側 HTML (呼出側が `html_escape` 済みの断片を渡す)。
#[derive(Debug, Clone)]
pub struct ActionListItem {
    pub handler: String,
    pub args: serde_json::Value,
    pub active: bool,
    pub body: String,
}

impl ActionListItem {
    /// handler 名と内側 HTML から最小の項目を作る (args なし・非 active)。
    pub fn new(handler: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            handler: handler.into(),
            args: serde_json::Value::Null,
            active: false,
            body: body.into(),
        }
    }

    /// `data-args` の JSON を設定する。
    pub fn with_args(mut self, args: serde_json::Value) -> Self {
        self.args = args;
        self
    }

    /// active フラグを設定する。
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

/// data-action 付き `<li>` 列の HTML 断片を組み立てる (属性値 escape 内蔵)。
///
/// handler 名・data-args JSON は属性値として `html_escape` を通す。`body` は呼出側が
/// 構築した内側 HTML (動的文字列は呼出側で `html_escape` 済みの前提)。返値は
/// `set_inner_html` へ流す HTML 断片。
pub fn render_action_list<I>(items: I) -> String
where
    I: IntoIterator<Item = ActionListItem>,
{
    let mut html = String::new();
    for item in items {
        html.push_str("<li");
        if item.active {
            html.push_str(r#" class="active""#);
        }
        html.push_str(r#" data-action="altp:activate:"#);
        html.push_str(&html_escape(&item.handler));
        html.push('"');
        if !item.args.is_null() {
            html.push_str(r#" data-args=""#);
            html.push_str(&html_escape(&item.args.to_string()));
            html.push('"');
        }
        html.push('>');
        html.push_str(&item.body);
        html.push_str("</li>");
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_handle_rejects_none_sentinel_and_negative() {
        // 0 は host 側 `query_selector` の None 番兵。負値も無効。
        assert_eq!(NodeHandle::from_abi(0), None);
        assert_eq!(NodeHandle::from_abi(-1), None);
    }

    #[test]
    fn node_handle_roundtrips_abi_value() {
        // ABI 値は host NodeId + 1 (>= 1)。包んでも素通しできる。
        let handle = NodeHandle::from_abi(1).expect("first node handle");
        assert_eq!(handle.to_abi(), 1);

        let handle = NodeHandle::from_abi(99).expect("node handle");
        assert_eq!(handle.to_abi(), 99);
    }

    #[test]
    fn html_escape_basic() {
        assert_eq!(html_escape("hello"), "hello");
        assert_eq!(html_escape("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(html_escape(r#"a"b'c&d"#), "a&quot;b&#39;c&amp;d");
    }

    #[test]
    fn html_escape_xss_payload() {
        let payload = r#"<script>alert("xss")</script>"#;
        assert_eq!(
            html_escape(payload),
            "&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;"
        );
    }

    #[test]
    fn html_escape_japanese_unchanged() {
        assert_eq!(html_escape("レイヤー"), "レイヤー");
    }

    #[test]
    fn parse_option_list_extracts_value_label_pairs() {
        let raw = r#"[{"size":"320x240","label":"Demo"},{"size":"640x480","label":"VGA"}]"#;
        let opts = parse_option_list(raw, "size", "label");
        assert_eq!(
            opts,
            vec![
                ("320x240".to_string(), "Demo".to_string()),
                ("640x480".to_string(), "VGA".to_string()),
            ]
        );
    }

    #[test]
    fn parse_option_list_handles_empty_and_invalid() {
        assert!(parse_option_list("", "id", "label").is_empty());
        assert!(parse_option_list("not json", "id", "label").is_empty());
        assert!(parse_option_list("{}", "id", "label").is_empty());
        // キー欠落のエントリは除外。
        assert!(parse_option_list(r#"[{"id":"x"}]"#, "id", "label").is_empty());
    }

    #[test]
    fn parse_option_list_uses_id_label_keys() {
        let raw = r#"[{"id":"review","label":"Review workspace"}]"#;
        let opts = parse_option_list(raw, "id", "label");
        assert_eq!(opts, vec![("review".to_string(), "Review workspace".to_string())]);
    }

    // ---- 水平 DOM ヘルパ (BL-143) ----

    #[test]
    fn render_options_marks_selected_and_escapes() {
        let html = render_options(
            [("normal", "通常"), ("multiply", "乗算")],
            "multiply",
        );
        assert_eq!(
            html,
            r#"<option value="normal">通常</option><option value="multiply" selected>乗算</option>"#
        );
    }

    #[test]
    fn render_options_escapes_value_and_label() {
        // value / label の双方を escape する (host state 由来の文字列でも安全)。
        let html = render_options([(r#"<a">"#, r#"<script>"#)], "");
        assert!(!html.contains("<script>"));
        assert!(!html.contains(r#"<a">"#));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains(r#"value="&lt;a&quot;&gt;""#));
    }

    #[test]
    fn render_options_empty_iter_yields_empty_string() {
        let html = render_options(std::iter::empty::<(&str, &str)>(), "x");
        assert_eq!(html, "");
    }

    #[test]
    fn render_action_list_builds_li_with_action_and_args() {
        let html = render_action_list([
            ActionListItem::new("select_layer", "<span>L1</span>")
                .with_args(serde_json::json!({ "value": 0 }))
                .active(true),
            ActionListItem::new("select_layer", "<span>L2</span>")
                .with_args(serde_json::json!({ "value": 1 })),
        ]);
        assert!(html.contains(r#"<li class="active" data-action="altp:activate:select_layer""#));
        // data-args は属性値として escape されるため、" は &quot; になる。
        assert!(html.contains(r#"data-args="{&quot;value&quot;:0}""#));
        assert!(html.contains("<span>L1</span>"));
        // 非 active 項目に class="active" は付かない。
        assert!(html.contains(r#"<li data-action="altp:activate:select_layer""#));
    }

    #[test]
    fn render_action_list_omits_data_args_when_null() {
        let html = render_action_list([ActionListItem::new("reload", "x")]);
        assert_eq!(html, r#"<li data-action="altp:activate:reload">x</li>"#);
        assert!(!html.contains("data-args"));
    }

    #[test]
    fn render_action_list_escapes_handler_name() {
        // handler 名は属性値として escape する (XSS 防止)。
        let html = render_action_list([ActionListItem::new(r#""><script>"#, "body")]);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn dom_setters_are_safe_noops_on_native() {
        // native では query_selector が None を返すため、副作用なしで通る。
        set_text("#x", "<b>");
        set_visible("#x", true);
        set_visible("#x", false);
        set_button_active("#x", true);
        set_slider("#x", 12, "#display");
        set_slider("#x", 12, "");
    }
}
