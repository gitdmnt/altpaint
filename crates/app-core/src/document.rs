use serde::{Deserialize, Serialize};

use crate::{DocumentCommand, SessionCommand};
use geometry::KomaLocalPoint;
use raster::{BlendMode, RgbaBitmap as CanvasBitmap};

mod layer_ops;
mod tool_state;

use self::layer_ops::{composite_koma_bitmap, ensure_koma_layers};

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
    Pen,
    Eraser,
    Bucket,
    LassoBucket,
    KomaRect,
}

impl ToolKind {
    /// ホスト↔パネル間で交換する wire 名 (snake_case)。
    pub const fn as_str(self) -> &'static str {
        match self {
            ToolKind::Pen => "pen",
            ToolKind::Eraser => "eraser",
            ToolKind::Bucket => "bucket",
            ToolKind::LassoBucket => "lasso_bucket",
            ToolKind::KomaRect => "koma_rect",
        }
    }

    /// wire 名を `ToolKind` へ解釈する。未知の名前は `None`。
    pub fn from_wire(name: &str) -> Option<Self> {
        match name {
            "pen" => Some(ToolKind::Pen),
            "eraser" => Some(ToolKind::Eraser),
            "bucket" => Some(ToolKind::Bucket),
            "lasso_bucket" => Some(ToolKind::LassoBucket),
            "koma_rect" => Some(ToolKind::KomaRect),
            _ => None,
        }
    }

    /// ステータスバー等の UI 表示用ラベル (PascalCase)。
    pub const fn display_label(self) -> &'static str {
        match self {
            ToolKind::Pen => "Pen",
            ToolKind::Eraser => "Eraser",
            ToolKind::Bucket => "Bucket",
            ToolKind::LassoBucket => "LassoBucket",
            ToolKind::KomaRect => "KomaRect",
        }
    }
}

/// ツール設定 UI の入力種別。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSettingControl {
    Slider,
    Checkbox,
}

/// 描画ツールが公開する設定項目定義。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSettingDefinition {
    pub key: String,
    pub label: String,
    pub control: ToolSettingControl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i32>,
}

impl ToolSettingDefinition {
    pub fn slider(key: impl Into<String>, label: impl Into<String>, min: i32, max: i32) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            control: ToolSettingControl::Slider,
            min: Some(min),
            max: Some(max),
        }
    }

    pub fn checkbox(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            control: ToolSettingControl::Checkbox,
            min: None,
            max: None,
        }
    }
}

/// `tools/` 配下からロードされる描画ツール定義。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    pub provider_plugin_id: String,
    #[serde(default = "default_bitmap_plugin_id")]
    pub drawing_plugin_id: String,
    #[serde(default)]
    pub settings: Vec<ToolSettingDefinition>,
    #[serde(default)]
    pub children: Vec<ToolDefinition>,
}

impl ToolDefinition {
    pub fn supports_setting(&self, key: &str) -> bool {
        self.settings.iter().any(|setting| setting.key == key)
    }
}

pub const DEFAULT_PAGE_WIDTH: usize = 2894;
pub const DEFAULT_PAGE_HEIGHT: usize = 4093;

/// ページ 1 辺の最大ピクセル数。
pub const MAX_PAGE_DIMENSION: usize = 8192;
/// ページ全体の最大ピクセル数。
pub const MAX_PAGE_PIXELS: usize = 16_777_216;

/// `"WIDTHxHEIGHT"` 形式の文字列をページ寸法へ解釈する。
///
/// 区切りは `x` / `×` / `,` / `;` / 空白を許容し、上限
/// (`MAX_PAGE_DIMENSION` / `MAX_PAGE_PIXELS`) を超える寸法や 0 は `None`。
pub fn parse_document_size(input: &str) -> Option<(usize, usize)> {
    let normalized = input.replace(['×', ',', ';'], "x");
    let parts = normalized
        .split(|ch: char| ch == 'x' || ch.is_whitespace())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }

    let width = parts[0].parse::<usize>().ok()?;
    let height = parts[1].parse::<usize>().ok()?;
    if width == 0
        || height == 0
        || width > MAX_PAGE_DIMENSION
        || height > MAX_PAGE_DIMENSION
        || width.saturating_mul(height) > MAX_PAGE_PIXELS
    {
        return None;
    }

    Some((width, height))
}

/// 外部読込可能な最小ペンプリセットを表す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenPreset {
    pub id: String,
    pub name: String,
    #[serde(default = "default_pen_plugin_id")]
    pub plugin_id: String,
    #[serde(default = "default_pen_size")]
    pub size: u32,
    #[serde(default = "default_pen_pressure_enabled")]
    pub pressure_enabled: bool,
    #[serde(default = "default_pen_antialias")]
    pub antialias: bool,
    #[serde(default)]
    pub stabilization: u8,
    #[serde(default)]
    pub engine: PenRuntimeEngine,
    #[serde(default = "default_spacing_percent")]
    pub spacing_percent: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default = "default_pen_opacity")]
    pub opacity: f32,
    #[serde(default = "default_pen_flow")]
    pub flow: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<PenTipBitmap>,
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
            plugin_id: default_pen_plugin_id(),
            size: default_pen_size(),
            pressure_enabled: default_pen_pressure_enabled(),
            antialias: default_pen_antialias(),
            stabilization: 0,
            engine: PenRuntimeEngine::default(),
            spacing_percent: default_spacing_percent(),
            rotation_degrees: 0.0,
            opacity: default_pen_opacity(),
            flow: default_pen_flow(),
            tip: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PenRuntimeEngine {
    #[default]
    Stamp,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PenTipBitmap {
    AlphaMask8 {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    Rgba8 {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    PngBlob {
        width: u32,
        height: u32,
        png: Vec<u8>,
    },
}

impl PenTipBitmap {
    pub fn width(&self) -> u32 {
        match self {
            Self::AlphaMask8 { width, .. }
            | Self::Rgba8 { width, .. }
            | Self::PngBlob { width, .. } => *width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::AlphaMask8 { height, .. }
            | Self::Rgba8 { height, .. }
            | Self::PngBlob { height, .. } => *height,
        }
    }
}

fn default_pen_size() -> u32 {
    4
}

fn default_pen_plugin_id() -> String {
    "builtin.bitmap".to_string()
}

fn default_bitmap_plugin_id() -> String {
    "builtin.bitmap".to_string()
}

fn default_pen_pressure_enabled() -> bool {
    true
}

fn default_pen_antialias() -> bool {
    true
}

fn default_spacing_percent() -> f32 {
    25.0
}

fn default_pen_opacity() -> f32 {
    1.0
}

fn default_pen_flow() -> f32 {
    1.0
}

fn default_pen_presets() -> Vec<PenPreset> {
    vec![PenPreset::default()]
}

fn default_active_pen_preset_id() -> String {
    PenPreset::default().id
}

fn default_active_page_index() -> usize {
    0
}

fn default_tool_catalog() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "builtin.pen".to_string(),
            name: "Pen".to_string(),
            kind: ToolKind::Pen,
            provider_plugin_id: "plugins/default-pens-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: vec![
                ToolSettingDefinition::slider("size", "太さ", 1, 10_000),
                ToolSettingDefinition::checkbox("pressure_enabled", "筆圧"),
                ToolSettingDefinition::checkbox("antialias", "なめらか"),
                ToolSettingDefinition::slider("stabilization", "手ぶれ補正", 0, 100),
            ],
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.eraser".to_string(),
            name: "Eraser".to_string(),
            kind: ToolKind::Eraser,
            provider_plugin_id: "plugins/default-erasers-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: vec![
                ToolSettingDefinition::slider("size", "太さ", 1, 10_000),
                ToolSettingDefinition::checkbox("antialias", "なめらか"),
                ToolSettingDefinition::slider("stabilization", "手ぶれ補正", 0, 100),
            ],
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.bucket".to_string(),
            name: "Bucket".to_string(),
            kind: ToolKind::Bucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.lasso-bucket".to_string(),
            name: "Lasso Bucket".to_string(),
            kind: ToolKind::LassoBucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.koma-rect".to_string(),
            name: "Koma Rect".to_string(),
            kind: ToolKind::KomaRect,
            provider_plugin_id: "plugins/default-koma-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
    ]
}

fn default_active_tool_id() -> String {
    default_tool_catalog()
        .first()
        .map(|tool| tool.id.clone())
        .unwrap_or_else(|| "builtin.pen".to_string())
}

fn default_active_koma_index() -> usize {
    0
}

fn default_page_width() -> usize {
    DEFAULT_PAGE_WIDTH
}

fn default_page_height() -> usize {
    DEFAULT_PAGE_HEIGHT
}

/// 作品を識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkId(pub u64);

/// ページを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// コマを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KomaId(pub u64);

/// レイヤーノードを識別する最小ID型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerNodeId(pub u64);

/// アプリケーションの永続状態全体を表すルートドキュメント。
///
/// 単一の `Work` と、ツール・ペン・表示変換などの編集状態を保持する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 現在編集中の作品。
    pub work: Work,
    /// 現在の最小ツール状態。
    pub active_tool: ToolKind,
    /// 現在アクティブな登録ツール ID。
    #[serde(default = "default_active_tool_id")]
    pub active_tool_id: String,
    /// 現在アクティブな子ツール ID。空文字列は未選択を表す。
    #[serde(default)]
    pub active_child_tool_id: String,
    /// 現在のブラシ色。
    #[serde(default)]
    pub active_color: ColorRgba8,
    /// 起動時に `tools/` から読み込まれるツールカタログ。
    #[serde(default = "default_tool_catalog")]
    pub tool_catalog: Vec<ToolDefinition>,
    /// 現在ロード済みのペンプリセット列。
    #[serde(default = "default_pen_presets")]
    pub pen_presets: Vec<PenPreset>,
    /// 現在アクティブなペンプリセット ID。
    #[serde(default = "default_active_pen_preset_id")]
    pub active_pen_preset_id: String,
    /// 現在の可変幅ペンサイズ。
    #[serde(default = "default_pen_size")]
    pub active_pen_size: u32,
    /// 現在アクティブなページ index。
    #[serde(default = "default_active_page_index")]
    pub active_page_index: usize,
    /// 現在アクティブなコマ index。
    #[serde(default = "default_active_koma_index")]
    pub active_koma_index: usize,
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
    /// ページの基準幅。
    #[serde(default = "default_page_width")]
    pub width: usize,
    /// ページの基準高さ。
    #[serde(default = "default_page_height")]
    pub height: usize,
    /// ページ内に含まれるコマ列。
    pub komas: Vec<Koma>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            id: PageId(1),
            width: default_page_width(),
            height: default_page_height(),
            komas: vec![Koma::default()],
        }
    }
}

/// ページ内のコマ矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KomaBounds {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl KomaBounds {
    pub fn full_page(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: width.max(1),
            height: height.max(1),
        }
    }

    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width)
            && y < self.y.saturating_add(self.height)
    }

    pub fn contains_canvas_point(self, point: geometry::PagePoint) -> bool {
        self.contains(point.x, point.y)
    }

    pub fn canvas_to_koma_local(
        self,
        point: geometry::PagePoint,
    ) -> Option<geometry::KomaLocalPoint> {
        self.contains_canvas_point(point)
            .then_some(geometry::KomaLocalPoint::new(
                point.x.saturating_sub(self.x),
                point.y.saturating_sub(self.y),
            ))
    }

    pub fn clamp_canvas_point(self, point: geometry::PagePoint) -> Option<geometry::PagePoint> {
        if self.is_empty() {
            return None;
        }

        let max_x = self.x.saturating_add(self.width.saturating_sub(1));
        let max_y = self.y.saturating_add(self.height.saturating_sub(1));
        Some(geometry::PagePoint::new(
            point.x.clamp(self.x, max_x),
            point.y.clamp(self.y, max_y),
        ))
    }

    pub fn koma_local_to_canvas(
        self,
        point: geometry::KomaLocalPoint,
    ) -> Option<geometry::PagePoint> {
        (point.x < self.width && point.y < self.height).then_some(geometry::PagePoint::new(
            self.x.saturating_add(point.x),
            self.y.saturating_add(point.y),
        ))
    }
}

impl Default for KomaBounds {
    fn default() -> Self {
        Self::full_page(DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

/// 漫画のコマを表す最小単位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Koma {
    /// コマID。
    pub id: KomaId,
    /// ページ内でのコマ矩形。
    #[serde(default)]
    pub bounds: KomaBounds,
    /// レイヤー列の合成結果キャッシュ (`layers` から導出される派生データ)。
    pub composite_cache: CanvasBitmap,
    /// コマを構成するラスタレイヤー列。index 0 が最下層。
    #[serde(default)]
    pub layers: Vec<RasterLayer>,
    /// 現在描画対象として選択されているレイヤー index。
    #[serde(default)]
    pub active_layer_index: usize,
    /// これまでに作成されたレイヤー数。
    #[serde(default = "default_created_layer_count")]
    pub created_layer_count: u64,
}

impl Default for Koma {
    fn default() -> Self {
        Self::new_blank(KomaId(1), DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

impl Koma {
    pub fn new_blank(id: KomaId, width: usize, height: usize) -> Self {
        let background = RasterLayer::background(
            LayerNodeId(1),
            "Layer 1".to_string(),
            width.max(1),
            height.max(1),
        );
        Self {
            id,
            bounds: KomaBounds::full_page(width, height),
            composite_cache: background.bitmap.clone(),
            layers: vec![background],
            active_layer_index: 0,
            created_layer_count: 1,
        }
    }
}

const fn default_created_layer_count() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl LayerMask {
    /// 指定座標のマスク alpha を返す。範囲外は 0 (完全マスク) を返す。
    pub fn alpha_at(&self, x: usize, y: usize) -> u8 {
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
            bitmap: CanvasBitmap::opaque_white(width, height),
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

/// 将来のズーム・回転・パンに備える表示変換状態。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewTransform {
    pub zoom: f32,
    pub rotation_degrees: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Default for CanvasViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            rotation_degrees: 0.0,
            pan_x: 0.0,
            pan_y: 0.0,
            flip_x: false,
            flip_y: false,
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new(DEFAULT_PAGE_WIDTH, DEFAULT_PAGE_HEIGHT)
    }
}

impl Document {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let tool_catalog = default_tool_catalog();
        let active_tool_id = tool_catalog
            .first()
            .map(|tool| tool.id.clone())
            .unwrap_or_else(default_active_tool_id);
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
                    width,
                    height,
                    komas: vec![Koma::new_blank(KomaId(1), width, height)],
                    ..Page::default()
                }],
                ..Work::default()
            },
            active_tool: ToolKind::default(),
            active_tool_id,
            active_child_tool_id: String::new(),
            active_color: ColorRgba8::default(),
            tool_catalog,
            pen_presets,
            active_pen_preset_id,
            active_pen_size,
            active_page_index: default_active_page_index(),
            active_koma_index: default_active_koma_index(),
            view_transform: CanvasViewTransform::default(),
        }
    }

    pub fn active_page_index(&self) -> usize {
        self.active_page_index
            .min(self.work.pages.len().saturating_sub(1))
    }

    pub fn active_koma_index(&self) -> usize {
        self.active_page()
            .map(|page| {
                self.active_koma_index
                    .min(page.komas.len().saturating_sub(1))
            })
            .unwrap_or(0)
    }

    pub fn active_page(&self) -> Option<&Page> {
        self.work.pages.get(self.active_page_index())
    }

    pub fn active_page_mut(&mut self) -> Option<&mut Page> {
        let index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        self.work.pages.get_mut(index)
    }

    pub fn active_koma(&self) -> Option<&Koma> {
        let koma_index = self.active_koma_index();
        self.active_page()
            .and_then(|page| page.komas.get(koma_index))
    }

    pub fn active_koma_mut(&mut self) -> Option<&mut Koma> {
        let page_index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        let koma_index = self.active_koma_index;
        self.work.pages.get_mut(page_index).and_then(|page| {
            let clamped_index = koma_index.min(page.komas.len().saturating_sub(1));
            page.komas.get_mut(clamped_index)
        })
    }

    pub fn active_bitmap(&self) -> Option<&CanvasBitmap> {
        self.active_koma().map(|koma| &koma.composite_cache)
    }

    pub fn active_layer_bitmap(&self) -> Option<&CanvasBitmap> {
        let koma = self.active_koma()?;
        koma
            .layers
            .get(
                koma
                    .active_layer_index
                    .min(koma.layers.len().saturating_sub(1)),
            )
            .map(|layer| &layer.bitmap)
    }

    pub fn active_layer_is_background(&self) -> Option<bool> {
        let koma = self.active_koma()?;
        Some(koma.active_layer_index == 0)
    }

    pub fn active_koma_contains_canvas_point(&self, point: geometry::PagePoint) -> bool {
        self.active_koma_bounds()
            .is_some_and(|bounds| bounds.contains_canvas_point(point))
    }

    pub fn active_koma_contains_local_point(&self, point: KomaLocalPoint) -> bool {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.koma_local_to_canvas(point))
            .is_some()
    }

    pub fn active_koma_canvas_to_local(
        &self,
        point: geometry::PagePoint,
    ) -> Option<KomaLocalPoint> {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.canvas_to_koma_local(point))
    }

    pub fn active_koma_local_to_canvas(
        &self,
        point: KomaLocalPoint,
    ) -> Option<geometry::PagePoint> {
        self.active_koma_bounds()
            .and_then(|bounds| bounds.koma_local_to_canvas(point))
    }

    pub fn tool_definition(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.tool_catalog.iter().find(|tool| tool.id == tool_id)
    }

    pub fn active_tool_definition(&self) -> Option<&ToolDefinition> {
        self.tool_definition(&self.active_tool_id)
            .or_else(|| {
                self.tool_catalog
                    .iter()
                    .find(|tool| tool.kind == self.active_tool)
            })
            .or_else(|| self.tool_catalog.first())
    }

    /// アクティブな子ツール definition を返す。
    pub fn active_child_tool_definition(&self) -> Option<&ToolDefinition> {
        let parent = self.active_tool_definition()?;
        if self.active_child_tool_id.is_empty() {
            return None;
        }
        parent.children.iter().find(|c| c.id == self.active_child_tool_id)
    }

    /// 指定された親・子 ID の子ツール definition を返す。
    pub fn child_tool_definition(&self, parent_id: &str, child_id: &str) -> Option<&ToolDefinition> {
        let parent = self.tool_definition(parent_id)?;
        parent.children.iter().find(|c| c.id == child_id)
    }

    pub fn active_tool_provider_plugin_id(&self) -> Option<&str> {
        self.active_tool_definition()
            .map(|tool| tool.provider_plugin_id.as_str())
    }

    pub fn active_tool_drawing_plugin_id(&self) -> Option<&str> {
        self.active_tool_definition()
            .map(|tool| tool.drawing_plugin_id.as_str())
    }

    pub fn active_tool_settings(&self) -> &[ToolSettingDefinition] {
        self.active_tool_definition()
            .map(|tool| tool.settings.as_slice())
            .unwrap_or(&[])
    }

    pub fn active_koma_bounds(&self) -> Option<KomaBounds> {
        self.active_koma().map(|koma| koma.bounds)
    }

    pub fn active_page_koma_count(&self) -> usize {
        self.active_page()
            .map(|page| page.komas.len())
            .unwrap_or(0)
    }

    pub fn active_page_dimensions(&self) -> (usize, usize) {
        self.active_page()
            .map(|page| (page.width.max(1), page.height.max(1)))
            .unwrap_or((1, 1))
    }

    pub fn select_koma(&mut self, index: usize) {
        let page_index = self.active_page_index();
        if let Some(page) = self.work.pages.get(page_index) {
            self.active_koma_index = index.min(page.komas.len().saturating_sub(1));
        }
    }

    pub fn select_next_koma(&mut self) {
        if let Some(page) = self.active_page() {
            let koma_count = page.komas.len().max(1);
            self.active_koma_index = (self.active_koma_index() + 1) % koma_count;
        }
    }

    pub fn select_previous_koma(&mut self) {
        if let Some(page) = self.active_page() {
            let koma_count = page.komas.len().max(1);
            self.active_koma_index = (self.active_koma_index() + koma_count - 1) % koma_count;
        }
    }

    pub fn add_koma(&mut self) {
        let next_id = next_koma_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };

        let next_count = page.komas.len().saturating_add(1);
        let next_bounds = default_koma_grid_bounds(page.width, page.height, next_count);
        let new_bounds = next_bounds
            .last()
            .copied()
            .unwrap_or_else(|| KomaBounds::full_page(page.width, page.height));
        let mut koma = Koma::new_blank(next_id, new_bounds.width, new_bounds.height);
        koma.bounds = new_bounds;
        page.komas.push(koma);
        relayout_page_komas(page);
        self.active_koma_index = page.komas.len().saturating_sub(1);
        self.focus_active_koma_view();
    }

    pub fn create_koma(&mut self, bounds: KomaBounds) {
        let next_id = next_koma_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        let Some(bounds) = clamp_koma_bounds(bounds, page.width, page.height) else {
            return;
        };

        let mut koma = Koma::new_blank(next_id, bounds.width, bounds.height);
        koma.bounds = bounds;
        page.komas.push(koma);
        self.active_koma_index = page.komas.len().saturating_sub(1);
        self.focus_active_koma_view();
    }

    pub fn remove_active_koma(&mut self) {
        let page_index = self.active_page_index();
        let active_koma_index = self.active_koma_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        if page.komas.len() <= 1 {
            return;
        }
        page.komas.remove(active_koma_index);
        relayout_page_komas(page);
        self.active_koma_index = active_koma_index.min(page.komas.len().saturating_sub(1));
        self.focus_active_koma_view();
    }

    pub fn focus_active_koma_view(&mut self) {
        self.view_transform = CanvasViewTransform::default();
    }

    pub fn set_view_transform(&mut self, transform: CanvasViewTransform) {
        self.view_transform = transform;
    }

    pub fn set_active_tool(&mut self, tool: ToolKind) {
        self.active_tool = tool;
        if let Some(tool_definition) = self.tool_catalog.iter().find(|entry| entry.kind == tool) {
            self.active_tool_id = tool_definition.id.clone();
        }
        self.active_child_tool_id = String::new();
    }

    pub fn set_active_tool_by_id(&mut self, tool_id: &str) -> bool {
        let Some(tool_definition) = self.tool_definition(tool_id).cloned() else {
            return false;
        };
        self.active_tool = tool_definition.kind;
        self.active_tool_id = tool_definition.id;
        self.active_child_tool_id = String::new();
        true
    }

    pub fn set_active_pen_size(&mut self, size: u32) {
        let size = self
            .active_pen_preset()
            .map(|preset| preset.clamp_size(size))
            .unwrap_or_else(|| size.max(1));
        self.active_pen_size = size;
    }

    pub fn set_active_pen_pressure_enabled(&mut self, enabled: bool) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.pressure_enabled = enabled;
        }
    }

    pub fn set_active_pen_antialias(&mut self, enabled: bool) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.antialias = enabled;
        }
    }

    pub fn set_active_pen_stabilization(&mut self, amount: u8) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.stabilization = amount.min(100);
        }
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

    pub fn replace_tool_catalog(&mut self, tool_catalog: Vec<ToolDefinition>) {
        self.tool_catalog = if tool_catalog.is_empty() {
            default_tool_catalog()
        } else {
            tool_catalog
        };
        self.ensure_tool_state();
    }

    pub fn merge_pen_presets(&mut self, pen_presets: Vec<PenPreset>) -> usize {
        if pen_presets.is_empty() {
            return 0;
        }

        let mut merged = 0;
        for preset in pen_presets {
            if let Some(existing) = self
                .pen_presets
                .iter_mut()
                .find(|existing| existing.id == preset.id)
            {
                *existing = preset;
            } else {
                self.pen_presets.push(preset);
            }
            merged += 1;
        }

        self.ensure_pen_state();
        merged
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

    fn active_pen_preset_mut(&mut self) -> Option<&mut PenPreset> {
        let index = self.active_pen_index();
        self.pen_presets.get_mut(index)
    }

    pub fn active_pen_index(&self) -> usize {
        self.pen_presets
            .iter()
            .position(|preset| preset.id == self.active_pen_preset_id)
            .unwrap_or(0)
    }

    /// ロード後のドキュメント不変条件を修復する。
    ///
    /// ツール状態の整合、空のページ列・コマ列の補完、各 index の clamp、
    /// 空 bounds コマの再レイアウト、レイヤー列の補完を行う。
    pub fn normalize_after_load(&mut self) {
        self.ensure_tool_state();
        if self.work.pages.is_empty() {
            self.work.pages.push(Page::default());
        }
        self.active_page_index = self.active_page_index();
        for page in &mut self.work.pages {
            page.width = page.width.max(1);
            page.height = page.height.max(1);
            if page.komas.is_empty() {
                page.komas
                    .push(Koma::new_blank(KomaId(1), page.width, page.height));
            }
            let needs_relayout = page.komas.iter().any(|koma| koma.bounds.is_empty());
            for koma in &mut page.komas {
                if koma.bounds.is_empty() {
                    koma.bounds = KomaBounds::full_page(page.width, page.height);
                }
                ensure_koma_layers(koma);
            }
            if needs_relayout {
                relayout_page_komas(page);
            }
        }
        self.active_koma_index = self.active_koma_index();
    }

    /// 純粋なドキュメント変異コマンドをドキュメント状態へ適用する。
    ///
    /// レイヤー・コマ・ドキュメント差し替えなど作品データのみを書き換える。
    /// ツール/色/ペン/ビューの変更は [`Document::apply_session_command`] が担う。
    pub fn apply(&mut self, command: &DocumentCommand) {
        match command {
            DocumentCommand::Noop => {}
            DocumentCommand::CreateKoma {
                x,
                y,
                width,
                height,
            } => {
                self.create_koma(KomaBounds {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                });
            }
            DocumentCommand::AddRasterLayer => {
                self.add_raster_layer();
            }
            DocumentCommand::RemoveActiveLayer => {
                self.remove_active_layer();
            }
            DocumentCommand::SelectLayer { index } => {
                self.select_layer(*index);
            }
            DocumentCommand::RenameActiveLayer { name } => {
                self.rename_active_layer(name);
            }
            DocumentCommand::MoveLayer {
                from_index,
                to_index,
            } => {
                self.move_layer(*from_index, *to_index);
            }
            DocumentCommand::SelectNextLayer => {
                self.select_next_layer();
            }
            DocumentCommand::CycleActiveLayerBlendMode => {
                self.cycle_active_layer_blend_mode();
            }
            DocumentCommand::SetActiveLayerBlendMode { mode } => {
                self.set_active_layer_blend_mode(mode.clone());
            }
            DocumentCommand::ToggleActiveLayerVisibility => {
                self.toggle_active_layer_visibility();
            }
            DocumentCommand::AddKoma => {
                self.add_koma();
            }
            DocumentCommand::RemoveActiveKoma => {
                self.remove_active_koma();
            }
            DocumentCommand::SelectKoma { index } => {
                self.select_koma(*index);
                self.focus_active_koma_view();
            }
            DocumentCommand::SelectNextKoma => {
                self.select_next_koma();
                self.focus_active_koma_view();
            }
            DocumentCommand::SelectPreviousKoma => {
                self.select_previous_koma();
                self.focus_active_koma_view();
            }
            DocumentCommand::FocusActiveKoma => {
                self.focus_active_koma_view();
            }
            DocumentCommand::NewDocumentSized { width, height } => {
                *self = Document::new(*width, *height);
            }
        }
    }

    /// エディタセッションコマンド (ツール/色/ペン/ビュー) を適用する。
    ///
    /// 作品データには触れず、`Document` 上のセッション状態のみを更新する。
    /// B5 (BL-073) で `editor-state` へ移設予定。
    pub fn apply_session_command(&mut self, command: &SessionCommand) {
        match command {
            SessionCommand::SelectTool { tool_id } => {
                let _ = self.set_active_tool_by_id(tool_id);
            }
            SessionCommand::SelectChildTool { child_id } => {
                if let Some(parent) = self.active_tool_definition()
                    && parent.children.iter().any(|c| c.id == *child_id)
                {
                    self.active_child_tool_id = child_id.clone();
                }
            }
            SessionCommand::SetActiveTool { tool } => {
                self.set_active_tool(*tool);
            }
            SessionCommand::SetActivePenSize { size } => {
                self.set_active_pen_size(*size);
            }
            SessionCommand::SetActivePenPressureEnabled { enabled } => {
                self.set_active_pen_pressure_enabled(*enabled);
            }
            SessionCommand::SetActivePenAntialias { enabled } => {
                self.set_active_pen_antialias(*enabled);
            }
            SessionCommand::SetActivePenStabilization { amount } => {
                self.set_active_pen_stabilization(*amount);
            }
            SessionCommand::SelectNextPenPreset => {
                self.select_next_pen_preset();
            }
            SessionCommand::SelectPreviousPenPreset => {
                self.select_previous_pen_preset();
            }
            SessionCommand::SetActiveColor { color } => {
                self.set_active_color(*color);
            }
            SessionCommand::SetViewZoom { zoom } => {
                self.view_transform.zoom = crate::view_policy::clamp_zoom(*zoom);
            }
            SessionCommand::ZoomViewBy { lines } => {
                self.view_transform.zoom =
                    crate::view_policy::zoom_after_lines(self.view_transform.zoom, *lines);
            }
            SessionCommand::PanView { delta_x, delta_y } => {
                self.view_transform.pan_x += delta_x;
                self.view_transform.pan_y += delta_y;
            }
            SessionCommand::PanViewByLines { x_lines, y_lines } => {
                self.view_transform.pan_x += x_lines * crate::view_policy::PAN_PIXELS_PER_LINE;
                self.view_transform.pan_y += y_lines * crate::view_policy::PAN_PIXELS_PER_LINE;
            }
            SessionCommand::SetViewPan { pan_x, pan_y } => {
                self.view_transform.pan_x = *pan_x;
                self.view_transform.pan_y = *pan_y;
            }
            SessionCommand::RotateView { quarter_turns } => {
                self.view_transform.rotation_degrees = (self.view_transform.rotation_degrees
                    + (*quarter_turns as f32 * 90.0))
                    .rem_euclid(360.0);
            }
            SessionCommand::SetViewRotation { rotation_degrees } => {
                self.view_transform.rotation_degrees = rotation_degrees.rem_euclid(360.0);
            }
            SessionCommand::FlipViewHorizontally => {
                self.view_transform.flip_x = !self.view_transform.flip_x;
            }
            SessionCommand::FlipViewVertically => {
                self.view_transform.flip_y = !self.view_transform.flip_y;
            }
            SessionCommand::ResetView => {
                self.view_transform = CanvasViewTransform::default();
            }
        }
    }
}

fn next_koma_id(pages: &[Page]) -> KomaId {
    let next = pages
        .iter()
        .flat_map(|page| page.komas.iter())
        .map(|koma| koma.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    KomaId(next)
}

fn default_koma_grid_bounds(
    page_width: usize,
    page_height: usize,
    koma_count: usize,
) -> Vec<KomaBounds> {
    let koma_count = koma_count.max(1);
    if koma_count == 1 {
        return vec![KomaBounds::full_page(page_width, page_height)];
    }

    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let columns = (koma_count as f32).sqrt().ceil() as usize;
    let rows = koma_count.div_ceil(columns);
    let margin_x = ((page_width as f32 * 0.04).round() as usize).clamp(12, 96);
    let margin_y = ((page_height as f32 * 0.04).round() as usize).clamp(12, 96);
    let gap_x = ((page_width as f32 * 0.015).round() as usize).clamp(8, 48);
    let gap_y = ((page_height as f32 * 0.015).round() as usize).clamp(8, 48);
    let available_width = page_width
        .saturating_sub(margin_x * 2)
        .saturating_sub(gap_x * columns.saturating_sub(1));
    let available_height = page_height
        .saturating_sub(margin_y * 2)
        .saturating_sub(gap_y * rows.saturating_sub(1));
    let cell_width = (available_width / columns.max(1)).max(64);
    let cell_height = (available_height / rows.max(1)).max(64);

    (0..koma_count)
        .map(|index| {
            let row = index / columns.max(1);
            let column = index % columns.max(1);
            KomaBounds {
                x: margin_x + column * (cell_width + gap_x),
                y: margin_y + row * (cell_height + gap_y),
                width: cell_width.min(page_width.max(1)),
                height: cell_height.min(page_height.max(1)),
            }
        })
        .collect()
}

fn relayout_page_komas(page: &mut Page) {
    let bounds = default_koma_grid_bounds(page.width, page.height, page.komas.len());
    for (koma, next_bounds) in page.komas.iter_mut().zip(bounds.into_iter()) {
        resize_koma_to_bounds(koma, next_bounds.width, next_bounds.height);
        koma.bounds = next_bounds;
    }
}

fn clamp_koma_bounds(
    bounds: KomaBounds,
    page_width: usize,
    page_height: usize,
) -> Option<KomaBounds> {
    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let x = bounds.x.min(page_width.saturating_sub(1));
    let y = bounds.y.min(page_height.saturating_sub(1));
    let max_width = page_width.saturating_sub(x);
    let max_height = page_height.saturating_sub(y);
    let width = bounds.width.min(max_width);
    let height = bounds.height.min(max_height);
    (width > 0 && height > 0).then_some(KomaBounds {
        x,
        y,
        width,
        height,
    })
}

fn resize_koma_to_bounds(koma: &mut Koma, width: usize, height: usize) {
    let width = width.max(1);
    let height = height.max(1);
    if koma.composite_cache.width == width && koma.composite_cache.height == height {
        return;
    }

    ensure_koma_layers(koma);
    for layer in &mut koma.layers {
        layer.bitmap = resize_bitmap_nearest(&layer.bitmap, width, height);
        if let Some(mask) = layer.mask.as_mut() {
            *mask = resize_mask_nearest(mask, width, height);
        }
    }
    koma.composite_cache = composite_koma_bitmap(koma);
}

fn resize_bitmap_nearest(bitmap: &CanvasBitmap, width: usize, height: usize) -> CanvasBitmap {
    let width = width.max(1);
    let height = height.max(1);
    if bitmap.width == width && bitmap.height == height {
        return bitmap.clone();
    }

    let mut resized = CanvasBitmap::transparent(width, height);
    for y in 0..height {
        let source_y = (((y as f32 / height as f32) * bitmap.height as f32).floor() as usize)
            .min(bitmap.height.saturating_sub(1));
        for x in 0..width {
            let source_x = (((x as f32 / width as f32) * bitmap.width as f32).floor() as usize)
                .min(bitmap.width.saturating_sub(1));
            let source_index = (source_y * bitmap.width + source_x) * 4;
            let target_index = (y * width + x) * 4;
            resized.pixels[target_index..target_index + 4]
                .copy_from_slice(&bitmap.pixels[source_index..source_index + 4]);
        }
    }
    resized
}

fn resize_mask_nearest(mask: &LayerMask, width: usize, height: usize) -> LayerMask {
    let width = width.max(1);
    let height = height.max(1);
    if mask.width == width && mask.height == height {
        return mask.clone();
    }

    let mut alpha = vec![0; width.saturating_mul(height)];
    for y in 0..height {
        let source_y = (((y as f32 / height as f32) * mask.height as f32).floor() as usize)
            .min(mask.height.saturating_sub(1));
        for x in 0..width {
            let source_x = (((x as f32 / width as f32) * mask.width as f32).floor() as usize)
                .min(mask.width.saturating_sub(1));
            alpha[y * width + x] = mask.alpha_at(source_x, source_y);
        }
    }

    LayerMask {
        width,
        height,
        alpha,
    }
}

#[cfg(test)]
mod tests;
