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
    DrawPoint { x: usize, y: usize, pressure: f32 },
    /// 指定座標へ1ピクセル消去する最小コマンド。
    ErasePoint { x: usize, y: usize, pressure: f32 },
    /// 指定した2点の間に最小ストロークを描画する。
    DrawStroke {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        pressure: f32,
    },
    /// 指定した2点の間を白で消去する。
    EraseStroke {
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        pressure: f32,
    },
    /// 現在のアクティブツールを切り替える。
    SetActiveTool { tool: ToolKind },
    /// 現在のアクティブペンサイズを切り替える。
    SetActivePenSize { size: u32 },
    /// アクティブペンの筆圧有効状態を切り替える。
    SetActivePenPressureEnabled { enabled: bool },
    /// アクティブペンのアンチエイリアス有効状態を切り替える。
    SetActivePenAntialias { enabled: bool },
    /// アクティブペンの手ぶれ補正強さを切り替える。
    SetActivePenStabilization { amount: u8 },
    /// 次のペンプリセットをアクティブにする。
    SelectNextPenPreset,
    /// 前のペンプリセットをアクティブにする。
    SelectPreviousPenPreset,
    /// 既定ペンディレクトリからペンプリセットを再読込する。
    ReloadPenPresets,
    /// 現在のブラシ色を切り替える。
    SetActiveColor { color: ColorRgba8 },
    /// 閉領域バケツ塗りを行う。
    FillRegion { x: usize, y: usize },
    /// 投げ縄で囲った領域を塗り潰す。
    FillLasso { points: Vec<(usize, usize)> },
    /// キャンバス表示倍率を設定する。
    SetViewZoom { zoom: f32 },
    /// キャンバス表示を平行移動する。
    PanView { delta_x: f32, delta_y: f32 },
    /// キャンバス表示のパン位置を絶対値で設定する。
    SetViewPan { pan_x: f32, pan_y: f32 },
    /// キャンバス表示を 90 度単位で回転する。
    RotateView { quarter_turns: i32 },
    /// キャンバス表示の回転角を度単位で設定する。
    SetViewRotation { rotation_degrees: f32 },
    /// キャンバス表示を左右反転する。
    FlipViewHorizontally,
    /// キャンバス表示を上下反転する。
    FlipViewVertically,
    /// キャンバス表示を既定位置へ戻す。
    ResetView,
    /// 新しいラスタレイヤーを追加する。
    AddRasterLayer,
    /// 現在のアクティブレイヤーを削除する。
    RemoveActiveLayer,
    /// アクティブレイヤーを指定 index に切り替える。
    SelectLayer { index: usize },
    /// アクティブレイヤー名を変更する。
    RenameActiveLayer { name: String },
    /// レイヤー順を指定 index 間で移動する。
    MoveLayer { from_index: usize, to_index: usize },
    /// 次のレイヤーをアクティブにする。
    SelectNextLayer,
    /// アクティブレイヤーの合成モードを循環させる。
    CycleActiveLayerBlendMode,
    /// アクティブレイヤーの合成モードを明示設定する。
    SetActiveLayerBlendMode { mode: crate::document::BlendMode },
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
    /// workspace preset カタログを再読込する。
    ReloadWorkspacePresets,
    /// 指定 workspace preset を適用する。
    ApplyWorkspacePreset { preset_id: String },
    /// 現在の workspace UI 状態を preset カタログへ保存する。
    SaveWorkspacePreset { preset_id: String, label: String },
    /// 現在の workspace UI 状態を単一 preset ファイルとして書き出す。
    ExportWorkspacePreset { preset_id: String, label: String },
    /// 指定パスへ現在の workspace UI 状態を単一 preset ファイルとして書き出す。
    ExportWorkspacePresetToPath {
        preset_id: String,
        label: String,
        path: String,
    },
    /// 外部ペンファイルを選択して読み込む。
    ImportPenPresets,
    /// 指定パスの外部ペンファイルを読み込む。
    ImportPenPresetsFromPath { path: String },
}
