//! Clip Studio SUT ブラシ形式 (SQLite ベース) のパース。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Map, Value, json};

use crate::pen_format::{AltPaintPen, PenPressureCurve, PenPressurePoint, PenSource, PenSourceKind};

use super::bytes::{build_pen_id, quote_identifier, read_u32_be, sha256_hex};
use super::{
    ImportedPenSet, PenExchangeError, PenImportIssue, PenImportIssueSeverity, PenImportReport,
};

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

pub(super) fn parse_clip_studio_sut(
    path: impl AsRef<Path>,
) -> Result<ImportedPenSet, PenExchangeError> {
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

fn matches_any(name: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| name.contains(pattern))
}

fn normalize_ratio(value: f64) -> f32 {
    if value > 1.0 {
        (value / 100.0).clamp(0.0, 1.0) as f32
    } else {
        value.clamp(0.0, 1.0) as f32
    }
}

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

fn extract_png_blobs(blob: &[u8]) -> Vec<Vec<u8>> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
    const PNG_END: &[u8; 8] = b"\x00\x00\x00\x00IEND";

    let mut result = Vec::new();
    let mut search_start = 0_usize;
    while let Some(start) = super::bytes::find_subslice(&blob[search_start..], PNG_SIG) {
        let absolute_start = search_start + start;
        let after_sig = absolute_start + PNG_SIG.len();
        if let Some(end) = super::bytes::find_subslice(&blob[after_sig..], PNG_END) {
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

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = read_u32_be(bytes, 16).ok()?;
    let height = read_u32_be(bytes, 20).ok()?;
    Some((width, height))
}
