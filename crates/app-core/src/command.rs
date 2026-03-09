use crate::document::ColorRgba8;
use crate::document::ToolKind;

/// アプリケーション状態を変更するための最小コマンド列挙型。
///
/// フェーズ0ではまだ実際の編集機能を持たないため、将来の変更経路を
/// 先に固定するためのプレースホルダとして最小コマンドだけを定義する。
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// 状態を変更しないダミーコマンド。
    Noop,
    /// 指定座標へ1ピクセル描画する最小コマンド。
    DrawPoint { x: usize, y: usize },
    /// 指定座標へ1ピクセル消去する最小コマンド。
    ErasePoint { x: usize, y: usize },
    /// 指定した2点の間に最小ストロークを描画する。
    DrawStroke {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    },
    /// 指定した2点の間を白で消去する。
    EraseStroke {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    },
    /// 現在のアクティブツールを切り替える。
    SetActiveTool { tool: ToolKind },
    /// 現在のアクティブペンサイズを切り替える。
    SetActivePenSize { size: u32 },
    /// 次のペンプリセットをアクティブにする。
    SelectNextPenPreset,
    /// 前のペンプリセットをアクティブにする。
    SelectPreviousPenPreset,
    /// 既定ペンディレクトリからペンプリセットを再読込する。
    ReloadPenPresets,
    /// 現在のブラシ色を切り替える。
    SetActiveColor { color: ColorRgba8 },
    /// キャンバス表示倍率を設定する。
    SetViewZoom { zoom: f32 },
    /// キャンバス表示を平行移動する。
    PanView { delta_x: f32, delta_y: f32 },
    /// キャンバス表示を既定位置へ戻す。
    ResetView,
    /// 新しいラスタレイヤーを追加する。
    AddRasterLayer,
    /// アクティブレイヤーを指定 index に切り替える。
    SelectLayer { index: usize },
    /// 次のレイヤーをアクティブにする。
    SelectNextLayer,
    /// アクティブレイヤーの合成モードを循環させる。
    CycleActiveLayerBlendMode,
    /// アクティブレイヤーの表示状態を切り替える。
    ToggleActiveLayerVisibility,
    /// アクティブレイヤーの最小デモマスクを切り替える。
    ToggleActiveLayerMask,
    /// 新規ドキュメントを作成する。
    NewDocument,
    /// 指定サイズで新規ドキュメントを作成する。
    NewDocumentSized { width: usize, height: usize },
    /// 現在のドキュメントを保存する。
    SaveProject,
    /// 保存先を選んで現在のドキュメントを保存する。
    SaveProjectAs,
    /// 指定パスへ現在のドキュメントを保存する。
    SaveProjectToPath { path: String },
    /// 既定パスからドキュメントを読み込む。
    LoadProject,
    /// 指定パスからドキュメントを読み込む。
    LoadProjectFromPath { path: String },
}
