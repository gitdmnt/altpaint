//! Photoshop ABR ブラシ形式のパース。

use std::io::{Cursor, Read};

use serde_json::{Map, json};

use crate::pen_format::{AltPaintPen, PenSource, PenSourceKind, PenTip};

use super::bytes::{
    align4, build_pen_id, find_subslice, path_stem, positive_dimension, read_cursor_i16_be,
    read_cursor_i32_be, read_cursor_u8, read_cursor_u16_be, read_cursor_u32_be, read_i32_be,
    read_photoshop_unicode_string, read_u16_be, read_u32_be,
};
use super::{
    ImportedPenSet, PenExchangeError, PenImportIssue, PenImportIssueSeverity, PenImportReport,
};

pub(super) fn parse_photoshop_abr_bytes(
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
