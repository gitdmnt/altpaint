//! Blitz HTML パネルビュー（GPU 直描画版）。
//!
//! - HTML を [`HtmlDocument`] にパース
//! - ユーザー指定 CSS を user-agent stylesheet として追加
//! - viewport 設定 → style/layout 解決 → `blitz-paint` で `vello::Scene` を構築（**CPU pixels なし**）
//! - `data-bind-*` を JSON snapshot で評価し、DOM の attribute / class / textContent を更新
//! - `data-action` を持つ要素のレイアウト矩形を CSS 解決後の絶対座標で収集
//!
//! ## 内部モジュール分割 (BL-100)
//!
//! `HtmlPanelView` は 1 つの状態構造体だが、責務別に `impl` を 4 つの子モジュールへ分割する:
//!
//! - [`dom`] — DOM 管理 (document / replace_document / mutation / 要素状態 snapshot/restore)
//! - [`layout`] — layout + サイズ (panel_size / 制約 / local size / resolve_layout)
//! - [`present`] — GPU 提示 (on_render / scene 構築 / chrome 描画)
//! - [`actions`] — `data-action` 矩形収集と descriptor 解釈
//!
//! ## GPU リソースの保持について
//!
//! `vello::Renderer` / `wgpu::Device` / `wgpu::Queue` は **保持しない** (`on_render` 引数で
//! 外部から借りる)。パネル毎の描画先テクスチャ ([`crate::gpu::PanelGpuTarget`]) のみは
//! 本 view が所有し、`on_render` 内でサイズに応じて再生成する。

mod actions;
mod dom;
mod layout;
mod present;

use blitz_dom::{BaseDocument, LocalName, Namespace, QualName, node::NodeData};
use blitz_html::HtmlDocument;

/// パネル描画器。`HtmlDocument` を保持し、layout 解決と vello scene 構築を行う。
pub struct HtmlPanelView {
    document: HtmlDocument,
    user_css: String,
    /// 直近の `replace_document` で渡された HTML 文字列。同一なら no-op。
    last_html: Option<String>,
    last_resolved: Option<(u32, u32)>,
    /// Wasm の DOM mutation API (`mark_mutated`) が呼ばれたか。`resolve_layout` でクリア。
    /// Blitz の `BaseDocument::has_changes()` は内部実装の都合で当てにできないため自前トラック。
    pending_mutation: bool,
    /// パネル単位の権威サイズ (chrome を含む幅・高さ)。
    /// Phase 11: workspace_layout の永続値が唯一の入力経路 (`set_panel_size` / `restore_size`)。
    /// view 自身は自動測定/上書きを行わない。
    panel_size: (u32, u32),
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

/// パネル上端に重ねる chrome (タイトルバー) の描画スタイル。
///
/// panel-html は **テーマを知らない** (§1.4)。chrome の高さと塗り色は呼出側
/// (panel-runtime / desktop のテーマ) が決め、本 DTO で注入する (BL-099)。
/// `height == 0` または `None` 指定なら chrome を描画しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeStyle {
    /// chrome の高さ (px)。body はこの分だけ下にオフセットして描画される。
    pub height: u32,
    /// chrome 矩形の塗り色 (RGBA, sRGB)。
    pub fill_rgba: [u8; 4],
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

/// `data-action` 要素のレイアウト矩形 (パネルローカル座標、u32 ピクセル)。
///
/// panel-html はローカルクレート依存を持たない (§1.4) ため、`geometry::WindowRect`
/// (usize) には統合せず、本クレート固有の型として保持する。ホスト側 (desktop) が
/// `WindowRect` へ変換して使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelActionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRect {
    pub node_id: usize,
    pub element_id: Option<String>,
    pub data_action: String,
    pub data_args: Option<String>,
    pub rect: PanelActionRect,
}

/// `panel_size` を viewport / chrome_height でクランプした描画用ローカルサイズ。
///
/// `on_render` (GPU 描画) と `resolve_action_rects` (hit 矩形収集) が
/// **必ず同一のクランプ規則** で layout を解決するための単一定義点。
/// 旧実装はこの規則を 2 箇所に複製し「同期を保つ」コメント運用に頼っていた (BL-043)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalRenderSize {
    /// chrome を含むパネル全体の幅。
    pub(crate) width: u32,
    /// chrome を含むパネル全体の高さ。
    pub(crate) height: u32,
    /// chrome を除いた body 部分の高さ。
    pub(crate) body_height: u32,
}

impl HtmlPanelView {
    pub fn new(html: &str, user_css: &str) -> Self {
        use blitz_dom::DocumentConfig;
        use blitz_html::HtmlProvider;
        use std::sync::Arc;
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
            panel_size: (1, 1),
            layout_dirty: true,
            render_dirty: true,
            gpu_target: None,
        }
    }
}

/// `identity_attrs` を先頭から探し、最初に見つかった属性値を要素の identity として返す。
/// dom / actions モジュール両方が使う共有ヘルパ。
fn element_identity(document: &BaseDocument, node_id: usize, identity_attrs: &[&str]) -> Option<String> {
    let node = document.get_node(node_id)?;
    let NodeData::Element(element) = &node.data else {
        return None;
    };
    identity_attrs
        .iter()
        .find_map(|attr| element.attr(LocalName::from(*attr)))
        .map(str::to_string)
}

/// blitz `QualName` を local 名から構築する。
fn qual_name(local: &str) -> QualName {
    QualName::new(None, Namespace::default(), LocalName::from(local))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(html: &str) -> HtmlPanelView {
        HtmlPanelView::new(html, "")
    }

    /// Phase 11: set_panel_size(size) は panel_size をその値で初期化する
    #[test]
    fn on_load_uses_passed_size() {
        let html = r#"<html><body><div style="width:80px;height:30px;"></div></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 300));
        assert_eq!(view.panel_size(), (400, 300));
    }

    /// Phase 11: 連続 set_panel_size が同じサイズを返す (intrinsic 自動測定で書き換わらない)
    #[test]
    fn on_load_does_not_invoke_intrinsic_measurement() {
        // body コンテンツは (10, 10) しかないが set_panel_size の引数 (320, 240) で確定する
        let html =
            r#"<html><body style="margin:0"><div style="width:10px;height:10px;"></div></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((320, 240));
        assert_eq!(view.panel_size(), (320, 240));
    }

    /// Phase 11: root 要素の CSS `min-width` / `max-width` / `min-height` / `max-height` を取り出す。
    #[test]
    fn root_size_constraints_reads_min_max_from_root_element_css() {
        let html = r#"<html><body><div class="panel" style="min-width:240px; max-width:600px; min-height:120px; max-height:480px;"></div></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 300));
        // resolve_layout を一度走らせて taffy::Style が生成される状態にする
        view.resolve_layout(400, 300, 1.0);
        let constraints = view.root_size_constraints();
        assert_eq!(constraints.min_width, Some(240));
        assert_eq!(constraints.max_width, Some(600));
        assert_eq!(constraints.min_height, Some(120));
        assert_eq!(constraints.max_height, Some(480));
    }

    /// Phase 11: CSS 指定が無い軸は `None` (= 制約なし) を返す。
    #[test]
    fn root_size_constraints_returns_none_when_unset() {
        let html = r#"<html><body><div class="panel"></div></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 300));
        view.resolve_layout(400, 300, 1.0);
        let constraints = view.root_size_constraints();
        assert_eq!(constraints.min_width, None);
        assert_eq!(constraints.max_width, None);
        assert_eq!(constraints.min_height, None);
        assert_eq!(constraints.max_height, None);
    }

    /// Phase 11: `%` 単位は制約なし扱い (`None`) として返す。
    #[test]
    fn root_size_constraints_returns_none_for_percent_units() {
        let html = r#"<html><body><div class="panel" style="min-width:50%;"></div></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 300));
        view.resolve_layout(400, 300, 1.0);
        let constraints = view.root_size_constraints();
        assert_eq!(constraints.min_width, None);
    }

    /// Phase 1.7: on_input(PointerMove) で hover state が更新され dirty が立つ
    #[test]
    fn on_input_pointer_move_updates_hover_and_marks_dirty() {
        let html = r#"<html><body style="margin:0"><button id="b" data-action="command:noop" style="display:block;width:80px;height:40px;">B</button></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 300));
        // 一旦 dirty フラグをクリアした想定で on_input が dirty を立てるかをテストする
        view.clear_dirty_for_test();
        let event = blitz_traits::events::UiEvent::PointerMove(test_pointer_event(40.0, 20.0));
        view.on_input(event);
        assert!(view.layout_dirty(), "layout_dirty after on_input");
        assert!(view.render_dirty(), "render_dirty after on_input");
    }

    /// D1: ASCII テキストが vello::Scene に glyph run として積まれることを確認する。
    /// vello は glyph を `encoding.resources.glyph_runs` に格納するため、その len を見る。
    /// ここが落ちる場合は paint_scene が glyph 描画コマンドを scene に積んでいない（原因 A）。
    #[test]
    fn ascii_text_emits_glyph_run_in_scene() {
        let html = r#"<html><body><p>Hello</p></body></html>"#;
        let mut view = view(html);
        let mut scene = vello::Scene::new();
        view.build_scene(&mut scene, 200, 80, 1.0);
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
        let mut view = view(html);
        let mut scene = vello::Scene::new();
        view.build_scene(&mut scene, 100, 60, 1.0);
        let encoding = scene.encoding();
        assert!(
            !encoding.path_tags.is_empty() || !encoding.draw_tags.is_empty(),
            "expected vello scene to contain at least one path or draw tag, got path_tags={} draw_tags={}",
            encoding.path_tags.len(),
            encoding.draw_tags.len()
        );
    }

    /// BL-099: action_descriptor_for_element_id が data-action/data-args を解釈する
    /// (属性名規約・CSS エスケープ・パースの単一定義点)。
    #[test]
    fn action_descriptor_for_element_id_parses_data_action_and_args() {
        use crate::action::ActionDescriptor;
        let html = r#"<html><body>
            <button id="tool.pen" data-action="altp:activate:tool.pen" data-args='{"k":1}'>P</button>
            <button id="plain">x</button>
        </body></html>"#;
        let view = view(html);
        // `.` を含む id でも CSS エスケープして引ける。
        let desc = view
            .action_descriptor_for_element_id("tool.pen")
            .expect("descriptor parsed");
        match desc {
            ActionDescriptor::Altp { node_id, payload } => {
                assert_eq!(node_id, "tool.pen");
                assert_eq!(payload.get("k").and_then(|v| v.as_i64()), Some(1));
            }
            other => panic!("expected altp descriptor, got {other:?}"),
        }
        // data-action の無い要素は None。
        assert!(view.action_descriptor_for_element_id("plain").is_none());
        // 存在しない id も None。
        assert!(view.action_descriptor_for_element_id("missing").is_none());
    }

    /// S2: collect_action_rects が CSS padding を反映する
    #[test]
    fn html_engine_collect_action_rects_returns_buttons_with_padding() {
        let html = r#"<html><body>
            <button id="a" data-action="command:undo" style="display:block;">A</button>
            <button id="b" data-action="command:redo" style="display:block;">B</button>
            <span>nope</span>
        </body></html>"#;
        let mut view = view(html);
        view.resolve_layout(300, 100, 1.0);
        let rects = view.collect_action_rects();
        assert_eq!(rects.len(), 2, "expected 2 data-action elements");
        assert!(rects.iter().any(|r| r.element_id.as_deref() == Some("a")));
        assert!(rects.iter().any(|r| r.element_id.as_deref() == Some("b")));
        let a = rects
            .iter()
            .find(|r| r.element_id.as_deref() == Some("a"))
            .unwrap();
        assert!(a.rect.width > 0 && a.rect.height > 0);
    }

    /// S2d: resolve_action_rects は複数スレッドの同時実行でも panic しない
    /// (ADR 015: prepare_present_frame 経由で並列テストが同時に layout 解決するため)
    #[test]
    fn concurrent_resolve_action_rects_is_safe() {
        let html = r#"<html><body>
            <div class="panel">
              <header>ツール パレット</header>
              <button id="a" data-action="command:noop">✏ ペン (日本語テキスト)</button>
              <button id="b" data-action="command:noop">⌫ 消しゴム</button>
              <p>選択中: ペン / サイズ: 12px の説明文。折り返しが発生する長さの日本語文章をここに置く。</p>
            </div>
        </body></html>"#;
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let mut view = HtmlPanelView::new(html, "");
                        view.set_panel_size((280, 640));
                        let rects = view.resolve_action_rects((1280, 720), 1.0, 24);
                        assert_eq!(rects.len(), 2);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("concurrent resolve must not panic");
        }
    }

    /// S2b: resolve_action_rects は GPU コンテキストなしで layout 解決と hit 収集を行う
    #[test]
    fn resolve_action_rects_returns_hits_without_gpu() {
        let html = r#"<html><body>
            <button id="app.save" data-action="service:project.save" style="display:block;">Save</button>
        </body></html>"#;
        let mut view = view(html);
        view.set_panel_size((280, 160));
        let rects = view.resolve_action_rects((1280, 720), 1.0, 24);
        assert_eq!(rects.len(), 1, "expected 1 data-action element");
        let hit = &rects[0];
        assert_eq!(hit.element_id.as_deref(), Some("app.save"));
        assert!(hit.rect.width > 0 && hit.rect.height > 0);
    }

    /// S2c: resolve_action_rects は viewport が panel_size より小さい場合
    /// on_render と同じ local size でクランプして解決する
    #[test]
    fn resolve_action_rects_clamps_to_viewport_like_on_render() {
        let html = r#"<html><body>
            <button id="a" data-action="command:noop" style="display:block;width:100%;">A</button>
        </body></html>"#;
        let mut view = view(html);
        view.set_panel_size((800, 600));
        let rects = view.resolve_action_rects((200, 124), 1.0, 24);
        assert_eq!(rects.len(), 1);
        // local_w = min(800, 200) = 200 なので width:100% のボタンは 200px を超えない
        assert!(
            rects[0].rect.width <= 200,
            "expected clamped width <= 200, got {}",
            rects[0].rect.width
        );
    }

    /// BL-043: local_render_size は viewport / chrome_height で panel_size を
    /// クランプし、body_height = height - chrome_height (>=1) を返す単一定義点。
    #[test]
    fn local_render_size_clamps_panel_size_and_derives_body_height() {
        let mut view = view(r#"<html><body><div></div></body></html>"#);
        view.set_panel_size((800, 600));

        // viewport が panel_size より小さい → viewport へクランプ
        let clamped = view.local_render_size((200, 124), 24);
        assert_eq!(clamped.width, 200);
        assert_eq!(clamped.height, 124);
        assert_eq!(clamped.body_height, 100);

        // viewport が panel_size より大きい → panel_size を採用
        let full = view.local_render_size((1280, 720), 24);
        assert_eq!(full.width, 800);
        assert_eq!(full.height, 600);
        assert_eq!(full.body_height, 576);

        // chrome_height が高さを超える退化ケースでも body_height は 1 以上
        let degenerate = view.local_render_size((10, 10), 100);
        assert_eq!(degenerate.height, 101);
        assert_eq!(degenerate.body_height, 1);
    }

    #[test]
    fn replace_document_swaps_html_and_marks_dirty() {
        let mut view = view(r#"<html><body><span id="a">A</span></body></html>"#);
        view.set_panel_size((400, 200));
        view.clear_dirty_for_test();
        view.replace_document(
            r#"<html><body><span id="b">B</span></body></html>"#,
            "",
        );
        assert!(view.render_dirty(), "after replace render_dirty=true");
        assert!(view.layout_dirty(), "after replace layout_dirty=true");
        assert!(view.find_element_id("b").is_some(), "new element id present");
        assert!(view.find_element_id("a").is_none(), "old element gone");
    }

    #[test]
    fn replace_document_with_same_html_is_no_op() {
        let html = r#"<html><body><span id="a">A</span></body></html>"#;
        let mut view = view(html);
        view.set_panel_size((400, 200));
        view.clear_dirty_for_test();
        view.replace_document(html, "");
        assert!(!view.render_dirty(), "no-op when html unchanged");
        assert!(!view.layout_dirty(), "no-op when html unchanged");
    }

    /// BL-099: 汎用 snapshot API は selector マッチ要素の identity を identity_attrs
    /// (呼出側指定) から集める。details/data-altp-id の規約は引数で表現される。
    #[test]
    fn snapshot_marker_identities_collects_by_caller_supplied_convention() {
        let html = r#"<html><body>
            <details open data-altp-id="s1"><summary>A</summary>x</details>
            <details data-altp-id="s2"><summary>B</summary>y</details>
        </body></html>"#;
        let view = view(html);
        let open = view.snapshot_marker_identities("details[open]", &["data-altp-id", "id"]);
        assert_eq!(open, vec!["s1".to_string()]);
    }

    /// BL-099: 汎用 restore API は keep に無い要素から marker_attr を取り除く
    /// (open/details/data-altp-id の規約は呼出側が引数で渡す = panel-html は持たない)。
    #[test]
    fn clear_marker_for_unlisted_removes_marker_only_for_unlisted_identities() {
        let html = r#"<html><body>
            <details open data-altp-id="s1"><summary>A</summary>x</details>
            <details open data-altp-id="s2"><summary>B</summary>y</details>
        </body></html>"#;
        let mut view = view(html);
        // s1 のみ「開いたまま保持」。s2 は open を外す。
        view.clear_marker_for_unlisted(
            "details",
            "open",
            &["data-altp-id", "id"],
            &["s1".to_string()],
        );
        let s1 = view.find_element_id_by_altp("s1").expect("s1 exists");
        let s2 = view.find_element_id_by_altp("s2").expect("s2 exists");
        assert!(view.element_has_attribute(s1, "open"), "s1 kept open");
        assert!(!view.element_has_attribute(s2, "open"), "s2 open removed");
    }

    /// BL-099: replace_document は details/open 規約を自動適用しない (純粋な差し替え)。
    /// 状態保持は呼出側が snapshot/restore API で囲んで行う。
    #[test]
    fn replace_document_with_snapshot_restore_preserves_open_details() {
        let initial = r#"<html><body><details open data-altp-id="s"><summary>S</summary>x</details></body></html>"#;
        let mut view = view(initial);
        view.set_panel_size((400, 200));
        // 呼出側ポリシー: 差し替え前に開いている details の identity を保存。
        let opened = view.snapshot_marker_identities("details[open]", &["data-altp-id", "id"]);
        // 再構築 HTML も open 付き。
        let next = r#"<html><body><details open data-altp-id="s"><summary>S</summary>y</details></body></html>"#;
        view.replace_document(next, "");
        // 保存済みに含まれない details の open を外す (s は保存済みなので維持)。
        view.clear_marker_for_unlisted("details", "open", &["data-altp-id", "id"], &opened);
        let details_id = view.find_element_id_by_altp("s").expect("details exists");
        assert!(
            view.element_has_attribute(details_id, "open"),
            "previously open details remains open via caller-driven snapshot/restore"
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

    impl HtmlPanelView {
        /// テスト用: blitz-paint で `vello::Scene` を埋める (offset なし)。
        /// 本番の実描画は `on_render` が `build_scene_with_offset` 経由で行う。
        pub(crate) fn build_scene(
            &mut self,
            scene: &mut vello::Scene,
            width: u32,
            height: u32,
            scale: f32,
        ) {
            self.build_scene_with_offset(scene, width, height, scale, 0, 0);
        }

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
}
