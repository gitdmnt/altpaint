//! エディタの一過性編集状態 (`EditorSession`)。
//!
//! アクティブツール・色・ペンプリセット・表示変換など、作品データには属さない
//! 編集セッション状態と、その整合・適用ロジックをまとめる。`document-model` の
//! `Document` が本セッションを `session` フィールドとして保持する。

use serde::{Deserialize, Serialize};

use crate::SessionCommand;
use crate::view_policy;

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

pub(crate) fn default_pen_size() -> u32 {
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

pub(crate) fn default_pen_presets() -> Vec<PenPreset> {
    vec![PenPreset::default()]
}

fn default_active_pen_preset_id() -> String {
    PenPreset::default().id
}

/// editor-state 既定のツールカタログ (構造のみのフォールバック)。
///
/// ツールの id / name / kind / settings を定義するが、`provider_plugin_id`
/// (プラグイン配置文字列) は desktop 固有の既定値であり editor-state は知らない。
/// 実運用では desktop が `tools/` 配下の定義 (または desktop 既定カタログ) を
/// `replace_tool_catalog` で注入するため、ここでは空文字列で初期化する。
pub(crate) fn default_tool_catalog() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            id: "builtin.pen".to_string(),
            name: "Pen".to_string(),
            kind: ToolKind::Pen,
            provider_plugin_id: String::new(),
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
            provider_plugin_id: String::new(),
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
            provider_plugin_id: String::new(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.lasso-bucket".to_string(),
            name: "Lasso Bucket".to_string(),
            kind: ToolKind::LassoBucket,
            provider_plugin_id: String::new(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
        ToolDefinition {
            id: "builtin.koma-rect".to_string(),
            name: "Koma Rect".to_string(),
            kind: ToolKind::KomaRect,
            provider_plugin_id: String::new(),
            drawing_plugin_id: default_bitmap_plugin_id(),
            settings: Vec::new(),
            children: Vec::new(),
        },
    ]
}

pub(crate) fn default_active_tool_id() -> String {
    default_tool_catalog()
        .first()
        .map(|tool| tool.id.clone())
        .unwrap_or_else(|| "builtin.pen".to_string())
}

/// エディタの一過性編集状態 (ツール/色/ペン/ビュー)。
///
/// 作品データには属さない。アクティブツール選択は `active_tool_id` を単一真実とし、
/// `ToolKind` は [`EditorSession::active_tool`] で導出する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSession {
    /// 現在アクティブな登録ツール ID (アクティブツールの単一真実)。
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
    /// キャンバスの表示変換状態。
    #[serde(default)]
    pub view_transform: CanvasViewTransform,
}

impl Default for EditorSession {
    fn default() -> Self {
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
            active_tool_id,
            active_child_tool_id: String::new(),
            active_color: ColorRgba8::default(),
            tool_catalog,
            pen_presets,
            active_pen_preset_id,
            active_pen_size,
            view_transform: CanvasViewTransform::default(),
        }
    }
}

impl EditorSession {
    /// `active_tool_id` から現在のアクティブツール種別を導出する。
    pub fn active_tool(&self) -> ToolKind {
        self.active_tool_definition()
            .map(|tool| tool.kind)
            .unwrap_or_default()
    }

    pub fn tool_definition(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.tool_catalog.iter().find(|tool| tool.id == tool_id)
    }

    pub fn active_tool_definition(&self) -> Option<&ToolDefinition> {
        self.tool_definition(&self.active_tool_id)
            .or_else(|| self.tool_catalog.first())
    }

    /// アクティブな子ツール definition を返す。
    pub fn active_child_tool_definition(&self) -> Option<&ToolDefinition> {
        let parent = self.active_tool_definition()?;
        if self.active_child_tool_id.is_empty() {
            return None;
        }
        parent
            .children
            .iter()
            .find(|c| c.id == self.active_child_tool_id)
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

    pub fn set_active_tool(&mut self, tool: ToolKind) {
        if let Some(tool_definition) = self.tool_catalog.iter().find(|entry| entry.kind == tool) {
            self.active_tool_id = tool_definition.id.clone();
        }
        self.active_child_tool_id = String::new();
    }

    pub fn set_active_tool_by_id(&mut self, tool_id: &str) -> bool {
        let Some(tool_definition) = self.tool_definition(tool_id).cloned() else {
            return false;
        };
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
        // 入替前のアクティブツール種別を控え、新カタログに同 id が無い場合の
        // フォールバック先 (kind 一致) を維持する。
        let fallback_kind = self.active_tool();
        self.tool_catalog = if tool_catalog.is_empty() {
            default_tool_catalog()
        } else {
            tool_catalog
        };
        self.ensure_tool_state(fallback_kind);
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

    pub fn set_view_transform(&mut self, transform: CanvasViewTransform) {
        self.view_transform = transform;
    }

    /// 表示変換を既定位置 (アクティブコマ中心) へ戻す。
    pub fn reset_view(&mut self) {
        self.view_transform = CanvasViewTransform::default();
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

    pub(crate) fn ensure_pen_state(&mut self) {
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

    /// ツール状態の整合を取る。
    ///
    /// `active_tool_id` がカタログに一致しなければ、保存時に渡された
    /// `fallback_kind` に一致するツール、なければ先頭ツールへフォールバックする。
    pub fn ensure_tool_state(&mut self, fallback_kind: ToolKind) {
        if self.tool_catalog.is_empty() {
            self.tool_catalog = default_tool_catalog();
        }

        if self
            .tool_catalog
            .iter()
            .any(|tool| tool.id == self.active_tool_id)
        {
            return;
        }

        if let Some(tool_definition) = self
            .tool_catalog
            .iter()
            .find(|tool| tool.kind == fallback_kind)
            .or_else(|| self.tool_catalog.first())
        {
            self.active_tool_id = tool_definition.id.clone();
        }
    }

    /// 現在ツールと筆圧から実効ブラシサイズを決定する。
    pub fn brush_size_for_pressure(&self, pressure: f32) -> u32 {
        let clamped_pressure = pressure.clamp(0.0, 1.0);
        match self.active_tool() {
            ToolKind::Eraser => self.active_pen_size.max(1),
            ToolKind::Pen => {
                let Some(preset) = self.active_pen_preset() else {
                    return self.active_pen_size.max(1);
                };
                let base = self.active_pen_size.max(1);
                if !preset.pressure_enabled {
                    return base;
                }
                let scaled = (base as f32 * (0.2 + clamped_pressure * 0.8)).round() as u32;
                scaled.max(1)
            }
            ToolKind::Bucket | ToolKind::LassoBucket | ToolKind::KomaRect => 1,
        }
    }

    /// エディタセッションコマンド (ツール/色/ペン/ビュー) を適用する。
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
                self.view_transform.zoom = view_policy::clamp_zoom(*zoom);
            }
            SessionCommand::ZoomViewBy { lines } => {
                self.view_transform.zoom =
                    view_policy::zoom_after_lines(self.view_transform.zoom, *lines);
            }
            SessionCommand::PanView { delta_x, delta_y } => {
                self.view_transform.pan_x += delta_x;
                self.view_transform.pan_y += delta_y;
            }
            SessionCommand::PanViewByLines { x_lines, y_lines } => {
                self.view_transform.pan_x += x_lines * view_policy::PAN_PIXELS_PER_LINE;
                self.view_transform.pan_y += y_lines * view_policy::PAN_PIXELS_PER_LINE;
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
