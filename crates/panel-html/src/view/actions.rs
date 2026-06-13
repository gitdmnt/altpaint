//! `HtmlPanelView` の `data-action` 矩形収集と descriptor 解釈 (BL-100)。
//!
//! `data-action` / `data-args` の属性名規約・CSS id エスケープ・`ActionDescriptor` への
//! パースを本モジュールへ集約する (BL-099)。`on_render` と同一のクランプ規則で layout を
//! 解決し、headless でも hit 矩形を収集できる。

use super::{ActionRect, HtmlPanelView, PanelActionRect};
use blitz_dom::{BaseDocument, LocalName, local_name, node::NodeData};

/// `data-action` 属性名 (アクション要素の規約マーカー)。
const DATA_ACTION_ATTR: &str = "data-action";
/// `data-args` 属性名 (アクション payload の JSON)。
const DATA_ARGS_ATTR: &str = "data-args";
/// `data-action` を持つ要素を選択する CSS セレクタ。
const ACTION_SELECTOR: &str = "[data-action]";

impl HtmlPanelView {
    /// GPU 非依存でレイアウトを解決し、`data-action` 要素の hit 矩形を返す。
    ///
    /// `on_render` と同一のクランプ規則 (panel_size を viewport / chrome_height で
    /// クランプした local size) で `resolve_layout` を走らせるため、GPU 描画と
    /// hit 矩形が常に一致する。headless (GPU コンテキストなし) でも動作する。
    pub fn resolve_action_rects(
        &mut self,
        viewport: (u32, u32),
        scale: f32,
        chrome_height: u32,
    ) -> Vec<ActionRect> {
        let local = self.local_render_size(viewport, chrome_height);
        if self.layout_dirty {
            self.resolve_layout(local.width, local.body_height, scale);
            self.layout_dirty = false;
        }
        self.collect_action_rects()
    }

    /// `data-action` 属性を持つ全要素の絶対矩形を返す（要 `resolve_layout` 済み）。
    pub fn collect_action_rects(&self) -> Vec<ActionRect> {
        let ids = match self.document.query_selector_all(ACTION_SELECTOR) {
            Ok(ids) => ids,
            Err(_) => return Vec::new(),
        };
        ids.into_iter()
            .filter_map(|id| self.action_rect_for(id))
            .collect()
    }

    /// DOM id (`#<id>`) で要素を引き、その `data-action`/`data-args` を [`ActionDescriptor`]
    /// に解釈して返す (BL-099: data-action 属性規約の単一定義点)。
    ///
    /// `id` 文字列の CSS エスケープ (`.` / `:` を含む id 対応) も本メソッドが行うため、
    /// 呼出側 (panel-runtime) は属性名規約もエスケープ規約も持たない。
    ///
    /// [`ActionDescriptor`]: crate::action::ActionDescriptor
    pub fn action_descriptor_for_element_id(
        &self,
        element_id: &str,
    ) -> Option<crate::action::ActionDescriptor> {
        let selector = format!("#{}", css_escape_id(element_id));
        let node_id = self.document.query_selector(&selector).ok().flatten()?;
        self.action_descriptor_for_element(node_id)
    }

    /// node の `data-action`/`data-args` 属性を読み、`ActionDescriptor` へ解釈する
    /// (BL-099: data-action / data-args の属性名規約を集約)。`data-action` が無い、
    /// または解釈に失敗したら `None`。
    pub fn action_descriptor_for_element(
        &self,
        node_id: usize,
    ) -> Option<crate::action::ActionDescriptor> {
        let (data_action, data_args) = self.action_attrs_for(node_id)?;
        crate::action::parse_data_action(&data_action, data_args.as_deref()).ok()
    }

    /// node の `data-action` (必須) と `data-args` (任意) 生文字列を返す。
    /// `data-action` が無い / 要素でない場合は `None`。
    fn action_attrs_for(&self, node_id: usize) -> Option<(String, Option<String>)> {
        let node = self.document.get_node(node_id)?;
        let NodeData::Element(element) = &node.data else {
            return None;
        };
        let data_action = element.attr(LocalName::from(DATA_ACTION_ATTR))?.to_string();
        let data_args = element.attr(LocalName::from(DATA_ARGS_ATTR)).map(str::to_string);
        Some((data_action, data_args))
    }

    fn action_rect_for(&self, node_id: usize) -> Option<ActionRect> {
        let (data_action, data_args) = self.action_attrs_for(node_id)?;
        let node = self.document.get_node(node_id)?;
        let NodeData::Element(element) = &node.data else {
            return None;
        };
        let element_id = element.attr(local_name!("id")).map(str::to_string);
        let (x, y) = compute_absolute_position(&self.document, node_id)?;
        let size = node.final_layout.size;
        let rect = PanelActionRect {
            x: x.max(0.0).floor() as u32,
            y: y.max(0.0).floor() as u32,
            width: size.width.max(0.0).ceil() as u32,
            height: size.height.max(0.0).ceil() as u32,
        };
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        Some(ActionRect {
            node_id,
            element_id,
            data_action,
            data_args,
            rect,
        })
    }
}

/// CSS セレクタ用に id をエスケープする (`.` や `:` を含む id 対応)。
/// 旧 panel-runtime `css_escape_id` を panel-html へ集約 (BL-099)。
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

fn compute_absolute_position(doc: &BaseDocument, start: usize) -> Option<(f32, f32)> {
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut current = start;
    let mut visited: std::collections::HashSet<usize> = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            return Some((x, y)); // 安全弁: ループ検出
        }
        let node = doc.get_node(current)?;
        x += node.final_layout.location.x;
        y += node.final_layout.location.y;
        match node.layout_parent.get() {
            Some(parent) => current = parent,
            None => return Some((x, y)),
        }
    }
}
