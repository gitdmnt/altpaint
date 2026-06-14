//! `HtmlPanelView` の DOM 管理 (BL-100)。
//!
//! document アクセス・差し替え・mutation 通知・UI イベント注入と、汎用の
//! 要素状態 snapshot/restore (`data-altp-id`/`details` 等の規約は呼出側が引数で渡す)。

use super::{HtmlPanelView, element_identity, qual_name};
use blitz_dom::{BaseDocument, EventDriver, NoopEventHandler};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_traits::events::UiEvent;
use std::sync::Arc;

impl HtmlPanelView {
    /// UiEvent (PointerDown/Up/Move 等) を Blitz に流す。
    /// `:hover` / `<details>` 開閉 / `<button>` のアクティブ状態などはこの経路でのみ反映される。
    pub fn on_input(&mut self, event: UiEvent) {
        let mut driver = EventDriver::new(&mut self.document, NoopEventHandler);
        driver.handle_ui_event(event);
        // pointer / key 系イベントは hover 状態 / focus / details 開閉 など
        // レイアウトが変わる可能性が常にあるため無条件で dirty を立てる。
        // damage を観測してから判断する API は Blitz 0.3.0-alpha では public でないため
        // 楽観的に再 resolve させる。
        self.layout_dirty = true;
        self.render_dirty = true;
    }

    pub fn document(&self) -> &BaseDocument {
        &self.document
    }

    /// Wasm DOM mutation API のために `HtmlDocument` への可変借用を返す。
    ///
    /// 呼び出し側 (panel-runtime) は `PanelWasmInstance::call_with_dom` のスコープ内でのみ使い、
    /// 戻り際に `mark_mutated()` を呼んで dirty を立てる契約。
    pub fn document_mut(&mut self) -> &mut HtmlDocument {
        &mut self.document
    }

    /// Wasm が DOM mutation を行ったあとに呼び、次フレームで再 layout/render を要求する。
    pub fn mark_mutated(&mut self) {
        self.pending_mutation = true;
        self.layout_dirty = true;
        self.render_dirty = true;
    }

    /// HTML / CSS を差し替えて document を再構築する。
    ///
    /// - 同一 `(html, css)` ならスキップ (idle frame 最適化、`render_dirty` も立てない)。
    /// - 異なる場合は新しい `HtmlDocument` を構築する。
    /// - `gpu_target` は維持する (size 不変ならそのまま使える)。
    /// - フォーカスや `<details>` 開閉などの要素状態は現状維持できない。保持が必要なら
    ///   呼出側が [`Self::snapshot_marker_identities`] / [`Self::clear_marker_for_unlisted`]
    ///   で囲んで保存・復元する (BL-099: details/data-altp-id 規約は panel-html が持たない)。
    pub fn replace_document(&mut self, html: &str, css: &str) {
        if self.last_html.as_deref() == Some(html) && self.user_css == css {
            return;
        }
        let mut config = blitz_dom::DocumentConfig::default();
        if !css.is_empty() {
            config.ua_stylesheets = Some(vec![css.to_string()]);
        }
        config.html_parser_provider = Some(Arc::new(HtmlProvider));
        self.document = HtmlDocument::from_html(html, config);
        self.user_css = css.to_string();
        self.last_html = Some(html.to_string());
        self.last_resolved = None;
        self.pending_mutation = true;
        self.layout_dirty = true;
        self.render_dirty = true;
    }

    /// `selector` にマッチする要素の identity を集める (汎用 要素状態 snapshot, BL-099)。
    ///
    /// identity は `identity_attrs` を先頭から探して最初に見つかった属性値。どの規約属性
    /// (`data-altp-id` / `id` 等) を identity に使うかは **呼出側が指定する** — panel-html は
    /// 規約名をハードコードしない。`replace_document` の前に呼んで保存し、後で
    /// [`Self::clear_marker_for_unlisted`] へ渡す。
    pub fn snapshot_marker_identities(
        &self,
        selector: &str,
        identity_attrs: &[&str],
    ) -> Vec<String> {
        let Ok(ids) = self.document.query_selector_all(selector) else {
            return Vec::new();
        };
        ids.into_iter()
            .filter_map(|node_id| element_identity(&self.document, node_id, identity_attrs))
            .collect()
    }

    /// `selector` にマッチする要素のうち identity が `keep` に無いものから属性 `marker_attr`
    /// を取り除く (汎用 要素状態 restore, BL-099)。
    ///
    /// 「再構築 HTML は常に `marker_attr` 付きで出力されるが、保存時に無かった要素は
    /// ユーザー操作で外された状態」という復元ポリシーは呼出側が `keep` と `marker_attr` を
    /// 与えて表現する。
    pub fn clear_marker_for_unlisted(
        &mut self,
        selector: &str,
        marker_attr: &str,
        identity_attrs: &[&str],
        keep: &[String],
    ) {
        let Ok(matched) = self.document.query_selector_all(selector) else {
            return;
        };
        for node_id in matched {
            let Some(identity) = element_identity(&self.document, node_id, identity_attrs) else {
                continue;
            };
            if keep.iter().any(|id| id == &identity) {
                continue;
            }
            let mut mutator = self.document.mutate();
            mutator.clear_attribute(node_id, qual_name(marker_attr));
            self.pending_mutation = true;
        }
    }
}
