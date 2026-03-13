use app_core::{PenPreset, PenRuntimeEngine, PenTipBitmap};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::PenExchangeError;

pub const CURRENT_PEN_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PenEngine {
    #[default]
    Stamp,
    Generated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PenSourceKind {
    #[default]
    AltPaint,
    PhotoshopAbr,
    ClipStudioSut,
    GimpGbr,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PenSource {
    #[serde(default)]
    pub kind: PenSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_file: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub raw_fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PenPressurePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PenPressureCurve {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<PenPressurePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PenDynamics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_pressure_curve: Option<PenPressureCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity_pressure_curve: Option<PenPressureCurve>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub flags: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PenTip {
    AlphaMask8 {
        width: u32,
        height: u32,
        data_base64: String,
    },
    Rgba8 {
        width: u32,
        height: u32,
        data_base64: String,
    },
    PngBlob {
        width: u32,
        height: u32,
        png_base64: String,
    },
}

impl PenTip {
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

    /// 現在の値を マスク bytes へ変換する。
    ///
    /// 失敗時はエラーを返します。
    pub fn alpha_mask_bytes(&self) -> Result<Vec<u8>, PenExchangeError> {
        match self {
            Self::AlphaMask8 { data_base64, .. } => BASE64
                .decode(data_base64)
                .map_err(|error| PenExchangeError::InvalidData(error.to_string())),
            _ => Err(PenExchangeError::UnsupportedFormat(
                "tip is not exportable as an alpha-mask brush".to_string(),
            )),
        }
    }

    /// 現在の値を bytes へ変換する。
    ///
    /// 失敗時はエラーを返します。
    pub fn rgba_bytes(&self) -> Result<Vec<u8>, PenExchangeError> {
        match self {
            Self::Rgba8 { data_base64, .. } => BASE64
                .decode(data_base64)
                .map_err(|error| PenExchangeError::InvalidData(error.to_string())),
            _ => Err(PenExchangeError::UnsupportedFormat(
                "tip is not exportable as RGBA brush data".to_string(),
            )),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn from_alpha_mask(width: u32, height: u32, bytes: &[u8]) -> Self {
        Self::AlphaMask8 {
            width,
            height,
            data_base64: BASE64.encode(bytes),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn from_rgba(width: u32, height: u32, bytes: &[u8]) -> Self {
        Self::Rgba8 {
            width,
            height,
            data_base64: BASE64.encode(bytes),
        }
    }

    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn from_png_blob(width: u32, height: u32, bytes: &[u8]) -> Self {
        Self::PngBlob {
            width,
            height,
            png_base64: BASE64.encode(bytes),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AltPaintPen {
    #[serde(default = "current_pen_format_version")]
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default = "default_plugin_id")]
    pub plugin_id: String,
    #[serde(default)]
    pub engine: PenEngine,
    #[serde(default = "default_base_size", alias = "size")]
    pub base_size: f32,
    #[serde(default = "default_min_size")]
    pub min_size: f32,
    #[serde(default = "default_max_size")]
    pub max_size: f32,
    #[serde(default = "default_spacing_percent")]
    pub spacing_percent: f32,
    #[serde(default)]
    pub rotation_deg: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_flow")]
    pub flow: f32,
    #[serde(default = "default_pen_pressure_enabled")]
    pub pressure_enabled: bool,
    #[serde(default = "default_pen_antialias")]
    pub antialias: bool,
    #[serde(default)]
    pub stabilization: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<PenTip>,
    #[serde(default)]
    pub dynamics: PenDynamics,
    #[serde(default)]
    pub source: PenSource,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extras: Map<String, Value>,
}

impl Default for AltPaintPen {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self {
            format_version: current_pen_format_version(),
            id: PenPreset::default().id,
            name: PenPreset::default().name,
            plugin_id: default_plugin_id(),
            engine: PenEngine::default(),
            base_size: default_base_size(),
            min_size: default_min_size(),
            max_size: default_max_size(),
            spacing_percent: default_spacing_percent(),
            rotation_deg: 0.0,
            opacity: default_opacity(),
            flow: default_flow(),
            pressure_enabled: default_pen_pressure_enabled(),
            antialias: default_pen_antialias(),
            stabilization: 0,
            tip: None,
            dynamics: PenDynamics::default(),
            source: PenSource::default(),
            extras: Map::new(),
        }
    }
}

impl AltPaintPen {
    /// 現在の値を output へ変換する。
    ///
    /// 失敗時はエラーを返します。
    pub fn validate(&self) -> Result<(), PenExchangeError> {
        if self.format_version != CURRENT_PEN_FORMAT_VERSION {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported altpaint pen format version: {}",
                self.format_version
            )));
        }
        if self.id.trim().is_empty() {
            return Err(PenExchangeError::InvalidData(
                "pen id must not be empty".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(PenExchangeError::InvalidData(
                "pen name must not be empty".to_string(),
            ));
        }
        if self.plugin_id.trim().is_empty() {
            return Err(PenExchangeError::InvalidData(
                "pen plugin_id must not be empty".to_string(),
            ));
        }
        if self.min_size <= 0.0 {
            return Err(PenExchangeError::InvalidData(
                "pen min_size must be greater than zero".to_string(),
            ));
        }
        if self.max_size < self.min_size {
            return Err(PenExchangeError::InvalidData(
                "pen max_size must be greater than or equal to min_size".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err(PenExchangeError::InvalidData(
                "pen opacity must be between 0.0 and 1.0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.flow) {
            return Err(PenExchangeError::InvalidData(
                "pen flow must be between 0.0 and 1.0".to_string(),
            ));
        }
        Ok(())
    }

    /// 現在の値を runtime preset 形式へ変換する。
    pub fn to_runtime_preset(&self) -> PenPreset {
        let min_size = self.min_size.max(1.0).round() as u32;
        let max_size = self.max_size.max(self.min_size).round() as u32;
        let size = self
            .base_size
            .round()
            .clamp(min_size as f32, max_size as f32) as u32;
        PenPreset {
            id: self.id.clone(),
            name: self.name.clone(),
            plugin_id: self.plugin_id.clone(),
            size,
            pressure_enabled: self.pressure_enabled,
            antialias: self.antialias,
            stabilization: self.stabilization.min(100),
            engine: match self.engine {
                PenEngine::Stamp => PenRuntimeEngine::Stamp,
                PenEngine::Generated => PenRuntimeEngine::Generated,
            },
            spacing_percent: self.spacing_percent,
            rotation_degrees: self.rotation_deg,
            opacity: self.opacity,
            flow: self.flow,
            tip: self.tip.as_ref().map(runtime_tip_from_storage),
        }
    }

    /// 既定値を使って新しいインスタンスを生成する。
    pub fn from_runtime_preset(preset: &PenPreset) -> Self {
        Self {
            id: preset.id.clone(),
            name: preset.name.clone(),
            plugin_id: preset.plugin_id.clone(),
            base_size: preset.size as f32,
            min_size: 1.0,
            max_size: 64.0_f32.max(preset.size as f32),
            pressure_enabled: preset.pressure_enabled,
            antialias: preset.antialias,
            stabilization: preset.stabilization,
            engine: match preset.engine {
                PenRuntimeEngine::Stamp => PenEngine::Stamp,
                PenRuntimeEngine::Generated => PenEngine::Generated,
            },
            spacing_percent: preset.spacing_percent,
            rotation_deg: preset.rotation_degrees,
            opacity: preset.opacity,
            flow: preset.flow,
            tip: preset.tip.as_ref().map(storage_tip_from_runtime),
            ..Self::default()
        }
    }
}

#[derive(Debug, Deserialize)]
struct LegacyAltPaintPen {
    #[serde(default = "legacy_format_version")]
    format_version: u32,
    id: String,
    name: String,
    #[serde(default = "legacy_size")]
    size: u32,
    #[serde(default = "legacy_min_size")]
    min_size: u32,
    #[serde(default = "legacy_max_size")]
    max_size: u32,
    #[serde(default = "default_pen_pressure_enabled")]
    pressure_enabled: bool,
    #[serde(default = "default_pen_antialias")]
    antialias: bool,
    #[serde(default)]
    stabilization: u8,
}

impl TryFrom<LegacyAltPaintPen> for AltPaintPen {
    type Error = PenExchangeError;

    /// Try from 用の表示文字列を組み立てる。
    ///
    /// 失敗時はエラーを返します。
    fn try_from(value: LegacyAltPaintPen) -> Result<Self, Self::Error> {
        if value.format_version != 1 {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported legacy altpaint pen format version: {}",
                value.format_version
            )));
        }
        let pen = AltPaintPen {
            format_version: CURRENT_PEN_FORMAT_VERSION,
            id: value.id,
            name: value.name,
            plugin_id: default_plugin_id(),
            base_size: value.size.max(1) as f32,
            min_size: value.min_size.max(1) as f32,
            max_size: value.max_size.max(value.min_size.max(1)) as f32,
            pressure_enabled: value.pressure_enabled,
            antialias: value.antialias,
            stabilization: value.stabilization,
            ..AltPaintPen::default()
        };
        pen.validate()?;
        Ok(pen)
    }
}

/// 入力を解析して altpaint ペン JSON に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
pub fn parse_altpaint_pen_json(text: &str) -> Result<AltPaintPen, PenExchangeError> {
    let value: Value = serde_json::from_str(text)?;
    let format_version = value
        .get("format_version")
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;

    match format_version {
        1 => {
            let legacy = serde_json::from_value::<LegacyAltPaintPen>(value)?;
            AltPaintPen::try_from(legacy)
        }
        CURRENT_PEN_FORMAT_VERSION => {
            let pen = serde_json::from_value::<AltPaintPen>(value)?;
            pen.validate()?;
            Ok(pen)
        }
        version => Err(PenExchangeError::UnsupportedFormat(format!(
            "unsupported altpaint pen format version: {version}"
        ))),
    }
}

/// 現在 ペン 形式 version を計算して返す。
fn current_pen_format_version() -> u32 {
    CURRENT_PEN_FORMAT_VERSION
}

/// 既定の プラグイン ID を返す。
fn default_plugin_id() -> String {
    "builtin.bitmap".to_string()
}

/// legacy 形式 version を計算して返す。
fn legacy_format_version() -> u32 {
    1
}

/// 入力や種別に応じて処理を振り分ける。
fn runtime_tip_from_storage(tip: &PenTip) -> PenTipBitmap {
    match tip {
        PenTip::AlphaMask8 {
            width,
            height,
            data_base64,
        } => PenTipBitmap::AlphaMask8 {
            width: *width,
            height: *height,
            data: BASE64.decode(data_base64).unwrap_or_default(),
        },
        PenTip::Rgba8 {
            width,
            height,
            data_base64,
        } => PenTipBitmap::Rgba8 {
            width: *width,
            height: *height,
            data: BASE64.decode(data_base64).unwrap_or_default(),
        },
        PenTip::PngBlob {
            width,
            height,
            png_base64,
        } => PenTipBitmap::PngBlob {
            width: *width,
            height: *height,
            png: BASE64.decode(png_base64).unwrap_or_default(),
        },
    }
}

/// 入力や種別に応じて処理を振り分ける。
fn storage_tip_from_runtime(tip: &PenTipBitmap) -> PenTip {
    match tip {
        PenTipBitmap::AlphaMask8 {
            width,
            height,
            data,
        } => PenTip::from_alpha_mask(*width, *height, data),
        PenTipBitmap::Rgba8 {
            width,
            height,
            data,
        } => PenTip::from_rgba(*width, *height, data),
        PenTipBitmap::PngBlob { width, height, png } => PenTip::from_png_blob(*width, *height, png),
    }
}

/// 既定の base サイズ を返す。
fn default_base_size() -> f32 {
    4.0
}

/// 現在の legacy サイズ を返す。
fn legacy_size() -> u32 {
    4
}

/// 既定の min サイズ を返す。
fn default_min_size() -> f32 {
    1.0
}

/// 現在の legacy min サイズ を返す。
fn legacy_min_size() -> u32 {
    1
}

/// 既定の max サイズ を返す。
fn default_max_size() -> f32 {
    64.0
}

/// 現在の legacy max サイズ を返す。
fn legacy_max_size() -> u32 {
    64
}

/// 既定の spacing percent を返す。
fn default_spacing_percent() -> f32 {
    25.0
}

/// 既定の 不透明度 を返す。
fn default_opacity() -> f32 {
    1.0
}

/// 既定の flow を返す。
fn default_flow() -> f32 {
    1.0
}

/// 既定の ペン pressure enabled を返す。
fn default_pen_pressure_enabled() -> bool {
    true
}

/// 既定の ペン アンチエイリアス を返す。
fn default_pen_antialias() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// parses legacy ペン JSON as v2 ペン が期待どおりに動作することを検証する。
    #[test]
    fn parses_legacy_pen_json_as_v2_pen() {
        let json = r#"{
  "format_version": 1,
  "id": "legacy.round",
  "name": "Legacy Round",
  "size": 7,
  "pressure_enabled": true,
  "antialias": false,
  "stabilization": 13
}"#;

        let pen = parse_altpaint_pen_json(json).expect("legacy pen parses");

        assert_eq!(pen.format_version, CURRENT_PEN_FORMAT_VERSION);
        assert_eq!(pen.id, "legacy.round");
        assert_eq!(pen.base_size, 7.0);
        assert!(!pen.antialias);
        assert_eq!(pen.stabilization, 13);
    }

    /// runtime conversion clamps サイズ が期待どおりに動作することを検証する。
    #[test]
    fn runtime_conversion_clamps_size() {
        let pen = AltPaintPen {
            id: "pen.test".to_string(),
            name: "Test".to_string(),
            base_size: 300.0,
            min_size: 1.0,
            max_size: 16.0,
            ..AltPaintPen::default()
        };

        let preset = pen.to_runtime_preset();

        assert_eq!(preset.size, 16);
    }
}
