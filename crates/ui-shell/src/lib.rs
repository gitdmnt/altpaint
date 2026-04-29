//! `ui-shell` は panel presentation と workspace 上の panel UI 制御を提供する。

mod focus;
mod presentation;
mod surface_render;
mod tree_query;
mod workspace;

#[cfg(test)]
mod tests;

// 9E-4: render::text 経路は撤去。ui-shell は GPU 直描画 (HtmlPanelEngine) に統一済み。

use app_core::{WorkspaceLayout, WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState};
use panel_api::{HostAction, PanelEvent, PanelTree};
use panel_runtime::PanelRuntime;
pub use presentation::PanelSurface;
use presentation::{FocusTarget, TextInputEditorState};
use std::collections::BTreeMap;
use surface_render::PANEL_SCROLL_PIXELS_PER_LINE;
use workspace::{WORKSPACE_PANEL_ID, event_panel_id, workspace_panel_actions};

/// パネルの presentation 状態を保持する。
///
/// Phase 9E-3 で CPU bitmap キャッシュ群 (`panel_content_cache` / `panel_bitmap_cache` /
/// `panel_measured_size_cache` / `panel_content_dirty` 系フラグ) は撤去された。
/// すべての DSL/HTML パネルは GPU 直描画 (`PanelRuntime::render_panels`) に移行した。
pub struct PanelPresentation {
    /// panel 並び順と表示状態。
    workspace_layout: WorkspaceLayout,
    /// 直近描画で使った実効パネル矩形。
    rendered_panel_rects: BTreeMap<String, render_types::PixelRect>,
    /// 現在の縦スクロール量。
    panel_scroll_offset: usize,
    /// 現在 focus 中の node。
    focused_target: Option<FocusTarget>,
    /// 展開中 dropdown。
    expanded_dropdown: Option<FocusTarget>,
    /// text input ごとの editor state。
    text_input_states: BTreeMap<(String, String), TextInputEditorState>,
    /// HTML パネル (GPU 直描画) の hit 情報。`update_html_panel_hits` で毎フレーム更新する。
    html_panel_hits: BTreeMap<String, HtmlPanelHitMap>,
    /// HTML パネルのタイトルバードラッグハンドル (screen 座標)。`update_html_panel_move_handle` で更新。
    html_panel_move_handles: BTreeMap<String, render_types::PixelRect>,
}

/// HTML パネル 1 枚分の hit 情報。screen 座標の矩形と panel-relative の hit 群。
#[derive(Debug, Clone)]
struct HtmlPanelHitMap {
    screen_rect: render_types::PixelRect,
    hits: Vec<HtmlPanelHitItem>,
}

#[derive(Debug, Clone)]
struct HtmlPanelHitItem {
    /// HTML 要素の `id` 属性。`HtmlPanelPlugin::handle_event` の matching に使われる。
    node_id: String,
    /// パネル原点を (0,0) とする矩形。
    rect_in_panel: render_types::PixelRect,
}

impl PanelPresentation {
    /// 既定値を使って新しいインスタンスを生成する。
    pub fn new() -> Self {
        Self {
            workspace_layout: WorkspaceLayout::default(),
            rendered_panel_rects: BTreeMap::new(),
            panel_scroll_offset: 0,
            focused_target: None,
            expanded_dropdown: None,
            text_input_states: BTreeMap::new(),
            html_panel_hits: BTreeMap::new(),
            html_panel_move_handles: BTreeMap::new(),
        }
    }

    /// HTML パネルのタイトルバー (move handle) 領域を screen 座標で更新する。
    pub fn update_html_panel_move_handle(
        &mut self,
        panel_id: &str,
        screen_rect: render_types::PixelRect,
    ) {
        self.html_panel_move_handles
            .insert(panel_id.to_string(), screen_rect);
    }

    /// 指定 panel_id の HTML パネル move handle を削除する。
    pub fn remove_html_panel_move_handle(&mut self, panel_id: &str) {
        self.html_panel_move_handles.remove(panel_id);
    }

    /// HTML パネル move handle を全削除する。
    pub fn clear_html_panel_move_handles(&mut self) {
        self.html_panel_move_handles.clear();
    }

    /// screen 座標 `(x, y)` の HTML パネル move handle を検索し、panel_id を返す。
    pub fn html_panel_move_handle_at(&self, x: usize, y: usize) -> Option<String> {
        for (panel_id, r) in &self.html_panel_move_handles {
            if x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height {
                return Some(panel_id.clone());
            }
        }
        None
    }

    /// HTML パネルの hit 情報を更新する。`hits` は (HTML 要素 id, panel-relative 矩形) の列。
    pub fn update_html_panel_hits(
        &mut self,
        panel_id: &str,
        screen_rect: render_types::PixelRect,
        hits: Vec<(String, render_types::PixelRect)>,
    ) {
        let items = hits
            .into_iter()
            .map(|(node_id, rect_in_panel)| HtmlPanelHitItem {
                node_id,
                rect_in_panel,
            })
            .collect();
        self.html_panel_hits.insert(
            panel_id.to_string(),
            HtmlPanelHitMap {
                screen_rect,
                hits: items,
            },
        );
    }

    /// 指定 panel_id の HTML パネル hit 情報を削除する。visibility off になった時などに呼ぶ。
    pub fn remove_html_panel_hits(&mut self, panel_id: &str) {
        self.html_panel_hits.remove(panel_id);
    }

    /// HTML パネル hit 情報を全削除する。
    pub fn clear_html_panel_hits(&mut self) {
        self.html_panel_hits.clear();
    }

    /// screen 座標 `(x, y)` が HTML パネル領域 (body 部分) のいずれかに入っていれば
    /// `(panel_id, local_x, local_y)` を返す。chrome 領域は除く（move handle 経路用）。
    /// `:hover` / `<details>` 開閉などの動的レイアウト追従のための入力転送に使う。
    pub fn html_panel_at(&self, x: usize, y: usize) -> Option<(String, u32, u32)> {
        for (panel_id, map) in &self.html_panel_hits {
            let r = map.screen_rect;
            if x < r.x || y < r.y || x >= r.x + r.width || y >= r.y + r.height {
                continue;
            }
            let local_x = (x - r.x) as u32;
            let local_y = (y - r.y) as u32;
            return Some((panel_id.clone(), local_x, local_y));
        }
        None
    }

    /// screen 座標 `(x, y)` の HTML パネル hit を検索し、`(panel_id, node_id)` を返す。
    pub fn html_panel_hit_at(&self, x: usize, y: usize) -> Option<(String, String)> {
        for (panel_id, map) in &self.html_panel_hits {
            let r = map.screen_rect;
            if x < r.x || y < r.y || x >= r.x + r.width || y >= r.y + r.height {
                continue;
            }
            let local_x = x - r.x;
            let local_y = y - r.y;
            for hit in &map.hits {
                let h = hit.rect_in_panel;
                if local_x >= h.x
                    && local_y >= h.y
                    && local_x < h.x + h.width
                    && local_y < h.y + h.height
                {
                    return Some((panel_id.clone(), hit.node_id.clone()));
                }
            }
        }
        None
    }

    /// パネル trees を計算して返す。
    pub fn panel_trees(&self, runtime: &PanelRuntime) -> Vec<PanelTree> {
        let mut trees = vec![self.workspace_manager_tree(runtime)];
        trees.extend(self.visible_panels_in_order(runtime));
        trees
    }

    /// 現在の ワークスペース レイアウト を返す。
    pub fn workspace_layout(&self) -> WorkspaceLayout {
        self.workspace_layout.clone()
    }

    /// ワークスペース レイアウト を置き換える。
    pub fn replace_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.ensure_workspace_manager_entry();
        // 9E-3: パネル CPU bitmap キャッシュは廃止。GPU 経路は次フレームの
        // `render_panels` 呼び出しで自然に再描画されるため dirty フラグ管理不要。
    }

    /// 既存データを走査して reconcile runtime panels を組み立てる。
    pub fn reconcile_runtime_panels(&mut self, runtime: &PanelRuntime) {
        let panel_ids = runtime
            .panel_trees()
            .into_iter()
            .map(|tree| tree.id)
            .collect::<Vec<_>>();
        self.reconcile_workspace_layout(panel_ids);
    }

    /// Runtime panels 差分 を更新する。
    ///
    /// 9E-3 以降は no-op。Engine 内部の dirty (render_dirty) が GPU 経路の再描画を判定する。
    pub fn mark_runtime_panels_dirty(
        &mut self,
        _changed_panel_ids: &std::collections::BTreeSet<String>,
    ) {
    }

    /// 既存データを走査して focused target を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }

    /// 9E-3 以降は CPU rasterize が無いため常に 0。プロファイラ互換のため残置。
    pub fn last_panel_rasterized_panels(&self) -> usize {
        0
    }

    /// 9E-3 以降は CPU compose が無いため常に 0。
    pub fn last_panel_composited_panels(&self) -> usize {
        0
    }

    /// 9E-3 以降は CPU rasterize が無いため常に 0。
    pub fn last_panel_raster_duration_ms(&self) -> f64 {
        0.0
    }

    /// 9E-3 以降は CPU compose が無いため常に 0。
    pub fn last_panel_compose_duration_ms(&self) -> f64 {
        0.0
    }

    /// 9E-3 以降は CPU 合成 dirty rect が存在しない。常に `None`。
    pub fn last_panel_surface_dirty_rect(&self) -> Option<render_types::PixelRect> {
        None
    }

    /// パネル スクロール オフセット を計算して返す。
    pub fn panel_scroll_offset(&self) -> usize {
        self.panel_scroll_offset
    }

    /// スクロール panels に必要な描画内容を組み立てる。
    pub fn scroll_panels(&mut self, delta_lines: i32, viewport_height: usize) -> bool {
        let delta_pixels = delta_lines.saturating_mul(PANEL_SCROLL_PIXELS_PER_LINE);
        let max_offset = self.max_panel_scroll_offset(viewport_height) as i32;
        let next_offset =
            (self.panel_scroll_offset as i32 + delta_pixels).clamp(0, max_offset) as usize;
        if next_offset == self.panel_scroll_offset {
            return false;
        }
        self.panel_scroll_offset = next_offset;
        true
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn handle_panel_event(
        &mut self,
        runtime: &PanelRuntime,
        event: &PanelEvent,
    ) -> PresentationEventResult {
        if let PanelEvent::Activate { panel_id, node_id } = event {
            self.focus_panel_node(runtime, panel_id, node_id);
            if self.is_dropdown_target(runtime, panel_id, node_id) {
                let dropdown = FocusTarget {
                    panel_id: panel_id.clone(),
                    node_id: node_id.clone(),
                };
                self.expanded_dropdown = if self.expanded_dropdown.as_ref() == Some(&dropdown) {
                    None
                } else {
                    Some(dropdown)
                };
                let _ = panel_id;
                return PresentationEventResult {
                    forward_to_runtime: false,
                    actions: Vec::new(),
                    changed: true,
                };
            }
        }
        if let PanelEvent::SetText {
            panel_id, node_id, ..
        } = event
            && self.is_dropdown_target(runtime, panel_id, node_id)
        {
            self.expanded_dropdown = None;
        }
        if event_panel_id(event) == WORKSPACE_PANEL_ID {
            let ordered_panels = self
                .workspace_panel_entries(runtime)
                .into_iter()
                .map(|(entry, _)| (entry.id.clone(), entry.visible))
                .collect::<Vec<_>>();
            let actions = workspace_panel_actions(ordered_panels.as_slice(), event);
            return PresentationEventResult {
                forward_to_runtime: false,
                actions,
                changed: true,
            };
        }
        PresentationEventResult {
            forward_to_runtime: true,
            actions: Vec::new(),
            changed: false,
        }
    }

    /// 9E-3 以降は no-op (CPU bitmap キャッシュ廃止)。focus.rs 等の既存呼び出し互換のため残置。
    pub(crate) fn mark_all_panel_content_dirty(&mut self) {}

    /// 9E-3 以降は no-op (CPU bitmap キャッシュ廃止)。focus.rs 等の既存呼び出し互換のため残置。
    pub(crate) fn mark_panel_content_dirty(&mut self, _panel_id: &str) {}
}

impl Default for PanelPresentation {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PresentationEventResult {
    pub forward_to_runtime: bool,
    pub actions: Vec<HostAction>,
    pub changed: bool,
}
