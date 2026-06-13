//! `panel-workspace` はワークスペース上のパネル配置・focus・hit テーブルを管理する。

mod focus;
mod handles;
mod workspace;
mod workspace_state;

#[cfg(test)]
mod tests;

use geometry::{PanelSurfacePoint, WindowPoint};
use focus::FocusTarget;
use std::collections::BTreeMap;

// パネル配置操作のジオメトリ型 (旧 panel-api、C9 で本クレートへ移設)。
pub use handles::{PanelMoveDirection, ResizeHandle};
// パネル配置の永続化状態とアンカー解決幾何 (旧 app-core::workspace)。
pub use workspace_state::{
    PanelConfigs, PanelLayoutDefaults, WorkspaceLayout, WorkspacePanelAnchor,
    WorkspacePanelPosition, WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};

/// 全パネルの配置 (workspace layout)・focus・hit テーブルの状態ストア。
///
/// すべてのパネルは GPU 直描画 (`PanelRuntime::render_panels`) で提示され、
/// panel-workspace は workspace layout・focus・hit table の管理だけを担う。
pub struct PanelWorkspace {
    /// panel 並び順と表示状態。
    workspace_layout: WorkspaceLayout,
    /// 現在 focus 中の node。
    focused_target: Option<FocusTarget>,
    /// HTML パネル (GPU 直描画) 1 枚ごとの screen 座標ジオメトリ。
    /// `update_panel_geometry` で毎フレーム原子的に更新する (BL-096)。
    /// full_rect / move_handle_rect / body_rect / node_hits を 1 構造体に束ね、
    /// 並列 map が部分更新で不整合になる余地を排除する。
    panel_geometries: BTreeMap<String, PanelGeometry>,
    /// パネルが meta.json で宣言した既定配置 (BL-095)。desktop から注入される。
    /// ビルトイン ID のハードコードに代わり、配置・既定表示・常時表示を解決する。
    panel_layout_defaults: BTreeMap<String, PanelLayoutDefaults>,
}

/// HTML パネル 1 枚分の screen 座標ジオメトリ (BL-096)。
///
/// chrome (タイトルバー) / body の分割は desktop ではなく本クレートが `full_rect` と
/// chrome 高さから導出する。これにより hit テーブルと GPU quad が常に同じ `full_rect`
/// を共有し、3 本の並列 map が独立更新でずれる不具合を構造的に防ぐ。
#[derive(Debug, Clone)]
struct PanelGeometry {
    /// パネル全体 (chrome + body) の screen 座標矩形。GPU quad 配置と resize hit に使う。
    full_rect: geometry::WindowRect,
    /// タイトルバー (move handle) 領域の screen 座標矩形。full_rect 上端の chrome 帯。
    move_handle_rect: geometry::WindowRect,
    /// body (chrome を除いた内容) 領域の screen 座標矩形。hit 解決の原点に使う。
    body_rect: geometry::WindowRect,
    /// data-action 要素の hit 群。矩形はパネル原点ではなく body 原点基準。
    node_hits: Vec<PanelHitItem>,
}

#[derive(Debug, Clone)]
struct PanelHitItem {
    /// HTML 要素の `id` 属性。`HtmlWasmPanel::handle_event` の matching に使われる。
    node_id: String,
    /// body 原点を (0,0) とする矩形。
    rect_in_panel: geometry::WindowRect,
}

impl PanelWorkspace {
    pub fn new() -> Self {
        Self {
            workspace_layout: WorkspaceLayout::default(),
            focused_target: None,
            panel_geometries: BTreeMap::new(),
            panel_layout_defaults: BTreeMap::new(),
        }
    }

    /// パネルの既定配置 (anchor / position / hidden_by_default / always_visible) を
    /// 注入する (BL-095)。desktop が panel-runtime の meta から構築して渡す。
    ///
    /// `reconcile_panels` より前に呼ぶこと。注入されたパネルは宣言された既定値で、
    /// 未注入のパネルは index ベースのフォールバックで配置される。
    pub fn set_panel_layout_defaults(
        &mut self,
        defaults: BTreeMap<String, PanelLayoutDefaults>,
    ) {
        self.panel_layout_defaults = defaults;
    }

    /// 指定パネルの既定配置を返す (未注入なら空既定)。
    pub(crate) fn layout_defaults_for(&self, panel_id: &str) -> PanelLayoutDefaults {
        self.panel_layout_defaults
            .get(panel_id)
            .cloned()
            .unwrap_or_default()
    }

    /// 常に表示する (ユーザーが非表示にできない) パネル ID 一覧を登録順で返す。
    pub(crate) fn always_visible_panel_ids(&self) -> Vec<String> {
        self.panel_layout_defaults
            .iter()
            .filter(|(_, defaults)| defaults.always_visible)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// HTML パネル 1 枚の screen 座標ジオメトリを原子的に更新する (BL-096)。
    ///
    /// `full_rect` (chrome + body) と chrome 高さ・body 原点基準の hit 群を受け取り、
    /// chrome (move handle) / body の分割を本クレート内で導出する。これにより
    /// full_rect / move_handle_rect / body_rect / node_hits が必ず同じ更新で揃う。
    pub fn update_panel_geometry(
        &mut self,
        panel_id: &str,
        full_rect: geometry::WindowRect,
        chrome_height: usize,
        hits: Vec<(String, geometry::WindowRect)>,
    ) {
        let move_handle_rect = geometry::WindowRect {
            x: full_rect.x,
            y: full_rect.y,
            width: full_rect.width,
            height: chrome_height,
        };
        let body_rect = geometry::WindowRect {
            x: full_rect.x,
            y: full_rect.y + chrome_height,
            width: full_rect.width,
            height: full_rect.height.saturating_sub(chrome_height),
        };
        let node_hits = hits
            .into_iter()
            .map(|(node_id, rect_in_panel)| PanelHitItem {
                node_id,
                rect_in_panel,
            })
            .collect();
        self.panel_geometries.insert(
            panel_id.to_string(),
            PanelGeometry {
                full_rect,
                move_handle_rect,
                body_rect,
                node_hits,
            },
        );
    }

    /// 指定 panel_id の HTML パネルジオメトリを削除する。visibility off になった時などに呼ぶ。
    pub fn remove_panel_geometry(&mut self, panel_id: &str) {
        self.panel_geometries.remove(panel_id);
    }

    /// window 座標の点にある HTML パネル move handle を検索し、panel_id を返す。
    pub fn panel_move_handle_at(&self, point: WindowPoint) -> Option<String> {
        self.panel_geometries
            .iter()
            .find(|(_, geometry)| geometry.move_handle_rect.contains(point))
            .map(|(panel_id, _)| panel_id.clone())
    }

    /// window 座標の点が HTML パネル領域 (body 部分) のいずれかに入っていれば
    /// `(panel_id, body 原点基準のローカル座標)` を返す。chrome 領域は除く（move handle 経路用）。
    /// `:hover` / `<details>` 開閉などの動的レイアウト追従のための入力転送に使う。
    pub fn panel_at(&self, point: WindowPoint) -> Option<(String, PanelSurfacePoint)> {
        self.panel_geometries.iter().find_map(|(panel_id, geometry)| {
            geometry
                .body_rect
                .to_panel_surface_point(point)
                .map(|local| (panel_id.clone(), local))
        })
    }

    /// 指定 panel_id の HTML パネル full rect (chrome + body の screen 座標矩形) を返す。
    /// GPU quad の配置 (`event_loop.rs`) が hit テーブル更新側と同じ矩形を共有するために使う。
    pub fn panel_full_rect(&self, panel_id: &str) -> Option<geometry::WindowRect> {
        self.panel_geometries
            .get(panel_id)
            .map(|geometry| geometry.full_rect)
    }

    /// Phase 11: window 座標の点のリサイズハンドル hit を検索し、
    /// `(panel_id, ResizeHandle)` を返す。角優先、辺は厚 6px、角は 12x12。
    /// パネル内側はリサイズ対象外なので `None`。
    pub fn panel_resize_hit_at(
        &self,
        point: WindowPoint,
    ) -> Option<(String, ResizeHandle)> {
        self.panel_geometries
            .iter()
            .find_map(|(panel_id, geometry)| {
                resize_hit_in_rect(point, geometry.full_rect)
                    .map(|handle| (panel_id.clone(), handle))
            })
    }

    /// window 座標の点の HTML パネル hit を検索し、`(panel_id, node_id)` を返す。
    pub fn panel_hit_at(&self, point: WindowPoint) -> Option<(String, String)> {
        self.panel_geometries.iter().find_map(|(panel_id, geometry)| {
            let local = geometry.body_rect.to_panel_surface_point(point)?;
            geometry
                .node_hits
                .iter()
                .find(|hit| hit.rect_in_panel.contains_local(local))
                .map(|hit| (panel_id.clone(), hit.node_id.clone()))
        })
    }

    pub fn workspace_layout(&self) -> WorkspaceLayout {
        self.workspace_layout.clone()
    }

    pub fn replace_workspace_layout(&mut self, workspace_layout: WorkspaceLayout) {
        self.workspace_layout = workspace_layout;
        self.ensure_always_visible_panel_entries();
    }

    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }
}

impl Default for PanelWorkspace {
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
    rect: geometry::WindowRect,
) -> Option<ResizeHandle> {
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
        return Some(ResizeHandle::NorthWest);
    }
    if in_top_corner && in_right_corner {
        return Some(ResizeHandle::NorthEast);
    }
    if in_bottom_corner && in_left_corner {
        return Some(ResizeHandle::SouthWest);
    }
    if in_bottom_corner && in_right_corner {
        return Some(ResizeHandle::SouthEast);
    }

    // 4 辺 (角に当たらなかった残り)
    if y < top + edge_h {
        return Some(ResizeHandle::North);
    }
    if y >= bottom - edge_h {
        return Some(ResizeHandle::South);
    }
    if x < left + edge_w {
        return Some(ResizeHandle::West);
    }
    if x >= right - edge_w {
        return Some(ResizeHandle::East);
    }

    // パネル内側
    None
}

#[cfg(test)]
mod resize_hit_tests {
    use super::*;
    use geometry::WindowRect;

    fn rect(x: usize, y: usize, w: usize, h: usize) -> WindowRect {
        WindowRect {
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
    fn corners_return_corner_handles() {
        let r = rect(100, 100, 200, 150);
        // NW
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(105, 105), r),
            Some(ResizeHandle::NorthWest)
        );
        // NE (右上角の内側)
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(295, 105), r),
            Some(ResizeHandle::NorthEast)
        );
        // SE (右下)
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(295, 245), r),
            Some(ResizeHandle::SouthEast)
        );
        // SW
        assert_eq!(
            resize_hit_in_rect(WindowPoint::new(105, 245), r),
            Some(ResizeHandle::SouthWest)
        );
    }

    #[test]
    fn edges_return_edge_handles() {
        let r = rect(100, 100, 200, 150);
        // 上辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 102), r), Some(ResizeHandle::North));
        // 右辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(298, 175), r), Some(ResizeHandle::East));
        // 下辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 248), r), Some(ResizeHandle::South));
        // 左辺中央
        assert_eq!(resize_hit_in_rect(WindowPoint::new(102, 175), r), Some(ResizeHandle::West));
    }

    #[test]
    fn inside_returns_none() {
        let r = rect(100, 100, 200, 150);
        // パネル中央付近 (角・辺どちらでもない)
        assert_eq!(resize_hit_in_rect(WindowPoint::new(200, 175), r), None);
    }

    #[test]
    fn titlebar_top_6px_returns_north_handle() {
        let r = rect(100, 100, 200, 150);
        // パネル上端 6px の範囲 (タイトルバーと重なる領域も North handle を返す)
        // ただし角の 12px は除く
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 100), r), Some(ResizeHandle::North));
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 105), r), Some(ResizeHandle::North));
        // 7px 目以降は North handle ではない
        assert_eq!(resize_hit_in_rect(WindowPoint::new(150, 107), r), None);
    }

    #[test]
    fn corner_takes_priority_over_edge() {
        let r = rect(100, 100, 200, 150);
        // 左上角 12x12 内 = NW (上辺の 6px とも重なるが角優先)
        assert_eq!(resize_hit_in_rect(WindowPoint::new(102, 102), r), Some(ResizeHandle::NorthWest));
    }

    #[test]
    fn zero_size_rect_returns_none() {
        let r = rect(100, 100, 0, 0);
        assert_eq!(resize_hit_in_rect(WindowPoint::new(100, 100), r), None);
    }
}
