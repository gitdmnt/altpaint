//! Wasm から Blitz `HtmlDocument` を mutate するための host function 群。
//!
//! 設計方針 (Phase 10):
//! - 関数名・責務は Blitz `DocumentMutator` / `BaseDocument` と同じにする (合成 API は提供しない)
//! - DOM への借用は呼出単位で完結 (1 host call = 1 `DocumentMutator`)
//! - DOM context は `HostCallContext::dom_ctx` に raw pointer で持たせ、
//!   `PanelWasmInstance::call_with_dom` のスコープ内でのみ有効
//!
//! NodeId エンコーディング:
//! - blitz NodeId (`usize`) を u64 として ABI に渡す
//! - `query_selector` の Option 返却は u64 で表現 (0 = None, それ以外は NodeId + 1)

use crate::HostCallContext;
use crate::memory::{push_error, read_utf8};
use blitz_dom::{LocalName, Namespace, QualName};
use blitz_html::HtmlDocument;
use panel_protocol::abi::DOM_IMPORT_MODULE;
use std::ptr::NonNull;
use wasmtime::{Caller, Linker};

/// Wasm 呼出スコープ内のみ有効な DOM コンテキスト。
///
/// `PanelWasmInstance::call_with_dom` が NonNull を立て、戻り際に None に戻す。
/// DOM API の host function は raw pointer を直接 deref せず、必ず [`with_document`]
/// 経由で `HtmlDocument` を借用する (BL-104: unsafe 不変条件を 1 箇所に集約)。
#[derive(Default)]
pub(crate) struct DomCtx {
    pub(crate) document: Option<NonNull<HtmlDocument>>,
}

impl DomCtx {
    pub(crate) fn clear(&mut self) {
        self.document = None;
    }
}

/// `DocumentMutator` / `BaseDocument` を Wasm に公開する host function 群を linker に登録する。
pub(crate) fn register_dom_host_functions(
    linker: &mut Linker<HostCallContext>,
) -> wasmtime::Result<()> {
    linker.func_wrap(DOM_IMPORT_MODULE, "query_selector", host_query_selector)?;
    linker.func_wrap(DOM_IMPORT_MODULE, "set_attribute", host_set_attribute)?;
    linker.func_wrap(DOM_IMPORT_MODULE, "clear_attribute", host_clear_attribute)?;
    linker.func_wrap(DOM_IMPORT_MODULE, "set_inner_html", host_set_inner_html)?;
    Ok(())
}

fn host_query_selector(mut caller: Caller<'_, HostCallContext>, ptr: i32, len: i32) -> i64 {
    let Some(selector) = read_utf8(&mut caller, ptr, len) else {
        push_error(&mut caller, "query_selector: invalid selector ptr/len");
        return 0;
    };
    let result = with_document(&mut caller, |doc| doc.query_selector(&selector));
    let Some(query_result) = result else {
        push_error(&mut caller, "query_selector: no DOM context");
        return 0;
    };
    match query_result {
        Ok(Some(id)) => (id as i64) + 1,
        Ok(None) => 0,
        Err(_) => {
            push_error(&mut caller, "query_selector: invalid CSS selector");
            0
        }
    }
}

fn host_set_attribute(
    mut caller: Caller<'_, HostCallContext>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
    value_ptr: i32,
    value_len: i32,
) {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_error(&mut caller, "set_attribute: invalid name ptr/len");
        return;
    };
    let Some(value) = read_utf8(&mut caller, value_ptr, value_len) else {
        push_error(&mut caller, "set_attribute: invalid value ptr/len");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_error(&mut caller, "set_attribute: invalid node_id");
            return;
        }
    };
    let applied = with_document(&mut caller, |doc| {
        doc.mutate().set_attribute(id, qual_name(&name), &value);
    });
    if applied.is_none() {
        push_error(&mut caller, "set_attribute: no DOM context");
    }
}

fn host_clear_attribute(
    mut caller: Caller<'_, HostCallContext>,
    node_id: i64,
    name_ptr: i32,
    name_len: i32,
) {
    let Some(name) = read_utf8(&mut caller, name_ptr, name_len) else {
        push_error(&mut caller, "clear_attribute: invalid name ptr/len");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_error(&mut caller, "clear_attribute: invalid node_id");
            return;
        }
    };
    let applied = with_document(&mut caller, |doc| {
        doc.mutate().clear_attribute(id, qual_name(&name));
    });
    if applied.is_none() {
        push_error(&mut caller, "clear_attribute: no DOM context");
    }
}

fn host_set_inner_html(
    mut caller: Caller<'_, HostCallContext>,
    node_id: i64,
    html_ptr: i32,
    html_len: i32,
) {
    let Some(html) = read_utf8(&mut caller, html_ptr, html_len) else {
        push_error(&mut caller, "set_inner_html: invalid html ptr/len");
        return;
    };
    let id = match decode_node_id(node_id) {
        Some(id) => id,
        None => {
            push_error(&mut caller, "set_inner_html: invalid node_id");
            return;
        }
    };
    let applied = with_document(&mut caller, |doc| {
        doc.mutate().set_inner_html(id, &html);
    });
    if applied.is_none() {
        push_error(&mut caller, "set_inner_html: no DOM context");
    }
}

// === ヘルパ ===

fn decode_node_id(raw: i64) -> Option<usize> {
    if raw <= 0 {
        return None;
    }
    Some((raw - 1) as usize)
}

/// DOM context が有効なら `&mut HtmlDocument` を `f` に渡し、結果を `Some` で返す。
/// context 未設定 (`call_with_dom` スコープ外) なら `None`。
///
/// 本クレートで `dom_ctx.document` (raw pointer) を dereference する **唯一の箇所**
/// (BL-104)。各 DOM host function は raw pointer を直接触らず本ヘルパ経由で document を
/// 借用する。
///
/// SAFETY (この 1 箇所に閉じる不変条件):
/// - `dom_ctx.document` は `PanelWasmInstance::call_with_dom` が `&mut HtmlDocument`
///   から立てた `NonNull` であり、スコープ内では生存し alias していない。
/// - deref 期間は `f` の実行 1 回分に閉じ、Wasm に再制御を渡す前に終わる。
/// - context 未設定 (`None`) のときは deref しない。
fn with_document<R>(
    caller: &mut Caller<'_, HostCallContext>,
    f: impl FnOnce(&mut HtmlDocument) -> R,
) -> Option<R> {
    let mut document = caller.data().dom_ctx.document?;
    // SAFETY: 上記の不変条件により、call_with_dom スコープ内でのみ `with_document` が
    // 呼ばれ、`document` は生存しており排他借用できる。
    let doc = unsafe { document.as_mut() };
    Some(f(doc))
}

fn qual_name(local: &str) -> QualName {
    QualName::new(None, Namespace::default(), LocalName::from(local))
}
