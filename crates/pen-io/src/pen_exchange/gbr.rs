//! GIMP GBR ブラシ形式の入出力。

use serde_json::{Map, json};

use crate::pen_format::{AltPaintPen, PenSource, PenSourceKind, PenTip};

use super::PenExchangeError;
use super::bytes::{build_pen_id, path_stem, read_u32_be, trim_trailing_nul, write_u32_be};

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

pub(super) fn parse_gimp_gbr_bytes(
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
