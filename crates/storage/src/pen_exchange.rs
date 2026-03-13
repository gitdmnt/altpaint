use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::pen_format::{
    AltPaintPen, PenPressureCurve, PenPressurePoint, PenSource, PenSourceKind, PenTip,
    parse_altpaint_pen_json,
};

#[derive(Debug, Error)]
pub enum PenExchangeError {
    #[error("i/o failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("json parse failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("unsupported pen format: {0}")]
    UnsupportedFormat(String),
    #[error("invalid pen data: {0}")]
    InvalidData(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PenImportIssueSeverity {
    Info,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PenImportIssue {
    pub severity: PenImportIssueSeverity,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PenImportReport {
    pub source: PenSourceKind,
    pub imported_count: usize,
    pub skipped_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<PenImportIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ImportedPenSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pens: Vec<AltPaintPen>,
    #[serde(default)]
    pub report: PenImportReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenFileKind {
    AltPaintJson,
    PhotoshopAbr,
    ClipStudioSut,
    GimpGbr,
}

/// 入力を解析して ペン file に変換する。
///
/// 失敗時はエラーを返します。
pub fn parse_pen_file(path: impl AsRef<Path>) -> Result<ImportedPenSet, PenExchangeError> {
    let path = path.as_ref();
    let kind = detect_pen_file_kind(path)?;
    match kind {
        PenFileKind::AltPaintJson => {
            let text = fs::read_to_string(path)?;
            let pen = parse_altpaint_pen_json(&text)?;
            Ok(ImportedPenSet {
                report: PenImportReport {
                    source: PenSourceKind::AltPaint,
                    imported_count: 1,
                    skipped_count: 0,
                    issues: Vec::new(),
                },
                pens: vec![pen],
            })
        }
        PenFileKind::PhotoshopAbr => {
            let bytes = fs::read(path)?;
            parse_photoshop_abr_bytes(
                &bytes,
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("brush.abr"),
            )
        }
        PenFileKind::ClipStudioSut => parse_clip_studio_sut(path),
        PenFileKind::GimpGbr => {
            let bytes = fs::read(path)?;
            let pen = parse_gimp_gbr_bytes(
                &bytes,
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("brush.gbr"),
            )?;
            Ok(ImportedPenSet {
                report: PenImportReport {
                    source: PenSourceKind::GimpGbr,
                    imported_count: 1,
                    skipped_count: 0,
                    issues: Vec::new(),
                },
                pens: vec![pen],
            })
        }
    }
}

/// 現在の値を altpaint ペン JSON へ変換する。
///
/// 失敗時はエラーを返します。
pub fn export_altpaint_pen_json(pen: &AltPaintPen) -> Result<String, PenExchangeError> {
    pen.validate()?;
    serde_json::to_string_pretty(pen).map_err(Into::into)
}

/// 現在の値を gimp gbr へ変換する。
///
/// 失敗時はエラーを返します。
pub fn export_gimp_gbr(pen: &AltPaintPen) -> Result<Vec<u8>, PenExchangeError> {
    pen.validate()?;
    let tip = pen.tip.as_ref().ok_or_else(|| {
        PenExchangeError::UnsupportedFormat(
            "only pens with embedded brush tips can be exported to GBR".to_string(),
        )
    })?;
    let width = tip.width();
    let height = tip.height();
    let name = pen.name.as_bytes();

    let (bytes_per_pixel, pixel_bytes) = match tip {
        PenTip::AlphaMask8 { .. } => {
            let alpha = tip.alpha_mask_bytes()?;
            let gbr_bytes = alpha
                .into_iter()
                .map(|value| 255_u8.saturating_sub(value))
                .collect();
            (1_u32, gbr_bytes)
        }
        PenTip::Rgba8 { .. } => (4_u32, tip.rgba_bytes()?),
        PenTip::PngBlob { .. } => {
            return Err(PenExchangeError::UnsupportedFormat(
                "png-blob tips must be rasterized before GBR export".to_string(),
            ));
        }
    };

    let header_size = 28_u32
        .checked_add(name.len() as u32)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| PenExchangeError::InvalidData("GBR header size overflow".to_string()))?;
    let spacing = pen.spacing_percent.round().clamp(1.0, 1000.0) as u32;

    let mut out = Vec::with_capacity(header_size as usize + pixel_bytes.len());
    write_u32_be(&mut out, header_size);
    write_u32_be(&mut out, 2);
    write_u32_be(&mut out, width);
    write_u32_be(&mut out, height);
    write_u32_be(&mut out, bytes_per_pixel);
    out.extend_from_slice(b"GIMP");
    write_u32_be(&mut out, spacing);
    out.extend_from_slice(name);
    out.push(0);
    out.extend_from_slice(&pixel_bytes);
    Ok(out)
}

/// 入力を解析して gimp gbr bytes に変換し、失敗時はエラーを返す。
pub fn parse_gimp_gbr_bytes(
    bytes: &[u8],
    file_name: &str,
) -> Result<AltPaintPen, PenExchangeError> {
    if bytes.len() < 20 {
        return Err(PenExchangeError::InvalidData(
            "GBR file is too small".to_string(),
        ));
    }

    let header_size = read_u32_be(bytes, 0)? as usize;
    let version = read_u32_be(bytes, 4)?;
    let width = read_u32_be(bytes, 8)?;
    let height = read_u32_be(bytes, 12)?;
    let bytes_per_pixel = read_u32_be(bytes, 16)?;

    let (name_offset, spacing_percent) = match version {
        1 => (20_usize, 25.0_f32),
        2 => {
            if bytes.len() < 28 {
                return Err(PenExchangeError::InvalidData(
                    "GBR v2 file is truncated".to_string(),
                ));
            }
            if &bytes[20..24] != b"GIMP" {
                return Err(PenExchangeError::InvalidData(
                    "GBR v2 magic does not match GIMP".to_string(),
                ));
            }
            let spacing = read_u32_be(bytes, 24)?;
            (28_usize, spacing as f32)
        }
        other => {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported GBR version: {other}"
            )));
        }
    };

    if header_size == 0 || header_size > bytes.len() || header_size <= name_offset {
        return Err(PenExchangeError::InvalidData(
            "GBR header_size is out of range".to_string(),
        ));
    }

    let name_bytes = &bytes[name_offset..header_size];
    let name = if version == 1 {
        String::from_utf8_lossy(trim_trailing_nul(name_bytes)).to_string()
    } else {
        String::from_utf8(trim_trailing_nul(name_bytes).to_vec())
            .unwrap_or_else(|_| String::from_utf8_lossy(trim_trailing_nul(name_bytes)).to_string())
    };

    let pixel_bytes = &bytes[header_size..];
    let expected = width
        .checked_mul(height)
        .and_then(|value| value.checked_mul(bytes_per_pixel))
        .ok_or_else(|| PenExchangeError::InvalidData("GBR pixel size overflow".to_string()))?
        as usize;
    if pixel_bytes.len() < expected {
        return Err(PenExchangeError::InvalidData(
            "GBR pixel data is truncated".to_string(),
        ));
    }

    let tip = match bytes_per_pixel {
        1 => {
            let alpha: Vec<u8> = pixel_bytes[..expected]
                .iter()
                .map(|value| 255_u8.saturating_sub(*value))
                .collect();
            PenTip::from_alpha_mask(width, height, &alpha)
        }
        4 => PenTip::from_rgba(width, height, &pixel_bytes[..expected]),
        other => {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported GBR bytes-per-pixel value: {other}"
            )));
        }
    };

    let mut source = PenSource {
        kind: PenSourceKind::GimpGbr,
        original_file: Some(file_name.to_string()),
        notes: Vec::new(),
        raw_fields: Map::new(),
    };
    source
        .raw_fields
        .insert("version".to_string(), json!(version));
    source.raw_fields.insert("width".to_string(), json!(width));
    source
        .raw_fields
        .insert("height".to_string(), json!(height));
    source
        .raw_fields
        .insert("bytes_per_pixel".to_string(), json!(bytes_per_pixel));

    let pen = AltPaintPen {
        id: build_pen_id("gbr", file_name, 1),
        name: if name.trim().is_empty() {
            path_stem(file_name).to_string()
        } else {
            name
        },
        base_size: width.max(height) as f32,
        min_size: 1.0,
        max_size: width.max(height).max(64) as f32,
        spacing_percent,
        pressure_enabled: false,
        antialias: true,
        stabilization: 0,
        tip: Some(tip),
        source,
        ..AltPaintPen::default()
    };
    pen.validate()?;
    Ok(pen)
}

/// 入力を解析して photoshop abr bytes に変換し、失敗時はエラーを返す。
pub fn parse_photoshop_abr_bytes(
    bytes: &[u8],
    file_name: &str,
) -> Result<ImportedPenSet, PenExchangeError> {
    if bytes.len() < 4 {
        return Err(PenExchangeError::InvalidData(
            "ABR file is too small".to_string(),
        ));
    }

    let mut cursor = Cursor::new(bytes);
    let version = read_cursor_u16_be(&mut cursor)?;
    let mut report = PenImportReport {
        source: PenSourceKind::PhotoshopAbr,
        imported_count: 0,
        skipped_count: 0,
        issues: Vec::new(),
    };

    let pens = match version {
        1 | 2 => {
            let count = read_cursor_u16_be(&mut cursor)? as usize;
            let mut pens = Vec::new();
            for index in 0..count {
                if let Some(pen) = parse_abr_v12_brush(
                    bytes,
                    &mut cursor,
                    version,
                    index + 1,
                    file_name,
                    &mut report,
                )? {
                    pens.push(pen);
                }
            }
            pens
        }
        6 => {
            let subversion = read_cursor_u16_be(&mut cursor)?;
            if !matches!(subversion, 1 | 2) {
                return Err(PenExchangeError::UnsupportedFormat(format!(
                    "unsupported ABR v6 subversion: {subversion}"
                )));
            }
            parse_abr_v6(bytes, subversion, file_name, &mut report)?
        }
        other => {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported ABR version: {other}"
            )));
        }
    };

    report.imported_count = pens.len();
    Ok(ImportedPenSet { pens, report })
}

/// 入力を解析して clip studio sut に変換し、失敗時はエラーを返す。
///
/// 失敗時はエラーを返します。
pub fn parse_clip_studio_sut(path: impl AsRef<Path>) -> Result<ImportedPenSet, PenExchangeError> {
    let path = path.as_ref();
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let uri = format!(
        "file:{}?immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let connection = Connection::open_with_flags(&uri, flags)
        .or_else(|_| Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY))?;

    let tables = list_sqlite_tables(&connection)?;
    if !tables.iter().any(|name| name == "Node") || !tables.iter().any(|name| name == "Variant") {
        return Err(PenExchangeError::UnsupportedFormat(
            "SUT file does not expose the expected Node/Variant tables".to_string(),
        ));
    }

    let material_pngs = load_material_png_metadata(&connection)?;
    let nodes = load_sut_nodes(&connection)?;

    let mut pens = Vec::new();
    let mut report = PenImportReport {
        source: PenSourceKind::ClipStudioSut,
        imported_count: 0,
        skipped_count: 0,
        issues: Vec::new(),
    };

    for (index, node) in nodes.into_iter().enumerate() {
        let Some(variant_id) = node.variant_id.or(node.init_variant_id) else {
            report.skipped_count += 1;
            report.issues.push(PenImportIssue {
                severity: PenImportIssueSeverity::Warning,
                code: "sut-node-missing-variant".to_string(),
                message: format!(
                    "skipped '{}' because it has no variant reference",
                    node.name
                ),
            });
            continue;
        };

        let variant = load_variant_row(&connection, variant_id)?;
        let mut source = PenSource {
            kind: PenSourceKind::ClipStudioSut,
            original_file: Some(
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
                    .to_string(),
            ),
            notes: Vec::new(),
            raw_fields: variant.raw_fields,
        };

        let base_size = variant.base_size.unwrap_or(4.0).clamp(1.0, 10000.0);
        let spacing_percent = variant.spacing_percent.unwrap_or(25.0).clamp(1.0, 1000.0);
        let opacity = variant.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        let flow = variant.flow.unwrap_or(1.0).clamp(0.0, 1.0);
        let stabilization = variant.stabilization.unwrap_or(0).min(100);
        let pressure_curve = variant.pressure_curve;

        if variant.base_size.is_none() {
            source
                .notes
                .push("size column could not be mapped confidently; defaulted to 4px".to_string());
        }
        if variant.spacing_percent.is_none() {
            source.notes.push(
                "spacing column could not be mapped confidently; defaulted to 25%".to_string(),
            );
        }
        if !material_pngs.is_empty() {
            source.raw_fields.insert(
                "material_pngs".to_string(),
                Value::Array(material_pngs.iter().cloned().map(Value::Object).collect()),
            );
            source.notes.push(
                "material PNG previews were discovered, but exact pen-to-material binding remains best-effort".to_string(),
            );
        }

        let pen = AltPaintPen {
            id: build_pen_id(
                "sut",
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("brush.sut"),
                index + 1,
            ),
            name: node.name,
            base_size,
            min_size: 1.0,
            max_size: base_size.max(64.0),
            spacing_percent,
            opacity,
            flow,
            pressure_enabled: pressure_curve.is_some(),
            antialias: true,
            stabilization,
            dynamics: crate::PenDynamics {
                size_pressure_curve: pressure_curve,
                ..Default::default()
            },
            source,
            ..AltPaintPen::default()
        };
        pen.validate()?;
        pens.push(pen);
    }

    report.imported_count = pens.len();
    Ok(ImportedPenSet { pens, report })
}

/// 現在の値を ペン file kind へ変換する。
///
/// 失敗時はエラーを返します。
fn detect_pen_file_kind(path: &Path) -> Result<PenFileKind, PenExchangeError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.ends_with(".altp-pen.json") {
        return Ok(PenFileKind::AltPaintJson);
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        Some(ext) if ext == "abr" => Ok(PenFileKind::PhotoshopAbr),
        Some(ext) if ext == "sut" => Ok(PenFileKind::ClipStudioSut),
        Some(ext) if ext == "gbr" => Ok(PenFileKind::GimpGbr),
        Some(ext) => Err(PenExchangeError::UnsupportedFormat(format!(
            "unsupported pen file extension: .{ext}"
        ))),
        None => Err(PenExchangeError::UnsupportedFormat(
            "pen file extension is missing".to_string(),
        )),
    }
}

/// 入力を解析して abr v12 ブラシ に変換し、失敗時はエラーを返す。
fn parse_abr_v12_brush(
    bytes: &[u8],
    cursor: &mut Cursor<&[u8]>,
    version: u16,
    index: usize,
    file_name: &str,
    report: &mut PenImportReport,
) -> Result<Option<AltPaintPen>, PenExchangeError> {
    let brush_type = read_cursor_u16_be(cursor)?;
    let brush_size = read_cursor_u32_be(cursor)? as u64;
    let next_pos = cursor.position().checked_add(brush_size).ok_or_else(|| {
        PenExchangeError::InvalidData("ABR brush record position overflow".to_string())
    })?;
    if next_pos > bytes.len() as u64 {
        return Err(PenExchangeError::InvalidData(
            "ABR brush record exceeds file size".to_string(),
        ));
    }

    match brush_type {
        1 => {
            cursor.set_position(next_pos);
            report.skipped_count += 1;
            report.issues.push(PenImportIssue {
                severity: PenImportIssueSeverity::Warning,
                code: "abr-computed-brush-skipped".to_string(),
                message: format!(
                    "skipped computed brush #{index}; only sampled brushes are imported"
                ),
            });
            Ok(None)
        }
        2 => {
            let _misc = read_cursor_u32_be(cursor)?;
            let spacing = read_cursor_u16_be(cursor)? as f32;
            let name = if version == 2 {
                read_photoshop_unicode_string(cursor)?
            } else {
                String::new()
            };
            let _antialias = read_cursor_u8(cursor)?;
            for _ in 0..4 {
                let _ = read_cursor_i16_be(cursor)?;
            }
            let top = read_cursor_i32_be(cursor)?;
            let left = read_cursor_i32_be(cursor)?;
            let bottom = read_cursor_i32_be(cursor)?;
            let right = read_cursor_i32_be(cursor)?;
            let depth_bits = read_cursor_u16_be(cursor)?;
            let compression = read_cursor_u8(cursor)?;

            let width = positive_dimension(right - left, "ABR width")?;
            let height = positive_dimension(bottom - top, "ABR height")?;
            let tip = read_abr_tip(cursor, width, height, depth_bits, compression)?;
            cursor.set_position(next_pos);

            let mut source = PenSource {
                kind: PenSourceKind::PhotoshopAbr,
                original_file: Some(file_name.to_string()),
                notes: Vec::new(),
                raw_fields: Map::new(),
            };
            source
                .raw_fields
                .insert("version".to_string(), json!(version));
            source
                .raw_fields
                .insert("brush_type".to_string(), json!(brush_type));
            source
                .raw_fields
                .insert("depth_bits".to_string(), json!(depth_bits));
            source
                .raw_fields
                .insert("compression".to_string(), json!(compression));

            let pen = AltPaintPen {
                id: build_pen_id("abr", file_name, index),
                name: if name.trim().is_empty() {
                    format!("{} {}", path_stem(file_name), index)
                } else {
                    name
                },
                base_size: width.max(height) as f32,
                min_size: 1.0,
                max_size: width.max(height).max(64) as f32,
                spacing_percent: spacing.clamp(1.0, 1000.0),
                pressure_enabled: false,
                antialias: true,
                stabilization: 0,
                tip: Some(tip),
                source,
                ..AltPaintPen::default()
            };
            pen.validate()?;
            Ok(Some(pen))
        }
        other => {
            cursor.set_position(next_pos);
            report.skipped_count += 1;
            report.issues.push(PenImportIssue {
                severity: PenImportIssueSeverity::Warning,
                code: "abr-unknown-brush-type".to_string(),
                message: format!("skipped unsupported ABR brush type {other} at index {index}"),
            });
            Ok(None)
        }
    }
}

/// 入力を解析して abr v6 に変換し、失敗時はエラーを返す。
fn parse_abr_v6(
    bytes: &[u8],
    subversion: u16,
    file_name: &str,
    report: &mut PenImportReport,
) -> Result<Vec<AltPaintPen>, PenExchangeError> {
    let Some(section_offset) = find_subslice(bytes, b"8BIMsamp") else {
        return Err(PenExchangeError::InvalidData(
            "ABR v6 file does not contain an 8BIM samp section".to_string(),
        ));
    };
    let size_offset = section_offset + 8;
    let section_size = read_u32_be(bytes, size_offset)? as usize;
    let section_start = size_offset + 4;
    let section_end = section_start
        .checked_add(section_size)
        .ok_or_else(|| PenExchangeError::InvalidData("ABR sample section overflow".to_string()))?;
    if section_end > bytes.len() {
        return Err(PenExchangeError::InvalidData(
            "ABR sample section exceeds file size".to_string(),
        ));
    }

    let section = &bytes[section_start..section_end];
    let mut offset = 0_usize;
    let mut blobs = Vec::new();
    while offset + 4 <= section.len() {
        let brush_size = read_u32_be(section, offset)? as usize;
        offset += 4;
        if offset + brush_size > section.len() {
            break;
        }
        let brush_blob = &section[offset..offset + brush_size];
        blobs.push(brush_blob);
        offset += align4(brush_size);
    }

    let mut pens = Vec::new();
    for (index, brush_blob) in blobs.iter().enumerate() {
        if let Some(pen) = parse_abr_v6_sample(
            brush_blob,
            subversion,
            blobs.len(),
            file_name,
            index + 1,
            report,
        )? {
            pens.push(pen);
        }
    }
    Ok(pens)
}

/// 入力を解析して abr v6 sample に変換する。
fn parse_abr_v6_sample(
    brush_blob: &[u8],
    subversion: u16,
    total_samples: usize,
    file_name: &str,
    index: usize,
    report: &mut PenImportReport,
) -> Result<Option<AltPaintPen>, PenExchangeError> {
    let mut candidates = Vec::new();
    let subversion_skip = if subversion == 1 { 47 } else { 301 };
    candidates.push(subversion_skip);
    let count_skip = if total_samples == 1 { 47 } else { 301 };
    if count_skip != subversion_skip {
        candidates.push(count_skip);
    }

    for skip in candidates {
        if let Some((width, height, depth_bits, compression, data_offset)) =
            validate_v6_layout(brush_blob, skip)
        {
            let mut cursor = Cursor::new(&brush_blob[data_offset..]);
            let tip = read_abr_tip(&mut cursor, width, height, depth_bits, compression)?;

            let mut source = PenSource {
                kind: PenSourceKind::PhotoshopAbr,
                original_file: Some(file_name.to_string()),
                notes: vec![
                    "ABR v6 spacing was not recoverable from the sampled section and defaulted to 25%".to_string(),
                ],
                raw_fields: Map::new(),
            };
            source.raw_fields.insert("version".to_string(), json!(6));
            source
                .raw_fields
                .insert("subversion".to_string(), json!(subversion));
            source
                .raw_fields
                .insert("layout_skip".to_string(), json!(skip));
            source
                .raw_fields
                .insert("depth_bits".to_string(), json!(depth_bits));
            source
                .raw_fields
                .insert("compression".to_string(), json!(compression));

            let pen = AltPaintPen {
                id: build_pen_id("abr", file_name, index),
                name: format!("{} {}", path_stem(file_name), index),
                base_size: width.max(height) as f32,
                min_size: 1.0,
                max_size: width.max(height).max(64) as f32,
                spacing_percent: 25.0,
                pressure_enabled: false,
                antialias: true,
                stabilization: 0,
                tip: Some(tip),
                source,
                ..AltPaintPen::default()
            };
            pen.validate()?;
            return Ok(Some(pen));
        }
    }

    report.skipped_count += 1;
    report.issues.push(PenImportIssue {
        severity: PenImportIssueSeverity::Warning,
        code: "abr-v6-sample-skipped".to_string(),
        message: format!(
            "skipped ABR v6 sampled brush #{index} because its sampled header could not be decoded safely"
        ),
    });
    Ok(None)
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn validate_v6_layout(blob: &[u8], skip: usize) -> Option<(u32, u32, u16, u8, usize)> {
    let header_end = skip.checked_add(4 * 4 + 2 + 1)?;
    if header_end > blob.len() {
        return None;
    }
    let top = read_i32_be(blob, skip).ok()?;
    let left = read_i32_be(blob, skip + 4).ok()?;
    let bottom = read_i32_be(blob, skip + 8).ok()?;
    let right = read_i32_be(blob, skip + 12).ok()?;
    let depth_bits = read_u16_be(blob, skip + 16).ok()?;
    let compression = *blob.get(skip + 18)?;
    let width = positive_dimension(right - left, "ABR width").ok()?;
    let height = positive_dimension(bottom - top, "ABR height").ok()?;
    let depth_bytes = (depth_bits / 8) as usize;
    if !(depth_bytes == 1 || depth_bytes == 2) {
        return None;
    }

    let expected_raw = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(depth_bytes)?;
    let remaining = blob.len().checked_sub(header_end)?;
    match compression {
        0 if remaining >= expected_raw => {
            Some((width, height, depth_bits, compression, header_end))
        }
        1 if remaining >= height as usize * 2 => {
            Some((width, height, depth_bits, compression, header_end))
        }
        _ => None,
    }
}

/// 入力や種別に応じて処理を振り分ける。
fn read_abr_tip(
    cursor: &mut Cursor<&[u8]>,
    width: u32,
    height: u32,
    depth_bits: u16,
    compression: u8,
) -> Result<PenTip, PenExchangeError> {
    let depth_bytes = match depth_bits / 8 {
        1 => 1_usize,
        2 => 2_usize,
        other => {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported ABR sampled brush depth: {} bits ({} bytes)",
                depth_bits, other
            )));
        }
    };
    let row_stride = width as usize * depth_bytes;
    let pixel_count = width as usize * height as usize;
    let raw = match compression {
        0 => {
            let mut buffer = vec![0_u8; row_stride * height as usize];
            cursor.read_exact(&mut buffer)?;
            buffer
        }
        1 => decode_packbits_rows(cursor, row_stride, height as usize)?,
        other => {
            return Err(PenExchangeError::UnsupportedFormat(format!(
                "unsupported ABR compression value: {other}"
            )));
        }
    };

    let alpha = if depth_bytes == 1 {
        raw.into_iter()
            .map(|value| 255_u8.saturating_sub(value))
            .collect::<Vec<_>>()
    } else {
        let mut out = Vec::with_capacity(pixel_count);
        for chunk in raw.chunks_exact(2) {
            let value = u16::from_be_bytes([chunk[0], chunk[1]]);
            out.push(255_u8.saturating_sub((value >> 8) as u8));
        }
        out
    };

    Ok(PenTip::from_alpha_mask(width, height, &alpha))
}

/// decode packbits rows に必要な処理を行う。
fn decode_packbits_rows(
    cursor: &mut Cursor<&[u8]>,
    row_stride: usize,
    height: usize,
) -> Result<Vec<u8>, PenExchangeError> {
    let mut row_lengths = Vec::with_capacity(height);
    for _ in 0..height {
        row_lengths.push(read_cursor_u16_be(cursor)? as usize);
    }

    let mut output = Vec::with_capacity(row_stride * height);
    for row_length in row_lengths {
        let mut encoded = vec![0_u8; row_length];
        cursor.read_exact(&mut encoded)?;
        let decoded = decode_packbits_stream(&encoded, row_stride)?;
        output.extend_from_slice(&decoded);
    }
    Ok(output)
}

/// 現在の値を packbits stream へ変換する。
///
/// 失敗時はエラーを返します。
fn decode_packbits_stream(data: &[u8], expected_len: usize) -> Result<Vec<u8>, PenExchangeError> {
    let mut cursor = 0_usize;
    let mut out = Vec::with_capacity(expected_len);
    while cursor < data.len() && out.len() < expected_len {
        let n = data[cursor] as i8;
        cursor += 1;
        match n {
            0..=127 => {
                let count = n as usize + 1;
                if cursor + count > data.len() {
                    return Err(PenExchangeError::InvalidData(
                        "PackBits literal run exceeds input size".to_string(),
                    ));
                }
                out.extend_from_slice(&data[cursor..cursor + count]);
                cursor += count;
            }
            -127..=-1 => {
                let count = (1_i16 - n as i16) as usize;
                let Some(byte) = data.get(cursor) else {
                    return Err(PenExchangeError::InvalidData(
                        "PackBits repeat run is truncated".to_string(),
                    ));
                };
                cursor += 1;
                out.extend(std::iter::repeat_n(*byte, count));
            }
            -128 => {}
        }
    }
    if out.len() != expected_len {
        return Err(PenExchangeError::InvalidData(format!(
            "PackBits row length mismatch: expected {expected_len}, got {}",
            out.len()
        )));
    }
    Ok(out)
}

#[derive(Debug)]
struct SutNode {
    name: String,
    variant_id: Option<i64>,
    init_variant_id: Option<i64>,
}

#[derive(Debug, Default)]
struct SutVariantData {
    base_size: Option<f32>,
    spacing_percent: Option<f32>,
    opacity: Option<f32>,
    flow: Option<f32>,
    stabilization: Option<u8>,
    pressure_curve: Option<PenPressureCurve>,
    raw_fields: Map<String, Value>,
}

/// 一覧 sqlite tables を計算して返す。
///
/// 失敗時はエラーを返します。
fn list_sqlite_tables(connection: &Connection) -> Result<Vec<String>, PenExchangeError> {
    let mut statement =
        connection.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?;
    let mut rows = statement.query([])?;
    let mut tables = Vec::new();
    while let Some(row) = rows.next()? {
        tables.push(row.get::<_, String>(0)?);
    }
    Ok(tables)
}

/// Sut nodes を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn load_sut_nodes(connection: &Connection) -> Result<Vec<SutNode>, PenExchangeError> {
    let mut statement = connection.prepare(
        "SELECT NodeName, NodeVariantId, NodeInitVariantId FROM Node WHERE trim(COALESCE(NodeName, '')) <> '' ORDER BY rowid",
    )?;
    let mut rows = statement.query([])?;
    let mut nodes = Vec::new();
    while let Some(row) = rows.next()? {
        nodes.push(SutNode {
            name: row.get(0)?,
            variant_id: row.get(1)?,
            init_variant_id: row.get(2)?,
        });
    }
    Ok(nodes)
}

/// 現在の値を variant row へ変換する。
fn load_variant_row(
    connection: &Connection,
    variant_id: i64,
) -> Result<SutVariantData, PenExchangeError> {
    let columns = sqlite_table_columns(connection, "Variant")?;
    if columns.is_empty() {
        return Err(PenExchangeError::InvalidData(
            "Variant table has no columns".to_string(),
        ));
    }
    let variant_id_column = columns
        .iter()
        .find(|name| name.eq_ignore_ascii_case("VariantID"))
        .cloned()
        .ok_or_else(|| {
            PenExchangeError::InvalidData("Variant table has no VariantID column".to_string())
        })?;

    let select_columns = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_columns} FROM Variant WHERE {} = ?1 LIMIT 1",
        quote_identifier(&variant_id_column)
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([variant_id])?;
    let Some(row) = rows.next()? else {
        return Err(PenExchangeError::InvalidData(format!(
            "Variant row {variant_id} was not found"
        )));
    };

    let mut data = SutVariantData::default();
    for (index, column) in columns.iter().enumerate() {
        let lower = column.to_ascii_lowercase();
        let value_ref = row.get_ref(index)?;
        match value_ref {
            ValueRef::Null => {}
            ValueRef::Integer(value) => {
                data.raw_fields.insert(column.clone(), json!(value));
                map_numeric_field(&lower, value as f64, &mut data);
            }
            ValueRef::Real(value) => {
                data.raw_fields.insert(column.clone(), json!(value));
                map_numeric_field(&lower, value, &mut data);
            }
            ValueRef::Text(bytes) => {
                data.raw_fields.insert(
                    column.clone(),
                    Value::String(String::from_utf8_lossy(bytes).to_string()),
                );
            }
            ValueRef::Blob(blob) => {
                if lower == "pressuregraph" {
                    if let Some(curve) = parse_csp_pressure_graph(blob) {
                        data.pressure_curve = Some(curve.clone());
                        data.raw_fields.insert(
                            column.clone(),
                            json!({
                                "bytes": blob.len(),
                                "points": curve.points.len(),
                                "sha256": sha256_hex(blob),
                            }),
                        );
                    }
                } else if lower.contains("texture") || lower.contains("pattern") {
                    let strings = extract_utf16le_strings(blob);
                    data.raw_fields.insert(
                        column.clone(),
                        json!({
                            "bytes": blob.len(),
                            "sha256": sha256_hex(blob),
                            "strings": strings,
                        }),
                    );
                } else {
                    data.raw_fields.insert(
                        column.clone(),
                        json!({
                            "bytes": blob.len(),
                            "sha256": sha256_hex(blob),
                        }),
                    );
                }
            }
        }
    }
    Ok(data)
}

/// Sqlite table columns 用の表示文字列を組み立てる。
fn sqlite_table_columns(
    connection: &Connection,
    table: &str,
) -> Result<Vec<String>, PenExchangeError> {
    let sql = format!("PRAGMA table_info({})", quote_identifier(table));
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    let mut columns = Vec::new();
    while let Some(row) = rows.next()? {
        columns.push(row.get::<_, String>(1)?);
    }
    Ok(columns)
}

/// 現在の値を material PNG metadata へ変換する。
fn load_material_png_metadata(
    connection: &Connection,
) -> Result<Vec<Map<String, Value>>, PenExchangeError> {
    let tables = list_sqlite_tables(connection)?;
    if !tables.iter().any(|name| name == "MaterialFile") {
        return Ok(Vec::new());
    }
    let columns = sqlite_table_columns(connection, "MaterialFile")?;
    let Some(file_data_column) = columns
        .iter()
        .find(|name| name.eq_ignore_ascii_case("FileData"))
        .cloned()
    else {
        return Ok(Vec::new());
    };

    let mut select_columns = Vec::new();
    for candidate in ["FileName", "MaterialName", "Name", "_PW_ID"] {
        if columns
            .iter()
            .any(|column| column.eq_ignore_ascii_case(candidate))
        {
            select_columns.push(candidate.to_string());
        }
    }
    select_columns.push(file_data_column.clone());

    let sql = format!(
        "SELECT {} FROM MaterialFile",
        select_columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let mut name_hint = None;
        for index in 0..select_columns.len().saturating_sub(1) {
            if let Ok(value) = row.get::<_, Option<String>>(index)
                && value.as_ref().is_some_and(|value| !value.trim().is_empty())
            {
                name_hint = value;
                break;
            }
        }
        let blob = row.get_ref(select_columns.len() - 1)?;
        if let ValueRef::Blob(bytes) = blob {
            for (png_index, png) in extract_png_blobs(bytes).into_iter().enumerate() {
                let mut metadata = Map::new();
                metadata.insert(
                    "label".to_string(),
                    Value::String(
                        name_hint
                            .clone()
                            .unwrap_or_else(|| format!("material-{}", png_index + 1)),
                    ),
                );
                metadata.insert("sha256".to_string(), Value::String(sha256_hex(&png)));
                metadata.insert("bytes".to_string(), json!(png.len()));
                if let Some((width, height)) = png_dimensions(&png) {
                    metadata.insert("width".to_string(), json!(width));
                    metadata.insert("height".to_string(), json!(height));
                }
                result.push(metadata);
            }
        }
    }
    Ok(result)
}

/// Numeric field を別座標系へ変換する。
fn map_numeric_field(name: &str, value: f64, data: &mut SutVariantData) {
    if data.base_size.is_none()
        && matches_any(
            name,
            &[
                "brushsize",
                "brush_size",
                "size",
                "diameter",
                "brushdiameter",
            ],
        )
    {
        data.base_size = Some(value as f32);
    }
    if data.spacing_percent.is_none()
        && matches_any(
            name,
            &[
                "brushspacing",
                "spacing",
                "interval",
                "step",
                "stepdistance",
            ],
        )
    {
        data.spacing_percent = Some(value as f32);
    }
    if data.opacity.is_none() && matches_any(name, &["opacity", "brushopacity"]) {
        data.opacity = Some(normalize_ratio(value));
    }
    if data.flow.is_none() && matches_any(name, &["flow", "paintamount", "density"]) {
        data.flow = Some(normalize_ratio(value));
    }
    if data.stabilization.is_none()
        && matches_any(
            name,
            &[
                "stabilization",
                "correction",
                "stabilizer",
                "stabilizerlevel",
            ],
        )
    {
        data.stabilization = Some(value.round().clamp(0.0, 100.0) as u8);
    }
}

/// 既存データを走査して matches any を組み立てる。
fn matches_any(name: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| name.contains(pattern))
}

/// Normalize ratio を有効範囲へ補正して返す。
fn normalize_ratio(value: f64) -> f32 {
    if value > 1.0 {
        (value / 100.0).clamp(0.0, 1.0) as f32
    } else {
        value.clamp(0.0, 1.0) as f32
    }
}

/// 入力を解析して csp pressure graph に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn parse_csp_pressure_graph(blob: &[u8]) -> Option<PenPressureCurve> {
    if blob.len() < 28 || !(blob.len() - 28).is_multiple_of(8) {
        return None;
    }
    let count = read_u32_be(blob, 4).ok()? as usize;
    let mut values = Vec::new();
    let mut offset = 28_usize;
    while offset + 8 <= blob.len() {
        let bytes: [u8; 8] = blob[offset..offset + 8].try_into().ok()?;
        values.push(f64::from_be_bytes(bytes));
        offset += 8;
    }
    if count != 0 && count != values.len() {
        return None;
    }
    if values.is_empty() {
        return None;
    }
    let last = (values.len() - 1).max(1) as f32;
    Some(PenPressureCurve {
        points: values
            .into_iter()
            .enumerate()
            .map(|(index, value)| PenPressurePoint {
                x: index as f32 / last,
                y: value.clamp(0.0, 1.0) as f32,
            })
            .collect(),
    })
}

/// 既存データを走査して extract utf16le strings を組み立てる。
fn extract_utf16le_strings(blob: &[u8]) -> Vec<String> {
    let mut strings = BTreeMap::<String, ()>::new();
    let mut cursor = 0_usize;
    while cursor + 4 <= blob.len() {
        let mut units = Vec::new();
        let start = cursor;
        while cursor + 1 < blob.len() {
            let unit = u16::from_le_bytes([blob[cursor], blob[cursor + 1]]);
            cursor += 2;
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        if units.len() >= 3 {
            let string = String::from_utf16_lossy(&units);
            if string.chars().all(|ch| !ch.is_control()) {
                strings.insert(string, ());
            }
        }
        if cursor == start {
            cursor += 2;
        }
    }
    strings.into_keys().collect()
}

/// 現在の値を PNG blobs へ変換する。
fn extract_png_blobs(blob: &[u8]) -> Vec<Vec<u8>> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
    const PNG_END: &[u8; 8] = b"\x00\x00\x00\x00IEND";

    let mut result = Vec::new();
    let mut search_start = 0_usize;
    while let Some(start) = find_subslice(&blob[search_start..], PNG_SIG) {
        let absolute_start = search_start + start;
        let after_sig = absolute_start + PNG_SIG.len();
        if let Some(end) = find_subslice(&blob[after_sig..], PNG_END) {
            let absolute_end = after_sig + end + 12;
            if absolute_end <= blob.len() {
                result.push(blob[absolute_start..absolute_end].to_vec());
                search_start = absolute_end;
                continue;
            }
        }
        break;
    }
    result
}

/// PNG dimensions を計算して返す。
///
/// 値を生成できない場合は `None` を返します。
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = read_u32_be(bytes, 16).ok()?;
    let height = read_u32_be(bytes, 20).ok()?;
    Some((width, height))
}

/// Sha256 16進文字列 用の表示文字列を組み立てる。
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// ペン ID を構築する。
fn build_pen_id(prefix: &str, file_name: &str, index: usize) -> String {
    format!(
        "{}.{}.{}",
        prefix,
        sanitize_identifier(path_stem(file_name)),
        index
    )
}

/// 現在の値を identifier へ変換する。
fn sanitize_identifier(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            result.push(ch);
        } else {
            result.push('-');
        }
    }
    result.trim_matches('-').to_string()
}

/// パス stem を計算して返す。
fn path_stem(file_name: &str) -> &str {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
}

/// 入力や種別に応じて処理を振り分ける。
fn trim_trailing_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|value| *value == 0) {
        Some(index) => &bytes[..index],
        None => bytes,
    }
}

/// Quote identifier 用の表示文字列を組み立てる。
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Positive 寸法 用の表示文字列を組み立てる。
///
/// 失敗時はエラーを返します。
fn positive_dimension(value: i32, label: &str) -> Result<u32, PenExchangeError> {
    if value <= 0 {
        return Err(PenExchangeError::InvalidData(format!(
            "{label} must be positive, got {value}"
        )));
    }
    Ok(value as u32)
}

/// align4 を計算して返す。
fn align4(value: usize) -> usize {
    (value + 3) & !3
}

/// find subslice を計算して返す。
///
/// 値を生成できない場合は `None` を返します。
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Cursor u8 を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn read_cursor_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8, PenExchangeError> {
    let mut byte = [0_u8; 1];
    cursor.read_exact(&mut byte)?;
    Ok(byte[0])
}

/// Cursor u16 be を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn read_cursor_u16_be(cursor: &mut Cursor<&[u8]>) -> Result<u16, PenExchangeError> {
    let mut bytes = [0_u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

/// Cursor i16 be を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn read_cursor_i16_be(cursor: &mut Cursor<&[u8]>) -> Result<i16, PenExchangeError> {
    let mut bytes = [0_u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(i16::from_be_bytes(bytes))
}

/// Cursor u32 be を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn read_cursor_u32_be(cursor: &mut Cursor<&[u8]>) -> Result<u32, PenExchangeError> {
    let mut bytes = [0_u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

/// Cursor i32 be を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn read_cursor_i32_be(cursor: &mut Cursor<&[u8]>) -> Result<i32, PenExchangeError> {
    let mut bytes = [0_u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(i32::from_be_bytes(bytes))
}

/// 現在の値を u32 be へ変換する。
///
/// 失敗時はエラーを返します。
fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, PenExchangeError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading u32".to_string())
    })?;
    Ok(u32::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

/// 現在の値を u16 be へ変換する。
///
/// 失敗時はエラーを返します。
fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16, PenExchangeError> {
    let slice = bytes.get(offset..offset + 2).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading u16".to_string())
    })?;
    Ok(u16::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

/// 現在の値を i32 be へ変換する。
///
/// 失敗時はエラーを返します。
fn read_i32_be(bytes: &[u8], offset: usize) -> Result<i32, PenExchangeError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading i32".to_string())
    })?;
    Ok(i32::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

/// 現在の値を photoshop unicode string へ変換する。
///
/// 失敗時はエラーを返します。
fn read_photoshop_unicode_string(cursor: &mut Cursor<&[u8]>) -> Result<String, PenExchangeError> {
    let char_count = read_cursor_u32_be(cursor)? as usize;
    let byte_len = char_count.checked_mul(2).ok_or_else(|| {
        PenExchangeError::InvalidData("unicode string length overflow".to_string())
    })?;
    let mut bytes = vec![0_u8; byte_len];
    cursor.read_exact(&mut bytes)?;
    let mut units = Vec::with_capacity(char_count);
    for chunk in bytes.chunks_exact(2) {
        units.push(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    let _ = read_cursor_u16_be(cursor).ok();
    Ok(String::from_utf16_lossy(&units))
}

/// U32 be を保存先へ書き出す。
fn write_u32_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use rusqlite::params;

    /// 現在の unique temp パス を返す。
    fn unique_temp_path(name: &str, extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "altpaint-{}-{}-{}.{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            extension
        ))
    }

    /// 現在の ワークスペース ペン パス を返す。
    fn workspace_pen_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }

    /// gbr round trip preserves 先端形状 dimensions が期待どおりに動作することを検証する。
    #[test]
    fn gbr_round_trip_preserves_tip_dimensions() {
        let pen = AltPaintPen {
            id: "gbr.roundtrip.1".to_string(),
            name: "Roundtrip".to_string(),
            base_size: 3.0,
            min_size: 1.0,
            max_size: 16.0,
            spacing_percent: 30.0,
            tip: Some(PenTip::from_alpha_mask(2, 2, &[255, 128, 64, 0])),
            ..AltPaintPen::default()
        };

        let bytes = export_gimp_gbr(&pen).expect("gbr exports");
        let parsed = parse_gimp_gbr_bytes(&bytes, "roundtrip.gbr").expect("gbr parses");

        assert_eq!(parsed.name, "Roundtrip");
        assert_eq!(parsed.spacing_percent, 30.0);
        assert_eq!(parsed.tip.as_ref().expect("tip").width(), 2);
        assert_eq!(parsed.tip.as_ref().expect("tip").height(), 2);
    }

    /// parses minimal abr v2 sampled ブラシ が期待どおりに動作することを検証する。
    #[test]
    fn parses_minimal_abr_v2_sampled_brush() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_u16.to_be_bytes());
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&2_u16.to_be_bytes());

        let mut brush = Vec::new();
        brush.extend_from_slice(&0_u32.to_be_bytes());
        brush.extend_from_slice(&55_u16.to_be_bytes());
        brush.extend_from_slice(&1_u32.to_be_bytes());
        brush.extend_from_slice(&(b'R' as u16).to_be_bytes());
        brush.extend_from_slice(&0_u16.to_be_bytes());
        brush.push(1);
        for _ in 0..4 {
            brush.extend_from_slice(&0_i16.to_be_bytes());
        }
        brush.extend_from_slice(&0_i32.to_be_bytes());
        brush.extend_from_slice(&0_i32.to_be_bytes());
        brush.extend_from_slice(&2_i32.to_be_bytes());
        brush.extend_from_slice(&2_i32.to_be_bytes());
        brush.extend_from_slice(&8_u16.to_be_bytes());
        brush.push(0);
        brush.extend_from_slice(&[0, 64, 128, 255]);

        bytes.extend_from_slice(&(brush.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&brush);

        let imported = parse_photoshop_abr_bytes(&bytes, "sample.abr").expect("abr parses");

        assert_eq!(imported.pens.len(), 1);
        let pen = &imported.pens[0];
        assert_eq!(pen.name, "R");
        assert_eq!(pen.spacing_percent, 55.0);
        assert_eq!(pen.tip.as_ref().expect("tip").width(), 2);
    }

    /// parses minimal abr v6 sampled ブラシ が期待どおりに動作することを検証する。
    #[test]
    fn parses_minimal_abr_v6_sampled_brush() {
        let mut sample = vec![0_u8; 47];
        sample.extend_from_slice(&0_i32.to_be_bytes());
        sample.extend_from_slice(&0_i32.to_be_bytes());
        sample.extend_from_slice(&2_i32.to_be_bytes());
        sample.extend_from_slice(&2_i32.to_be_bytes());
        sample.extend_from_slice(&8_u16.to_be_bytes());
        sample.push(0);
        sample.extend_from_slice(&[0, 64, 128, 255]);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&6_u16.to_be_bytes());
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(b"8BIMsamp");
        bytes.extend_from_slice(&(4_u32 + sample.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&(sample.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&sample);

        let imported = parse_photoshop_abr_bytes(&bytes, "sample-v6.abr").expect("abr v6 parses");

        assert_eq!(imported.pens.len(), 1);
        assert_eq!(imported.pens[0].spacing_percent, 25.0);
        assert!(
            imported.pens[0]
                .source
                .notes
                .iter()
                .any(|note| note.contains("defaulted to 25%"))
        );
    }

    /// parses minimal sut metadata from sqlite が期待どおりに動作することを検証する。
    #[test]
    fn parses_minimal_sut_metadata_from_sqlite() {
        let path = unique_temp_path("sut", "sut");
        let connection = Connection::open(&path).expect("sqlite open");
        connection
            .execute(
                "CREATE TABLE Node (NodeName TEXT, NodeVariantId INTEGER, NodeInitVariantId INTEGER)",
                [],
            )
            .expect("create node");
        connection
            .execute(
                "CREATE TABLE Variant (VariantID INTEGER PRIMARY KEY, BrushSize REAL, Spacing REAL, PressureGraph BLOB)",
                [],
            )
            .expect("create variant");
        connection
            .execute(
                "CREATE TABLE MaterialFile (FileName TEXT, FileData BLOB)",
                [],
            )
            .expect("create material");

        let pressure_blob = {
            let mut blob = Vec::new();
            blob.extend_from_slice(&1_u32.to_be_bytes());
            blob.extend_from_slice(&3_u32.to_be_bytes());
            blob.extend_from_slice(&8_u32.to_be_bytes());
            blob.extend_from_slice(&0_u32.to_be_bytes());
            blob.extend_from_slice(&0_u32.to_be_bytes());
            blob.extend_from_slice(&0_u32.to_be_bytes());
            blob.extend_from_slice(&0_u32.to_be_bytes());
            for value in [0.0_f64, 0.5_f64, 1.0_f64] {
                blob.extend_from_slice(&value.to_be_bytes());
            }
            blob
        };
        let png = vec![
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x72, 0xB6, 0x0D, 0x24, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE,
            0x42, 0x60, 0x82,
        ];

        connection
            .execute(
                "INSERT INTO Node (NodeName, NodeVariantId, NodeInitVariantId) VALUES (?1, ?2, ?3)",
                params!["Ink", 1_i64, Option::<i64>::None],
            )
            .expect("insert node");
        connection
            .execute(
                "INSERT INTO Variant (VariantID, BrushSize, Spacing, PressureGraph) VALUES (?1, ?2, ?3, ?4)",
                params![1_i64, 12.5_f64, 40.0_f64, pressure_blob],
            )
            .expect("insert variant");
        connection
            .execute(
                "INSERT INTO MaterialFile (FileName, FileData) VALUES (?1, ?2)",
                params!["tip.layer", png],
            )
            .expect("insert material");
        drop(connection);

        let imported = parse_clip_studio_sut(&path).expect("sut parses");
        let _ = fs::remove_file(&path);

        assert_eq!(imported.pens.len(), 1);
        let pen = &imported.pens[0];
        assert_eq!(pen.name, "Ink");
        assert_eq!(pen.base_size, 12.5);
        assert_eq!(pen.spacing_percent, 40.0);
        assert!(pen.dynamics.size_pressure_curve.is_some());
        assert!(
            pen.source
                .raw_fields
                .get("material_pngs")
                .is_some_and(|value| value.as_array().is_some_and(|items| !items.is_empty()))
        );
    }

    /// parses ワークスペース abr file が期待どおりに動作することを検証する。
    #[test]
    fn parses_workspace_abr_file() {
        let path = workspace_pen_path("pens/abr/manga.abr");
        if !path.exists() {
            return;
        }

        let bytes = fs::read(&path).expect("abr should read");
        let imported =
            parse_photoshop_abr_bytes(&bytes, "manga.abr").expect("workspace abr parses");

        assert!(
            !imported.pens.is_empty(),
            "expected at least one sampled brush from manga.abr"
        );
        assert!(
            imported
                .pens
                .iter()
                .all(|pen| pen.source.kind == PenSourceKind::PhotoshopAbr),
            "all imported pens should retain photoshop source metadata"
        );
    }

    /// parses ワークスペース sut file が期待どおりに動作することを検証する。
    #[test]
    fn parses_workspace_sut_file() {
        let path = workspace_pen_path("pens/sut/しげペン改[WEB用].sut");
        if !path.exists() {
            return;
        }

        let imported = parse_clip_studio_sut(&path).expect("workspace sut parses");

        assert!(
            !imported.pens.is_empty(),
            "expected at least one pen from workspace sut"
        );
        assert!(
            imported
                .pens
                .iter()
                .all(|pen| pen.source.kind == PenSourceKind::ClipStudioSut),
            "all imported pens should retain sut source metadata"
        );
    }
}
