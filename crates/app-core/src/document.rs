use serde::{Deserialize, Serialize};

use crate::{Command, PanelLocalPoint};

mod bitmap;
mod layer_ops;
mod pen_state;

use self::layer_ops::{composite_panel_bitmap, ensure_panel_layers};

/// ホストと保存形式の間で共有する最小RGBA色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl ColorRgba8 {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 現在の値を rgba8 形式へ変換する。
    pub const fn to_rgba8(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// 現在の値を RGB 形式へ変換する。
    pub fn hex_rgb(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl Default for ColorRgba8 {
    /// 既定値を持つインスタンスを返す。
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
    PanelRect,
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
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn slider(key: impl Into<String>, label: impl Into<String>, min: i32, max: i32) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            control: ToolSettingControl::Slider,
            min: Some(min),
            max: Some(max),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
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
}

impl ToolDefinition {
    /// Supports setting かどうかを返す。
    pub fn supports_setting(&self, key: &str) -> bool {
        self.settings.iter().any(|setting| setting.key == key)
    }
}

pub const DEFAULT_DOCUMENT_WIDTH: usize = 2894;
pub const DEFAULT_DOCUMENT_HEIGHT: usize = 4093;

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
    /// 現在の 補正 サイズ を返す。
    pub fn clamp_size(&self, size: u32) -> u32 {
        size.clamp(
            1, 10000, // 将来の拡大に備えて大きな上限を許す
        )
    }
}

impl Default for PenPreset {
    /// 既定値を持つインスタンスを返す。
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
    /// 現在の 幅 を返す。
    pub fn width(&self) -> u32 {
        match self {
            Self::AlphaMask8 { width, .. }
            | Self::Rgba8 { width, .. }
            | Self::PngBlob { width, .. } => *width,
        }
    }

    /// 現在の 高さ を返す。
    pub fn height(&self) -> u32 {
        match self {
            Self::AlphaMask8 { height, .. }
            | Self::Rgba8 { height, .. }
            | Self::PngBlob { height, .. } => *height,
        }
    }
}

/// 既定の ペン サイズ を返す。
fn default_pen_size() -> u32 {
    4
}

/// 既定の ペン プラグイン ID を返す。
fn default_pen_plugin_id() -> String {
    "builtin.bitmap".to_string()
}

/// 既定の ビットマップ プラグイン ID を返す。
fn default_bitmap_plugin_id() -> String {
    "builtin.bitmap".to_string()
}

/// 既定の ペン pressure enabled を返す。
fn default_pen_pressure_enabled() -> bool {
    true
}

/// 既定の ペン アンチエイリアス を返す。
fn default_pen_antialias() -> bool {
    true
}

/// 既定の spacing percent を返す。
fn default_spacing_percent() -> f32 {
    25.0
}

/// 既定の ペン 不透明度 を返す。
fn default_pen_opacity() -> f32 {
    1.0
}

/// 既定の ペン flow を返す。
fn default_pen_flow() -> f32 {
    1.0
}

/// 既定の ペン presets を返す。
fn default_pen_presets() -> Vec<PenPreset> {
    vec![PenPreset::default()]
}

/// 既定の アクティブ ペン preset ID を返す。
fn default_active_pen_preset_id() -> String {
    PenPreset::default().id
}

/// 既定の アクティブ ページ インデックス を返す。
fn default_active_page_index() -> usize {
    0
}

/// 既定の ツール カタログ を返す。
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
        },
        ToolDefinition {
            id: "builtin.bucket".to_string(),
            name: "Bucket".to_string(),
            kind: ToolKind::Bucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.lasso-bucket".to_string(),
            name: "Lasso Bucket".to_string(),
            kind: ToolKind::LassoBucket,
            provider_plugin_id: "plugins/default-fill-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.panel-rect".to_string(),
            name: "Panel Rect".to_string(),
            kind: ToolKind::PanelRect,
            provider_plugin_id: "plugins/default-panel-tools-plugin".to_string(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
        },
    ]
}

/// 既定の アクティブ ツール ID を返す。
fn default_active_tool_id() -> String {
    default_tool_catalog()
        .first()
        .map(|tool| tool.id.clone())
        .unwrap_or_else(|| "builtin.pen".to_string())
}

/// 既定の アクティブ パネル インデックス を返す。
fn default_active_panel_index() -> usize {
    0
}

/// 既定の ページ 幅 を返す。
fn default_page_width() -> usize {
    DEFAULT_DOCUMENT_WIDTH
}

/// 既定の ページ 高さ を返す。
fn default_page_height() -> usize {
    DEFAULT_DOCUMENT_HEIGHT
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
    /// 現在アクティブな登録ツール ID。
    #[serde(default = "default_active_tool_id")]
    pub active_tool_id: String,
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
    #[serde(default = "default_active_panel_index")]
    pub active_panel_index: usize,
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
    /// 既定値を持つインスタンスを返す。
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
    pub panels: Vec<Panel>,
}

impl Default for Page {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self {
            id: PageId(1),
            width: default_page_width(),
            height: default_page_height(),
            panels: vec![Panel::default()],
        }
    }
}

/// ページ内のコマ矩形を表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelBounds {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PanelBounds {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn full_page(width: usize, height: usize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// Is empty かどうかを返す。
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 対象 が範囲内に含まれるか判定する。
    pub fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width)
            && y < self.y.saturating_add(self.height)
    }

    /// キャンバス 点 が範囲内に含まれるか判定する。
    pub fn contains_canvas_point(self, point: crate::CanvasPoint) -> bool {
        self.contains(point.x, point.y)
    }

    /// キャンバス to パネル local に必要な処理を行う。
    pub fn canvas_to_panel_local(
        self,
        point: crate::CanvasPoint,
    ) -> Option<crate::PanelLocalPoint> {
        self.contains_canvas_point(point)
            .then_some(crate::PanelLocalPoint::new(
                point.x.saturating_sub(self.x),
                point.y.saturating_sub(self.y),
            ))
    }

    /// 補正 キャンバス 点 を有効範囲へ補正して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn clamp_canvas_point(self, point: crate::CanvasPoint) -> Option<crate::CanvasPoint> {
        if self.is_empty() {
            return None;
        }

        let max_x = self.x.saturating_add(self.width.saturating_sub(1));
        let max_y = self.y.saturating_add(self.height.saturating_sub(1));
        Some(crate::CanvasPoint::new(
            point.x.clamp(self.x, max_x),
            point.y.clamp(self.y, max_y),
        ))
    }

    /// パネル local to キャンバス に必要な処理を行う。
    pub fn panel_local_to_canvas(
        self,
        point: crate::PanelLocalPoint,
    ) -> Option<crate::CanvasPoint> {
        (point.x < self.width && point.y < self.height).then_some(crate::CanvasPoint::new(
            self.x.saturating_add(point.x),
            self.y.saturating_add(point.y),
        ))
    }
}

impl Default for PanelBounds {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::full_page(DEFAULT_DOCUMENT_WIDTH, DEFAULT_DOCUMENT_HEIGHT)
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
    /// ページ内でのコマ矩形。
    #[serde(default)]
    pub bounds: PanelBounds,
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
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new_blank(PanelId(1), DEFAULT_DOCUMENT_WIDTH, DEFAULT_DOCUMENT_HEIGHT)
    }
}

impl Panel {
    /// 既定値を使って新しいインスタンスを生成する。
    pub fn new_blank(id: PanelId, width: usize, height: usize) -> Self {
        let background = RasterLayer::background(
            LayerNodeId(1),
            "Layer 1".to_string(),
            width.max(1),
            height.max(1),
        );
        Self {
            id,
            bounds: PanelBounds::full_page(width, height),
            root_layer: LayerNode::default(),
            bitmap: background.bitmap.clone(),
            layers: vec![background],
            active_layer_index: 0,
            created_layer_count: 1,
        }
    }
}

/// 既定の created レイヤー 件数 を返す。
const fn default_created_layer_count() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Add,
    Custom(String),
}

impl BlendMode {
    /// 現在の値を str 形式へ変換する。
    pub fn as_str(&self) -> &str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Add => "add",
            Self::Custom(value) => value.as_str(),
        }
    }

    /// 入力を解析して 名前 に変換する。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn parse_name(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        match trimmed.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "add" => Some(Self::Add),
            _ => Some(Self::Custom(trimmed.to_string())),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    fn next(&self) -> Self {
        match self {
            Self::Normal => Self::Multiply,
            Self::Multiply => Self::Screen,
            Self::Screen => Self::Add,
            Self::Add => Self::Normal,
            Self::Custom(_) => Self::Normal,
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
    /// 入力値を束ねた新しいインスタンスを生成する。
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

    /// 指定位置の アルファ を計算して返す。
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

/// 既定の レイヤー 表示状態 を返す。
fn default_layer_visible() -> bool {
    true
}

impl RasterLayer {
    /// 入力値を束ねた新しいインスタンスを生成する。
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

    /// 入力値を束ねた新しいインスタンスを生成する。
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
    /// 既定値を持つインスタンスを返す。
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
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self::new(DEFAULT_DOCUMENT_WIDTH, DEFAULT_DOCUMENT_HEIGHT)
    }
}

impl Document {
    /// 既定値を使って新しいインスタンスを生成する。
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
                    panels: vec![Panel::new_blank(PanelId(1), width, height)],
                    ..Page::default()
                }],
                ..Work::default()
            },
            active_tool: ToolKind::default(),
            active_tool_id,
            active_color: ColorRgba8::default(),
            tool_catalog,
            pen_presets,
            active_pen_preset_id,
            active_pen_size,
            active_page_index: default_active_page_index(),
            active_panel_index: default_active_panel_index(),
            view_transform: CanvasViewTransform::default(),
        }
    }

    /// アクティブな ページ インデックス を返す。
    pub fn active_page_index(&self) -> usize {
        self.active_page_index
            .min(self.work.pages.len().saturating_sub(1))
    }

    /// アクティブな パネル インデックス を返す。
    pub fn active_panel_index(&self) -> usize {
        self.active_page()
            .map(|page| {
                self.active_panel_index
                    .min(page.panels.len().saturating_sub(1))
            })
            .unwrap_or(0)
    }

    /// アクティブな ページ を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_page(&self) -> Option<&Page> {
        self.work.pages.get(self.active_page_index())
    }

    /// アクティブな ページ への可変参照を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_page_mut(&mut self) -> Option<&mut Page> {
        let index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        self.work.pages.get_mut(index)
    }

    /// アクティブな パネル を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_panel(&self) -> Option<&Panel> {
        let panel_index = self.active_panel_index();
        self.active_page()
            .and_then(|page| page.panels.get(panel_index))
    }

    /// アクティブな パネル への可変参照を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_panel_mut(&mut self) -> Option<&mut Panel> {
        let page_index = self
            .active_page_index
            .min(self.work.pages.len().saturating_sub(1));
        let panel_index = self.active_panel_index;
        self.work.pages.get_mut(page_index).and_then(|page| {
            let clamped_index = panel_index.min(page.panels.len().saturating_sub(1));
            page.panels.get_mut(clamped_index)
        })
    }

    /// アクティブな ビットマップ を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_bitmap(&self) -> Option<&CanvasBitmap> {
        self.active_panel().map(|panel| &panel.bitmap)
    }

    /// アクティブな レイヤー ビットマップ を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_layer_bitmap(&self) -> Option<&CanvasBitmap> {
        let panel = self.active_panel()?;
        panel
            .layers
            .get(
                panel
                    .active_layer_index
                    .min(panel.layers.len().saturating_sub(1)),
            )
            .map(|layer| &layer.bitmap)
    }

    /// アクティブな レイヤー is 背景 を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_layer_is_background(&self) -> Option<bool> {
        let panel = self.active_panel()?;
        Some(panel.active_layer_index == 0)
    }

    /// アクティブな パネル contains キャンバス 点 を返す。
    pub fn active_panel_contains_canvas_point(&self, point: crate::CanvasPoint) -> bool {
        self.active_panel_bounds()
            .is_some_and(|bounds| bounds.contains_canvas_point(point))
    }

    /// アクティブな パネル contains local 点 を返す。
    pub fn active_panel_contains_local_point(&self, point: PanelLocalPoint) -> bool {
        self.active_panel_bounds()
            .and_then(|bounds| bounds.panel_local_to_canvas(point))
            .is_some()
    }

    /// アクティブ パネル キャンバス to local に必要な処理を行う。
    pub fn active_panel_canvas_to_local(
        &self,
        point: crate::CanvasPoint,
    ) -> Option<PanelLocalPoint> {
        self.active_panel_bounds()
            .and_then(|bounds| bounds.canvas_to_panel_local(point))
    }

    /// アクティブ パネル local to キャンバス に必要な処理を行う。
    pub fn active_panel_local_to_canvas(
        &self,
        point: PanelLocalPoint,
    ) -> Option<crate::CanvasPoint> {
        self.active_panel_bounds()
            .and_then(|bounds| bounds.panel_local_to_canvas(point))
    }

    /// 既存データを走査して ツール definition を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn tool_definition(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.tool_catalog.iter().find(|tool| tool.id == tool_id)
    }

    /// アクティブな ツール definition を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_tool_definition(&self) -> Option<&ToolDefinition> {
        self.tool_definition(&self.active_tool_id)
            .or_else(|| {
                self.tool_catalog
                    .iter()
                    .find(|tool| tool.kind == self.active_tool)
            })
            .or_else(|| self.tool_catalog.first())
    }

    /// アクティブな ツール provider プラグイン ID を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_tool_provider_plugin_id(&self) -> Option<&str> {
        self.active_tool_definition()
            .map(|tool| tool.provider_plugin_id.as_str())
    }

    /// アクティブな ツール 描画 プラグイン ID を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_tool_drawing_plugin_id(&self) -> Option<&str> {
        self.active_tool_definition()
            .map(|tool| tool.drawing_plugin_id.as_str())
    }

    /// アクティブな ツール 設定 を返す。
    pub fn active_tool_settings(&self) -> &[ToolSettingDefinition] {
        self.active_tool_definition()
            .map(|tool| tool.settings.as_slice())
            .unwrap_or(&[])
    }

    /// アクティブな パネル 範囲 を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_panel_bounds(&self) -> Option<PanelBounds> {
        self.active_panel().map(|panel| panel.bounds)
    }

    /// アクティブな ページ パネル 件数 を返す。
    pub fn active_page_panel_count(&self) -> usize {
        self.active_page()
            .map(|page| page.panels.len())
            .unwrap_or(0)
    }

    /// アクティブな ページ dimensions を返す。
    pub fn active_page_dimensions(&self) -> (usize, usize) {
        self.active_page()
            .map(|page| (page.width.max(1), page.height.max(1)))
            .unwrap_or((1, 1))
    }

    /// パネル を選択状態へ更新する。
    pub fn select_panel(&mut self, index: usize) {
        let page_index = self.active_page_index();
        if let Some(page) = self.work.pages.get(page_index) {
            self.active_panel_index = index.min(page.panels.len().saturating_sub(1));
        }
    }

    /// 次 パネル を選択状態へ更新する。
    pub fn select_next_panel(&mut self) {
        if let Some(page) = self.active_page() {
            let panel_count = page.panels.len().max(1);
            self.active_panel_index = (self.active_panel_index() + 1) % panel_count;
        }
    }

    /// 前 パネル を選択状態へ更新する。
    pub fn select_previous_panel(&mut self) {
        if let Some(page) = self.active_page() {
            let panel_count = page.panels.len().max(1);
            self.active_panel_index = (self.active_panel_index() + panel_count - 1) % panel_count;
        }
    }

    /// パネル を追加する。
    pub fn add_panel(&mut self) {
        let next_id = next_panel_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };

        let next_count = page.panels.len().saturating_add(1);
        let next_bounds = default_panel_grid_bounds(page.width, page.height, next_count);
        let new_bounds = next_bounds
            .last()
            .copied()
            .unwrap_or_else(|| PanelBounds::full_page(page.width, page.height));
        let mut panel = Panel::new_blank(next_id, new_bounds.width, new_bounds.height);
        panel.bounds = new_bounds;
        page.panels.push(panel);
        relayout_page_panels(page);
        self.active_panel_index = page.panels.len().saturating_sub(1);
        self.focus_active_panel_view();
    }

    /// パネル を構築する。
    pub fn create_panel(&mut self, bounds: PanelBounds) {
        let next_id = next_panel_id(&self.work.pages);
        let page_index = self.active_page_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        let Some(bounds) = clamp_panel_bounds(bounds, page.width, page.height) else {
            return;
        };

        let mut panel = Panel::new_blank(next_id, bounds.width, bounds.height);
        panel.bounds = bounds;
        page.panels.push(panel);
        self.active_panel_index = page.panels.len().saturating_sub(1);
        self.focus_active_panel_view();
    }

    /// アクティブ パネル を削除する。
    pub fn remove_active_panel(&mut self) {
        let page_index = self.active_page_index();
        let active_panel_index = self.active_panel_index();
        let Some(page) = self.work.pages.get_mut(page_index) else {
            return;
        };
        if page.panels.len() <= 1 {
            return;
        }
        page.panels.remove(active_panel_index);
        relayout_page_panels(page);
        self.active_panel_index = active_panel_index.min(page.panels.len().saturating_sub(1));
        self.focus_active_panel_view();
    }

    /// アクティブ パネル ビュー へフォーカスを移す。
    pub fn focus_active_panel_view(&mut self) {
        self.view_transform = CanvasViewTransform::default();
    }

    /// ビュー 変換 を設定する。
    pub fn set_view_transform(&mut self, transform: CanvasViewTransform) {
        self.view_transform = transform;
    }

    /// アクティブ ツール を設定する。
    pub fn set_active_tool(&mut self, tool: ToolKind) {
        self.active_tool = tool;
        if let Some(tool_definition) = self.tool_catalog.iter().find(|entry| entry.kind == tool) {
            self.active_tool_id = tool_definition.id.clone();
        }
    }

    /// アクティブ ツール by ID を設定する。
    pub fn set_active_tool_by_id(&mut self, tool_id: &str) -> bool {
        let Some(tool_definition) = self.tool_definition(tool_id).cloned() else {
            return false;
        };
        self.active_tool = tool_definition.kind;
        self.active_tool_id = tool_definition.id;
        true
    }

    /// アクティブ ペン サイズ を設定する。
    pub fn set_active_pen_size(&mut self, size: u32) {
        let size = self
            .active_pen_preset()
            .map(|preset| preset.clamp_size(size))
            .unwrap_or_else(|| size.max(1));
        self.active_pen_size = size;
    }

    /// アクティブ ペン pressure enabled を設定する。
    pub fn set_active_pen_pressure_enabled(&mut self, enabled: bool) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.pressure_enabled = enabled;
        }
    }

    /// アクティブ ペン アンチエイリアス を設定する。
    pub fn set_active_pen_antialias(&mut self, enabled: bool) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.antialias = enabled;
        }
    }

    /// アクティブ ペン stabilization を設定する。
    pub fn set_active_pen_stabilization(&mut self, amount: u8) {
        if let Some(preset) = self.active_pen_preset_mut() {
            preset.stabilization = amount.min(100);
        }
    }

    /// アクティブ 色 を設定する。
    pub fn set_active_color(&mut self, color: ColorRgba8) {
        self.active_color = color;
    }

    /// ペン presets を置き換える。
    pub fn replace_pen_presets(&mut self, pen_presets: Vec<PenPreset>) {
        self.pen_presets = if pen_presets.is_empty() {
            default_pen_presets()
        } else {
            pen_presets
        };
        self.ensure_pen_state();
    }

    /// ツール カタログ を置き換える。
    pub fn replace_tool_catalog(&mut self, tool_catalog: Vec<ToolDefinition>) {
        self.tool_catalog = if tool_catalog.is_empty() {
            default_tool_catalog()
        } else {
            tool_catalog
        };
        self.ensure_tool_state();
    }

    /// ペン presets を統合する。
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

    /// 次 ペン preset を選択状態へ更新する。
    pub fn select_next_pen_preset(&mut self) {
        self.cycle_pen_preset(1);
    }

    /// 前 ペン preset を選択状態へ更新する。
    pub fn select_previous_pen_preset(&mut self) {
        self.cycle_pen_preset(-1);
    }

    /// アクティブな ペン preset を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn active_pen_preset(&self) -> Option<&PenPreset> {
        self.pen_presets
            .iter()
            .find(|preset| preset.id == self.active_pen_preset_id)
            .or_else(|| self.pen_presets.first())
    }

    /// アクティブな ペン preset への可変参照を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn active_pen_preset_mut(&mut self) -> Option<&mut PenPreset> {
        let index = self.active_pen_index();
        self.pen_presets.get_mut(index)
    }

    /// アクティブな ペン インデックス を返す。
    pub fn active_pen_index(&self) -> usize {
        self.pen_presets
            .iter()
            .position(|preset| preset.id == self.active_pen_preset_id)
            .unwrap_or(0)
    }

    /// 解決済みの paint サイズ with pressure を返す。
    pub fn resolved_paint_size_with_pressure(&self, pressure: f32) -> u32 {
        self.active_draw_size_with_pressure(pressure)
    }

    /// 既存データを走査して normalize phase9 状態 を組み立てる。
    pub fn normalize_phase9_state(&mut self) {
        self.ensure_tool_state();
        if self.work.pages.is_empty() {
            self.work.pages.push(Page::default());
        }
        self.active_page_index = self.active_page_index();
        for page in &mut self.work.pages {
            page.width = page.width.max(1);
            page.height = page.height.max(1);
            if page.panels.is_empty() {
                page.panels
                    .push(Panel::new_blank(PanelId(1), page.width, page.height));
            }
            let needs_relayout = page.panels.iter().any(|panel| panel.bounds.is_empty());
            for panel in &mut page.panels {
                if panel.bounds.is_empty() {
                    panel.bounds = PanelBounds::full_page(page.width, page.height);
                }
                ensure_panel_layers(panel);
            }
            if needs_relayout {
                relayout_page_panels(page);
            }
        }
        self.active_panel_index = self.active_panel_index();
    }

    /// 入力や種別に応じて処理を振り分ける。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn apply_command(&mut self, command: &Command) -> Option<crate::CanvasDirtyRect> {
        match command {
            Command::Noop => None,
            Command::SelectTool { tool_id } => {
                let _ = self.set_active_tool_by_id(tool_id);
                None
            }
            Command::SetActiveTool { tool } => {
                self.set_active_tool(*tool);
                None
            }
            Command::SetActivePenSize { size } => {
                self.set_active_pen_size(*size);
                None
            }
            Command::SetActivePenPressureEnabled { enabled } => {
                self.set_active_pen_pressure_enabled(*enabled);
                None
            }
            Command::SetActivePenAntialias { enabled } => {
                self.set_active_pen_antialias(*enabled);
                None
            }
            Command::SetActivePenStabilization { amount } => {
                self.set_active_pen_stabilization(*amount);
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
            Command::CreatePanel {
                x,
                y,
                width,
                height,
            } => {
                self.create_panel(PanelBounds {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                });
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
            Command::SetViewPan { pan_x, pan_y } => {
                self.view_transform.pan_x = *pan_x;
                self.view_transform.pan_y = *pan_y;
                None
            }
            Command::RotateView { quarter_turns } => {
                self.view_transform.rotation_degrees = (self.view_transform.rotation_degrees
                    + (*quarter_turns as f32 * 90.0))
                    .rem_euclid(360.0);
                None
            }
            Command::SetViewRotation { rotation_degrees } => {
                self.view_transform.rotation_degrees = rotation_degrees.rem_euclid(360.0);
                None
            }
            Command::FlipViewHorizontally => {
                self.view_transform.flip_x = !self.view_transform.flip_x;
                None
            }
            Command::FlipViewVertically => {
                self.view_transform.flip_y = !self.view_transform.flip_y;
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
                self.set_active_layer_blend_mode(mode.clone());
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
            Command::AddPanel => {
                self.add_panel();
                None
            }
            Command::RemoveActivePanel => {
                self.remove_active_panel();
                None
            }
            Command::SelectPanel { index } => {
                self.select_panel(*index);
                self.focus_active_panel_view();
                None
            }
            Command::SelectNextPanel => {
                self.select_next_panel();
                self.focus_active_panel_view();
                None
            }
            Command::SelectPreviousPanel => {
                self.select_previous_panel();
                self.focus_active_panel_view();
                None
            }
            Command::FocusActivePanel => {
                self.focus_active_panel_view();
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
            | Command::LoadProjectFromPath { .. }
            | Command::ReloadWorkspacePresets
            | Command::ApplyWorkspacePreset { .. }
            | Command::SaveWorkspacePreset { .. }
            | Command::ExportWorkspacePreset { .. }
            | Command::ExportWorkspacePresetToPath { .. }
            | Command::ImportPenPresets
            | Command::ImportPenPresetsFromPath { .. }
            | Command::Undo
            | Command::Redo => None,
        }
    }
}

/// パネル ID をひとつ先へ切り替える。
fn next_panel_id(pages: &[Page]) -> PanelId {
    let next = pages
        .iter()
        .flat_map(|page| page.panels.iter())
        .map(|panel| panel.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    PanelId(next)
}

/// 既定の パネル grid 範囲 を返す。
fn default_panel_grid_bounds(
    page_width: usize,
    page_height: usize,
    panel_count: usize,
) -> Vec<PanelBounds> {
    let panel_count = panel_count.max(1);
    if panel_count == 1 {
        return vec![PanelBounds::full_page(page_width, page_height)];
    }

    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let columns = (panel_count as f32).sqrt().ceil() as usize;
    let rows = panel_count.div_ceil(columns);
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

    (0..panel_count)
        .map(|index| {
            let row = index / columns.max(1);
            let column = index % columns.max(1);
            PanelBounds {
                x: margin_x + column * (cell_width + gap_x),
                y: margin_y + row * (cell_height + gap_y),
                width: cell_width.min(page_width.max(1)),
                height: cell_height.min(page_height.max(1)),
            }
        })
        .collect()
}

/// ページ panels を再配置する。
fn relayout_page_panels(page: &mut Page) {
    let bounds = default_panel_grid_bounds(page.width, page.height, page.panels.len());
    for (panel, next_bounds) in page.panels.iter_mut().zip(bounds.into_iter()) {
        resize_panel_to_bounds(panel, next_bounds.width, next_bounds.height);
        panel.bounds = next_bounds;
    }
}

/// 現在の 補正 パネル 範囲 を返す。
fn clamp_panel_bounds(
    bounds: PanelBounds,
    page_width: usize,
    page_height: usize,
) -> Option<PanelBounds> {
    let page_width = page_width.max(1);
    let page_height = page_height.max(1);
    let x = bounds.x.min(page_width.saturating_sub(1));
    let y = bounds.y.min(page_height.saturating_sub(1));
    let max_width = page_width.saturating_sub(x);
    let max_height = page_height.saturating_sub(y);
    let width = bounds.width.min(max_width);
    let height = bounds.height.min(max_height);
    (width > 0 && height > 0).then_some(PanelBounds {
        x,
        y,
        width,
        height,
    })
}

/// 現在の リサイズ パネル to 範囲 を返す。
fn resize_panel_to_bounds(panel: &mut Panel, width: usize, height: usize) {
    let width = width.max(1);
    let height = height.max(1);
    if panel.bitmap.width == width && panel.bitmap.height == height {
        return;
    }

    ensure_panel_layers(panel);
    for layer in &mut panel.layers {
        layer.bitmap = resize_bitmap_nearest(&layer.bitmap, width, height);
        if let Some(mask) = layer.mask.as_mut() {
            *mask = resize_mask_nearest(mask, width, height);
        }
    }
    panel.bitmap = composite_panel_bitmap(panel);
}

/// ピクセル走査を行い、リサイズ ビットマップ nearest 用のビットマップ結果を生成する。
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

/// リサイズ マスク nearest を計算して返す。
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
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self {
            id: LayerNodeId(1),
            name: "Layer 1".to_string(),
        }
    }
}

#[cfg(test)]
mod tests;
