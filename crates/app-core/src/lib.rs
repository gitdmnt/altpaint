//! `app-core` は `altpaint` の解体途上にある縮小クレート。
//!
//! 作品ドメインモデル (`Document` / `Work` / `Page` / `Koma` / `RasterLayer` /
//! `DocumentCommand` 等) は `document-model` クレートへ移設済み。座標系・矩形・dirty rect
//! 演算は `geometry`、`RgbaBitmap`・ブレンド・ラスタライズ・`BitmapEdit` は `raster`、
//! エディタの一過性編集状態は `editor-state`、ワークスペース UI 状態 (`WorkspaceUiState`
//! 等) は `panel-workspace` が担う。
//!
//! 本クレートには Undo/Redo 履歴・ペイント入力型のみが残る。
//! これらは後続チャンクで各所掌クレートへ移設され、本クレートは消滅する。

pub mod history;
pub mod paint_params;
pub mod painting;

pub use history::{DEFAULT_HISTORY_CAPACITY, EditHistory, HistoryEntry, OpaqueGpuData};
pub use painting::{PaintInput, PaintPlugin, PaintPluginContext};
// ワークスペース UI 状態は panel-workspace へ移設済み (BL-075)。
// 参照付け替え完了までの暫定再エクスポート。
pub use panel_workspace::{
    PanelConfigs, WorkspaceLayout, WorkspacePanelAnchor, WorkspacePanelPosition,
    WorkspacePanelSize, WorkspacePanelState, WorkspaceUiState,
};
