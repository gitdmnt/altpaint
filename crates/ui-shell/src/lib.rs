//! `ui-shell` は panel presentation と workspace 上の panel UI 制御を提供する。

mod focus;
mod workspace;

#[cfg(test)]
mod tests;

use app_core::{
    PanelSurfacePoint, WindowPoint, WorkspaceLayout, WorkspacePanelPosition, WorkspacePanelSize,
    WorkspacePanelState,
};
use focus::FocusTarget;
use std::collections::BTreeMap;

// hit-test API の戻り値型。利用側が panel-api へ直接依存しなくて済むよう再公開する。
pub use panel_api::ResizeEdge;

/// パネルの presentation 状態を保持する。
///
/// すべてのパネルは GPU 直描画 (`PanelRuntime::render_panels`) で提示され、
/// ui-shell は workspace layout・focus・hit table の管理だけを担う。
pub struct PanelPresentation {
    /// panel 並び順と表示状態。
    workspace_layout: WorkspaceLayout,
    /// 現在 focus 中の node。
    focused_target: Option<FocusTarget>,
    /// HTML パネル (GPU 直描画) の hit 情報。`update_html_panel_hits` で毎フレーム更新する。
    html_panel_hits: BTreeMap<String, HtmlPanelHitMap>,
    /// HTML パネルのタイトルバードラッグハンドル (screen 座標)。`update_html_panel_move_handle` で更新。
    html_panel_move_handles: BTreeMap<String, render_types::PixelRect>,
    /// Phase 11: HTML パネル全体 (chrome + body) の screen 座標矩形。
    /// `update_html_panel_full_rect` で毎フレーム更新し、リサイズハンドルの hit テストに使う。
    html_panel_full_rects: BTreeMap<String, render_types::PixelRect>,
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
            focused_target: None,
            html_panel_hits: BTreeMap::new(),
            html_panel_move_handles: BTreeMap::new(),
            html_panel_full_rects: BTreeMap::new(),
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

    /// window 座標の点にある HTML パネル move handle を検索し、panel_id を返す。
    pub fn html_panel_move_handle_at(&self, point: WindowPoint) -> Option<String> {
        self.html_panel_move_handles
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(panel_id, _)| panel_id.clone())
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

    /// window 座標の点が HTML パネル領域 (body 部分) のいずれかに入っていれば
    /// `(panel_id, パネル原点基準のローカル座標)` を返す。chrome 領域は除く（move handle 経路用）。
    /// `:hover` / `<details>` 開閉などの動的レイアウト追従のための入力転送に使う。
    pub fn html_panel_at(&self, point: WindowPoint) -> Option<(String, PanelSurfacePoint)> {
        self.html_panel_hits.iter().find_map(|(panel_id, map)| {
            map.screen_rect
                .to_local_point(point)
                .map(|local| (panel_id.clone(), local))
        })
    }

    /// Phase 11: HTML パネル全体 (chrome + body) の screen 座標矩形を更新する。
    /// リサイズハンドルの hit テストに使う。
    pub fn update_html_panel_full_rect(
        &mut self,
        panel_id: &str,
        screen_rect: render_types::PixelRect,
    ) {
        self.html_panel_full_rects
            .insert(panel_id.to_string(), screen_rect);
    }

    /// 指定 panel_id の HTML パネル full rect (chrome + body の screen 座標矩形) を返す。
    /// GPU quad の配置 (`runtime.rs`) が hit テーブル更新側と同じ矩形を共有するために使う。
    pub fn html_panel_full_rect(&self, panel_id: &str) -> Option<render_types::PixelRect> {
        self.html_panel_full_rects.get(panel_id).copied()
    }

    /// 指定 panel_id の HTML パネル full rect を削除する。
    pub fn remove_html_panel_full_rect(&mut self, panel_id: &str) {
        self.html_panel_full_rects.remove(panel_id);
    }

    /// Phase 11: window 座標の点のリサイズハンドル hit を検索し、
    /// `(panel_id, ResizeEdge)` を返す。角優先、辺は厚 6px、角は 12x12。
    /// パネル内側はリサイズ対象外なので `None`。
    pub fn panel_resize_hit_at(
        &self,
        point: WindowPoint,
    ) -> Option<(String, panel_api::ResizeEdge)> {
        self.html_panel_full_rects
            .iter()
            .find_map(|(panel_id, rect)| {
                resize_hit_in_rect(point, *rect).map(|edge| (panel_id.clone(), edge))
            })
    }

    /// window 座標の点の HTML パネル hit を検索し、`(panel_id, node_id)` を返す。
    pub fn html_panel_hit_at(&self, point: WindowPoint) -> Option<(String, String)> {
        self.html_panel_hits.iter().find_map(|(panel_id, map)| {
            let local = map.screen_rect.to_local_point(point)?;
            map.hits
                .iter()
                .find(|hit| hit.rect_in_panel.contains_local(local))
                .map(|hit| (panel_id.clone(), hit.node_id.clone()))
        })
    }

    /// 現在の ワークスペース レイアウト を返す。
    pub fn workspace_layout(&self) -> WorkspaceLayout {
        self.workspace_layout.clone()
    }

    /// ワークスペース レイアウト を置き換える。
    pub fn replace_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.ensure_workspace_manager_entry();
    }

    /// 登録済みパネル ID 一覧と workspace layout を整合させる。
    ///
    /// 未知のパネルにはエントリと既定位置を補い、不可視パネルから focus を外す。
    pub fn reconcile_panels(&mut self, panel_ids: Vec<&'static str>) {
        self.reconcile_workspace_layout(panel_ids);
    }

    /// 既存データを走査して focused target を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }
}

impl Default for PanelPresentation {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new()
    }
}

/// Phase 11: リサイズハンドル hit zone の厚み (px)。
/// 辺は 6px 厚、角は 12x12px の正方形 (角優先)。
const RESIZE_HANDLE_EDGE_PX: usize = 6;
const RESIZE_HANDLE_CORNER_PX: usize = 12;

/// window 座標の点がパネル矩形 `rect` のリサイズハンドルに当たるかを判定する純粋関数。
/// 角優先 → 辺 → 内側 (None) の順で評価する。
fn resize_hit_in_rect(
    point: WindowPoint,
    rect: render_types::PixelRect,
) -> Option<panel_api::ResizeEdge> {
    use panel_api::ResizeEdge;

    if rect.width == 0 || rect.height == 0 {
        return None;
    }
    // パネル矩形外
    if !rect.contains(point) {
        return None;
    }
    let (x, y) = (point.x as usize, point.y as usize);

    let left = rect.x;
    let top = rect.y;
    let right = rect.x + rect.width;
    let bottom = rect.y + rect.height;

    // 角のサイズはパネルの寸法を超えないようクランプ
    let corner_w = RESIZE_HANDLE_CORNER_PX.min(rect.width);
    let corner_h = RESIZE_HANDLE_CORNER_PX.min(rect.height);
    let edge_w = RESIZE_HANDLE_EDGE_PX.min(rect.width);
    let edge_h = RESIZE_HANDLE_EDGE_PX.min(rect.height);

    // 角優先 (4 角)
    let in_left_corner = x < left + corner_w;
    let in_right_corner = x >= right - corner_w;
    let in_top_corner = y < top + corner_h;
    let in_bottom_corner = y >= bottom - corner_h;

    if in_top_corner && in_left_corner {
        return Some(ResizeEdge::NorthWest);
    }
    if in_top_corner && in_right_corner {
        return Some(ResizeEdge::NorthEast);
    }
    if in_bottom_corner && in_left_corner {
        return Some(ResizeEdge::SouthWest);
    }
    if in_bottom_corner && in_right_corner {
        return Some(ResizeEdge::SouthEast);
    }

    // 4 辺 (角に当たらなかった残り)
    if y < top + edge_h {
        return Some(ResizeEdge::North);
    }
    if y >= bottom - edge_h {
        return Some(ResizeEdge::South);
    }
    if x < left + edge_w {
        return Some(ResizeEdge::West);
    }
    if x >= right - edge_w {
        return Some(ResizeEdge::East);
    }

    // パネル内側
    None
}

#[cfg(test)]
mod resize_hit_tests {
    use super::*;
    use panel_api::ResizeEdge;
    use render_types::PixelRect;

    fn rect(x: usize, y: usize, w: usize, h: usize) -> PixelRect {
        PixelRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn outside_returns_none() {
        let r = rect(100, 100, 200, 150);
        assert_eq!(resize_hit_in_rect(WindowPoint::new(50, 50), r), None);
        assert_eq!(resize_hit_in_rect(WindowPoint::new(500, 500), r), None);
    }

    #[test]
    fn corners_return_corner_edges() {
        let r = rect(100, 100, 200, 150);
        // NW
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(105, 105), r),
            Some(ResizeEdge::NorthWest)
        );
        // NE (右上角の内側)
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(295, 105), r),
            Some(ResizeEdge::NorthEast)
        );
        // SE (右下)
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(295, 245), r),
            Some(ResizeEdge::SouthEast)
        );
        // SW
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(105, 245), r),
            Some(ResizeEdge::SouthWest)
        );
    }

    #[test]
    fn edges_return_edge_directions() {
        let r = rect(100, 100, 200, 150);
        // 上辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 102), r), Some(ResizeEdge::North));
        // 右辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(298, 175), r), Some(ResizeEdge::East));
        // 下辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 248), r), Some(ResizeEdge::South));
        // 左辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(102, 175), r), Some(ResizeEdge::West));
    }

    #[test]
    fn inside_returns_none() {
        let r = rect(100, 100, 200, 150);
        // パネル中央付近 (角・辺どちらでもない)
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 175), r), None);
    }

    #[test]
    fn titlebar_top_6px_returns_north_edge() {
        let r = rect(100, 100, 200, 150);
        // パネル上端 6px の範囲 (タイトルバーと重なる領域も N edge を返す)
        // ただし角の 12px は除く
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 100), r), Some(ResizeEdge::North));
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 105), r), Some(ResizeEdge::North));
        // 7px 目以降は N edge ではない
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 107), r), None);
    }

    #[test]
    fn corner_takes_priority_over_edge() {
        let r = rect(100, 100, 200, 150);
        // 左上角 12x12 内 = NW (上辺の 6px とも重なるが角優先)
        assert_eq!(resize_hit_in_rect(WindowPoint::new(102, 102), r), Some(ResizeEdge::NorthWest));
    }

    #[test]
    fn zero_size_rect_returns_none() {
        let r = rect(100, 100, 0, 0);
        assert_eq!(resize_hit_in_rect(WindowPoint::new(100, 100), r), None);
    }
}
