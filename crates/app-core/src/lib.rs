//! `app-core` は `altpaint` のドメイン語彙を中継する縮小クレート。
//!
//! 作品ドメインモデル (`Document` / `Work` / `Page` / `Koma` / `RasterLayer` /
//! `DocumentCommand` 等) は `document-model` クレートが、座標系・矩形・dirty rect 演算は
//! `geometry` が、`RgbaBitmap`・ブレンド・ラスタライズ・`BitmapEdit` は `raster` が、
//! エディタの一過性編集状態は `editor-state` が担う。本クレートには Undo/Redo 履歴・
//! ペイント入力型・ワークスペース UI 状態のみが残り、参照元の付け替えに伴い消滅する。

pub mod history;
pub mod paint_params;
pub mod painting;
pub mod workspace;

pub use document_model::blend;
pub use document_model::{
    DEFAULT_PAGE_HEIGHT, DEFAULT_PAGE_WIDTH, Document, DocumentCommand, Koma, KomaBounds, KomaId,
    LayerMask, LayerNodeId, MAX_PAGE_DIMENSION, MAX_PAGE_PIXELS, Page, PageId, RasterLayer, Work,
    WorkId, parse_document_size,
};
pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{PaintInput, PaintPlugin, PaintPluginContext};
pub use workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
