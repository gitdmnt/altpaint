use serde::{Deserialize, Serialize};

use crate::Command;

mod bitmap;
mod layer_ops;
mod pen_state;

use self::layer_ops::ensure_panel_layers;

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
mod tests;
