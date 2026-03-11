//! `app-core` は `altpaint` の最小ドメインモデルを保持するクレート。
//!
//! フェーズ0では、作品・ページ・コマ・レイヤーという最小構造と、
//! 今後の変更経路の入口になる `Command` 型だけを定義する。

pub mod command;
pub mod coordinates;
pub mod document;
pub mod error;
pub mod painting;
pub mod workspace;

pub use command::Command;
pub use coordinates::{
    CanvasDirtyRect, CanvasDisplayPoint, CanvasPoint, CanvasViewportPoint, ClampToCanvasBounds,
    MergeInSpace, PanelLocalPoint, PanelSurfaceDirtyRect, PanelSurfacePoint, PanelSurfaceRect,
    WindowDirtyRect, WindowPoint, WindowRect,
};
pub use document::{
    BlendMode, CanvasBitmap, CanvasViewTransform, ColorRgba8, DEFAULT_DOCUMENT_HEIGHT,
    DEFAULT_DOCUMENT_WIDTH, Document, LayerMask, LayerNode, LayerNodeId, Page, PageId, Panel,
    PanelBounds, PanelId, PenPreset, PenRuntimeEngine, PenTipBitmap, RasterLayer, ToolKind, Work,
    ToolDefinition, ToolSettingControl, ToolSettingDefinition, WorkId,
};
pub use error::CoreError;
pub use painting::{
    BitmapComposite, BitmapCompositor, BitmapEdit, PaintInput, PaintPlugin, PaintPluginContext,
};
pub use workspace::{
    WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition, WorkspacePanelSize,
    WorkspacePanelState,
};
