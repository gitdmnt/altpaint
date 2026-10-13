//! ペン形式 (.altp-pen / ABR / SUT / GBR) の相互変換エントリポイント。
//!
//! 形式別の実装は `abr` / `sut` / `gbr` サブモジュールに分割し、
//! バイト読み書きの低レベルユーティリティは `bytes` に集約する。

mod abr;
mod bytes;
mod gbr;
mod sut;

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pen_format::{AltPaintPen, PenSourceKind, parse_altpaint_pen_json};

pub use gbr::export_gimp_gbr;

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
            abr::parse_photoshop_abr_bytes(
                &bytes,
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("brush.abr"),
            )
        }
        PenFileKind::ClipStudioSut => sut::parse_clip_studio_sut(path),
        PenFileKind::GimpGbr => {
            let bytes = fs::read(path)?;
            let pen = gbr::parse_gimp_gbr_bytes(
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

pub fn export_altpaint_pen_json(pen: &AltPaintPen) -> Result<String, PenExchangeError> {
    pen.validate()?;
    serde_json::to_string_pretty(pen).map_err(Into::into)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pen_format::PenTip;
    use rusqlite::{Connection, params};
    use std::fs;
    use std::path::PathBuf;

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

    fn workspace_pen_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }

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
        let parsed = gbr::parse_gimp_gbr_bytes(&bytes, "roundtrip.gbr").expect("gbr parses");

        assert_eq!(parsed.name, "Roundtrip");
        assert_eq!(parsed.spacing_percent, 30.0);
        assert_eq!(parsed.tip.as_ref().expect("tip").width(), 2);
        assert_eq!(parsed.tip.as_ref().expect("tip").height(), 2);
    }

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

        let imported = parse_pen_abr(&bytes, "sample.abr").expect("abr parses");

        assert_eq!(imported.pens.len(), 1);
        let pen = &imported.pens[0];
        assert_eq!(pen.name, "R");
        assert_eq!(pen.spacing_percent, 55.0);
        assert_eq!(pen.tip.as_ref().expect("tip").width(), 2);
    }

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

        let imported = parse_pen_abr(&bytes, "sample-v6.abr").expect("abr v6 parses");

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

        let imported = parse_pen_file(&path).expect("sut parses");
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

    #[test]
    fn parses_workspace_abr_file() {
        let path = workspace_pen_path("pens/abr/manga.abr");
        if !path.exists() {
            return;
        }

        let bytes = fs::read(&path).expect("abr should read");
        let imported = parse_pen_abr(&bytes, "manga.abr").expect("workspace abr parses");

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

    #[test]
    fn parses_workspace_sut_file() {
        let path = workspace_pen_path("pens/sut/しげペン改[WEB用].sut");
        if !path.exists() {
            return;
        }

        let imported = parse_pen_file(&path).expect("workspace sut parses");

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

    /// テスト専用: ABR バイト列を直接パースする (旧 `parse_photoshop_abr_bytes` 相当)。
    fn parse_pen_abr(bytes: &[u8], file_name: &str) -> Result<ImportedPenSet, PenExchangeError> {
        super::abr::parse_photoshop_abr_bytes(bytes, file_name)
    }
}
