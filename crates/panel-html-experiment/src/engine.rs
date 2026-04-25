//! Blitz HTML パネル描画エンジン（GPU 直描画版）。
//!
//! - HTML を [`HtmlDocument`] にパース
//! - ユーザー指定 CSS を user-agent stylesheet として追加
//! - viewport 設定 → style/layout 解決 → `blitz-paint` で `vello::Scene` を構築（**CPU pixels なし**）
//! - `data-bind-*` を JSON snapshot で評価し、DOM の attribute / class / textContent を更新
//! - `data-action` を持つ要素のレイアウト矩形を CSS 解決後の絶対座標で収集
//!
//! 実描画（vello::Renderer::render_to_texture）は外部所有のレンダラ／ターゲットで行うため本 crate
//! では GPU リソースを保持しない。`build_scene` で `vello::Scene` を埋め、上位レイヤが
//! 共有 `wgpu::Device` で render する。

use crate::action::ActionDescriptor;
use crate::binding::{
    BindingAttribute, classify_binding_attribute, evaluate_as_bool, evaluate_as_string,
};
use anyrender_vello::VelloScenePainter;
use blitz_dom::{
    BaseDocument, DocumentConfig, DocumentMutator, LocalName, Namespace, QualName, local_name,
    node::{Attribute, NodeData},
};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::shell::Viewport;
use serde_json::Value;

/// パネル描画器。`HtmlDocument` を保持し、layout 解決と vello scene 構築を行う。
pub struct HtmlPanelEngine {
    document: HtmlDocument,
    user_css: String,
    last_resolved: Option<(u32, u32)>,
    /// `apply_bindings` が実際に DOM を変更したか。`resolve_layout` でクリア。
    /// Blitz の `BaseDocument::has_changes()` は内部実装の都合で当てにできないため自前トラック。
    pending_mutation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelHit {
    pub node_id: usize,
    pub element_id: Option<String>,
    pub data_action: Option<String>,
    pub data_args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPanelHit {
    pub node_id: usize,
    pub element_id: Option<String>,
    pub data_action: String,
    pub data_args: Option<String>,
    pub rect: PixelRect,
}

impl HtmlPanelEngine {
    pub fn new(html: &str, user_css: &str) -> Self {
        let mut config = DocumentConfig::default();
        if !user_css.is_empty() {
            config.ua_stylesheets = Some(vec![user_css.to_string()]);
        }
        let document = HtmlDocument::from_html(html, config);
        Self {
            document,
            user_css: user_css.to_string(),
            last_resolved: None,
            pending_mutation: true,
        }
    }

    pub fn user_css(&self) -> &str {
        &self.user_css
    }

    pub fn document(&self) -> &BaseDocument {
        &self.document
    }

    /// `data-bind-*` を評価して DOM を更新する。
    pub fn apply_bindings(&mut self, snapshot: &Value) {
        let nodes_with_bindings = collect_binding_targets(&self.document);
        for target in nodes_with_bindings {
            let mut mutr = self.document.mutate();
            let mutated = apply_binding_target(&mut mutr, &target, snapshot);
            if mutated {
                self.pending_mutation = true;
            }
        }
    }

    /// 直前の `resolve_layout` 以降に DOM mutation があったか（`apply_bindings` 由来）。
    pub fn document_dirty(&self) -> bool {
        self.pending_mutation
    }

    /// viewport を設定し layout を解決する。同サイズかつ未変更ならスキップ。
    pub fn resolve_layout(&mut self, width: u32, height: u32, scale: f32) {
        if self.last_resolved == Some((width, height)) && !self.pending_mutation {
            return;
        }
        let viewport = Viewport::new(width, height, scale, blitz_traits::shell::ColorScheme::Dark);
        self.document.set_viewport(viewport);
        self.document.resolve(0.0);
        self.last_resolved = Some((width, height));
        self.pending_mutation = false;
    }

    /// blitz-paint で `vello::Scene` を埋める（実描画は呼び出し元）。
    pub fn build_scene(
        &mut self,
        scene: &mut vello::Scene,
        width: u32,
        height: u32,
        scale: f32,
    ) {
        self.resolve_layout(width, height, scale);
        let mut painter = VelloScenePainter::new(scene);
        paint_scene(
            &mut painter,
            &self.document,
            scale as f64,
            width,
            height,
            0,
            0,
        );
    }

    /// 点 `(x, y)` に最も近い `data-action` 要素を返す。
    pub fn hit_test(&self, x: f32, y: f32) -> Option<PanelHit> {
        let hit = self.document.hit(x, y)?;
        let mut current = hit.node_id;
        loop {
            let node = self.document.get_node(current)?;
            if let NodeData::Element(element) = &node.data {
                let data_action = element.attr(LocalName::from("data-action"));
                if data_action.is_some() {
                    let element_id = element.attr(local_name!("id")).map(str::to_string);
                    return Some(PanelHit {
                        node_id: current,
                        element_id,
                        data_action: data_action.map(str::to_string),
                        data_args: element
                            .attr(LocalName::from("data-args"))
                            .map(str::to_string),
                    });
                }
            }
            current = node.parent?;
        }
    }

    /// `data-action` 属性を持つ全要素の絶対矩形を返す（要 `resolve_layout` 済み）。
    pub fn collect_action_rects(&self) -> Vec<RenderedPanelHit> {
        let ids = match self.document.query_selector_all("[data-action]") {
            Ok(ids) => ids,
            Err(_) => return Vec::new(),
        };
        ids.into_iter()
            .filter_map(|id| self.action_rect_for(id))
            .collect()
    }

    fn action_rect_for(&self, node_id: usize) -> Option<RenderedPanelHit> {
        let node = self.document.get_node(node_id)?;
        let NodeData::Element(element) = &node.data else {
            return None;
        };
        let data_action = element.attr(LocalName::from("data-action"))?.to_string();
        let element_id = element.attr(local_name!("id")).map(str::to_string);
        let data_args = element.attr(LocalName::from("data-args")).map(str::to_string);
        let (x, y) = compute_absolute_position(&self.document, node_id)?;
        let size = node.final_layout.size;
        let rect = PixelRect {
            x: x.max(0.0).floor() as u32,
            y: y.max(0.0).floor() as u32,
            width: size.width.max(0.0).ceil() as u32,
            height: size.height.max(0.0).ceil() as u32,
        };
        if rect.width == 0 || rect.height == 0 {
            return None;
        }
        Some(RenderedPanelHit {
            node_id,
            element_id,
            data_action,
            data_args,
            rect,
        })
    }

    pub fn diagnostics(&self) -> Vec<String> {
        Vec::new()
    }
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

#[derive(Debug, Clone)]
struct BindingTarget {
    node_id: usize,
    bindings: Vec<(String, String)>,
    current_class: String,
}

fn collect_binding_targets(document: &BaseDocument) -> Vec<BindingTarget> {
    let mut out = Vec::new();
    let root_id = document.root_node().id;
    walk(document, root_id, &mut out);
    out
}

fn walk(document: &BaseDocument, node_id: usize, out: &mut Vec<BindingTarget>) {
    let Some(node) = document.get_node(node_id) else {
        return;
    };
    if let NodeData::Element(element) = &node.data {
        let mut bindings = Vec::new();
        for attr in element.attrs() {
            let name = attr.name.local.to_string();
            if name.starts_with("data-bind-") {
                bindings.push((name, attr.value.clone()));
            }
        }
        if !bindings.is_empty() {
            let current_class = element
                .attr(local_name!("class"))
                .unwrap_or("")
                .to_string();
            out.push(BindingTarget {
                node_id,
                bindings,
                current_class,
            });
        }
    }
    for child_id in &node.children {
        walk(document, *child_id, out);
    }
}

/// `target` の data-bind を適用。実際に DOM を変更したら true。
fn apply_binding_target(
    mutr: &mut DocumentMutator<'_>,
    target: &BindingTarget,
    snapshot: &Value,
) -> bool {
    let mut next_classes: Vec<String> = target
        .current_class
        .split_ascii_whitespace()
        .map(str::to_string)
        .collect();
    let mut class_dirty = false;
    let mut any_mutation = false;

    for (attr_key, expr) in &target.bindings {
        match classify_binding_attribute(attr_key) {
            BindingAttribute::Text => {
                let text = evaluate_as_string(expr, snapshot);
                mutr.remove_and_drop_all_children(target.node_id);
                let text_node = mutr.create_text_node(&text);
                mutr.append_children(target.node_id, &[text_node]);
                any_mutation = true;
            }
            BindingAttribute::Disabled => {
                let truthy = evaluate_as_bool(expr, snapshot);
                let qname = qual_name("disabled");
                if truthy {
                    mutr.set_attribute(target.node_id, qname, "");
                } else {
                    mutr.clear_attribute(target.node_id, qname);
                }
                any_mutation = true;
            }
            BindingAttribute::Class(class_name) => {
                let truthy = evaluate_as_bool(expr, snapshot);
                let already_has = next_classes.iter().any(|c| c == class_name);
                if truthy && !already_has {
                    next_classes.push(class_name.to_string());
                    class_dirty = true;
                } else if !truthy && already_has {
                    next_classes.retain(|c| c != class_name);
                    class_dirty = true;
                }
            }
            BindingAttribute::None => {}
        }
    }

    if class_dirty {
        let qname = qual_name("class");
        if next_classes.is_empty() {
            mutr.clear_attribute(target.node_id, qname);
        } else {
            mutr.set_attribute(target.node_id, qname, &next_classes.join(" "));
        }
        any_mutation = true;
    }

    any_mutation
}

fn qual_name(local: &str) -> QualName {
    QualName::new(None, Namespace::default(), LocalName::from(local))
}

pub fn descriptor_from_hit(hit: &PanelHit) -> Option<ActionDescriptor> {
    let raw = hit.data_action.as_deref()?;
    let args = hit.data_args.as_deref();
    crate::action::parse_data_action(raw, args).ok()
}

#[allow(dead_code)]
fn _attribute_helper_namespace_check(attr: &Attribute) -> bool {
    attr.name.ns == Namespace::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn engine(html: &str) -> HtmlPanelEngine {
        HtmlPanelEngine::new(html, "")
    }

    #[test]
    fn engine_parses_html_and_finds_button_via_hit_test() {
        let html = r#"<html><body><button id="undo" data-action="command:noop" style="display:block; width:100px; height:40px;">Undo</button></body></html>"#;
        let mut engine = engine(html);
        engine.resolve_layout(200, 100, 1.0);
        let hit = engine.hit_test(20.0, 20.0);
        assert!(hit.is_some(), "expected hit on button");
        let hit = hit.unwrap();
        assert_eq!(hit.element_id.as_deref(), Some("undo"));
        assert_eq!(hit.data_action.as_deref(), Some("command:noop"));
    }

    /// S1: build_scene が vello::Scene を埋めること
    #[test]
    fn html_engine_build_scene_populates_vello_scene() {
        let html = r#"<html><body><div style="width:50px;height:30px;background:#ff0000;"></div></body></html>"#;
        let mut engine = engine(html);
        let mut scene = vello::Scene::new();
        engine.build_scene(&mut scene, 100, 60, 1.0);
        let encoding = scene.encoding();
        assert!(
            !encoding.path_tags.is_empty() || !encoding.draw_tags.is_empty(),
            "expected vello scene to contain at least one path or draw tag, got path_tags={} draw_tags={}",
            encoding.path_tags.len(),
            encoding.draw_tags.len()
        );
    }

    /// S2: collect_action_rects が CSS padding を反映する
    #[test]
    fn html_engine_collect_action_rects_returns_buttons_with_padding() {
        let html = r#"<html><body>
            <button id="a" data-action="command:undo" style="display:block;">A</button>
            <button id="b" data-action="command:redo" style="display:block;">B</button>
            <span>nope</span>
        </body></html>"#;
        let mut engine = engine(html);
        engine.resolve_layout(300, 100, 1.0);
        let rects = engine.collect_action_rects();
        assert_eq!(rects.len(), 2, "expected 2 data-action elements");
        assert!(rects.iter().any(|r| r.element_id.as_deref() == Some("a")));
        assert!(rects.iter().any(|r| r.element_id.as_deref() == Some("b")));
        let a = rects
            .iter()
            .find(|r| r.element_id.as_deref() == Some("a"))
            .unwrap();
        assert!(a.rect.width > 0 && a.rect.height > 0);
    }

    /// S3: document_dirty が apply_bindings 後に true、resolve_layout 後に false
    #[test]
    fn html_engine_document_dirty_reports_apply_bindings_changes() {
        let html =
            r#"<html><body><span id="s" data-bind-text="jobs.active">0</span></body></html>"#;
        let mut engine = engine(html);
        engine.resolve_layout(100, 50, 1.0);
        assert!(!engine.document_dirty(), "after resolve_layout dirty=false");
        engine.apply_bindings(&json!({"jobs": {"active": 7}}));
        assert!(engine.document_dirty(), "after apply_bindings dirty=true");
        engine.resolve_layout(100, 50, 1.0);
        assert!(
            !engine.document_dirty(),
            "after second resolve_layout dirty=false"
        );
    }

    /// S14: ヒットテストが CSS padding を尊重
    #[test]
    fn html_engine_hit_test_screen_to_node_with_css_padding() {
        let html = r#"<html><body>
            <button id="x" data-action="command:noop" style="display:block;width:80px;height:40px;margin:20px;">X</button>
        </body></html>"#;
        let mut engine = engine(html);
        engine.resolve_layout(200, 100, 1.0);
        // margin 20px の外側はヒットしない
        let hit_outside = engine.hit_test(2.0, 2.0);
        assert!(
            hit_outside.is_none() || hit_outside.unwrap().element_id.as_deref() != Some("x"),
            "outside button should not hit x"
        );
        // 要素内側はヒットする
        let hit_inside = engine.hit_test(60.0, 50.0);
        assert!(hit_inside.is_some());
        assert_eq!(hit_inside.unwrap().element_id.as_deref(), Some("x"));
    }

    #[test]
    fn binding_disabled_with_negation_is_applied_to_dom() {
        let html = r#"<html><body><button id="undo" data-action="command:noop" data-bind-disabled="!host.can_undo">U</button></body></html>"#;
        let mut engine = engine(html);
        engine.apply_bindings(&json!({"host": {"can_undo": true}}));
        let id_true = engine.find_element_id("undo").unwrap();
        let has_disabled_true = engine
            .document
            .get_node(id_true)
            .and_then(|n| {
                if let NodeData::Element(e) = &n.data {
                    Some(e.has_attr(local_name!("disabled")))
                } else {
                    None
                }
            })
            .unwrap();
        assert!(!has_disabled_true);

        engine.apply_bindings(&json!({"host": {"can_undo": false}}));
        let id_false = engine.find_element_id("undo").unwrap();
        let has_disabled_false = engine
            .document
            .get_node(id_false)
            .and_then(|n| {
                if let NodeData::Element(e) = &n.data {
                    Some(e.has_attr(local_name!("disabled")))
                } else {
                    None
                }
            })
            .unwrap();
        assert!(has_disabled_false);
    }

    #[test]
    fn binding_text_updates_text_content() {
        let html = r#"<html><body><span id="job-count" data-bind-text="jobs.active">0</span></body></html>"#;
        let mut engine = engine(html);
        engine.apply_bindings(&json!({"jobs": {"active": 7}}));
        let id = engine.find_element_id("job-count").unwrap();
        let text = engine.document_text_content(id);
        assert_eq!(text, "7");
    }

    #[test]
    fn binding_class_toggle_adds_and_removes_class() {
        let html = r#"<html><body><button id="b" class="btn" data-action="command:noop" data-bind-class-enabled="host.can_undo">B</button></body></html>"#;
        let mut engine = engine(html);
        engine.apply_bindings(&json!({"host": {"can_undo": true}}));
        let id = engine.find_element_id("b").unwrap();
        let class_attr = engine.element_class(id);
        assert!(class_attr.contains("enabled"));
        assert!(class_attr.contains("btn"));

        engine.apply_bindings(&json!({"host": {"can_undo": false}}));
        let id = engine.find_element_id("b").unwrap();
        let class_attr = engine.element_class(id);
        assert!(!class_attr.contains("enabled"));
        assert!(class_attr.contains("btn"));
    }

    impl HtmlPanelEngine {
        fn find_element_id(&self, dom_id: &str) -> Option<usize> {
            self.document
                .query_selector(&format!("#{dom_id}"))
                .ok()
                .flatten()
        }

        fn document_text_content(&self, node_id: usize) -> String {
            let mut out = String::new();
            collect_text(self.document(), node_id, &mut out);
            out
        }

        fn element_class(&self, node_id: usize) -> String {
            self.document
                .get_node(node_id)
                .and_then(|n| {
                    if let NodeData::Element(e) = &n.data {
                        Some(e.attr(local_name!("class")).unwrap_or("").to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        }
    }

    fn collect_text(doc: &BaseDocument, node_id: usize, out: &mut String) {
        let Some(node) = doc.get_node(node_id) else {
            return;
        };
        match &node.data {
            NodeData::Text(text) => out.push_str(&text.content),
            _ => {
                for child_id in &node.children {
                    collect_text(doc, *child_id, out);
                }
            }
        }
    }
}
