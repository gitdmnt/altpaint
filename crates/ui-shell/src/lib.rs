//! `ui-shell` は panel presentation と workspace 上の panel UI 制御を提供する。

mod focus;
mod presentation;
mod surface_render;
mod tree_query;
mod workspace;

#[cfg(test)]
mod tests;

pub use render::{
    draw_text_rgba, measure_text_width, text_backend_name, text_line_height, wrap_text_lines,
};

use app_core::{WorkspaceLayout, WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState};
use panel_api::{HostAction, PanelEvent, PanelTree};
use panel_runtime::PanelRuntime;
pub use presentation::PanelSurface;
use presentation::{FocusTarget, TextInputEditorState};
use std::collections::{BTreeMap, BTreeSet};
use surface_render::PANEL_SCROLL_PIXELS_PER_LINE;
use workspace::{WORKSPACE_PANEL_ID, event_panel_id, workspace_panel_actions};

/// パネルの presentation 状態を保持する。
pub struct PanelPresentation {
    /// panel 並び順と表示状態。
    workspace_layout: WorkspaceLayout,
    /// スクロール前 content surface のキャッシュ。
    panel_content_cache: Option<PanelSurface>,
    /// 現在キャッシュしている panel surface の生成元 viewport サイズ。
    panel_content_viewport: Option<(usize, usize)>,
    /// 個別パネルごとのラスタライズ済み content キャッシュ。
    panel_bitmap_cache: BTreeMap<String, PanelSurface>,
    /// 個別パネルごとの計測済みサイズキャッシュ。
    panel_measured_size_cache: BTreeMap<String, render::MeasuredPanelSize>,
    /// 直近描画で使った実効パネル矩形。
    rendered_panel_rects: BTreeMap<String, render::PixelRect>,
    /// panel content を再構築すべきかのフラグ。
    panel_content_dirty: bool,
    /// 次回 rasterize が全パネル対象かどうか。
    full_panel_raster_dirty: bool,
    /// 次回 rasterize が必要な panel id 群。
    dirty_panel_ids: BTreeSet<String>,
    /// パネル位置だけが変化し、再合成だけで済むかどうかのフラグ。
    panel_layout_dirty: bool,
    /// 直近の render_panel_surface 呼び出しで再ラスタライズしたパネル数。
    last_panel_rasterized_panels: usize,
    /// 直近の render_panel_surface 呼び出しで再合成したパネル数。
    last_panel_composited_panels: usize,
    /// 直近の panel rasterize に要した時間。
    last_panel_raster_duration_ms: f64,
    /// 直近の panel compose に要した時間。
    last_panel_compose_duration_ms: f64,
    /// 直近の panel surface 更新で実際に変化したグローバル矩形。
    last_panel_surface_dirty_rect: Option<render::PixelRect>,
    /// 現在の縦スクロール量。
    panel_scroll_offset: usize,
    /// content 全体の高さ。
    panel_content_height: usize,
    /// 計測済みサイズキャッシュに対応する viewport サイズ。
    panel_measure_viewport: Option<(usize, usize)>,
    /// 現在 focus 中の node。
    focused_target: Option<FocusTarget>,
    /// 展開中 dropdown。
    expanded_dropdown: Option<FocusTarget>,
    /// text input ごとの editor state。
    text_input_states: BTreeMap<(String, String), TextInputEditorState>,
    /// HTML パネル (GPU 直描画) の hit 情報。`update_html_panel_hits` で毎フレーム更新する。
    html_panel_hits: BTreeMap<String, HtmlPanelHitMap>,
    /// HTML パネルのタイトルバードラッグハンドル (screen 座標)。`update_html_panel_move_handle` で更新。
    html_panel_move_handles: BTreeMap<String, render::PixelRect>,
}

/// HTML パネル 1 枚分の hit 情報。screen 座標の矩形と panel-relative の hit 群。
#[derive(Debug, Clone)]
struct HtmlPanelHitMap {
    screen_rect: render::PixelRect,
    hits: Vec<HtmlPanelHitItem>,
}

#[derive(Debug, Clone)]
struct HtmlPanelHitItem {
    /// HTML 要素の `id` 属性。`HtmlPanelPlugin::handle_event` の matching に使われる。
    node_id: String,
    /// パネル原点を (0,0) とする矩形。
    rect_in_panel: render::PixelRect,
}

impl PanelPresentation {
    /// 既定値を使って新しいインスタンスを生成する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn new() -> Self {
        Self {
            workspace_layout: WorkspaceLayout::default(),
            panel_content_cache: None,
            panel_content_viewport: None,
            panel_bitmap_cache: BTreeMap::new(),
            panel_measured_size_cache: BTreeMap::new(),
            rendered_panel_rects: BTreeMap::new(),
            panel_content_dirty: true,
            full_panel_raster_dirty: true,
            dirty_panel_ids: BTreeSet::new(),
            panel_layout_dirty: true,
            last_panel_rasterized_panels: 0,
            last_panel_composited_panels: 0,
            last_panel_raster_duration_ms: 0.0,
            last_panel_compose_duration_ms: 0.0,
            last_panel_surface_dirty_rect: None,
            panel_scroll_offset: 0,
            panel_content_height: 0,
            panel_measure_viewport: None,
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
        screen_rect: render::PixelRect,
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
        screen_rect: render::PixelRect,
        hits: Vec<(String, render::PixelRect)>,
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
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn replace_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.ensure_workspace_manager_entry();
        self.mark_all_panel_content_dirty();
        self.panel_layout_dirty = true;
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

    /// Runtime panels 差分 を更新し、必要な dirty 状態も記録する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn mark_runtime_panels_dirty(
        &mut self,
        changed_panel_ids: &std::collections::BTreeSet<String>,
    ) {
        if changed_panel_ids.is_empty() {
            return;
        }
        for panel_id in changed_panel_ids {
            self.mark_panel_content_dirty(panel_id);
        }
    }

    /// 既存データを走査して focused target を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }

    /// last パネル rasterized panels を計算して返す。
    pub fn last_panel_rasterized_panels(&self) -> usize {
        self.last_panel_rasterized_panels
    }

    /// last パネル composited panels を計算して返す。
    pub fn last_panel_composited_panels(&self) -> usize {
        self.last_panel_composited_panels
    }

    /// last パネル raster duration ms を計算して返す。
    pub fn last_panel_raster_duration_ms(&self) -> f64 {
        self.last_panel_raster_duration_ms
    }

    /// last パネル 合成 duration ms を計算して返す。
    pub fn last_panel_compose_duration_ms(&self) -> f64 {
        self.last_panel_compose_duration_ms
    }

    /// Last パネル サーフェス 差分 矩形 に必要な差分領域だけを描画または合成する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn last_panel_surface_dirty_rect(&self) -> Option<render::PixelRect> {
        self.last_panel_surface_dirty_rect
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
                self.mark_panel_content_dirty(panel_id);
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
            self.mark_panel_content_dirty(panel_id);
        }
        if event_panel_id(event) == WORKSPACE_PANEL_ID {
            let ordered_panels = self
                .workspace_panel_entries(runtime)
                .into_iter()
                .map(|(entry, _)| (entry.id.clone(), entry.visible))
                .collect::<Vec<_>>();
            let actions = workspace_panel_actions(ordered_panels.as_slice(), event);
            self.mark_all_panel_content_dirty();
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

    /// All パネル content 差分 を更新し、必要な dirty 状態も記録する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(crate) fn mark_all_panel_content_dirty(&mut self) {
        self.panel_content_dirty = true;
        self.full_panel_raster_dirty = true;
        self.dirty_panel_ids.clear();
        self.panel_measured_size_cache.clear();
        self.panel_measure_viewport = None;
    }

    /// 現在の値を パネル content 差分 へ変換する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(crate) fn mark_panel_content_dirty(&mut self, panel_id: &str) {
        self.panel_content_dirty = true;
        if !self.full_panel_raster_dirty {
            self.dirty_panel_ids.insert(panel_id.to_string());
        }
        self.panel_measured_size_cache.remove(panel_id);
    }
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
