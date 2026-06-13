//! `editor-state` はエディタの一過性編集状態を担う水平土台クレート。
//!
//! アクティブツール・色・ペンプリセット・表示変換などの編集セッション状態
//! (`EditorSession`)、ツール/ペン定義型 (`ToolDefinition` / `PenPreset` 等)、
//! セッションコマンド (`SessionCommand`)、ビュー操作ポリシー (`view_policy`) を提供する。
//!
//! 作品データ (`Document` / `Work` 等) は `document-model` が担い、本クレートは
//! それに依存しない (循環回避)。

pub mod command;
pub mod session;
pub mod view_policy;

pub use command::SessionCommand;
pub use session::{
    CanvasViewTransform, ColorRgba8, EditorSession, PenPreset, PenRuntimeEngine, PenTipBitmap,
    StrokeMode, ToolDefinition, ToolKind, ToolSettingControl, ToolSettingDefinition,
};
