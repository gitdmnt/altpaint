//! `app-core` は `altpaint` の解体途上にある縮小クレート。
//!
//! 作品ドメインモデル (`Document` / `Work` / `Page` / `Koma` / `RasterLayer` /
//! `DocumentCommand` 等) は `document-model` クレートへ移設済み。座標系・矩形・dirty rect
//! 演算は `geometry`、`RgbaBitmap`・ブレンド・ラスタライズ・`BitmapEdit` は `raster`、
//! エディタの一過性編集状態は `editor-state` が担う。
//!
//! 本クレートには Undo/Redo 履歴・ペイント入力型・ワークスペース UI 状態のみが残る。
//! これらは後続チャンクで各所掌クレートへ移設され、本クレートは消滅する。

pub mod history;
pub mod paint_params;
pub mod painting;
pub mod workspace;

pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{PaintInput, PaintPlugin, PaintPluginContext};
pub use workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
