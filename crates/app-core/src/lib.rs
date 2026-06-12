//! `app-core` は `altpaint` のドメインモデルを保持するクレート。
//!
//! 作品・ページ・コマ・レイヤーのドメイン構造に加え、変更経路の入口になる
//! `Command` 型、座標系、Undo/Redo 履歴、ペイント基本型、ワークスペース UI 状態を定義する。

pub mod command;
pub mod coordinates;
pub mod document;
pub mod history;
pub mod paint_params;
pub mod painting;
pub mod workspace;

pub use command::Command;
pub use coordinates::{
    PageDirtyRect, CanvasDisplayPoint, PagePoint, PagePointF, CanvasViewportPoint,
    ClampToCanvasBounds, MergeInSpace, KomaLocalPoint, PanelSurfaceDirtyRect, PanelSurfacePoint,
    PanelSurfaceRect, WindowDirtyRect, WindowPoint, WindowRect,
};
pub use document::{
    BlendMode, CanvasBitmap, CanvasViewTransform, ColorRgba8, DEFAULT_PAGE_HEIGHT,
    DEFAULT_PAGE_WIDTH, Document, LayerMask, LayerNodeId, Page, PageId, Koma,
    KomaBounds, KomaId, PenPreset, PenRuntimeEngine, PenTipBitmap, RasterLayer, ToolDefinition,
    ToolKind, ToolSettingControl, ToolSettingDefinition, Work, WorkId,
};
pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{
    BitmapComposite, BitmapCompositor, BitmapEdit, PaintInput, PaintPlugin, PaintPluginContext,
};
pub use workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
