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
}
