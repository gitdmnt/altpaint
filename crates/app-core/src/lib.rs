//! `app-core` は `altpaint` の最小ドメインモデルを保持するクレート。
//!
//! フェーズ0では、作品・ページ・コマ・レイヤーという最小構造と、
//! 今後の変更経路の入口になる `Command` 型だけを定義する。

pub mod command;
pub mod document;
pub mod error;
pub mod workspace;

pub use command::Command;
pub use document::{
    BlendMode, CanvasBitmap, CanvasViewTransform, ColorRgba8, DirtyRect, Document, LayerMask,
    LayerNode, LayerNodeId, Page, PageId, Panel, PanelId, PenPreset, RasterLayer, ToolKind,
    Work, WorkId, DEFAULT_DOCUMENT_HEIGHT, DEFAULT_DOCUMENT_WIDTH,
};
pub use error::CoreError;
pub use workspace::{WorkspaceLayout, WorkspacePanelState};
