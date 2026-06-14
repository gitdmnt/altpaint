//! バイト列の読み書きと、形式パーサ間で共有する小さなユーティリティ。

use std::io::{Cursor, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use super::PenExchangeError;

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn build_pen_id(prefix: &str, file_name: &str, index: usize) -> String {
    format!(
        "{}.{}.{}",
        prefix,
        sanitize_identifier(path_stem(file_name)),
        index
    )
}

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

pub(super) fn path_stem(file_name: &str) -> &str {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(file_name)
}

pub(super) fn trim_trailing_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|value| *value == 0) {
        Some(index) => &bytes[..index],
        None => bytes,
    }
}

pub(super) fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(super) fn positive_dimension(value: i32, label: &str) -> Result<u32, PenExchangeError> {
    if value <= 0 {
        return Err(PenExchangeError::InvalidData(format!(
            "{label} must be positive, got {value}"
        )));
    }
    Ok(value as u32)
}

pub(super) fn align4(value: usize) -> usize {
    (value + 3) & !3
}

pub(super) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn read_cursor_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8, PenExchangeError> {
    let mut byte = [0_u8; 1];
    cursor.read_exact(&mut byte)?;
    Ok(byte[0])
}

pub(super) fn read_cursor_u16_be(cursor: &mut Cursor<&[u8]>) -> Result<u16, PenExchangeError> {
    let mut bytes = [0_u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

pub(super) fn read_cursor_i16_be(cursor: &mut Cursor<&[u8]>) -> Result<i16, PenExchangeError> {
    let mut bytes = [0_u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(i16::from_be_bytes(bytes))
}

pub(super) fn read_cursor_u32_be(cursor: &mut Cursor<&[u8]>) -> Result<u32, PenExchangeError> {
    let mut bytes = [0_u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

pub(super) fn read_cursor_i32_be(cursor: &mut Cursor<&[u8]>) -> Result<i32, PenExchangeError> {
    let mut bytes = [0_u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(i32::from_be_bytes(bytes))
}

pub(super) fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, PenExchangeError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading u32".to_string())
    })?;
    Ok(u32::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

pub(super) fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16, PenExchangeError> {
    let slice = bytes.get(offset..offset + 2).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading u16".to_string())
    })?;
    Ok(u16::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

pub(super) fn read_i32_be(bytes: &[u8], offset: usize) -> Result<i32, PenExchangeError> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| {
        PenExchangeError::InvalidData("unexpected end of data while reading i32".to_string())
    })?;
    Ok(i32::from_be_bytes(
        slice.try_into().expect("slice length checked"),
    ))
}

pub(super) fn read_photoshop_unicode_string(
    cursor: &mut Cursor<&[u8]>,
) -> Result<String, PenExchangeError> {
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

pub(super) fn write_u32_be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
