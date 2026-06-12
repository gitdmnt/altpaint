//! `app-core` は `altpaint` のドメインモデルを保持するクレート。
//!
//! 作品・ページ・コマ・レイヤーのドメイン構造に加え、変更経路の入口になる
//! `DocumentCommand` / `SessionCommand` 型、座標系、Undo/Redo 履歴、
//! ペイント基本型、ワークスペース UI 状態を定義する。

pub mod blend;
pub mod command;
pub mod document;
pub mod history;
pub mod paint_params;
pub mod painting;
pub mod view_policy;
pub mod workspace;

pub use command::{DocumentCommand, SessionCommand};
pub use geometry::{
    CanvasDisplayPoint, CanvasViewportPoint, ClampToCanvasBounds, KomaLocalPoint, MergeInSpace,
    PageDirtyRect, PagePoint, PagePointF, PanelSurfaceDirtyRect, PanelSurfacePoint,
    PanelSurfaceRect, WindowDirtyRect, WindowPoint, WindowRect,
};
pub use document::{
    BlendMode, CanvasBitmap, CanvasViewTransform, ColorRgba8, DEFAULT_PAGE_HEIGHT,
    DEFAULT_PAGE_WIDTH, Document, LayerMask, LayerNodeId, MAX_PAGE_DIMENSION, MAX_PAGE_PIXELS,
    Page, PageId, Koma, KomaBounds, KomaId, PenPreset, PenRuntimeEngine, PenTipBitmap,
    RasterLayer, ToolDefinition, ToolKind, ToolSettingControl, ToolSettingDefinition, Work,
    WorkId, parse_document_size,
};
pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{
    BitmapComposite, BitmapCompositor, BitmapEdit, PaintInput, PaintPlugin, PaintPluginContext,
};
pub use workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
