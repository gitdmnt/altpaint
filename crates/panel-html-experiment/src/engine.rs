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
    BaseDocument, DocumentConfig, DocumentMutator, EventDriver, LocalName, Namespace,
    NoopEventHandler, QualName, local_name,
    node::{Attribute, NodeData},
};
use blitz_html::HtmlDocument;
use blitz_paint::paint_scene;
use blitz_traits::events::UiEvent;
use blitz_traits::shell::Viewport;
use serde_json::Value;

/// パネル描画器。`HtmlDocument` を保持し、layout 解決と vello scene 構築を行う。
pub struct HtmlPanelEngine {
    document: HtmlDocument,
    user_css: String,
    /// 直近の `replace_document` で渡された HTML 文字列。同一なら no-op。
    last_html: Option<String>,
    last_resolved: Option<(u32, u32)>,
    /// `apply_bindings` が実際に DOM を変更したか。`resolve_layout` でクリア。
    /// Blitz の `BaseDocument::has_changes()` は内部実装の都合で当てにできないため自前トラック。
    pending_mutation: bool,
    /// パネル単位の権威サイズ (chrome を含まない HTML 本体の幅・高さ)。
    /// `on_load` で初期化、`on_render` で intrinsic 結果に応じて更新。
    measured_size: (u32, u32),
    /// 次フレームで `resolve_layout` が必要か。
    /// `on_host_snapshot` / `on_input` / 初回ロードで true を立てる。
    layout_dirty: bool,
    /// 次フレームで実描画が必要か。サイズ変化 / DOM mutation / 初回ロードで true。
    render_dirty: bool,
    /// 直近の `on_render` で measured_size が変化したか。
    /// 上位レイヤが `take_size_change` で吸い取って永続化に流す。
    pending_size_change: bool,
    /// パネルの GPU レンダーターゲット。`on_render` 内でサイズに応じて再生成。
    gpu_target: Option<crate::gpu::PanelGpuTarget>,
}

/// `on_render` の結果。dirty なら `Rendered`、再利用なら `Skipped`。
pub enum RenderOutcome<'a> {
    Rendered(&'a crate::gpu::PanelGpuTarget),
    Skipped(&'a crate::gpu::PanelGpuTarget),
}

impl<'a> RenderOutcome<'a> {
    pub fn target(&self) -> &crate::gpu::PanelGpuTarget {
        match self {
            RenderOutcome::Rendered(t) | RenderOutcome::Skipped(t) => t,
        }
    }
    pub fn is_rendered(&self) -> bool {
        matches!(self, RenderOutcome::Rendered(_))
    }
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
            last_html: Some(html.to_string()),
            last_resolved: None,
            pending_mutation: true,
            measured_size: (1, 1),
            layout_dirty: true,
            render_dirty: true,
            pending_size_change: false,
            gpu_target: None,
        }
    }

    /// パネルロード時に呼ぶ。永続化されたサイズがあれば復元、無ければ intrinsic 測定で初期化。
    /// 戻り値後の `measured_size()` で初期サイズを取得できる。
    pub fn on_load(&mut self, restored_size: Option<(u32, u32)>) {
        let (w, h) = match restored_size {
            Some((w, h)) => (w.max(1), h.max(1)),
            None => self.measure_intrinsic(8192),
        };
        self.measured_size = (w, h);
        self.layout_dirty = true;
        self.render_dirty = true;
        self.pending_size_change = false;
    }

    /// 現在の権威サイズ (HTML 本体の width, height)。
    pub fn measured_size(&self) -> (u32, u32) {
        self.measured_size
    }

    /// 次フレームで resolve が必要か。
    pub fn layout_dirty(&self) -> bool {
        self.layout_dirty
    }

    /// 次フレームで実描画が必要か。
    pub fn render_dirty(&self) -> bool {
        self.render_dirty
    }

    /// `on_render` でサイズが変化した直後 true。`take_size_change` で吸い取られる。
    pub fn pending_size_change(&self) -> bool {
        self.pending_size_change
    }

    /// pending な size 変化フラグを取り出してクリアする。
    /// 上位は変化があった panel_id ごとに workspace_layout に書き戻して永続化する。
    pub fn take_size_change(&mut self) -> Option<(u32, u32)> {
        if self.pending_size_change {
            self.pending_size_change = false;
            Some(self.measured_size)
        } else {
            None
        }
    }

    /// 巨大 viewport で resolve し、body のコンテンツ占有サイズを返す（intrinsic 測定）。
    ///
    /// 同一 document を変更するため、呼び出し側は次フレームで自身に必要な viewport で
    /// 再 resolve する想定（`on_render` がそれを行う）。
    pub fn measure_intrinsic(&mut self, max_width: u32) -> (u32, u32) {
        let max_w = max_width.clamp(1, 8192);
        let viewport = Viewport::new(max_w, 8192, 1.0, blitz_traits::shell::ColorScheme::Dark);
        self.document.set_viewport(viewport);
        self.document.resolve(0.0);
        // 内部 cache を無効化（次回 resolve_layout で必ず再計算させる）
        self.last_resolved = None;
        self.layout_dirty = true;
        let (w, h) = compute_intrinsic_from_body(&self.document).unwrap_or((1, 1));
        (w.min(max_w), h.min(8192))
    }

    /// `apply_bindings` の新名。host snapshot を DOM に流し、変化があれば dirty を立てる。
    pub fn on_host_snapshot(&mut self, snapshot: &Value) {
        self.apply_bindings(snapshot);
        if self.pending_mutation {
            self.layout_dirty = true;
            self.render_dirty = true;
        }
    }

    /// 現在の GPU target への参照（render 後に外部が view を作るため）。
    pub fn gpu_target(&self) -> Option<&crate::gpu::PanelGpuTarget> {
        self.gpu_target.as_ref()
    }

    /// パネルを GPU テクスチャに描画する（責務集約）。
    ///
    /// 動作：
    /// 1. layout_dirty なら `resolve_layout(measured_w, body_h)` → root content size を再測定
    ///    → measured_size が変われば `pending_size_change` を立て GPU target を再作成
    /// 2. viewport (画面側) でクランプ：`render_w = min(measured_w, viewport_w)` 等
    /// 3. render_dirty なら scene 構築 + chrome 描画 + render_to_texture
    ///
    /// 戻り値: `RenderOutcome::Rendered(target)` か `Skipped(target)`。
    /// `target` は `gpu_target()` でも取得可能。
    #[allow(clippy::too_many_arguments)]
    pub fn on_render<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut vello::Renderer,
        scene_buf: &mut vello::Scene,
        viewport: (u32, u32),
        scale: f32,
        chrome_height: u32,
    ) -> RenderOutcome<'a> {
        // viewport クランプ：画面サイズを超えないように measured_size を調整
        let (vp_w, vp_h) = (viewport.0.max(1), viewport.1.max(chrome_height + 1));
        let (mw, mh) = self.measured_size;
        let clamped_w = mw.min(vp_w);
        let clamped_h = mh.min(vp_h);
        if clamped_w != mw || clamped_h != mh {
            self.measured_size = (clamped_w, clamped_h);
            self.pending_size_change = true;
            self.layout_dirty = true;
            self.render_dirty = true;
        }

        // body 部分の高さ (chrome を除く)
        let body_h = self.measured_size.1.saturating_sub(chrome_height).max(1);
        let panel_w = self.measured_size.0.max(1);

        // layout_dirty なら resolve → content size を再測定して measured を更新
        if self.layout_dirty {
            self.resolve_layout(panel_w, body_h, scale);
            // resolve 後、body コンテンツの実サイズを取って measured_size と比較
            if let Some((new_w, new_h)) = self.compute_body_content_size() {
                let new_panel_h = new_h.saturating_add(chrome_height).max(chrome_height + 1);
                let final_w = new_w.max(1).min(vp_w);
                let final_h = new_panel_h.min(vp_h);
                if (final_w, final_h) != self.measured_size {
                    self.measured_size = (final_w, final_h);
                    self.pending_size_change = true;
                    self.render_dirty = true;
                    // resolve サイズが変わったので invalidate して次回再 resolve
                    self.last_resolved = None;
                    // 新サイズで一度 resolve しなおして scene 構築に備える
                    let new_body_h = final_h.saturating_sub(chrome_height).max(1);
                    self.resolve_layout(final_w, new_body_h, scale);
                }
            }
            self.layout_dirty = false;
        }

        // GPU target サイズを measured_size に合わせる
        let target_size_changed = self
            .gpu_target
            .as_ref()
            .map(|t| t.width != self.measured_size.0 || t.height != self.measured_size.1)
            .unwrap_or(true);
        if target_size_changed {
            self.gpu_target = Some(crate::gpu::PanelGpuTarget::create(
                device,
                self.measured_size.0,
                self.measured_size.1,
            ));
            self.render_dirty = true;
        }

        if !self.render_dirty {
            return RenderOutcome::Skipped(self.gpu_target.as_ref().expect("target ensured"));
        }

        // scene 構築 + chrome 描画
        scene_buf.reset();
        let body_h_now = self.measured_size.1.saturating_sub(chrome_height).max(1);
        self.build_scene_with_offset(
            scene_buf,
            self.measured_size.0,
            body_h_now,
            scale,
            0,
            chrome_height,
        );
        if chrome_height > 0 {
            paint_chrome_rect(scene_buf, self.measured_size.0, chrome_height);
        }

        let target = self.gpu_target.as_ref().expect("target ensured");
        let view = target.create_render_view();
        renderer
            .render_to_texture(
                device,
                queue,
                scene_buf,
                &view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width: self.measured_size.0,
                    height: self.measured_size.1,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .expect("vello render_to_texture failed");

        self.render_dirty = false;
        RenderOutcome::Rendered(self.gpu_target.as_ref().expect("target ensured"))
    }

    /// `<body>` 直下のコンテンツ占有サイズを取得する。
    fn compute_body_content_size(&self) -> Option<(u32, u32)> {
        compute_intrinsic_from_body(&self.document)
    }

    /// UiEvent (PointerDown/Up/Move 等) を Blitz に流す。
    /// `:hover` / `<details>` 開閉 / `<button>` のアクティブ状態などはこの経路でのみ反映される。
    /// 戻り値: layout_dirty を立てた場合 true（呼び出し側がフレーム再描画判断に使う）。
    pub fn on_input(&mut self, event: UiEvent) -> bool {
        let mut driver = EventDriver::new(&mut self.document, NoopEventHandler);
        driver.handle_ui_event(event);
        // pointer / key 系イベントは hover 状態 / focus / details 開閉 など
        // レイアウトが変わる可能性が常にあるため無条件で dirty を立てる。
        // damage を観測してから判断する API は Blitz 0.3.0-alpha では public でないため
        // 楽観的に再 resolve させる。
        self.layout_dirty = true;
        self.render_dirty = true;
        true
    }

    pub fn user_css(&self) -> &str {
        &self.user_css
    }

    pub fn document(&self) -> &BaseDocument {
        &self.document
    }

    /// HTML / CSS を差し替えて document を再構築する。
    ///
    /// - 同一 `(html, css)` ならスキップ (idle frame 最適化、`render_dirty` も立てない)。
    /// - 異なる場合は新しい `HtmlDocument` を構築し、開いていた `<details>` の状態を
    ///   element id で再適用する (DSL state には details の open/close が無いため)。
    /// - `gpu_target` は維持する (size 不変ならそのまま使える)。
    /// - フォーカスは現状維持できないため呼び出し側で `preserve_focus` を利用すること。
    pub fn replace_document(&mut self, html: &str, css: &str) {
        if self.last_html.as_deref() == Some(html) && self.user_css == css {
            return;
        }
        let opened_details = self.collect_open_details_ids();
        let mut config = blitz_dom::DocumentConfig::default();
        if !css.is_empty() {
            config.ua_stylesheets = Some(vec![css.to_string()]);
        }
        self.document = HtmlDocument::from_html(html, config);
        self.user_css = css.to_string();
        self.last_html = Some(html.to_string());
        self.last_resolved = None;
        self.pending_mutation = true;
        self.layout_dirty = true;
        self.render_dirty = true;

        if !opened_details.is_empty() {
            self.reapply_open_details(&opened_details);
        }
    }

    fn collect_open_details_ids(&self) -> Vec<String> {
        let Ok(ids) = self.document.query_selector_all("details[open]") else {
            return Vec::new();
        };
        ids.into_iter()
            .filter_map(|node_id| {
                let node = self.document.get_node(node_id)?;
                let NodeData::Element(element) = &node.data else {
                    return None;
                };
                element
                    .attr(LocalName::from("data-altp-id"))
                    .or_else(|| element.attr(local_name!("id")))
                    .map(str::to_string)
            })
            .collect()
    }

    fn reapply_open_details(&mut self, ids: &[String]) {
        let Ok(all_details) = self.document.query_selector_all("details") else {
            return;
        };
        for node_id in all_details {
            let Some(node) = self.document.get_node(node_id) else {
                continue;
            };
            let NodeData::Element(element) = &node.data else {
                continue;
            };
            let identity = element
                .attr(LocalName::from("data-altp-id"))
                .or_else(|| element.attr(local_name!("id")))
                .map(str::to_string);
            let Some(identity) = identity else { continue };
            if ids.iter().any(|id| id == &identity) {
                continue; // すでに open 属性付き翻訳結果なら無視
            }
            // 翻訳器は常に `<details open>` を出力するため、ids リストに無いものは
            // 「ユーザーが閉じた」状態。open 属性を取り除く。
            let mut mutator = self.document.mutate();
            mutator.clear_attribute(node_id, qual_name("open"));
            self.pending_mutation = true;
        }
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
        self.build_scene_with_offset(scene, width, height, scale, 0, 0);
    }

    /// `build_scene` の offset 版。HTML 本体を `(x_offset, y_offset)` ピクセル分ずらして描画する。
    /// ホスト描画タイトルバーを上に重ねるためのオフセット指定に使う。
    pub fn build_scene_with_offset(
        &mut self,
        scene: &mut vello::Scene,
        width: u32,
        height: u32,
        scale: f32,
        x_offset: u32,
        y_offset: u32,
    ) {
        self.resolve_layout(width, height, scale);
        let mut painter = VelloScenePainter::new(scene);
        paint_scene(
            &mut painter,
            &self.document,
            scale as f64,
            width,
            height,
            x_offset,
            y_offset,
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

/// `<body>` 要素のコンテンツ占有サイズを返す。
///
/// 取得方法: body の `final_layout.content_size` を使う。これは taffy が計算した
/// 「コンテンツ自体が必要としたサイズ」で、block/inline どちらの content layout でも反映される。
/// content_size が 0 の場合（body 自体が空）は (1, 1)。
/// body が見つからない場合は None。
fn compute_intrinsic_from_body(document: &BaseDocument) -> Option<(u32, u32)> {
    let body_id = document.query_selector("body").ok().flatten()?;
    let body_node = document.get_node(body_id)?;
    let content_size = body_node.final_layout.content_size;
    let w = content_size.width.max(1.0).ceil() as u32;
    let h = content_size.height.max(1.0).ceil() as u32;
    Some((w, h))
}

/// HTML パネル上端のタイトルバー (chrome) を vello シーンに矩形で描画する。
/// テキスト描画は将来追加。Plugin 側から Engine に移管された描画ロジック。
fn paint_chrome_rect(scene: &mut vello::Scene, width: u32, chrome_height: u32) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::{Color, Fill};
    let rect = Rect::new(0.0, 0.0, width as f64, chrome_height as f64);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(40, 60, 90, 255),
        None,
        &rect,
    );
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

    /// Phase 1.1: measure_intrinsic が指定 max_width で root 自然サイズを返す
    #[test]
    fn measure_intrinsic_returns_root_size_for_simple_html() {
        let html = r#"<html><body style="margin:0"><div style="width:120px;height:40px;background:red;"></div></body></html>"#;
        let mut engine = engine(html);
        let (w, h) = engine.measure_intrinsic(8192);
        assert!(
            (w as i32 - 120).abs() <= 2,
            "expected width ≈120, got {w}"
        );
        assert!(
            (h as i32 - 40).abs() <= 2,
            "expected height ≈40, got {h}"
        );
    }

    /// Phase 1.3: on_load(Some) は measured_size をその値で初期化する
    #[test]
    fn on_load_with_persisted_size_uses_it() {
        let html = r#"<html><body><div style="width:80px;height:30px;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load(Some((400, 300)));
        assert_eq!(engine.measured_size(), (400, 300));
    }

    /// Phase 1.4: on_load(None) は intrinsic 測定で初期化する
    #[test]
    fn on_load_without_persisted_size_uses_intrinsic() {
        let html = r#"<html><body style="margin:0"><div style="width:150px;height:60px;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load(None);
        let (w, h) = engine.measured_size();
        assert!(
            (w as i32 - 150).abs() <= 4,
            "expected ≈150 width, got {w}"
        );
        assert!(
            (h as i32 - 60).abs() <= 4,
            "expected ≈60 height, got {h}"
        );
    }

    /// Phase 1.7: on_input(PointerMove) で hover state が更新され dirty が立つ
    #[test]
    fn on_input_pointer_move_updates_hover_and_marks_dirty() {
        let html = r#"<html><body style="margin:0"><button id="b" data-action="command:noop" style="display:block;width:80px;height:40px;">B</button></body></html>"#;
        let mut engine = engine(html);
        engine.on_load(None);
        // 一旦 dirty フラグをクリアした想定で on_input が dirty を立てるかをテストする
        engine.clear_dirty_for_test();
        let event = blitz_traits::events::UiEvent::PointerMove(test_pointer_event(40.0, 20.0));
        let changed = engine.on_input(event);
        assert!(changed, "PointerMove should mark layout dirty");
        assert!(engine.layout_dirty(), "layout_dirty after on_input");
    }

    /// Phase 1.6: on_host_snapshot は DOM mutation 時に layout_dirty / render_dirty を立てる
    #[test]
    fn on_host_snapshot_marks_dirty_when_dom_mutates() {
        let html = r#"<html><body><span id="s" data-bind-text="jobs.active">0</span></body></html>"#;
        let mut engine = engine(html);
        engine.on_load(None);
        // on_load 直後は dirty が両方とも立っている (初回 render 必須)。
        // on_host_snapshot 後も立っていることを確認する。
        engine.on_host_snapshot(&json!({"jobs": {"active": 7}}));
        assert!(engine.layout_dirty(), "after on_host_snapshot layout_dirty=true");
        assert!(engine.render_dirty(), "after on_host_snapshot render_dirty=true");
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

    /// D1: ASCII テキストが vello::Scene に glyph run として積まれることを確認する。
    /// vello は glyph を `encoding.resources.glyph_runs` に格納するため、その len を見る。
    /// ここが落ちる場合は paint_scene が glyph 描画コマンドを scene に積んでいない（原因 A）。
    #[test]
    fn ascii_text_emits_glyph_run_in_scene() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let mut engine = engine(html);
        let mut scene = vello::Scene::new();
        engine.build_scene(&mut scene, 200, 80, 1.0);
        let glyph_runs = scene.encoding().resources.glyph_runs.len();
        assert!(
            glyph_runs > 0,
            "expected vello scene to contain at least one glyph run, got {glyph_runs}",
        );
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

    #[test]
    fn replace_document_swaps_html_and_marks_dirty() {
        let mut engine = engine(r#"<html><body><span id="a">A</span></body></html>"#);
        engine.on_load(None);
        engine.clear_dirty_for_test();
        engine.replace_document(
            r#"<html><body><span id="b">B</span></body></html>"#,
            "",
        );
        assert!(engine.render_dirty(), "after replace render_dirty=true");
        assert!(engine.layout_dirty(), "after replace layout_dirty=true");
        assert!(engine.find_element_id("b").is_some(), "new element id present");
        assert!(engine.find_element_id("a").is_none(), "old element gone");
    }

    #[test]
    fn replace_document_with_same_html_is_no_op() {
        let html = r#"<html><body><span id="a">A</span></body></html>"#;
        let mut engine = engine(html);
        engine.on_load(None);
        engine.clear_dirty_for_test();
        engine.replace_document(html, "");
        assert!(!engine.render_dirty(), "no-op when html unchanged");
        assert!(!engine.layout_dirty(), "no-op when html unchanged");
    }

    #[test]
    fn replace_document_keeps_open_details_when_id_was_open() {
        // 初期: 開いている details が 1 つ
        let initial = r#"<html><body><details open data-altp-id="s"><summary>S</summary>x</details></body></html>"#;
        let mut engine = engine(initial);
        engine.on_load(None);
        // 翻訳結果も open: そのまま open を維持すべき
        let next = r#"<html><body><details open data-altp-id="s"><summary>S</summary>y</details></body></html>"#;
        engine.replace_document(next, "");
        let details_id = engine.find_element_id_by_altp("s").expect("details exists");
        assert!(
            engine.element_has_attribute(details_id, "open"),
            "previously open details should remain open after replace"
        );
    }

    fn test_pointer_event(x: f32, y: f32) -> blitz_traits::events::BlitzPointerEvent {
        use blitz_traits::events::{
            BlitzPointerEvent, BlitzPointerId, MouseEventButton, MouseEventButtons,
            PointerCoords, PointerDetails,
        };
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                client_x: x,
                client_y: y,
                screen_x: x,
                screen_y: y,
            },
            button: MouseEventButton::Main,
            buttons: MouseEventButtons::empty(),
            mods: keyboard_types::Modifiers::empty(),
            details: PointerDetails::default(),
        }
    }

    impl HtmlPanelEngine {
        /// Phase 1.7 テスト用: dirty フラグを手動でクリアする
        pub(crate) fn clear_dirty_for_test(&mut self) {
            self.layout_dirty = false;
            self.render_dirty = false;
        }

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

        pub(crate) fn find_element_id_by_altp(&self, identity: &str) -> Option<usize> {
            let ids = self.document.query_selector_all("[data-altp-id]").ok()?;
            for node_id in ids {
                let node = self.document.get_node(node_id)?;
                let NodeData::Element(element) = &node.data else { continue };
                if element.attr(LocalName::from("data-altp-id")) == Some(identity) {
                    return Some(node_id);
                }
            }
            None
        }

        pub(crate) fn element_has_attribute(&self, node_id: usize, attr: &str) -> bool {
            let Some(node) = self.document.get_node(node_id) else { return false };
            let NodeData::Element(element) = &node.data else { return false };
            element.attr(LocalName::from(attr)).is_some()
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
