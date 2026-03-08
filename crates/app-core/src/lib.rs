//! `app-core` は `altpaint` の最小ドメインモデルを保持するクレート。
//!
//! フェーズ0では、作品・ページ・コマ・レイヤーという最小構造と、
//! 今後の変更経路の入口になる `Command` 型だけを定義する。

pub mod command;
pub mod document;
pub mod error;

pub use command::Command;
pub use document::{
    CanvasBitmap, CanvasViewTransform, ColorRgba8, DirtyRect, Document, LayerNode, LayerNodeId,
    Page, PageId, Panel, PanelId, ToolKind, Work, WorkId,
};
pub use error::CoreError;
