use serde::{Deserialize, Serialize};

use crate::Command;

/// 現在の描画ツール。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolKind {
    #[default]
    Brush,
    Eraser,
}

/// 作品を識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkId(pub u64);

/// ページを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// コマを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PanelId(pub u64);

/// レイヤーノードを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerNodeId(pub u64);

/// アプリケーションの永続状態全体を表すルートドキュメント。
///
/// フェーズ0では単一の `Work` のみを保持する。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Document {
    /// 現在編集中の作品。
    pub work: Work,
    /// 現在の最小ツール状態。
    pub active_tool: ToolKind,
    /// キャンバスの表示変換状態。
    pub view_transform: CanvasViewTransform,
}

/// 漫画作品全体を表す最小単位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Work {
    /// 作品ID。
    pub id: WorkId,
    /// 表示用タイトル。
    pub title: String,
    /// ページ列。
    pub pages: Vec<Page>,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            id: WorkId(1),
            title: "Untitled".to_string(),
            pages: vec![Page::default()],
        }
    }
}

/// 作品を構成するページ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// ページID。
    pub id: PageId,
    /// ページ内に含まれるコマ列。
    pub panels: Vec<Panel>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            id: PageId(1),
            panels: vec![Panel::default()],
        }
    }
}

/// 漫画のコマを表す最小単位。
///
/// 将来的には境界情報やスナップショット参照を持つが、
/// 現段階ではレイヤーツリーのルートだけを持つ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    /// コマID。
    pub id: PanelId,
    /// このコマが持つレイヤーツリーのルート。
    pub root_layer: LayerNode,
    /// フェーズ2の最小ラスタキャンバス。
    pub bitmap: CanvasBitmap,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            id: PanelId(1),
            root_layer: LayerNode::default(),
            bitmap: CanvasBitmap::new(64, 64),
        }
    }
}

/// キャンバス上で変更が発生した矩形領域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyRect {
    /// 左上X座標。
    pub x: usize,
    /// 左上Y座標。
    pub y: usize,
    /// 横幅。
    pub width: usize,
    /// 高さ。
    pub height: usize,
}

impl DirtyRect {
    /// 左上と右下の両端を含む矩形を作る。
    pub fn from_inclusive_points(from_x: usize, from_y: usize, to_x: usize, to_y: usize) -> Self {
        let min_x = from_x.min(to_x);
        let min_y = from_y.min(to_y);
        let max_x = from_x.max(to_x);
        let max_y = from_y.max(to_y);

        Self {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        }
    }

    /// 2つのdirty矩形を包含する最小矩形を返す。
    pub fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    /// ビットマップ境界に収まるよう矩形をクランプする。
    pub fn clamp_to_bitmap(self, width: usize, height: usize) -> Self {
        if width == 0 || height == 0 {
            return Self {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
        }

        let max_x = width - 1;
        let max_y = height - 1;
        let left = self.x.min(max_x);
        let top = self.y.min(max_y);
        let right = self
            .x
            .saturating_add(self.width.saturating_sub(1))
            .min(max_x);
        let bottom = self
            .y
            .saturating_add(self.height.saturating_sub(1))
            .min(max_y);

        Self::from_inclusive_points(left, top, right, bottom)
    }
}

/// 将来のズーム・回転・パンに備える表示変換状態。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewTransform {
    pub zoom: f32,
    pub rotation_degrees: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for CanvasViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            rotation_degrees: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// フェーズ2で使う最小のラスタキャンバス。
///
/// 白いキャンバス上に黒ピクセルを打つだけの単純なビットマップとして実装する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasBitmap {
    /// 横幅ピクセル数。
    pub width: usize,
    /// 高さピクセル数。
    pub height: usize,
    /// RGBA8 の生ピクセル列。
    pub pixels: Vec<u8>,
}

impl CanvasBitmap {
    /// 白背景で初期化された新しいビットマップを作る。
    pub fn new(width: usize, height: usize) -> Self {
        // RGBA8 で全ピクセルを白(255, 255, 255, 255)で初期化する。
        let mut pixels = vec![0; width * height * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
            chunk[3] = 255;
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// ビットマップ内の1点を黒で塗る。
    /// ここで与えられる座標はビットマップのローカル座標で、(0, 0) が左上隅を指すものとする。
    pub fn draw_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.write_pixel(x, y, [0, 0, 0, 255])
    }

    /// ビットマップ内の1点を白で塗る。
    pub fn erase_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.write_pixel(x, y, [255, 255, 255, 255])
    }

    fn write_pixel(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> DirtyRect {
        if x >= self.width || y >= self.height {
            return DirtyRect::from_inclusive_points(
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
                x.min(self.width.saturating_sub(1)),
                y.min(self.height.saturating_sub(1)),
            );
        }

        let index = (y * self.width + x) * 4;
        self.pixels[index] = rgba[0];
        self.pixels[index + 1] = rgba[1];
        self.pixels[index + 2] = rgba[2];
        self.pixels[index + 3] = rgba[3];

        DirtyRect::from_inclusive_points(x, y, x, y)
    }

    /// 2点間を結ぶ最小ストロークを描く。
    ///
    /// Bresenham の線分アルゴリズムを使い、筆圧や太さを持たない1px線として描画する。
    pub fn draw_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> DirtyRect {
        let mut x0 = from_x as isize;
        let mut y0 = from_y as isize;
        let x1 = to_x as isize;
        let y1 = to_y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 {
                let _ = self.draw_point(x0 as usize, y0 as usize);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }

            let doubled_error = 2 * error;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }

        DirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }

    pub fn erase_line(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> DirtyRect {
        let mut x0 = from_x as isize;
        let mut y0 = from_y as isize;
        let x1 = to_x as isize;
        let y1 = to_y as isize;

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 {
                let _ = self.erase_point(x0 as usize, y0 as usize);
            }

            if x0 == x1 && y0 == y1 {
                break;
            }

            let doubled_error = 2 * error;
            if doubled_error >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled_error <= dx {
                error += dx;
                y0 += sy;
            }
        }

        DirtyRect::from_inclusive_points(from_x, from_y, to_x, to_y)
    }
}

impl Default for CanvasBitmap {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

impl Document {
    pub fn active_bitmap(&self) -> Option<&CanvasBitmap> {
        self.work
            .pages
            .first()
            .and_then(|page| page.panels.first())
            .map(|panel| &panel.bitmap)
    }

    pub fn set_view_transform(&mut self, transform: CanvasViewTransform) {
        self.view_transform = transform;
    }

    pub fn set_active_tool(&mut self, tool: ToolKind) {
        self.active_tool = tool;
    }

    pub fn apply_command(&mut self, command: &Command) -> Option<DirtyRect> {
        match command {
            Command::Noop => None,
            Command::DrawPoint { x, y } => self.draw_point(*x, *y),
            Command::ErasePoint { x, y } => self.erase_point(*x, *y),
            Command::DrawStroke {
                from_x,
                from_y,
                to_x,
                to_y,
            } => self.draw_stroke(*from_x, *from_y, *to_x, *to_y),
            Command::EraseStroke {
                from_x,
                from_y,
                to_x,
                to_y,
            } => self.erase_stroke(*from_x, *from_y, *to_x, *to_y),
            Command::SetActiveTool { tool } => {
                self.set_active_tool(*tool);
                None
            }
            Command::NewDocument => {
                *self = Document::default();
                None
            }
            Command::SaveProject | Command::LoadProject => None,
        }
    }

    /// 先頭のコマのビットマップへ1点描画する。
    ///
    /// フェーズ2では対象は常に最初のページ・最初のコマに固定する。
    /// ここで与えられる座標はコマのローカル座標で、(0, 0) が左上隅を指すものとする。
    pub fn draw_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            return Some(panel.bitmap.draw_point(x, y));
        }

        None
    }

    /// 先頭のコマのビットマップへ最小ストロークを描画する。
    pub fn draw_stroke(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> Option<DirtyRect> {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            return Some(panel.bitmap.draw_line(from_x, from_y, to_x, to_y));
        }

        None
    }

    pub fn erase_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            return Some(panel.bitmap.erase_point(x, y));
        }

        None
    }

    pub fn erase_stroke(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> Option<DirtyRect> {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            return Some(panel.bitmap.erase_line(from_x, from_y, to_x, to_y));
        }

        None
    }
}

/// レイヤーツリーの最小ノード。
///
/// フェーズ0では名前付き単一ノードのみを扱い、
/// 将来的に子ノードや種別情報を追加する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerNode {
    /// レイヤーノードID。
    pub id: LayerNodeId,
    /// UIで表示するレイヤー名。
    pub name: String,
}

impl Default for LayerNode {
    fn default() -> Self {
        Self {
            id: LayerNodeId(1),
            name: "Layer 1".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小ドキュメント構造がフェーズ0の前提を満たすことを確認する。
    #[test]
    fn default_document_has_single_page_single_panel_single_layer() {
        let document = Document::default();

        assert_eq!(document.work.title, "Untitled");
        assert_eq!(document.work.pages.len(), 1);
        assert_eq!(document.work.pages[0].panels.len(), 1);
        assert_eq!(document.work.pages[0].panels[0].root_layer.name, "Layer 1");
        assert_eq!(document.work.pages[0].panels[0].bitmap.width, 64);
        assert_eq!(document.work.pages[0].panels[0].bitmap.height, 64);
    }

    /// 点描画が対象ピクセルを黒に変えることを確認する。
    #[test]
    fn draw_point_marks_target_pixel_black() {
        let mut document = Document::default();

        let dirty = document.draw_point(3, 4).expect("panel should exist");

        let bitmap = &document.work.pages[0].panels[0].bitmap;
        let index = (4 * bitmap.width + 3) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
        assert_eq!(dirty, DirtyRect::from_inclusive_points(3, 4, 3, 4));
    }

    /// ストローク描画が始点と終点の間を連続的に塗ることを確認する。
    #[test]
    fn draw_stroke_draws_continuous_line() {
        let mut document = Document::default();

        let dirty = document
            .draw_stroke(2, 2, 6, 2)
            .expect("panel should exist");

        let bitmap = &document.work.pages[0].panels[0].bitmap;
        for x in 2..=6 {
            let index = (2 * bitmap.width + x) * 4;
            assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
        }
        assert_eq!(dirty, DirtyRect::from_inclusive_points(2, 2, 6, 2));
    }

    #[test]
    fn erase_point_marks_target_pixel_white() {
        let mut document = Document::default();
        let _ = document.draw_point(3, 4);

        let dirty = document.erase_point(3, 4).expect("panel should exist");

        let bitmap = &document.work.pages[0].panels[0].bitmap;
        let index = (4 * bitmap.width + 3) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[255, 255, 255, 255]);
        assert_eq!(dirty, DirtyRect::from_inclusive_points(3, 4, 3, 4));
    }

    #[test]
    fn active_tool_defaults_to_brush() {
        let document = Document::default();

        assert_eq!(document.active_tool, ToolKind::Brush);
    }

    /// dirty矩形のunionが両方を含む最小矩形になることを確認する。
    #[test]
    fn dirty_rect_union_merges_bounds() {
        let left = DirtyRect::from_inclusive_points(2, 3, 4, 5);
        let right = DirtyRect::from_inclusive_points(6, 1, 7, 4);

        assert_eq!(
            left.union(right),
            DirtyRect {
                x: 2,
                y: 1,
                width: 6,
                height: 5,
            }
        );
    }

    /// 初期キャンバスが白背景で塗られていることを確認する。
    #[test]
    fn canvas_defaults_to_white_background() {
        let bitmap = CanvasBitmap::default();

        assert_eq!(&bitmap.pixels[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn apply_command_switches_active_tool() {
        let mut document = Document::default();

        let dirty = document.apply_command(&Command::SetActiveTool {
            tool: ToolKind::Eraser,
        });

        assert_eq!(dirty, None);
        assert_eq!(document.active_tool, ToolKind::Eraser);
    }

    #[test]
    fn apply_command_draw_stroke_returns_dirty_rect() {
        let mut document = Document::default();

        let dirty = document.apply_command(&Command::DrawStroke {
            from_x: 1,
            from_y: 1,
            to_x: 3,
            to_y: 1,
        });

        assert_eq!(dirty, Some(DirtyRect::from_inclusive_points(1, 1, 3, 1)));
        let bitmap = &document.work.pages[0].panels[0].bitmap;
        let index = (1 * bitmap.width + 2) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn dirty_rect_clamps_to_bitmap_bounds() {
        let rect = DirtyRect {
            x: 60,
            y: 62,
            width: 10,
            height: 10,
        };

        assert_eq!(
            rect.clamp_to_bitmap(64, 64),
            DirtyRect {
                x: 60,
                y: 62,
                width: 4,
                height: 2,
            }
        );
    }

    #[test]
    fn document_stores_canvas_view_transform() {
        let mut document = Document::default();
        let transform = CanvasViewTransform {
            zoom: 2.0,
            rotation_degrees: 12.5,
            pan_x: 18.0,
            pan_y: -6.0,
        };

        document.set_view_transform(transform);

        assert_eq!(document.view_transform, transform);
    }
}
