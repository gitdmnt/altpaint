use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use app_core::{
    Document, LayerMask, LayerNodeId, Page, PageId, Koma, KomaBounds, KomaId, RasterLayer, Work,
    WorkId, WorkspaceLayout,
};
use editor_state::{CanvasViewTransform, ColorRgba8, EditorSession, PenPreset, ToolKind};
use raster::{BlendMode, RgbaBitmap};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use app_core::{PanelConfigs, WorkspaceUiState};

use crate::project_file::{CURRENT_PROJECT_FORMAT_VERSION, LoadedProject, ProjectStoreError};

pub(crate) const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
pub const DEFAULT_PROJECT_CHUNK_SIZE: usize = 256;

const METADATA_FORMAT_VERSION: &str = "format_version";
const METADATA_DOCUMENT: &str = "document";
const METADATA_UI_STATE: &str = "ui_state";
const METADATA_SAVE_OPTIONS: &str = "save_options";

const CHUNK_ENCODING_SOLID: i64 = 0;
const CHUNK_ENCODING_ZSTD: i64 = 1;
const ZSTD_COMPRESSION_LEVEL: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSaveMode {
    #[default]
    Full,
    Delta,
}

impl ProjectSaveMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Delta => "delta",
        }
    }

    fn from_db(value: &str) -> Result<Self, ProjectStoreError> {
        match value {
            "full" => Ok(Self::Full),
            "delta" => Ok(Self::Delta),
            other => Err(ProjectStoreError::InvalidProject(format!(
                "unknown project save mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSaveOptions {
    pub chunk_size: usize,
    pub save_mode: ProjectSaveMode,
    pub persist_current_composites: bool,
}

impl Default for ProjectSaveOptions {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_PROJECT_CHUNK_SIZE,
            save_mode: ProjectSaveMode::Full,
            persist_current_composites: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedKomaCompositeSummary {
    pub composite_id: String,
    pub page_id: PageId,
    pub koma_id: KomaId,
    pub save_mode: ProjectSaveMode,
    pub chunk_size: usize,
}

#[derive(Debug, Clone)]
pub struct PersistedKomaComposite {
    pub summary: PersistedKomaCompositeSummary,
    pub bitmap: RgbaBitmap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectKomaSummary {
    pub id: KomaId,
    pub width: usize,
    pub height: usize,
    pub layer_count: usize,
    pub composite_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPageSummary {
    pub id: PageId,
    pub komas: Vec<ProjectKomaSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectManifest {
    pub format_version: u32,
    pub work_id: WorkId,
    pub title: String,
    pub save_mode: ProjectSaveMode,
    pub chunk_size: usize,
    pub pages: Vec<ProjectPageSummary>,
    pub workspace_layout: WorkspaceLayout,
    pub panel_configs: PanelConfigs,
    pub composites: Vec<PersistedKomaCompositeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SqliteDocumentRecord {
    work_id: u64,
    title: String,
    active_tool: ToolKind,
    active_color: ColorRgba8,
    pen_presets: Vec<PenPreset>,
    active_pen_preset_id: String,
    active_pen_size: u32,
    active_page_index: usize,
    active_koma_index: usize,
    view_transform: CanvasViewTransform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredKomaRecord {
    bounds: KomaBounds,
    active_layer_index: usize,
    created_layer_count: u64,
    composed_width: usize,
    composed_height: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredLayerRecord {
    id: LayerNodeId,
    name: String,
    visible: bool,
    blend_mode: BlendMode,
    width: usize,
    height: usize,
    mask: Option<LayerMask>,
}

#[derive(Debug, Clone)]
struct StoredChunk {
    chunk_x: usize,
    chunk_y: usize,
    width: usize,
    height: usize,
    encoding: i64,
    rgba: Option<[u8; 4]>,
    data: Option<Vec<u8>>,
}

pub(crate) fn file_has_sqlite_header(path: impl AsRef<Path>) -> Result<bool, ProjectStoreError> {
    let path = path.as_ref();
    let mut file = File::open(path)?;
    let mut header = [0u8; SQLITE_HEADER.len()];
    let read = file.read(&mut header)?;
    Ok(read == SQLITE_HEADER.len() && header == *SQLITE_HEADER)
}

pub(crate) fn save_project_to_sqlite_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    panel_configs: &std::collections::BTreeMap<String, Value>,
    options: ProjectSaveOptions,
) -> Result<(), ProjectStoreError> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_file(path)?;
    }

    let options = normalize_options(options);
    let mut connection = Connection::open(path)?;
    initialize_schema(&connection)?;
    let transaction = connection.transaction()?;

    put_metadata(
        &transaction,
        METADATA_FORMAT_VERSION,
        &CURRENT_PROJECT_FORMAT_VERSION,
    )?;
    put_metadata(
        &transaction,
        METADATA_DOCUMENT,
        &SqliteDocumentRecord {
            work_id: document.work.id.0,
            title: document.work.title.clone(),
            active_tool: document.session.active_tool(),
            active_color: document.session.active_color,
            pen_presets: document.session.pen_presets.clone(),
            active_pen_preset_id: document.session.active_pen_preset_id.clone(),
            active_pen_size: document.session.active_pen_size,
            active_page_index: document.active_page_index,
            active_koma_index: document.active_koma_index,
            view_transform: document.session.view_transform,
        },
    )?;
    put_metadata(
        &transaction,
        METADATA_UI_STATE,
        &WorkspaceUiState::new(workspace_layout.clone(), panel_configs.clone()),
    )?;
    put_metadata(&transaction, METADATA_SAVE_OPTIONS, &options)?;

    for (page_index, page) in document.work.pages.iter().enumerate() {
        transaction.execute(
            "INSERT INTO pages (page_id, page_index, width, height) VALUES (?1, ?2, ?3, ?4)",
            params![
                page.id.0 as i64,
                page_index as i64,
                page.width as i64,
                page.height as i64,
            ],
        )?;

        for (koma_index, koma) in page.komas.iter().enumerate() {
            let koma_record = StoredKomaRecord {
                bounds: koma.bounds,
                active_layer_index: koma.active_layer_index,
                created_layer_count: koma.created_layer_count,
                composed_width: koma.composite_cache.width,
                composed_height: koma.composite_cache.height,
            };
            transaction.execute(
				"INSERT INTO komas (page_id, koma_id, koma_index, metadata_json) VALUES (?1, ?2, ?3, ?4)",
				params![
					page.id.0 as i64,
					koma.id.0 as i64,
					koma_index as i64,
					encode_json(&koma_record)?
				],
			)?;

            for (layer_index, layer) in koma.layers.iter().enumerate() {
                let layer_record = StoredLayerRecord {
                    id: layer.id,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    blend_mode: layer.blend_mode.clone(),
                    width: layer.bitmap.width,
                    height: layer.bitmap.height,
                    mask: layer.mask.clone(),
                };
                transaction.execute(
                    "INSERT INTO layers (koma_id, layer_index, metadata_json) VALUES (?1, ?2, ?3)",
                    params![
                        koma.id.0 as i64,
                        layer_index as i64,
                        encode_json(&layer_record)?
                    ],
                )?;
                insert_layer_chunks(
                    &transaction,
                    koma.id,
                    layer_index,
                    &layer.bitmap,
                    options.chunk_size,
                )?;
            }

            if options.persist_current_composites {
                let composite_id = current_composite_id(page.id, koma.id);
                transaction.execute(
                    "INSERT INTO koma_composites (
						composite_id,
						page_id,
						koma_id,
						created_at_unix_ms,
						save_mode,
						width,
						height,
						chunk_size
					) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        composite_id,
                        page.id.0 as i64,
                        koma.id.0 as i64,
                        current_unix_ms()?,
                        options.save_mode.as_str(),
                        koma.composite_cache.width as i64,
                        koma.composite_cache.height as i64,
                        options.chunk_size as i64,
                    ],
                )?;
                insert_composite_chunks(
                    &transaction,
                    &current_composite_id(page.id, koma.id),
                    &koma.composite_cache,
                    options.chunk_size,
                )?;
            }
        }
    }

    transaction.commit()?;
    Ok(())
}

pub(crate) fn load_project_from_sqlite_path(
    path: impl AsRef<Path>,
) -> Result<LoadedProject, ProjectStoreError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    let document_record: SqliteDocumentRecord = get_metadata(&connection, METADATA_DOCUMENT)?;
    let ui_state: WorkspaceUiState = get_metadata(&connection, METADATA_UI_STATE)?;
    let pages = load_all_pages(&connection)?;

    let mut document = Document {
        work: Work {
            id: WorkId(document_record.work_id),
            title: document_record.title,
            pages,
        },
        active_page_index: document_record.active_page_index,
        active_koma_index: document_record.active_koma_index,
        session: EditorSession {
            // ツール選択は `active_tool` (kind) を種として復元する。
            // tool_catalog はランタイムで再ロードされるため空で開始し、
            // `normalize_after_load` が kind フォールバックで active_tool_id を補修する。
            active_tool_id: String::new(),
            active_child_tool_id: String::new(),
            active_color: document_record.active_color,
            tool_catalog: Vec::new(),
            pen_presets: document_record.pen_presets,
            active_pen_preset_id: document_record.active_pen_preset_id,
            active_pen_size: document_record.active_pen_size,
            view_transform: document_record.view_transform,
        },
    };
    document
        .session
        .ensure_tool_state(document_record.active_tool);
    document.normalize_after_load();

    Ok(LoadedProject { document, ui_state })
}

pub(crate) fn load_project_manifest_from_sqlite_path(
    path: impl AsRef<Path>,
) -> Result<ProjectManifest, ProjectStoreError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;

    let document_record: SqliteDocumentRecord = get_metadata(&connection, METADATA_DOCUMENT)?;
    let ui_state: WorkspaceUiState = get_metadata(&connection, METADATA_UI_STATE)?;
    let options: ProjectSaveOptions = get_metadata(&connection, METADATA_SAVE_OPTIONS)?;
    let composites = load_composite_summaries(&connection)?;

    let composite_ids_by_koma: HashMap<u64, Vec<String>> =
        composites.iter().fold(HashMap::new(), |mut map, composite| {
            map.entry(composite.koma_id.0)
                .or_default()
                .push(composite.composite_id.clone());
            map
        });

    let mut page_statement =
        connection.prepare("SELECT page_id FROM pages ORDER BY page_index ASC")?;
    let page_ids = page_statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut pages = Vec::with_capacity(page_ids.len());
    for raw_page_id in page_ids {
        let page_id = PageId(raw_page_id as u64);
        let mut koma_statement = connection.prepare(
			"SELECT koma_id, metadata_json FROM komas WHERE page_id = ?1 ORDER BY koma_index ASC",
		)?;
        let koma_rows = koma_statement
            .query_map([raw_page_id], |row| {
                let koma_id = row.get::<_, i64>(0)?;
                let metadata_json = row.get::<_, String>(1)?;
                Ok((koma_id, metadata_json))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut komas = Vec::with_capacity(koma_rows.len());
        for (raw_koma_id, metadata_json) in koma_rows {
            let koma_record: StoredKomaRecord = decode_json(&metadata_json)?;
            let layer_count = connection.query_row(
                "SELECT COUNT(*) FROM layers WHERE koma_id = ?1",
                [raw_koma_id],
                |row| row.get::<_, i64>(0),
            )?;
            komas.push(ProjectKomaSummary {
                id: KomaId(raw_koma_id as u64),
                width: koma_record.composed_width,
                height: koma_record.composed_height,
                layer_count: layer_count as usize,
                composite_ids: composite_ids_by_koma
                    .get(&(raw_koma_id as u64))
                    .cloned()
                    .unwrap_or_default(),
            });
        }

        pages.push(ProjectPageSummary {
            id: page_id,
            komas,
        });
    }

    Ok(ProjectManifest {
        format_version: CURRENT_PROJECT_FORMAT_VERSION,
        work_id: WorkId(document_record.work_id),
        title: document_record.title,
        save_mode: options.save_mode,
        chunk_size: options.chunk_size,
        pages,
        workspace_layout: ui_state.workspace_layout,
        panel_configs: ui_state.panel_configs,
        composites,
    })
}

pub(crate) fn load_page_from_sqlite_path(
    path: impl AsRef<Path>,
    page_id: PageId,
) -> Result<Page, ProjectStoreError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    load_page(&connection, page_id)
}

pub(crate) fn load_koma_composite_from_sqlite_path(
    path: impl AsRef<Path>,
    composite_id: &str,
) -> Result<Option<PersistedKomaComposite>, ProjectStoreError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    load_koma_composite(&connection, composite_id)
}

fn normalize_options(options: ProjectSaveOptions) -> ProjectSaveOptions {
    ProjectSaveOptions {
        chunk_size: options.chunk_size.max(1),
        ..options
    }
}

fn open_read_only(path: &Path) -> Result<Connection, ProjectStoreError> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

fn initialize_schema(connection: &Connection) -> Result<(), ProjectStoreError> {
    connection.execute_batch(
        "
		PRAGMA foreign_keys = OFF;

		CREATE TABLE metadata (
			key TEXT PRIMARY KEY,
			value_json TEXT NOT NULL
		);

		CREATE TABLE pages (
			page_id INTEGER PRIMARY KEY,
			page_index INTEGER NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL
		);

		CREATE TABLE komas (
			koma_id INTEGER PRIMARY KEY,
			page_id INTEGER NOT NULL,
			koma_index INTEGER NOT NULL,
			metadata_json TEXT NOT NULL
		);

		CREATE TABLE layers (
			koma_id INTEGER NOT NULL,
			layer_index INTEGER NOT NULL,
			metadata_json TEXT NOT NULL,
			PRIMARY KEY (koma_id, layer_index)
		);

		CREATE TABLE layer_chunks (
			koma_id INTEGER NOT NULL,
			layer_index INTEGER NOT NULL,
			chunk_x INTEGER NOT NULL,
			chunk_y INTEGER NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			encoding INTEGER NOT NULL,
			rgba INTEGER,
			data BLOB,
			PRIMARY KEY (koma_id, layer_index, chunk_x, chunk_y)
		);

		CREATE TABLE koma_composites (
			composite_id TEXT PRIMARY KEY,
			page_id INTEGER NOT NULL,
			koma_id INTEGER NOT NULL,
			created_at_unix_ms INTEGER NOT NULL,
			save_mode TEXT NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			chunk_size INTEGER NOT NULL
		);

		CREATE TABLE koma_composite_chunks (
			composite_id TEXT NOT NULL,
			chunk_x INTEGER NOT NULL,
			chunk_y INTEGER NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			encoding INTEGER NOT NULL,
			rgba INTEGER,
			data BLOB,
			PRIMARY KEY (composite_id, chunk_x, chunk_y)
		);

		CREATE INDEX idx_pages_order ON pages(page_index);
		CREATE INDEX idx_komas_page_order ON komas(page_id, koma_index);
		CREATE INDEX idx_layers_koma_order ON layers(koma_id, layer_index);
		CREATE INDEX idx_composites_koma ON koma_composites(koma_id);
		",
    )?;
    Ok(())
}

fn put_metadata<T: Serialize>(
    transaction: &Transaction<'_>,
    key: &str,
    value: &T,
) -> Result<(), ProjectStoreError> {
    transaction.execute(
        "INSERT INTO metadata (key, value_json) VALUES (?1, ?2)",
        params![key, encode_json(value)?],
    )?;
    Ok(())
}

fn get_metadata<T: DeserializeOwned>(
    connection: &Connection,
    key: &str,
) -> Result<T, ProjectStoreError> {
    let value = connection
        .query_row(
            "SELECT value_json FROM metadata WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| ProjectStoreError::InvalidProject(format!("missing metadata key: {key}")))?;
    decode_json(&value)
}

/// 保存された format_version が現行版と一致することを検証する。
///
/// 旧版の受理は行わない (alpha 方針で互換を持たない)。
fn validate_format_version(connection: &Connection) -> Result<(), ProjectStoreError> {
    let format_version: u32 = get_metadata(connection, METADATA_FORMAT_VERSION)?;
    if format_version != CURRENT_PROJECT_FORMAT_VERSION {
        return Err(ProjectStoreError::UnsupportedFormatVersion(format_version));
    }
    Ok(())
}

fn encode_json<T: Serialize>(value: &T) -> Result<String, ProjectStoreError> {
    serde_json::to_string(value).map_err(ProjectStoreError::SerializeMetadataJson)
}

fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T, ProjectStoreError> {
    serde_json::from_str(value).map_err(ProjectStoreError::DeserializeMetadataJson)
}

fn insert_layer_chunks(
    transaction: &Transaction<'_>,
    koma_id: KomaId,
    layer_index: usize,
    bitmap: &RgbaBitmap,
    chunk_size: usize,
) -> Result<(), ProjectStoreError> {
    for chunk in chunk_bitmap(bitmap, chunk_size)? {
        transaction.execute(
            "INSERT INTO layer_chunks (
				koma_id,
				layer_index,
				chunk_x,
				chunk_y,
				width,
				height,
				encoding,
				rgba,
				data
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                koma_id.0 as i64,
                layer_index as i64,
                chunk.chunk_x as i64,
                chunk.chunk_y as i64,
                chunk.width as i64,
                chunk.height as i64,
                chunk.encoding,
                chunk.rgba.map(encode_rgba),
                chunk.data,
            ],
        )?;
    }
    Ok(())
}

fn insert_composite_chunks(
    transaction: &Transaction<'_>,
    composite_id: &str,
    bitmap: &RgbaBitmap,
    chunk_size: usize,
) -> Result<(), ProjectStoreError> {
    for chunk in chunk_bitmap(bitmap, chunk_size)? {
        transaction.execute(
            "INSERT INTO koma_composite_chunks (
				composite_id,
				chunk_x,
				chunk_y,
				width,
				height,
				encoding,
				rgba,
				data
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                composite_id,
                chunk.chunk_x as i64,
                chunk.chunk_y as i64,
                chunk.width as i64,
                chunk.height as i64,
                chunk.encoding,
                chunk.rgba.map(encode_rgba),
                chunk.data,
            ],
        )?;
    }
    Ok(())
}

fn chunk_bitmap(
    bitmap: &RgbaBitmap,
    chunk_size: usize,
) -> Result<Vec<StoredChunk>, ProjectStoreError> {
    let mut chunks = Vec::new();
    let chunk_size = chunk_size.max(1);
    for chunk_y in (0..bitmap.height).step_by(chunk_size) {
        for chunk_x in (0..bitmap.width).step_by(chunk_size) {
            let width = (bitmap.width - chunk_x).min(chunk_size);
            let height = (bitmap.height - chunk_y).min(chunk_size);
            let pixels = extract_chunk_pixels(bitmap, chunk_x, chunk_y, width, height);
            if let Some(rgba) = solid_rgba(&pixels) {
                chunks.push(StoredChunk {
                    chunk_x,
                    chunk_y,
                    width,
                    height,
                    encoding: CHUNK_ENCODING_SOLID,
                    rgba: Some(rgba),
                    data: None,
                });
            } else {
                let compressed =
                    zstd::stream::encode_all(Cursor::new(pixels), ZSTD_COMPRESSION_LEVEL)
                        .map_err(ProjectStoreError::Compress)?;
                chunks.push(StoredChunk {
                    chunk_x,
                    chunk_y,
                    width,
                    height,
                    encoding: CHUNK_ENCODING_ZSTD,
                    rgba: None,
                    data: Some(compressed),
                });
            }
        }
    }
    Ok(chunks)
}

fn extract_chunk_pixels(
    bitmap: &RgbaBitmap,
    chunk_x: usize,
    chunk_y: usize,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width.saturating_mul(height).saturating_mul(4));
    for y in chunk_y..chunk_y + height {
        let row_start = (y * bitmap.width + chunk_x) * 4;
        let row_end = row_start + width * 4;
        pixels.extend_from_slice(&bitmap.pixels[row_start..row_end]);
    }
    pixels
}

fn solid_rgba(pixels: &[u8]) -> Option<[u8; 4]> {
    let first = pixels.get(0..4)?;
    if pixels.chunks_exact(4).all(|chunk| chunk == first) {
        Some([first[0], first[1], first[2], first[3]])
    } else {
        None
    }
}

fn encode_rgba(rgba: [u8; 4]) -> i64 {
    (((rgba[0] as u32) << 24)
        | ((rgba[1] as u32) << 16)
        | ((rgba[2] as u32) << 8)
        | (rgba[3] as u32)) as i64
}

fn decode_rgba(value: i64) -> [u8; 4] {
    let value = value as u32;
    [
        ((value >> 24) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]
}

fn load_all_pages(connection: &Connection) -> Result<Vec<Page>, ProjectStoreError> {
    let mut statement = connection.prepare("SELECT page_id FROM pages ORDER BY page_index ASC")?;
    let page_ids = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    page_ids
        .into_iter()
        .map(|page_id| load_page(connection, PageId(page_id as u64)))
        .collect()
}

fn load_page(connection: &Connection, page_id: PageId) -> Result<Page, ProjectStoreError> {
    let page_dimensions = connection
        .query_row(
            "SELECT width, height FROM pages WHERE page_id = ?1",
            [page_id.0 as i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((width, height)) = page_dimensions else {
        return Err(ProjectStoreError::PageNotFound(page_id.0));
    };

    let mut koma_statement = connection
        .prepare("SELECT koma_id FROM komas WHERE page_id = ?1 ORDER BY koma_index ASC")?;
    let koma_ids = koma_statement
        .query_map([page_id.0 as i64], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let komas = koma_ids
        .into_iter()
        .map(|koma_id| load_koma(connection, page_id, KomaId(koma_id as u64)))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Page {
        id: page_id,
        width: width as usize,
        height: height as usize,
        komas,
    })
}

fn load_koma(
    connection: &Connection,
    page_id: PageId,
    koma_id: KomaId,
) -> Result<Koma, ProjectStoreError> {
    let metadata_json = connection
        .query_row(
            "SELECT metadata_json FROM komas WHERE page_id = ?1 AND koma_id = ?2",
            params![page_id.0 as i64, koma_id.0 as i64],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(ProjectStoreError::KomaNotFound {
            page_id: page_id.0,
            koma_id: koma_id.0,
        })?;
    let koma_record: StoredKomaRecord = decode_json(&metadata_json)?;

    let mut layer_statement = connection.prepare(
		"SELECT layer_index, metadata_json FROM layers WHERE koma_id = ?1 ORDER BY layer_index ASC",
	)?;
    let layer_rows = layer_statement
        .query_map([koma_id.0 as i64], |row| {
            let layer_index = row.get::<_, i64>(0)?;
            let metadata_json = row.get::<_, String>(1)?;
            Ok((layer_index, metadata_json))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut layers = Vec::with_capacity(layer_rows.len());
    for (layer_index, metadata_json) in layer_rows {
        let layer_record: StoredLayerRecord = decode_json(&metadata_json)?;
        let bitmap = load_layer_bitmap(
            connection,
            koma_id,
            layer_index as usize,
            layer_record.width,
            layer_record.height,
        )?;
        layers.push(RasterLayer {
            id: layer_record.id,
            name: layer_record.name,
            visible: layer_record.visible,
            blend_mode: layer_record.blend_mode,
            bitmap,
            mask: layer_record.mask,
        });
    }

    let composite_cache =
        match load_koma_composite(connection, &current_composite_id(page_id, koma_id))? {
            Some(composite) => composite.bitmap,
            None => app_core::blend::composite_layers(
                koma_record.composed_width.max(1),
                koma_record.composed_height.max(1),
                &layers,
            ),
        };

    Ok(Koma {
        id: koma_id,
        bounds: koma_record.bounds,
        composite_cache,
        layers,
        active_layer_index: koma_record.active_layer_index,
        created_layer_count: koma_record.created_layer_count,
    })
}

fn load_layer_bitmap(
    connection: &Connection,
    koma_id: KomaId,
    layer_index: usize,
    width: usize,
    height: usize,
) -> Result<RgbaBitmap, ProjectStoreError> {
    let mut bitmap = RgbaBitmap::transparent(width.max(1), height.max(1));
    let mut statement = connection.prepare(
        "SELECT chunk_x, chunk_y, width, height, encoding, rgba, data
		 FROM layer_chunks
		 WHERE koma_id = ?1 AND layer_index = ?2
		 ORDER BY chunk_y ASC, chunk_x ASC",
    )?;
    let chunks = statement
        .query_map(params![koma_id.0 as i64, layer_index as i64], |row| {
            Ok(StoredChunk {
                chunk_x: row.get::<_, i64>(0)? as usize,
                chunk_y: row.get::<_, i64>(1)? as usize,
                width: row.get::<_, i64>(2)? as usize,
                height: row.get::<_, i64>(3)? as usize,
                encoding: row.get(4)?,
                rgba: row.get::<_, Option<i64>>(5)?.map(decode_rgba),
                data: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    apply_chunks(&mut bitmap, &chunks)?;
    Ok(bitmap)
}

fn load_koma_composite(
    connection: &Connection,
    composite_id: &str,
) -> Result<Option<PersistedKomaComposite>, ProjectStoreError> {
    let row = connection
        .query_row(
            "SELECT page_id, koma_id, save_mode, width, height, chunk_size
			 FROM koma_composites
			 WHERE composite_id = ?1",
            [composite_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;

    let Some((raw_page_id, raw_koma_id, save_mode, width, height, chunk_size)) = row else {
        return Ok(None);
    };

    let mut bitmap = RgbaBitmap::transparent(width as usize, height as usize);
    let mut statement = connection.prepare(
        "SELECT chunk_x, chunk_y, width, height, encoding, rgba, data
		 FROM koma_composite_chunks
		 WHERE composite_id = ?1
		 ORDER BY chunk_y ASC, chunk_x ASC",
    )?;
    let chunks = statement
        .query_map([composite_id], |row| {
            Ok(StoredChunk {
                chunk_x: row.get::<_, i64>(0)? as usize,
                chunk_y: row.get::<_, i64>(1)? as usize,
                width: row.get::<_, i64>(2)? as usize,
                height: row.get::<_, i64>(3)? as usize,
                encoding: row.get(4)?,
                rgba: row.get::<_, Option<i64>>(5)?.map(decode_rgba),
                data: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    apply_chunks(&mut bitmap, &chunks)?;

    Ok(Some(PersistedKomaComposite {
        summary: PersistedKomaCompositeSummary {
            composite_id: composite_id.to_string(),
            page_id: PageId(raw_page_id as u64),
            koma_id: KomaId(raw_koma_id as u64),
            save_mode: ProjectSaveMode::from_db(&save_mode)?,
            chunk_size: chunk_size as usize,
        },
        bitmap,
    }))
}

fn load_composite_summaries(
    connection: &Connection,
) -> Result<Vec<PersistedKomaCompositeSummary>, ProjectStoreError> {
    let mut statement = connection.prepare(
        "SELECT composite_id, page_id, koma_id, save_mode, chunk_size
		 FROM koma_composites
		 ORDER BY page_id ASC, koma_id ASC, composite_id ASC",
    )?;
    statement
        .query_map([], |row| {
            let save_mode = row.get::<_, String>(3)?;
            Ok(PersistedKomaCompositeSummary {
                composite_id: row.get(0)?,
                page_id: PageId(row.get::<_, i64>(1)? as u64),
                koma_id: KomaId(row.get::<_, i64>(2)? as u64),
                save_mode: ProjectSaveMode::from_db(&save_mode)
                    .map_err(to_sqlite_conversion_error)?,
                chunk_size: row.get::<_, i64>(4)? as usize,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ProjectStoreError::Sqlite)
}

fn apply_chunks(bitmap: &mut RgbaBitmap, chunks: &[StoredChunk]) -> Result<(), ProjectStoreError> {
    for chunk in chunks {
        match chunk.encoding {
            CHUNK_ENCODING_SOLID => fill_chunk(
                bitmap,
                chunk.chunk_x,
                chunk.chunk_y,
                chunk.width,
                chunk.height,
                chunk.rgba.ok_or_else(|| {
                    ProjectStoreError::InvalidProject("solid chunk is missing rgba payload".to_string())
                })?,
            ),
            CHUNK_ENCODING_ZSTD => {
                let decoded = zstd::stream::decode_all(Cursor::new(
                    chunk.data.as_deref().ok_or_else(|| {
                        ProjectStoreError::InvalidProject(
                            "compressed chunk is missing payload".to_string(),
                        )
                    })?,
                ))
                .map_err(ProjectStoreError::Decompress)?;
                blit_chunk(
                    bitmap,
                    chunk.chunk_x,
                    chunk.chunk_y,
                    chunk.width,
                    chunk.height,
                    &decoded,
                )?;
            }
            encoding => {
                return Err(ProjectStoreError::InvalidProject(format!(
                    "unknown chunk encoding: {encoding}"
                )));
            }
        }
    }
    Ok(())
}

fn fill_chunk(
    bitmap: &mut RgbaBitmap,
    chunk_x: usize,
    chunk_y: usize,
    width: usize,
    height: usize,
    rgba: [u8; 4],
) {
    for y in chunk_y..chunk_y + height {
        for x in chunk_x..chunk_x + width {
            let index = (y * bitmap.width + x) * 4;
            bitmap.pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }
}

fn blit_chunk(
    bitmap: &mut RgbaBitmap,
    chunk_x: usize,
    chunk_y: usize,
    width: usize,
    height: usize,
    pixels: &[u8],
) -> Result<(), ProjectStoreError> {
    let expected_len = width.saturating_mul(height).saturating_mul(4);
    if pixels.len() != expected_len {
        return Err(ProjectStoreError::InvalidProject(format!(
            "chunk payload length mismatch: expected {expected_len}, got {}",
            pixels.len()
        )));
    }
    for row in 0..height {
        let src_start = row * width * 4;
        let src_end = src_start + width * 4;
        let dst_start = ((chunk_y + row) * bitmap.width + chunk_x) * 4;
        let dst_end = dst_start + width * 4;
        bitmap.pixels[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_end]);
    }
    Ok(())
}

fn current_composite_id(page_id: PageId, koma_id: KomaId) -> String {
    format!("page:{}:koma:{}:current", page_id.0, koma_id.0)
}

fn current_unix_ms() -> Result<i64, ProjectStoreError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ProjectStoreError::InvalidProject(format!("system clock error: {error}")))?
        .as_millis() as i64)
}

fn to_sqlite_conversion_error(error: ProjectStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
