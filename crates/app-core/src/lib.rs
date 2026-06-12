//! `app-core` は `altpaint` のドメインモデルを保持するクレート。
//!
//! 作品・ページ・コマ・レイヤーのドメイン構造に加え、変更経路の入口になる
//! `DocumentCommand` / `SessionCommand` 型、Undo/Redo 履歴、
//! ペイント入力型、ワークスペース UI 状態を定義する。
//! 座標系・矩形・dirty rect 演算は `geometry` クレートが、`RgbaBitmap`・ブレンド・
//! ラスタライズ・`BitmapEdit` は `raster` クレートが担う。

pub mod blend;
pub mod command;
pub mod document;
pub mod history;
pub mod paint_params;
pub mod painting;
pub mod session;
pub mod view_policy;
pub mod workspace;

pub use command::{DocumentCommand, SessionCommand};
pub use document::{
    DEFAULT_PAGE_HEIGHT, DEFAULT_PAGE_WIDTH, Document, LayerMask, LayerNodeId, MAX_PAGE_DIMENSION,
    MAX_PAGE_PIXELS, Page, PageId, Koma, KomaBounds, KomaId, RasterLayer, Work, WorkId,
    parse_document_size,
};
pub use session::{
    CanvasViewTransform, ColorRgba8, EditorSession, PenPreset, PenRuntimeEngine, PenTipBitmap,
    ToolDefinition, ToolKind, ToolSettingControl, ToolSettingDefinition,
};
pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{PaintInput, PaintPlugin, PaintPluginContext};
pub use workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
