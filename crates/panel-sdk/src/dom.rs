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

/// Wasm から見た Blitz NodeId の不透明ハンドル。
///
/// 内部表現は host 側 NodeId+1 (0 は None を意味するため避ける)。
pub type NodeId = i64;

/// CSS セレクタにマッチする最初の要素を返す。マッチなしなら `None`。
pub fn query_selector(selector: &str) -> Option<NodeId> {
    let bytes = selector.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let raw = imports::query_selector(bytes.as_ptr(), bytes.len() as i32);
        if raw == 0 { None } else { Some(raw) }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = bytes;
        None
    }
}

/// 属性をセットする。
pub fn set_attribute(node: NodeId, name: &str, value: &str) {
    let nb = name.as_bytes();
    let vb = value.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::set_attribute(
            node,
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
pub fn clear_attribute(node: NodeId, name: &str) {
    let nb = name.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::clear_attribute(node, nb.as_ptr(), nb.len() as i32);
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
pub fn set_inner_html(node: NodeId, html: &str) {
    let hb = html.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::set_inner_html(node, hb.as_ptr(), hb.len() as i32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (node, hb);
    }
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
}
