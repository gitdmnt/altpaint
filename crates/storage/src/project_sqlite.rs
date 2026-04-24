use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use app_core::{
    BlendMode, CanvasBitmap, CanvasDirtyRect, CanvasViewTransform, ClampToCanvasBounds, ColorRgba8,
    Document, LayerMask, LayerNode, LayerNodeId, Page, PageId, Panel, PanelBounds, PanelId,
    PenPreset, RasterLayer, ToolKind, Work, WorkId, WorkspaceLayout,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use workspace_persistence::{PluginConfigs, WorkspaceUiState};

use crate::project_file::{CURRENT_FORMAT_VERSION, LoadedProject, StorageError};

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
    /// 既定値を使って新しいインスタンスを生成する。
    fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Delta => "delta",
        }
    }

    /// 保存済み文字列を列挙値へ復元する。
    ///
    /// 失敗時はエラーを返します。
    fn from_db(value: &str) -> Result<Self, StorageError> {
        match value {
            "full" => Ok(Self::Full),
            "delta" => Ok(Self::Delta),
            other => Err(StorageError::InvalidProject(format!(
                "unknown project save mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSaveOptions {
    pub chunk_size: usize,
    pub save_mode: ProjectSaveMode,
    pub persist_current_snapshots: bool,
}

impl Default for ProjectSaveOptions {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_PROJECT_CHUNK_SIZE,
            save_mode: ProjectSaveMode::Full,
            persist_current_snapshots: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedPanelSnapshotSummary {
    pub snapshot_id: String,
    pub page_id: PageId,
    pub panel_id: PanelId,
    pub save_mode: ProjectSaveMode,
    pub chunk_size: usize,
}

#[derive(Debug, Clone)]
pub struct PersistedPanelSnapshot {
    pub summary: PersistedPanelSnapshotSummary,
    pub bitmap: CanvasBitmap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPanelSummary {
    pub id: PanelId,
    pub width: usize,
    pub height: usize,
    pub layer_count: usize,
    pub snapshot_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPageSummary {
    pub id: PageId,
    pub panels: Vec<ProjectPanelSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectIndex {
    pub format_version: u32,
    pub work_id: WorkId,
    pub title: String,
    pub save_mode: ProjectSaveMode,
    pub chunk_size: usize,
    pub pages: Vec<ProjectPageSummary>,
    pub workspace_layout: WorkspaceLayout,
    pub plugin_configs: PluginConfigs,
    pub snapshots: Vec<PersistedPanelSnapshotSummary>,
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
    active_panel_index: usize,
    view_transform: CanvasViewTransform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPanelRecord {
    bounds: PanelBounds,
    root_layer: LayerNode,
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

/// Is sqlite プロジェクト パス かどうかを返す。
///
/// 失敗時はエラーを返します。
pub(crate) fn is_sqlite_project_path(path: impl AsRef<Path>) -> Result<bool, StorageError> {
    let path = path.as_ref();
    let mut file = File::open(path)?;
    let mut header = [0u8; SQLITE_HEADER.len()];
    let read = file.read(&mut header)?;
    Ok(read == SQLITE_HEADER.len() && header == *SQLITE_HEADER)
}

/// プロジェクト to sqlite パス を保存先へ書き出す。
pub(crate) fn save_project_to_sqlite_path(
    path: impl AsRef<Path>,
    document: &Document,
    workspace_layout: &WorkspaceLayout,
    plugin_configs: &std::collections::BTreeMap<String, Value>,
    options: ProjectSaveOptions,
) -> Result<(), StorageError> {
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
        &CURRENT_FORMAT_VERSION,
    )?;
    put_metadata(
        &transaction,
        METADATA_DOCUMENT,
        &SqliteDocumentRecord {
            work_id: document.work.id.0,
            title: document.work.title.clone(),
            active_tool: document.active_tool,
            active_color: document.active_color,
            pen_presets: document.pen_presets.clone(),
            active_pen_preset_id: document.active_pen_preset_id.clone(),
            active_pen_size: document.active_pen_size,
            active_page_index: document.active_page_index,
            active_panel_index: document.active_panel_index,
            view_transform: document.view_transform,
        },
    )?;
    put_metadata(
        &transaction,
        METADATA_UI_STATE,
        &WorkspaceUiState::new(workspace_layout.clone(), plugin_configs.clone()),
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

        for (panel_index, panel) in page.panels.iter().enumerate() {
            let panel_record = StoredPanelRecord {
                bounds: panel.bounds,
                root_layer: panel.root_layer.clone(),
                active_layer_index: panel.active_layer_index,
                created_layer_count: panel.created_layer_count,
                composed_width: panel.bitmap.width,
                composed_height: panel.bitmap.height,
            };
            transaction.execute(
				"INSERT INTO panels (page_id, panel_id, panel_index, metadata_json) VALUES (?1, ?2, ?3, ?4)",
				params![
					page.id.0 as i64,
					panel.id.0 as i64,
					panel_index as i64,
					encode_json(&panel_record)?
				],
			)?;

            for (layer_index, layer) in panel.layers.iter().enumerate() {
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
                    "INSERT INTO layers (panel_id, layer_index, metadata_json) VALUES (?1, ?2, ?3)",
                    params![
                        panel.id.0 as i64,
                        layer_index as i64,
                        encode_json(&layer_record)?
                    ],
                )?;
                insert_layer_chunks(
                    &transaction,
                    panel.id,
                    layer_index,
                    &layer.bitmap,
                    options.chunk_size,
                )?;
            }

            if options.persist_current_snapshots {
                let snapshot_id = current_snapshot_id(page.id, panel.id);
                transaction.execute(
                    "INSERT INTO panel_snapshots (
						snapshot_id,
						page_id,
						panel_id,
						created_at_unix_ms,
						save_mode,
						width,
						height,
						chunk_size
					) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        snapshot_id,
                        page.id.0 as i64,
                        panel.id.0 as i64,
                        current_unix_ms()?,
                        options.save_mode.as_str(),
                        panel.bitmap.width as i64,
                        panel.bitmap.height as i64,
                        options.chunk_size as i64,
                    ],
                )?;
                insert_snapshot_chunks(
                    &transaction,
                    &current_snapshot_id(page.id, panel.id),
                    &panel.bitmap,
                    options.chunk_size,
                )?;
            }
        }
    }

    transaction.commit()?;
    Ok(())
}

/// プロジェクト from sqlite パス を読み込み、必要に応じて整形して返す。
pub(crate) fn load_project_from_sqlite_path(
    path: impl AsRef<Path>,
) -> Result<LoadedProject, StorageError> {
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
        active_tool: document_record.active_tool,
        active_tool_id: String::new(),
        active_color: document_record.active_color,
        tool_catalog: Vec::new(),
        pen_presets: document_record.pen_presets,
        active_pen_preset_id: document_record.active_pen_preset_id,
        active_pen_size: document_record.active_pen_size,
        active_page_index: document_record.active_page_index,
        active_panel_index: document_record.active_panel_index,
        view_transform: document_record.view_transform,
        active_child_tool_id: String::new(),
    };
    document.normalize_phase9_state();

    Ok(LoadedProject { document, ui_state })
}

/// プロジェクト インデックス from sqlite パス を読み込み、必要に応じて整形して返す。
pub(crate) fn load_project_index_from_sqlite_path(
    path: impl AsRef<Path>,
) -> Result<ProjectIndex, StorageError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;

    let document_record: SqliteDocumentRecord = get_metadata(&connection, METADATA_DOCUMENT)?;
    let ui_state: WorkspaceUiState = get_metadata(&connection, METADATA_UI_STATE)?;
    let options: ProjectSaveOptions = get_metadata(&connection, METADATA_SAVE_OPTIONS)?;
    let snapshots = load_snapshot_summaries(&connection)?;

    let snapshot_ids_by_panel: HashMap<u64, Vec<String>> =
        snapshots.iter().fold(HashMap::new(), |mut map, snapshot| {
            map.entry(snapshot.panel_id.0)
                .or_default()
                .push(snapshot.snapshot_id.clone());
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
        let mut panel_statement = connection.prepare(
			"SELECT panel_id, metadata_json FROM panels WHERE page_id = ?1 ORDER BY panel_index ASC",
		)?;
        let panel_rows = panel_statement
            .query_map([raw_page_id], |row| {
                let panel_id = row.get::<_, i64>(0)?;
                let metadata_json = row.get::<_, String>(1)?;
                Ok((panel_id, metadata_json))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut panels = Vec::with_capacity(panel_rows.len());
        for (raw_panel_id, metadata_json) in panel_rows {
            let panel_record: StoredPanelRecord = decode_json(&metadata_json)?;
            let layer_count = connection.query_row(
                "SELECT COUNT(*) FROM layers WHERE panel_id = ?1",
                [raw_panel_id],
                |row| row.get::<_, i64>(0),
            )?;
            panels.push(ProjectPanelSummary {
                id: PanelId(raw_panel_id as u64),
                width: panel_record.composed_width,
                height: panel_record.composed_height,
                layer_count: layer_count as usize,
                snapshot_ids: snapshot_ids_by_panel
                    .get(&(raw_panel_id as u64))
                    .cloned()
                    .unwrap_or_default(),
            });
        }

        pages.push(ProjectPageSummary {
            id: page_id,
            panels,
        });
    }

    Ok(ProjectIndex {
        format_version: CURRENT_FORMAT_VERSION,
        work_id: WorkId(document_record.work_id),
        title: document_record.title,
        save_mode: options.save_mode,
        chunk_size: options.chunk_size,
        pages,
        workspace_layout: ui_state.workspace_layout,
        plugin_configs: ui_state.plugin_configs,
        snapshots,
    })
}

/// ページ from sqlite パス を読み込み、必要に応じて整形して返す。
pub(crate) fn load_page_from_sqlite_path(
    path: impl AsRef<Path>,
    page_id: PageId,
) -> Result<Page, StorageError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    load_page(&connection, page_id)
}

/// パネル from sqlite パス を読み込み、必要に応じて整形して返す。
pub(crate) fn load_panel_from_sqlite_path(
    path: impl AsRef<Path>,
    page_id: PageId,
    panel_id: PanelId,
) -> Result<Panel, StorageError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    load_panel(&connection, page_id, panel_id)
}

/// パネル スナップショット from sqlite パス を読み込み、必要に応じて整形して返す。
pub(crate) fn load_panel_snapshot_from_sqlite_path(
    path: impl AsRef<Path>,
    snapshot_id: &str,
) -> Result<Option<PersistedPanelSnapshot>, StorageError> {
    let connection = open_read_only(path.as_ref())?;
    validate_format_version(&connection)?;
    load_panel_snapshot(&connection, snapshot_id)
}

/// normalize オプション を計算して返す。
fn normalize_options(options: ProjectSaveOptions) -> ProjectSaveOptions {
    ProjectSaveOptions {
        chunk_size: options.chunk_size.max(1),
        ..options
    }
}

/// Read only を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn open_read_only(path: &Path) -> Result<Connection, StorageError> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

/// Initialize schema に対応するビットマップ処理を行う。
fn initialize_schema(connection: &Connection) -> Result<(), StorageError> {
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

		CREATE TABLE panels (
			panel_id INTEGER PRIMARY KEY,
			page_id INTEGER NOT NULL,
			panel_index INTEGER NOT NULL,
			metadata_json TEXT NOT NULL
		);

		CREATE TABLE layers (
			panel_id INTEGER NOT NULL,
			layer_index INTEGER NOT NULL,
			metadata_json TEXT NOT NULL,
			PRIMARY KEY (panel_id, layer_index)
		);

		CREATE TABLE layer_chunks (
			panel_id INTEGER NOT NULL,
			layer_index INTEGER NOT NULL,
			chunk_x INTEGER NOT NULL,
			chunk_y INTEGER NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			encoding INTEGER NOT NULL,
			rgba INTEGER,
			data BLOB,
			PRIMARY KEY (panel_id, layer_index, chunk_x, chunk_y)
		);

		CREATE TABLE panel_snapshots (
			snapshot_id TEXT PRIMARY KEY,
			page_id INTEGER NOT NULL,
			panel_id INTEGER NOT NULL,
			created_at_unix_ms INTEGER NOT NULL,
			save_mode TEXT NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			chunk_size INTEGER NOT NULL
		);

		CREATE TABLE panel_snapshot_chunks (
			snapshot_id TEXT NOT NULL,
			chunk_x INTEGER NOT NULL,
			chunk_y INTEGER NOT NULL,
			width INTEGER NOT NULL,
			height INTEGER NOT NULL,
			encoding INTEGER NOT NULL,
			rgba INTEGER,
			data BLOB,
			PRIMARY KEY (snapshot_id, chunk_x, chunk_y)
		);

		CREATE INDEX idx_pages_order ON pages(page_index);
		CREATE INDEX idx_panels_page_order ON panels(page_id, panel_index);
		CREATE INDEX idx_layers_panel_order ON layers(panel_id, layer_index);
		CREATE INDEX idx_snapshots_panel ON panel_snapshots(panel_id);
		",
    )?;
    Ok(())
}

/// put metadata に必要な処理を行う。
fn put_metadata<T: Serialize>(
    transaction: &Transaction<'_>,
    key: &str,
    value: &T,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO metadata (key, value_json) VALUES (?1, ?2)",
        params![key, encode_json(value)?],
    )?;
    Ok(())
}

/// Get metadata 用の表示文字列を組み立てる。
fn get_metadata<T: DeserializeOwned>(
    connection: &Connection,
    key: &str,
) -> Result<T, StorageError> {
    let value = connection
        .query_row(
            "SELECT value_json FROM metadata WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| StorageError::InvalidProject(format!("missing metadata key: {key}")))?;
    decode_json(&value)
}

/// 検証 形式 version を計算して返す。
///
/// 失敗時はエラーを返します。
fn validate_format_version(connection: &Connection) -> Result<(), StorageError> {
    let format_version: u32 = get_metadata(connection, METADATA_FORMAT_VERSION)?;
    if !(1..=CURRENT_FORMAT_VERSION).contains(&format_version) {
        return Err(StorageError::UnsupportedFormatVersion(format_version));
    }
    Ok(())
}

/// 現在の値を JSON へ変換する。
fn encode_json<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(StorageError::SerializeMetadataJson)
}

/// 入力を解析して JSON に変換し、失敗時はエラーを返す。
fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T, StorageError> {
    serde_json::from_str(value).map_err(StorageError::DeserializeMetadataJson)
}

/// Insert レイヤー chunks に対応するビットマップ処理を行う。
fn insert_layer_chunks(
    transaction: &Transaction<'_>,
    panel_id: PanelId,
    layer_index: usize,
    bitmap: &CanvasBitmap,
    chunk_size: usize,
) -> Result<(), StorageError> {
    for chunk in chunk_bitmap(bitmap, chunk_size)? {
        transaction.execute(
            "INSERT INTO layer_chunks (
				panel_id,
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
                panel_id.0 as i64,
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

/// Insert スナップショット chunks に対応するビットマップ処理を行う。
fn insert_snapshot_chunks(
    transaction: &Transaction<'_>,
    snapshot_id: &str,
    bitmap: &CanvasBitmap,
    chunk_size: usize,
) -> Result<(), StorageError> {
    for chunk in chunk_bitmap(bitmap, chunk_size)? {
        transaction.execute(
            "INSERT INTO panel_snapshot_chunks (
				snapshot_id,
				chunk_x,
				chunk_y,
				width,
				height,
				encoding,
				rgba,
				data
			) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                snapshot_id,
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

/// ピクセル走査を行い、チャンク ビットマップ 用のビットマップ結果を生成する。
fn chunk_bitmap(
    bitmap: &CanvasBitmap,
    chunk_size: usize,
) -> Result<Vec<StoredChunk>, StorageError> {
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
                        .map_err(StorageError::Compress)?;
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

/// Extract チャンク pixels に対応するビットマップ処理を行う。
fn extract_chunk_pixels(
    bitmap: &CanvasBitmap,
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

/// Solid RGBA に対応するビットマップ処理を行う。
///
/// 値を生成できない場合は `None` を返します。
fn solid_rgba(pixels: &[u8]) -> Option<[u8; 4]> {
    let first = pixels.get(0..4)?;
    if pixels.chunks_exact(4).all(|chunk| chunk == first) {
        Some([first[0], first[1], first[2], first[3]])
    } else {
        None
    }
}

/// 現在の値を RGBA へ変換する。
fn encode_rgba(rgba: [u8; 4]) -> i64 {
    (((rgba[0] as u32) << 24)
        | ((rgba[1] as u32) << 16)
        | ((rgba[2] as u32) << 8)
        | (rgba[3] as u32)) as i64
}

/// Decode RGBA に対応するビットマップ処理を行う。
fn decode_rgba(value: i64) -> [u8; 4] {
    let value = value as u32;
    [
        ((value >> 24) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]
}

/// All pages を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn load_all_pages(connection: &Connection) -> Result<Vec<Page>, StorageError> {
    let mut statement = connection.prepare("SELECT page_id FROM pages ORDER BY page_index ASC")?;
    let page_ids = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    page_ids
        .into_iter()
        .map(|page_id| load_page(connection, PageId(page_id as u64)))
        .collect()
}

/// ページ を読み込み、必要に応じて整形して返す。
///
/// 失敗時はエラーを返します。
fn load_page(connection: &Connection, page_id: PageId) -> Result<Page, StorageError> {
    let page_dimensions = connection
        .query_row(
            "SELECT width, height FROM pages WHERE page_id = ?1",
            [page_id.0 as i64],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((width, height)) = page_dimensions else {
        return Err(StorageError::PageNotFound(page_id.0));
    };

    let mut panel_statement = connection
        .prepare("SELECT panel_id FROM panels WHERE page_id = ?1 ORDER BY panel_index ASC")?;
    let panel_ids = panel_statement
        .query_map([page_id.0 as i64], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let panels = panel_ids
        .into_iter()
        .map(|panel_id| load_panel(connection, page_id, PanelId(panel_id as u64)))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Page {
        id: page_id,
        width: width as usize,
        height: height as usize,
        panels,
    })
}

/// 入力や種別に応じて処理を振り分ける。
fn load_panel(
    connection: &Connection,
    page_id: PageId,
    panel_id: PanelId,
) -> Result<Panel, StorageError> {
    let metadata_json = connection
        .query_row(
            "SELECT metadata_json FROM panels WHERE page_id = ?1 AND panel_id = ?2",
            params![page_id.0 as i64, panel_id.0 as i64],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(StorageError::PanelNotFound {
            page_id: page_id.0,
            panel_id: panel_id.0,
        })?;
    let panel_record: StoredPanelRecord = decode_json(&metadata_json)?;

    let mut layer_statement = connection.prepare(
		"SELECT layer_index, metadata_json FROM layers WHERE panel_id = ?1 ORDER BY layer_index ASC",
	)?;
    let layer_rows = layer_statement
        .query_map([panel_id.0 as i64], |row| {
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
            panel_id,
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

    let bitmap = match load_panel_snapshot(connection, &current_snapshot_id(page_id, panel_id))? {
        Some(snapshot) => snapshot.bitmap,
        None => compose_panel_bitmap(
            panel_record.composed_width.max(1),
            panel_record.composed_height.max(1),
            &layers,
        ),
    };

    Ok(Panel {
        id: panel_id,
        bounds: panel_record.bounds,
        root_layer: panel_record.root_layer,
        bitmap,
        layers,
        active_layer_index: panel_record.active_layer_index,
        created_layer_count: panel_record.created_layer_count,
    })
}

/// レイヤー ビットマップ を読み込み、必要に応じて整形して返す。
fn load_layer_bitmap(
    connection: &Connection,
    panel_id: PanelId,
    layer_index: usize,
    width: usize,
    height: usize,
) -> Result<CanvasBitmap, StorageError> {
    let mut bitmap = CanvasBitmap::transparent(width.max(1), height.max(1));
    let mut statement = connection.prepare(
        "SELECT chunk_x, chunk_y, width, height, encoding, rgba, data
		 FROM layer_chunks
		 WHERE panel_id = ?1 AND layer_index = ?2
		 ORDER BY chunk_y ASC, chunk_x ASC",
    )?;
    let chunks = statement
        .query_map(params![panel_id.0 as i64, layer_index as i64], |row| {
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

/// 現在の値を パネル スナップショット へ変換する。
fn load_panel_snapshot(
    connection: &Connection,
    snapshot_id: &str,
) -> Result<Option<PersistedPanelSnapshot>, StorageError> {
    let row = connection
        .query_row(
            "SELECT page_id, panel_id, save_mode, width, height, chunk_size
			 FROM panel_snapshots
			 WHERE snapshot_id = ?1",
            [snapshot_id],
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

    let Some((raw_page_id, raw_panel_id, save_mode, width, height, chunk_size)) = row else {
        return Ok(None);
    };

    let mut bitmap = CanvasBitmap::transparent(width as usize, height as usize);
    let mut statement = connection.prepare(
        "SELECT chunk_x, chunk_y, width, height, encoding, rgba, data
		 FROM panel_snapshot_chunks
		 WHERE snapshot_id = ?1
		 ORDER BY chunk_y ASC, chunk_x ASC",
    )?;
    let chunks = statement
        .query_map([snapshot_id], |row| {
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

    Ok(Some(PersistedPanelSnapshot {
        summary: PersistedPanelSnapshotSummary {
            snapshot_id: snapshot_id.to_string(),
            page_id: PageId(raw_page_id as u64),
            panel_id: PanelId(raw_panel_id as u64),
            save_mode: ProjectSaveMode::from_db(&save_mode)?,
            chunk_size: chunk_size as usize,
        },
        bitmap,
    }))
}

/// スナップショット summaries を読み込み、必要に応じて整形して返す。
fn load_snapshot_summaries(
    connection: &Connection,
) -> Result<Vec<PersistedPanelSnapshotSummary>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT snapshot_id, page_id, panel_id, save_mode, chunk_size
		 FROM panel_snapshots
		 ORDER BY page_id ASC, panel_id ASC, snapshot_id ASC",
    )?;
    statement
        .query_map([], |row| {
            let save_mode = row.get::<_, String>(3)?;
            Ok(PersistedPanelSnapshotSummary {
                snapshot_id: row.get(0)?,
                page_id: PageId(row.get::<_, i64>(1)? as u64),
                panel_id: PanelId(row.get::<_, i64>(2)? as u64),
                save_mode: ProjectSaveMode::from_db(&save_mode)
                    .map_err(to_sqlite_conversion_error)?,
                chunk_size: row.get::<_, i64>(4)? as usize,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::Sqlite)
}

/// 現在の値を chunks へ変換する。
///
/// 失敗時はエラーを返します。
fn apply_chunks(bitmap: &mut CanvasBitmap, chunks: &[StoredChunk]) -> Result<(), StorageError> {
    for chunk in chunks {
        match chunk.encoding {
            CHUNK_ENCODING_SOLID => fill_chunk(
                bitmap,
                chunk.chunk_x,
                chunk.chunk_y,
                chunk.width,
                chunk.height,
                chunk.rgba.ok_or_else(|| {
                    StorageError::InvalidProject("solid chunk is missing rgba payload".to_string())
                })?,
            ),
            CHUNK_ENCODING_ZSTD => {
                let decoded = zstd::stream::decode_all(Cursor::new(
                    chunk.data.as_deref().ok_or_else(|| {
                        StorageError::InvalidProject(
                            "compressed chunk is missing payload".to_string(),
                        )
                    })?,
                ))
                .map_err(StorageError::Decompress)?;
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
                return Err(StorageError::InvalidProject(format!(
                    "unknown chunk encoding: {encoding}"
                )));
            }
        }
    }
    Ok(())
}

/// 塗りつぶし チャンク に必要な描画内容を組み立てる。
fn fill_chunk(
    bitmap: &mut CanvasBitmap,
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

/// Blit チャンク に必要な描画内容を組み立てる。
fn blit_chunk(
    bitmap: &mut CanvasBitmap,
    chunk_x: usize,
    chunk_y: usize,
    width: usize,
    height: usize,
    pixels: &[u8],
) -> Result<(), StorageError> {
    let expected_len = width.saturating_mul(height).saturating_mul(4);
    if pixels.len() != expected_len {
        return Err(StorageError::InvalidProject(format!(
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

/// 合成 パネル ビットマップ に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn compose_panel_bitmap(width: usize, height: usize, layers: &[RasterLayer]) -> CanvasBitmap {
    let mut bitmap = CanvasBitmap::transparent(width, height);
    for layer in layers.iter().filter(|layer| layer.visible) {
        composite_layer_region_into(
            &mut bitmap,
            layer,
            CanvasDirtyRect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
    }
    bitmap
}

/// ピクセル走査を行い、composite レイヤー 領域 into 用のビットマップ結果を生成する。
///
/// 必要に応じて dirty 状態も更新します。
fn composite_layer_region_into(
    target: &mut CanvasBitmap,
    layer: &RasterLayer,
    dirty: CanvasDirtyRect,
) {
    let dirty = dirty.clamp_to_canvas_bounds(
        target.width.min(layer.bitmap.width).max(1),
        target.height.min(layer.bitmap.height).max(1),
    );

    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let target_index = (y * target.width + x) * 4;
            let source_index = (y * layer.bitmap.width + x) * 4;
            let mut src = [
                layer.bitmap.pixels[source_index],
                layer.bitmap.pixels[source_index + 1],
                layer.bitmap.pixels[source_index + 2],
                layer.bitmap.pixels[source_index + 3],
            ];
            if let Some(mask) = &layer.mask {
                src[3] = ((src[3] as u16 * mask_alpha_at(mask, x, y) as u16) / 255) as u8;
            }
            let dst = [
                target.pixels[target_index],
                target.pixels[target_index + 1],
                target.pixels[target_index + 2],
                target.pixels[target_index + 3],
            ];
            let blended = blend_pixel(dst, src, &layer.blend_mode);
            target.pixels[target_index..target_index + 4].copy_from_slice(&blended);
        }
    }
}

/// 指定位置の マスク アルファ を計算して返す。
fn mask_alpha_at(mask: &LayerMask, x: usize, y: usize) -> u8 {
    if x >= mask.width || y >= mask.height {
        return 0;
    }
    mask.alpha[y * mask.width + x]
}

/// 入力や種別に応じて処理を振り分ける。
fn blend_pixel(dst: [u8; 4], src: [u8; 4], mode: &BlendMode) -> [u8; 4] {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return dst;
    }
    let dst_a = dst[3] as f32 / 255.0;

    let blend_channel = |dst_c: u8, src_c: u8| -> f32 {
        let d = dst_c as f32 / 255.0;
        let s = src_c as f32 / 255.0;
        match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => 1.0 - (1.0 - s) * (1.0 - d),
            BlendMode::Add => (s + d).min(1.0),
        }
    };

    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let dst_c = dst[channel] as f32 / 255.0;
        let mixed = blend_channel(dst[channel], src[channel]);
        let out_c = mixed * src_a + dst_c * (1.0 - src_a);
        out[channel] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}

/// 現在 スナップショット ID 用の表示文字列を組み立てる。
fn current_snapshot_id(page_id: PageId, panel_id: PanelId) -> String {
    format!("page:{}:panel:{}:current", page_id.0, panel_id.0)
}

/// 現在 unix ms 用の表示文字列を組み立てる。
///
/// 失敗時はエラーを返します。
fn current_unix_ms() -> Result<i64, StorageError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::InvalidProject(format!("system clock error: {error}")))?
        .as_millis() as i64)
}

/// 現在の値を sqlite conversion エラー 形式へ変換する。
fn to_sqlite_conversion_error(error: StorageError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
