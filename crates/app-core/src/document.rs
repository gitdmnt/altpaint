use serde::{Deserialize, Serialize};

use crate::Command;

/// ホストと保存形式の間で共有する最小RGBA色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorRgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_rgba8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn hex_rgb(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl Default for ColorRgba8 {
    fn default() -> Self {
        Self::new(0, 0, 0, 255)
    }
}

/// 現在の描画ツール。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolKind {
    #[default]
    Brush,
    Pen,
    Eraser,
}

/// 外部読込可能な最小ペンプリセットを表す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PenPreset {
    pub id: String,
    pub name: String,
    #[serde(default = "default_pen_size")]
    pub size: u32,
}

impl PenPreset {
    pub fn clamp_size(&self, size: u32) -> u32 {
        size.clamp(
            1, 10000, // 将来の拡大に備えて大きな上限を許す
        )
    }
}

impl Default for PenPreset {
    fn default() -> Self {
        Self {
            id: "builtin.round-pen".to_string(),
            name: "Round Pen".to_string(),
            size: default_pen_size(),
        }
    }
}

fn default_pen_size() -> u32 {
    4
}

fn default_pen_presets() -> Vec<PenPreset> {
    vec![PenPreset::default()]
}

fn default_active_pen_preset_id() -> String {
    PenPreset::default().id
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 現在編集中の作品。
    pub work: Work,
    /// 現在の最小ツール状態。
    pub active_tool: ToolKind,
    /// 現在のブラシ色。
    #[serde(default)]
    pub active_color: ColorRgba8,
    /// 現在ロード済みのペンプリセット列。
    #[serde(default = "default_pen_presets")]
    pub pen_presets: Vec<PenPreset>,
    /// 現在アクティブなペンプリセット ID。
    #[serde(default = "default_active_pen_preset_id")]
    pub active_pen_preset_id: String,
    /// 現在の可変幅ペンサイズ。
    #[serde(default = "default_pen_size")]
    pub active_pen_size: u32,
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
    /// フェーズ9の最小レイヤー列。
    #[serde(default)]
    pub layers: Vec<RasterLayer>,
    /// 現在描画対象として選択されているレイヤー index。
    #[serde(default)]
    pub active_layer_index: usize,
    /// これまでに作成されたレイヤー数。
    #[serde(default = "default_created_layer_count")]
    pub created_layer_count: u64,
}

impl Default for Panel {
    fn default() -> Self {
        let background = RasterLayer::background(LayerNodeId(1), "Layer 1".to_string(), 64, 64);
        Self {
            id: PanelId(1),
            root_layer: LayerNode::default(),
            bitmap: background.bitmap.clone(),
            layers: vec![background],
            active_layer_index: 0,
            created_layer_count: 1,
        }
    }
}

const fn default_created_layer_count() -> u64 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Add,
}

impl BlendMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Add => "add",
        }
    }

    pub fn parse_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "add" => Some(Self::Add),
            _ => None,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Normal => Self::Multiply,
            Self::Multiply => Self::Screen,
            Self::Screen => Self::Add,
            Self::Add => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl LayerMask {
    pub fn demo(width: usize, height: usize) -> Self {
        let mut alpha = vec![255; width.saturating_mul(height)];
        for y in 0..height {
            for x in 0..width {
                if x < width / 3 || y >= (height * 5) / 6 {
                    alpha[y * width + x] = 0;
                }
            }
        }
        Self {
            width,
            height,
            alpha,
        }
    }

    fn alpha_at(&self, x: usize, y: usize) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.alpha[y * self.width + x]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterLayer {
    pub id: LayerNodeId,
    pub name: String,
    #[serde(default = "default_layer_visible")]
    pub visible: bool,
    #[serde(default)]
    pub blend_mode: BlendMode,
    pub bitmap: CanvasBitmap,
    #[serde(default)]
    pub mask: Option<LayerMask>,
}

fn default_layer_visible() -> bool {
    true
}

impl RasterLayer {
    fn background(id: LayerNodeId, name: String, width: usize, height: usize) -> Self {
        Self {
            id,
            name,
            visible: true,
            blend_mode: BlendMode::Normal,
            bitmap: CanvasBitmap::new(width, height),
            mask: None,
        }
    }

    fn transparent(id: LayerNodeId, name: String, width: usize, height: usize) -> Self {
        Self {
            id,
            name,
            visible: true,
            blend_mode: BlendMode::Normal,
            bitmap: CanvasBitmap::transparent(width, height),
            mask: None,
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

    pub fn transparent(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }

    /// ビットマップ内の1点を黒で塗る。
    /// ここで与えられる座標はビットマップのローカル座標で、(0, 0) が左上隅を指すものとする。
    pub fn draw_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.draw_point_rgba(x, y, [0, 0, 0, 255])
    }

    /// ビットマップ内の1点を任意色で塗る。
    pub fn draw_point_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> DirtyRect {
        self.write_pixel(x, y, rgba)
    }

    /// ビットマップ内の1点を白で塗る。
    pub fn erase_point(&mut self, x: usize, y: usize) -> DirtyRect {
        self.write_pixel(x, y, [255, 255, 255, 255])
    }

    pub fn draw_point_sized_rgba(
        &mut self,
        x: usize,
        y: usize,
        rgba: [u8; 4],
        size: u32,
    ) -> DirtyRect {
        self.paint_disk(x as isize, y as isize, size, rgba)
    }

    pub fn erase_point_sized(&mut self, x: usize, y: usize, size: u32) -> DirtyRect {
        self.paint_disk(x as isize, y as isize, size, [255, 255, 255, 255])
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
        self.draw_line_rgba(from_x, from_y, to_x, to_y, [0, 0, 0, 255])
    }

    pub fn draw_line_rgba(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        rgba: [u8; 4],
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
                let _ = self.draw_point_rgba(x0 as usize, y0 as usize, rgba);
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

    pub fn draw_line_sized_rgba(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        rgba: [u8; 4],
        size: u32,
    ) -> DirtyRect {
        self.paint_line_disks(from_x, from_y, to_x, to_y, size, rgba)
    }

    pub fn erase_line_sized(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
    ) -> DirtyRect {
        self.paint_line_disks(from_x, from_y, to_x, to_y, size, [255, 255, 255, 255])
    }

    fn paint_line_disks(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        size: u32,
        rgba: [u8; 4],
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
        let mut dirty = self.paint_disk(x0, y0, size, rgba);

        loop {
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
            dirty = dirty.union(self.paint_disk(x0, y0, size, rgba));
        }

        dirty
    }

    fn paint_disk(
        &mut self,
        center_x: isize,
        center_y: isize,
        size: u32,
        rgba: [u8; 4],
    ) -> DirtyRect {
        let radius = (size.max(1) as f32) * 0.5;
        let left = (center_x as f32 - radius).floor().max(0.0) as usize;
        let top = (center_y as f32 - radius).floor().max(0.0) as usize;
        let right = (center_x as f32 + radius).ceil().max(0.0) as usize;
        let bottom = (center_y as f32 + radius).ceil().max(0.0) as usize;
        let mut dirty: Option<DirtyRect> = None;

        for y in top..=bottom {
            for x in left..=right {
                let dx = x as f32 + 0.5 - (center_x as f32 + 0.5);
                let dy = y as f32 + 0.5 - (center_y as f32 + 0.5);
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let rect = self.write_pixel(x, y, rgba);
                dirty = Some(match dirty {
                    Some(current) => current.union(rect),
                    None => rect,
                });
            }
        }

        dirty.unwrap_or_else(|| DirtyRect::from_inclusive_points(left, top, right, bottom))
    }
}

impl Default for CanvasBitmap {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new(64, 64)
    }
}

impl Document {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let background =
            RasterLayer::background(LayerNodeId(1), "Layer 1".to_string(), width, height);
        let pen_presets = default_pen_presets();
        let active_pen_preset_id = pen_presets
            .first()
            .map(|preset| preset.id.clone())
            .unwrap_or_else(default_active_pen_preset_id);
        let active_pen_size = pen_presets
            .first()
            .map(|preset| preset.size)
            .unwrap_or_else(default_pen_size);

        Self {
            work: Work {
                pages: vec![Page {
                    panels: vec![Panel {
                        bitmap: background.bitmap.clone(),
                        layers: vec![background],
                        active_layer_index: 0,
                        ..Panel::default()
                    }],
                    ..Page::default()
                }],
                ..Work::default()
            },
            active_tool: ToolKind::default(),
            active_color: ColorRgba8::default(),
            pen_presets,
            active_pen_preset_id,
            active_pen_size,
            view_transform: CanvasViewTransform::default(),
        }
    }

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

    pub fn set_active_pen_size(&mut self, size: u32) {
        let size = self
            .active_pen_preset()
            .map(|preset| preset.clamp_size(size))
            .unwrap_or_else(|| size.max(1));
        self.active_pen_size = size;
    }

    pub fn set_active_color(&mut self, color: ColorRgba8) {
        self.active_color = color;
    }

    pub fn replace_pen_presets(&mut self, pen_presets: Vec<PenPreset>) {
        self.pen_presets = if pen_presets.is_empty() {
            default_pen_presets()
        } else {
            pen_presets
        };
        self.ensure_pen_state();
    }

    pub fn select_next_pen_preset(&mut self) {
        self.cycle_pen_preset(1);
    }

    pub fn select_previous_pen_preset(&mut self) {
        self.cycle_pen_preset(-1);
    }

    pub fn active_pen_preset(&self) -> Option<&PenPreset> {
        self.pen_presets
            .iter()
            .find(|preset| preset.id == self.active_pen_preset_id)
            .or_else(|| self.pen_presets.first())
    }

    pub fn active_pen_index(&self) -> usize {
        self.pen_presets
            .iter()
            .position(|preset| preset.id == self.active_pen_preset_id)
            .unwrap_or(0)
    }

    fn cycle_pen_preset(&mut self, delta: isize) {
        self.ensure_pen_state();
        if self.pen_presets.is_empty() {
            return;
        }

        let len = self.pen_presets.len() as isize;
        let next_index = (self.active_pen_index() as isize + delta).rem_euclid(len) as usize;
        let preset = &self.pen_presets[next_index];
        self.active_pen_preset_id = preset.id.clone();
        self.active_pen_size = preset.clamp_size(preset.size);
    }

    fn ensure_pen_state(&mut self) {
        if self.pen_presets.is_empty() {
            self.pen_presets = default_pen_presets();
        }

        if self
            .pen_presets
            .iter()
            .all(|preset| preset.id != self.active_pen_preset_id)
            && let Some(preset) = self.pen_presets.first()
        {
            self.active_pen_preset_id = preset.id.clone();
        }

        if let Some(preset) = self.active_pen_preset() {
            self.active_pen_size = preset.clamp_size(self.active_pen_size);
        } else {
            self.active_pen_size = default_pen_size();
        }
    }

    fn active_draw_size(&self) -> u32 {
        match self.active_tool {
            ToolKind::Brush | ToolKind::Eraser => 1,
            ToolKind::Pen => self.active_pen_size.max(1),
        }
    }

    pub fn normalize_phase9_state(&mut self) {
        for page in &mut self.work.pages {
            for panel in &mut page.panels {
                ensure_panel_layers(panel);
            }
        }
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
            Command::SetActivePenSize { size } => {
                self.set_active_pen_size(*size);
                None
            }
            Command::SelectNextPenPreset => {
                self.select_next_pen_preset();
                None
            }
            Command::SelectPreviousPenPreset => {
                self.select_previous_pen_preset();
                None
            }
            Command::ReloadPenPresets => None,
            Command::SetActiveColor { color } => {
                self.set_active_color(*color);
                None
            }
            Command::SetViewZoom { zoom } => {
                self.view_transform.zoom = zoom.clamp(0.25, 16.0);
                None
            }
            Command::PanView { delta_x, delta_y } => {
                self.view_transform.pan_x += delta_x;
                self.view_transform.pan_y += delta_y;
                None
            }
            Command::ResetView => {
                self.view_transform = CanvasViewTransform::default();
                None
            }
            Command::AddRasterLayer => {
                self.add_raster_layer();
                None
            }
            Command::RemoveActiveLayer => {
                self.remove_active_layer();
                None
            }
            Command::SelectLayer { index } => {
                self.select_layer(*index);
                None
            }
            Command::RenameActiveLayer { name } => {
                self.rename_active_layer(name);
                None
            }
            Command::MoveLayer {
                from_index,
                to_index,
            } => {
                self.move_layer(*from_index, *to_index);
                None
            }
            Command::SelectNextLayer => {
                self.select_next_layer();
                None
            }
            Command::CycleActiveLayerBlendMode => {
                self.cycle_active_layer_blend_mode();
                None
            }
            Command::SetActiveLayerBlendMode { mode } => {
                self.set_active_layer_blend_mode(*mode);
                None
            }
            Command::ToggleActiveLayerVisibility => {
                self.toggle_active_layer_visibility();
                None
            }
            Command::ToggleActiveLayerMask => {
                self.toggle_active_layer_mask();
                None
            }
            Command::NewDocument => {
                *self = Document::default();
                None
            }
            Command::NewDocumentSized { width, height } => {
                *self = Document::new(*width, *height);
                None
            }
            Command::SaveProject
            | Command::SaveProjectAs
            | Command::SaveProjectToPath { .. }
            | Command::LoadProject
            | Command::LoadProjectFromPath { .. } => None,
        }
    }

    /// 先頭のコマのビットマップへ1点描画する。
    ///
    /// フェーズ2では対象は常に最初のページ・最初のコマに固定する。
    /// ここで与えられる座標はコマのローカル座標で、(0, 0) が左上隅を指すものとする。
    pub fn draw_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size();
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, color, false, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
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
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size();
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty =
                draw_line_on_active_layer(panel, from_x, from_y, to_x, to_y, color, false, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    pub fn erase_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        let size = self.active_draw_size();
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, [0, 0, 0, 0], true, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
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
        let size = self.active_draw_size();
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_line_on_active_layer(
                panel,
                from_x,
                from_y,
                to_x,
                to_y,
                [0, 0, 0, 0],
                true,
                size,
            );
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    pub fn add_raster_layer(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            panel.created_layer_count = panel.created_layer_count.saturating_add(1);
            let next_index = panel.created_layer_count;
            let (width, height) = (panel.bitmap.width, panel.bitmap.height);
            panel.layers.push(RasterLayer::transparent(
                LayerNodeId(next_index),
                format!("Layer {next_index}"),
                width,
                height,
            ));
            panel.active_layer_index = panel.layers.len().saturating_sub(1);
            sync_root_layer_summary(panel);
        }
    }

    pub fn remove_active_layer(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if panel.layers.len() <= 1 {
                return;
            }
            panel.layers.remove(panel.active_layer_index);
            panel.active_layer_index = panel
                .active_layer_index
                .min(panel.layers.len().saturating_sub(1));
            panel.bitmap = composite_panel_bitmap(panel);
            sync_root_layer_summary(panel);
        }
    }

    pub fn select_layer(&mut self, index: usize) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            panel.active_layer_index = index.min(panel.layers.len().saturating_sub(1));
            sync_root_layer_summary(panel);
        }
    }

    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.name = name.to_string();
                sync_root_layer_summary(panel);
            }
        }
    }

    pub fn move_layer(&mut self, from_index: usize, to_index: usize) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if panel.layers.len() <= 1 {
                return;
            }

            let last_index = panel.layers.len().saturating_sub(1);
            let from_index = from_index.min(last_index);
            let to_index = to_index.min(last_index);
            if from_index == to_index {
                return;
            }

            let moved = panel.layers.remove(from_index);
            panel.layers.insert(to_index, moved);

            panel.active_layer_index = match panel.active_layer_index {
                index if index == from_index => to_index,
                index if from_index < index && index <= to_index => index.saturating_sub(1),
                index if to_index <= index && index < from_index => index + 1,
                index => index,
            };

            panel.bitmap = composite_panel_bitmap(panel);
            sync_root_layer_summary(panel);
        }
    }

    pub fn select_next_layer(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            panel.active_layer_index = (panel.active_layer_index + 1) % panel.layers.len().max(1);
            sync_root_layer_summary(panel);
        }
    }

    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = mode;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.visible = !layer.visible;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    pub fn toggle_active_layer_mask(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.mask = if layer.mask.is_some() {
                    None
                } else {
                    Some(LayerMask::demo(layer.bitmap.width, layer.bitmap.height))
                };
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }
}

fn ensure_panel_layers(panel: &mut Panel) {
    let mut repaired = false;
    if panel.layers.is_empty() {
        panel.layers.push(RasterLayer::background(
            panel.root_layer.id,
            panel.root_layer.name.clone(),
            panel.bitmap.width,
            panel.bitmap.height,
        ));
        if let Some(layer) = panel.layers.first_mut() {
            layer.bitmap = panel.bitmap.clone();
        }
        repaired = true;
    }
    panel.created_layer_count = panel
        .created_layer_count
        .max(panel.layers.len() as u64)
        .max(1);
    panel.active_layer_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    sync_root_layer_summary(panel);
    if repaired {
        panel.bitmap = composite_panel_bitmap(panel);
    }
}

fn sync_root_layer_summary(panel: &mut Panel) {
    if let Some(layer) = panel.layers.get(panel.active_layer_index) {
        panel.root_layer.id = layer.id;
        panel.root_layer.name = layer.name.clone();
    }
}

fn draw_on_active_layer(
    panel: &mut Panel,
    x: usize,
    y: usize,
    color: [u8; 4],
    erase: bool,
    size: u32,
) -> DirtyRect {
    let active_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer.bitmap.erase_point_sized(x, y, size)
        } else {
            layer.bitmap.draw_point_sized_rgba(x, y, color, size)
        }
    } else {
        layer.bitmap.draw_point_sized_rgba(x, y, color, size)
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line_on_active_layer(
    panel: &mut Panel,
    from_x: usize,
    from_y: usize,
    to_x: usize,
    to_y: usize,
    color: [u8; 4],
    erase: bool,
    size: u32,
) -> DirtyRect {
    let active_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer
                .bitmap
                .erase_line_sized(from_x, from_y, to_x, to_y, size)
        } else {
            layer
                .bitmap
                .draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size)
        }
    } else {
        layer
            .bitmap
            .draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size)
    }
}

fn composite_panel_bitmap(panel: &Panel) -> CanvasBitmap {
    let width = panel.bitmap.width.max(1);
    let height = panel.bitmap.height.max(1);
    let mut result = CanvasBitmap::transparent(width, height);
    for layer in &panel.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(
            &mut result,
            layer,
            DirtyRect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
    }
    result
}

fn composite_panel_bitmap_region(panel: &mut Panel, dirty: DirtyRect) {
    let dirty = dirty.clamp_to_bitmap(panel.bitmap.width.max(1), panel.bitmap.height.max(1));
    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let index = (y * panel.bitmap.width + x) * 4;
            panel.bitmap.pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
    }

    for layer in &panel.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(&mut panel.bitmap, layer, dirty);
    }
}

fn composite_layer_region_into(target: &mut CanvasBitmap, layer: &RasterLayer, dirty: DirtyRect) {
    let dirty = dirty.clamp_to_bitmap(
        target.width.min(layer.bitmap.width).max(1),
        target.height.min(layer.bitmap.height).max(1),
    );
    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let index = (y * target.width + x) * 4;
            let mut src = [
                layer.bitmap.pixels[index],
                layer.bitmap.pixels[index + 1],
                layer.bitmap.pixels[index + 2],
                layer.bitmap.pixels[index + 3],
            ];
            if let Some(mask) = &layer.mask {
                src[3] = ((src[3] as u16 * mask.alpha_at(x, y) as u16) / 255) as u8;
            }
            let dst = [
                target.pixels[index],
                target.pixels[index + 1],
                target.pixels[index + 2],
                target.pixels[index + 3],
            ];
            let blended = blend_pixel(dst, src, layer.blend_mode);
            target.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
}

fn blend_pixel(dst: [u8; 4], src: [u8; 4], mode: BlendMode) -> [u8; 4] {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return dst;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let blend_channel = |dst_c: u8, src_c: u8| -> f32 {
        let d = dst_c as f32 / 255.0;
        let s = src_c as f32 / 255.0;
        match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => 1.0 - (1.0 - s) * (1.0 - d),
            BlendMode::Add => (s + d).min(1.0),
        }
    };
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let dst_c = dst[channel] as f32 / 255.0;
        let mixed = blend_channel(dst[channel], src[channel]);
        let out_c = mixed * src_a + dst_c * (1.0 - src_a);
        out[channel] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
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

    #[test]
    fn active_color_defaults_to_black() {
        let document = Document::default();

        assert_eq!(document.active_color, ColorRgba8::new(0, 0, 0, 255));
    }

    #[test]
    fn default_document_has_round_pen_preset() {
        let document = Document::default();

        assert_eq!(document.pen_presets.len(), 1);
        assert_eq!(document.active_pen_preset_id, "builtin.round-pen");
        assert_eq!(document.active_pen_size, 4);
    }

    #[test]
    fn draw_point_uses_active_color() {
        let mut document = Document::default();
        document.set_active_color(ColorRgba8::new(0xe5, 0x39, 0x35, 0xff));

        let _ = document.draw_point(3, 4);

        let bitmap = &document.work.pages[0].panels[0].bitmap;
        let index = (4 * bitmap.width + 3) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0xe5, 0x39, 0x35, 0xff]);
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
            tool: ToolKind::Pen,
        });

        assert_eq!(dirty, None);
        assert_eq!(document.active_tool, ToolKind::Pen);
    }

    #[test]
    fn apply_command_updates_pen_size() {
        let mut document = Document::default();

        let dirty = document.apply_command(&Command::SetActivePenSize { size: 12 });

        assert_eq!(dirty, None);
        assert_eq!(document.active_pen_size, 12);
    }

    #[test]
    fn apply_command_switches_active_color() {
        let mut document = Document::default();

        let dirty = document.apply_command(&Command::SetActiveColor {
            color: ColorRgba8::new(0x43, 0xa0, 0x47, 0xff),
        });

        assert_eq!(dirty, None);
        assert_eq!(
            document.active_color,
            ColorRgba8::new(0x43, 0xa0, 0x47, 0xff)
        );
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
        let index = (bitmap.width + 2) * 4;
        assert_eq!(&bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn pen_draws_wider_than_single_pixel_brush() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::SetActiveTool {
            tool: ToolKind::Pen,
        });
        let _ = document.apply_command(&Command::SetActivePenSize { size: 5 });

        let dirty = document.draw_point(10, 10).expect("panel should exist");

        assert!(dirty.width >= 5);
        assert!(dirty.height >= 5);
        let bitmap = &document.work.pages[0].panels[0].bitmap;
        let center = (10 * bitmap.width + 10) * 4;
        let edge = (10 * bitmap.width + 8) * 4;
        assert_eq!(&bitmap.pixels[center..center + 4], &[0, 0, 0, 255]);
        assert_eq!(&bitmap.pixels[edge..edge + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn cycling_pen_presets_updates_active_size() {
        let mut document = Document::default();
        document.replace_pen_presets(vec![
            PenPreset {
                id: "fine".to_string(),
                name: "Fine".to_string(),
                size: 2,
            },
            PenPreset {
                id: "bold".to_string(),
                name: "Bold".to_string(),
                size: 9,
            },
        ]);

        document.select_next_pen_preset();

        assert_eq!(document.active_pen_preset_id, "bold");
        assert_eq!(document.active_pen_size, 9);
    }

    #[test]
    fn document_new_uses_requested_canvas_size() {
        let document = Document::new(320, 240);

        let bitmap = document.active_bitmap().expect("bitmap exists");
        assert_eq!((bitmap.width, bitmap.height), (320, 240));
    }

    #[test]
    fn apply_command_new_document_sized_replaces_bitmap_dimensions() {
        let mut document = Document::default();

        let dirty = document.apply_command(&Command::NewDocumentSized {
            width: 512,
            height: 384,
        });

        assert_eq!(dirty, None);
        let bitmap = document.active_bitmap().expect("bitmap exists");
        assert_eq!((bitmap.width, bitmap.height), (512, 384));
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

    #[test]
    fn add_raster_layer_selects_new_layer() {
        let mut document = Document::default();

        let _ = document.apply_command(&Command::AddRasterLayer);

        let panel = &document.work.pages[0].panels[0];
        assert_eq!(panel.layers.len(), 2);
        assert_eq!(panel.active_layer_index, 1);
        assert_eq!(panel.layers[1].name, "Layer 2");
    }

    #[test]
    fn add_raster_layer_uses_created_layer_counter_for_names() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.apply_command(&Command::RemoveActiveLayer);

        let _ = document.apply_command(&Command::AddRasterLayer);

        let panel = &document.work.pages[0].panels[0];
        let names = panel
            .layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Layer 1", "Layer 2", "Layer 4"]);
        assert_eq!(panel.created_layer_count, 4);
    }

    #[test]
    fn remove_active_layer_keeps_at_least_one_layer() {
        let mut document = Document::default();

        let _ = document.apply_command(&Command::RemoveActiveLayer);

        let panel = &document.work.pages[0].panels[0];
        assert_eq!(panel.layers.len(), 1);
        assert_eq!(panel.active_layer_index, 0);
    }

    #[test]
    fn remove_active_layer_selects_remaining_layer() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.apply_command(&Command::AddRasterLayer);

        let _ = document.apply_command(&Command::RemoveActiveLayer);

        let panel = &document.work.pages[0].panels[0];
        assert_eq!(panel.layers.len(), 2);
        assert_eq!(panel.active_layer_index, 1);
        assert_eq!(panel.layers[1].name, "Layer 2");
    }

    #[test]
    fn move_layer_reorders_layers_and_tracks_active_selection() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.apply_command(&Command::AddRasterLayer);

        let _ = document.apply_command(&Command::MoveLayer {
            from_index: 2,
            to_index: 0,
        });

        let panel = &document.work.pages[0].panels[0];
        let names = panel
            .layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Layer 3", "Layer 1", "Layer 2"]);
        assert_eq!(panel.active_layer_index, 0);
    }

    #[test]
    fn rename_active_layer_updates_selected_layer_name() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);

        let _ = document.apply_command(&Command::RenameActiveLayer {
            name: "Ink".to_string(),
        });

        let panel = &document.work.pages[0].panels[0];
        assert_eq!(panel.layers[1].name, "Ink");
        assert_eq!(panel.root_layer.name, "Ink");
    }

    #[test]
    fn set_active_layer_blend_mode_sets_requested_mode() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::SetActiveLayerBlendMode {
            mode: BlendMode::Screen,
        });

        let panel = &document.work.pages[0].panels[0];
        assert_eq!(panel.layers[0].blend_mode, BlendMode::Screen);
    }

    #[test]
    fn toggle_active_layer_visibility_reveals_underlying_layer() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.draw_point(5, 5);

        let visible_bitmap = document.active_bitmap().expect("bitmap exists").clone();
        let _ = document.apply_command(&Command::ToggleActiveLayerVisibility);
        let hidden_bitmap = document.active_bitmap().expect("bitmap exists");

        let index = (5 * visible_bitmap.width + 5) * 4;
        assert_eq!(&visible_bitmap.pixels[index..index + 4], &[0, 0, 0, 255]);
        assert_eq!(
            &hidden_bitmap.pixels[index..index + 4],
            &[255, 255, 255, 255]
        );
    }

    #[test]
    fn toggle_active_layer_mask_applies_demo_mask() {
        let mut document = Document::default();
        let _ = document.apply_command(&Command::AddRasterLayer);
        let _ = document.draw_point(1, 1);

        let before_mask = document.active_bitmap().expect("bitmap exists").clone();
        let _ = document.apply_command(&Command::ToggleActiveLayerMask);
        let after_mask = document.active_bitmap().expect("bitmap exists");

        let index = (before_mask.width + 1) * 4;
        assert_eq!(&before_mask.pixels[index..index + 4], &[0, 0, 0, 255]);
        assert_eq!(&after_mask.pixels[index..index + 4], &[255, 255, 255, 255]);
    }
}
