//! Wasm 著者向け DOM mutation API。
//!
//! 関数名は Blitz `DocumentMutator` / `BaseDocument` と一致させる (合成 API は提供しない)。
//! Wasm ABI 上必要な ptr/len 変換だけを吸収する free function 群。
//!
//! 使用例:
//! ```ignore
//! use plugin_sdk::dom::{query_selector, set_attribute, set_inner_html, html_escape};
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
        pub fn query_selector_all(ptr: *const u8, len: i32) -> i64;
        pub fn iter_next(handle: i64) -> i64;
        pub fn iter_drop(handle: i64);
        pub fn get_attribute_len(node: i64, name_ptr: *const u8, name_len: i32) -> i32;
        pub fn get_attribute_copy(
            node: i64,
            name_ptr: *const u8,
            name_len: i32,
            buf_ptr: *mut u8,
            buf_cap: i32,
        ) -> i32;
        pub fn set_attribute(
            node: i64,
            name_ptr: *const u8,
            name_len: i32,
            value_ptr: *const u8,
            value_len: i32,
        );
        pub fn clear_attribute(node: i64, name_ptr: *const u8, name_len: i32);
        pub fn create_text_node(text_ptr: *const u8, text_len: i32) -> i64;
        pub fn append_children(parent: i64, children_ptr: *const u8, count: i32);
        pub fn remove_and_drop_all_children(node: i64);
        pub fn set_inner_html(node: i64, html_ptr: *const u8, html_len: i32);
    }
}

/// Wasm から見た Blitz NodeId の不透明ハンドル。
///
/// 内部表現は host 側 NodeId+1 (0 は None を意味するため避ける)。
pub type NodeId = i64;

/// `query_selector` の結果に対応する iterator handle (内部用)。
pub struct QueryAllIter(i64);

impl Drop for QueryAllIter {
    fn drop(&mut self) {
        if self.0 != 0 {
            #[cfg(target_arch = "wasm32")]
            unsafe {
                imports::iter_drop(self.0);
            }
        }
    }
}

impl Iterator for QueryAllIter {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.0 == 0 {
            return None;
        }
        #[cfg(target_arch = "wasm32")]
        unsafe {
            let next = imports::iter_next(self.0);
            if next == 0 { None } else { Some(next) }
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    }
}

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

/// CSS セレクタにマッチする全要素のイテレータを返す。
pub fn query_selector_all(selector: &str) -> QueryAllIter {
    let bytes = selector.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let handle = imports::query_selector_all(bytes.as_ptr(), bytes.len() as i32);
        QueryAllIter(handle)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = bytes;
        QueryAllIter(0)
    }
}

/// 属性値を返す。属性が存在しない場合は `None`。
pub fn get_attribute(node: NodeId, name: &str) -> Option<String> {
    let name_bytes = name.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let len = imports::get_attribute_len(node, name_bytes.as_ptr(), name_bytes.len() as i32);
        if len < 0 {
            return None;
        }
        let len = len as usize;
        let mut buf = vec![0u8; len];
        let written =
            imports::get_attribute_copy(node, name_bytes.as_ptr(), name_bytes.len() as i32, buf.as_mut_ptr(), len as i32);
        if written < 0 {
            return None;
        }
        String::from_utf8(buf).ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (node, name_bytes);
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

/// テキストノードを作成し、その NodeId を返す。
pub fn create_text_node(text: &str) -> NodeId {
    let tb = text.as_bytes();
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::create_text_node(tb.as_ptr(), tb.len() as i32)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = tb;
        0
    }
}

/// 子ノードを末尾に追加する。
pub fn append_children(parent: NodeId, children: &[NodeId]) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::append_children(
            parent,
            children.as_ptr() as *const u8,
            children.len() as i32,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (parent, children);
    }
}

/// 全子ノードを削除する。
pub fn remove_and_drop_all_children(node: NodeId) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        imports::remove_and_drop_all_children(node);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = node;
    }
}

/// 要素の inner HTML を置き換える (HTML 断片を Blitz パーサに通す)。
///
/// **信頼境界**: `html` 引数は Blitz の HTML パーサに直接流される。
/// host snapshot 由来の文字列を埋め込む場合は必ず `html_escape` を経由すること。
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
