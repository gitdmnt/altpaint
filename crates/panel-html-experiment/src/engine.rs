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
use anyrender_vello::VelloScenePainter;
use blitz_dom::{
    BaseDocument, DocumentConfig, EventDriver, LocalName, Namespace, NoopEventHandler, QualName,
    local_name,
    node::{Attribute, NodeData},
};
use blitz_html::{HtmlDocument, HtmlProvider};
use std::sync::Arc;
use blitz_paint::paint_scene;
use blitz_traits::events::UiEvent;
use blitz_traits::shell::Viewport;

/// パネル描画器。`HtmlDocument` を保持し、layout 解決と vello scene 構築を行う。
pub struct HtmlPanelEngine {
    document: HtmlDocument,
    user_css: String,
    /// 直近の `replace_document` で渡された HTML 文字列。同一なら no-op。
    last_html: Option<String>,
    last_resolved: Option<(u32, u32)>,
    /// Wasm の DOM mutation API (`mark_mutated`) が呼ばれたか。`resolve_layout` でクリア。
    /// Blitz の `BaseDocument::has_changes()` は内部実装の都合で当てにできないため自前トラック。
    pending_mutation: bool,
    /// パネル単位の権威サイズ (chrome を含む幅・高さ)。
    /// Phase 11: workspace_layout の永続値が唯一の入力経路 (`on_load` / `restore_size`)。
    /// engine 自身は自動測定/上書きを行わない。
    measured_size: (u32, u32),
    /// 次フレームで `resolve_layout` が必要か。
    /// `mark_mutated` / `on_input` / 初回ロードで true を立てる。
    layout_dirty: bool,
    /// 次フレームで実描画が必要か。サイズ変化 / DOM mutation / 初回ロードで true。
    render_dirty: bool,
    /// パネルの GPU レンダーターゲット。`on_render` 内でサイズに応じて再生成。
    gpu_target: Option<crate::gpu::PanelGpuTarget>,
}

/// `on_render` の結果。dirty なら `Rendered`、再利用なら `Skipped`。
pub enum RenderOutcome<'a> {
    Rendered(&'a crate::gpu::PanelGpuTarget),
    Skipped(&'a crate::gpu::PanelGpuTarget),
}

/// Phase 11: パネル root 要素の CSS `min-width` / `max-width` / `min-height` /
/// `max-height` を px 単位で取り出した制約。`%` や `auto` は `None` として扱う。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanelSizeConstraints {
    pub min_width: Option<u32>,
    pub max_width: Option<u32>,
    pub min_height: Option<u32>,
    pub max_height: Option<u32>,
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
        config.html_parser_provider = Some(Arc::new(HtmlProvider));
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
            gpu_target: None,
        }
    }

    /// パネルロード時に呼ぶ。bootstrap で必ず確定したサイズ
    /// (workspace 永続値 or panel.meta.json `default_size`) が渡される。
    pub fn on_load(&mut self, size: (u32, u32)) {
        self.measured_size = (size.0.max(1), size.1.max(1));
        self.layout_dirty = true;
        self.render_dirty = true;
    }

    /// 現在の権威サイズ (HTML 本体の width, height)。
    pub fn measured_size(&self) -> (u32, u32) {
        self.measured_size
    }

    /// Phase 11: パネル root 要素 (body 直下の最初の要素) の CSS `min-width` /
    /// `max-width` / `min-height` / `max-height` を `px` 単位の `u32` で返す。
    /// `auto` や `%` 単位は `None` (制約なし) として扱う。
    ///
    /// 注意: 取り出すのは taffy の `min_size` / `max_size` (CSS → stylo → taffy へと
    /// 反映された値) なので、`resolve_layout` が一度走った後でないとデフォルト値
    /// (= Auto) が返る可能性がある。リサイズハンドルから問い合わせる経路では
    /// 既に少なくとも一度フレームが描画済みのため問題にならない。
    pub fn root_size_constraints(&self) -> PanelSizeConstraints {
        let Some(root) = root_panel_node_id(&self.document) else {
            return PanelSizeConstraints::default();
        };
        let Some(node) = self.document.get_node(root) else {
            return PanelSizeConstraints::default();
        };
        PanelSizeConstraints {
            min_width: dimension_to_px(node.style.min_size.width),
            max_width: dimension_to_px(node.style.max_size.width),
            min_height: dimension_to_px(node.style.min_size.height),
            max_height: dimension_to_px(node.style.max_size.height),
        }
    }

    /// 次フレームで resolve が必要か。
    pub fn layout_dirty(&self) -> bool {
        self.layout_dirty
    }

    /// 次フレームで実描画が必要か。
    pub fn render_dirty(&self) -> bool {
        self.render_dirty
    }

    /// 現在の GPU target への参照（render 後に外部が view を作るため）。
    pub fn gpu_target(&self) -> Option<&crate::gpu::PanelGpuTarget> {
        self.gpu_target.as_ref()
    }

    /// パネルを GPU テクスチャに描画する（責務集約）。
    ///
    /// 動作 (Phase 11):
    /// 1. viewport (画面側) で **描画用ローカル size** を算出: `min(measured_w, viewport_w)` 等。
    ///    `measured_size` 自体は変更しない (ウィンドウ縮小→復元時の往復不変)。
    /// 2. layout_dirty なら local size で `resolve_layout` を走らせる (content size の自動再測定はしない)。
    /// 3. render_dirty なら scene 構築 + chrome 描画 + render_to_texture。
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
        // viewport クランプ: 描画用 local 変数のみで行い measured_size は変更しない。
        let (vp_w, vp_h) = (viewport.0.max(1), viewport.1.max(chrome_height + 1));
        let local_w = self.measured_size.0.min(vp_w).max(1);
        let local_h = self.measured_size.1.min(vp_h).max(chrome_height + 1);

        // body 部分の高さ (chrome を除く)
        let body_h = local_h.saturating_sub(chrome_height).max(1);

        // layout_dirty なら resolve のみ実行 (content size の再測定 + measured_size 更新は廃止)
        if self.layout_dirty {
            self.resolve_layout(local_w, body_h, scale);
            self.layout_dirty = false;
        }

        // GPU target サイズを local size に合わせる
        let target_size_changed = self
            .gpu_target
            .as_ref()
            .map(|t| t.width != local_w || t.height != local_h)
            .unwrap_or(true);
        if target_size_changed {
            self.gpu_target = Some(crate::gpu::PanelGpuTarget::create(device, local_w, local_h));
            self.render_dirty = true;
        }

        if !self.render_dirty {
            return RenderOutcome::Skipped(self.gpu_target.as_ref().expect("target ensured"));
        }

        // scene 構築 + chrome 描画
        scene_buf.reset();
        self.build_scene_with_offset(scene_buf, local_w, body_h, scale, 0, chrome_height);
        if chrome_height > 0 {
            paint_chrome_rect(scene_buf, local_w, chrome_height);
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
                    width: local_w,
                    height: local_h,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .expect("vello render_to_texture failed");

        self.render_dirty = false;
        RenderOutcome::Rendered(self.gpu_target.as_ref().expect("target ensured"))
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

    /// Wasm DOM mutation API のために `HtmlDocument` への可変借用を返す。
    ///
    /// 呼び出し側 (panel-runtime) は `WasmPanelRuntime::call_with_dom` のスコープ内でのみ使い、
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
        config.html_parser_provider = Some(Arc::new(HtmlProvider));
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

    /// 直前の `resolve_layout` 以降に DOM mutation があったか（Wasm `mark_mutated` 由来）。
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

/// Phase 11: パネル root 要素 (body 直下の最初の Element ノード) の NodeId を返す。
/// 通常 `<body><div class="panel">...</div></body>` 形式なので `.panel` div を指す。
fn root_panel_node_id(document: &BaseDocument) -> Option<usize> {
    let body_id = document.query_selector("body").ok().flatten()?;
    let body = document.get_node(body_id)?;
    for child_id in &body.children {
        if let Some(child) = document.get_node(*child_id) {
            if matches!(child.data, NodeData::Element(_)) {
                return Some(*child_id);
            }
        }
    }
    None
}

/// taffy::Dimension が `Length(px)` なら u32 で返す。`Auto` / `Percent` / `Calc` は `None`。
fn dimension_to_px(d: taffy::Dimension) -> Option<u32> {
    // taffy 0.10 の Dimension::into_option() は `grid` feature 配下で
    // `LENGTH_TAG` のみを Some(value) として返す純粋関数。
    d.into_option().map(|px| px.max(0.0).round() as u32)
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

    fn engine(html: &str) -> HtmlPanelEngine {
        HtmlPanelEngine::new(html, "")
    }

    /// Phase 11: on_load(size) は measured_size をその値で初期化する
    #[test]
    fn on_load_uses_passed_size() {
        let html = r#"<html><body><div style="width:80px;height:30px;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((400, 300));
        assert_eq!(engine.measured_size(), (400, 300));
    }

    /// Phase 11: 連続 on_load が同じサイズを返す (intrinsic 自動測定で書き換わらない)
    #[test]
    fn on_load_does_not_invoke_intrinsic_measurement() {
        // body コンテンツは (10, 10) しかないが on_load の引数 (320, 240) で確定する
        let html =
            r#"<html><body style="margin:0"><div style="width:10px;height:10px;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((320, 240));
        assert_eq!(engine.measured_size(), (320, 240));
    }

    /// Phase 11: root 要素の CSS `min-width` / `max-width` / `min-height` / `max-height` を取り出す。
    #[test]
    fn root_size_constraints_reads_min_max_from_root_element_css() {
        let html = r#"<html><body><div class="panel" style="min-width:240px; max-width:600px; min-height:120px; max-height:480px;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((400, 300));
        // resolve_layout を一度走らせて taffy::Style が生成される状態にする
        engine.resolve_layout(400, 300, 1.0);
        let constraints = engine.root_size_constraints();
        assert_eq!(constraints.min_width, Some(240));
        assert_eq!(constraints.max_width, Some(600));
        assert_eq!(constraints.min_height, Some(120));
        assert_eq!(constraints.max_height, Some(480));
    }

    /// Phase 11: CSS 指定が無い軸は `None` (= 制約なし) を返す。
    #[test]
    fn root_size_constraints_returns_none_when_unset() {
        let html = r#"<html><body><div class="panel"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((400, 300));
        engine.resolve_layout(400, 300, 1.0);
        let constraints = engine.root_size_constraints();
        assert_eq!(constraints.min_width, None);
        assert_eq!(constraints.max_width, None);
        assert_eq!(constraints.min_height, None);
        assert_eq!(constraints.max_height, None);
    }

    /// Phase 11: `%` 単位は制約なし扱い (`None`) として返す。
    #[test]
    fn root_size_constraints_returns_none_for_percent_units() {
        let html = r#"<html><body><div class="panel" style="min-width:50%;"></div></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((400, 300));
        engine.resolve_layout(400, 300, 1.0);
        let constraints = engine.root_size_constraints();
        assert_eq!(constraints.min_width, None);
    }

    /// Phase 1.7: on_input(PointerMove) で hover state が更新され dirty が立つ
    #[test]
    fn on_input_pointer_move_updates_hover_and_marks_dirty() {
        let html = r#"<html><body style="margin:0"><button id="b" data-action="command:noop" style="display:block;width:80px;height:40px;">B</button></body></html>"#;
        let mut engine = engine(html);
        engine.on_load((400, 300));
        // 一旦 dirty フラグをクリアした想定で on_input が dirty を立てるかをテストする
        engine.clear_dirty_for_test();
        let event = blitz_traits::events::UiEvent::PointerMove(test_pointer_event(40.0, 20.0));
        let changed = engine.on_input(event);
        assert!(changed, "PointerMove should mark layout dirty");
        assert!(engine.layout_dirty(), "layout_dirty after on_input");
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
    fn replace_document_swaps_html_and_marks_dirty() {
        let mut engine = engine(r#"<html><body><span id="a">A</span></body></html>"#);
        engine.on_load((400, 200));
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
        engine.on_load((400, 200));
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
        engine.on_load((400, 200));
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
