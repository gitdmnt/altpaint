//! `HtmlPanelView` の layout + サイズ管理 (BL-100)。
//!
//! panel_size の保持、CSS min/max 制約の抽出、描画用 local size の単一クランプ規則、
//! stylo の直列化された `resolve`。

use super::{HtmlPanelView, LocalRenderSize, PanelSizeConstraints};
use blitz_dom::{BaseDocument, node::NodeData};
use blitz_traits::shell::Viewport;

/// stylo の resolve を直列化するグローバルロック。
///
/// Blitz の `BaseDocument::resolve` はグローバル rayon プール (StyleThread) で
/// スタイル計算を行い、複数ドキュメントの並行 resolve は stylo 内部の
/// `atomic_refcell` borrow 競合で panic する。プロダクションでは resolve は
/// 単一 UI スレッドからのみ呼ばれるため無競合 (ロックコストは実質ゼロ)。
static STYLE_RESOLVE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl HtmlPanelView {
    /// `panel_size` を viewport / chrome_height でクランプした描画用ローカルサイズを返す。
    ///
    /// `on_render` と `resolve_action_rects` の両方がこの 1 箇所を経由することで、
    /// GPU 描画と hit 矩形が常に同一の local size で layout 解決される。
    pub(crate) fn local_render_size(
        &self,
        viewport: (u32, u32),
        chrome_height: u32,
    ) -> LocalRenderSize {
        let (vp_w, vp_h) = (viewport.0.max(1), viewport.1.max(chrome_height + 1));
        let width = self.panel_size.0.min(vp_w).max(1);
        let height = self.panel_size.1.min(vp_h).max(chrome_height + 1);
        let body_height = height.saturating_sub(chrome_height).max(1);
        LocalRenderSize {
            width,
            height,
            body_height,
        }
    }

    /// パネルロード時に呼ぶ。bootstrap で必ず確定したサイズ
    /// (workspace 永続値 or panel.meta.json `default_size`) が渡される。
    pub fn set_panel_size(&mut self, size: (u32, u32)) {
        self.panel_size = (size.0.max(1), size.1.max(1));
        self.layout_dirty = true;
        self.render_dirty = true;
    }

    /// 現在の権威サイズ (HTML 本体の width, height)。
    pub fn panel_size(&self) -> (u32, u32) {
        self.panel_size
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

    /// viewport を設定し layout を解決する。同サイズかつ未変更ならスキップ。
    pub fn resolve_layout(&mut self, width: u32, height: u32, scale: f32) {
        if self.last_resolved == Some((width, height)) && !self.pending_mutation {
            return;
        }
        // stylo (Blitz `resolve`) はグローバル rayon プール (StyleThread) を共有しており、
        // 別ドキュメントの並行 resolve は atomic_refcell の borrow 競合で panic する。
        // プロダクションは単一 UI スレッドのため無競合だが、並列テストの安全のため
        // resolve をグローバルに直列化する (ADR 015)。
        let _style_lock = STYLE_RESOLVE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let viewport = Viewport::new(width, height, scale, blitz_traits::shell::ColorScheme::Dark);
        self.document.set_viewport(viewport);
        self.document.resolve(0.0);
        self.last_resolved = Some((width, height));
        self.pending_mutation = false;
    }
}

/// Phase 11: パネル root 要素 (body 直下の最初の Element ノード) の NodeId を返す。
/// 通常 `<body><div class="panel">...</div></body>` 形式なので `.panel` div を指す。
fn root_panel_node_id(document: &BaseDocument) -> Option<usize> {
    let body_id = document.query_selector("body").ok().flatten()?;
    let body = document.get_node(body_id)?;
    for child_id in &body.children {
        if let Some(child) = document.get_node(*child_id)
            && matches!(child.data, NodeData::Element(_))
        {
            return Some(*child_id);
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
