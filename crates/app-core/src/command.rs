use crate::document::ColorRgba8;
use crate::document::ToolKind;

/// `Document` の内容そのものを変える純粋なドキュメント変異コマンド。
///
/// レイヤー・コマ・ドキュメント差し替えなど、作品データを書き換える操作だけを表す。
/// I/O (保存・読込・preset 入出力・undo/redo) はホスト側のサービス経路で処理するため
/// ここには含まない。エディタセッション (ツール/色/ペン/ビュー) は [`SessionCommand`] が担う。
#[derive(Debug, Clone, PartialEq)]
pub enum DocumentCommand {
    /// 状態を変更しないダミーコマンド。
    Noop,
    /// 指定矩形のコマを現在ページへ追加する。
    CreateKoma {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
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
    SetActiveLayerBlendMode { mode: raster::BlendMode },
    /// アクティブレイヤーの表示状態を切り替える。
    ToggleActiveLayerVisibility,
    /// 新しいコマを現在ページへ追加する。
    AddKoma,
    /// 現在のアクティブコマを削除する。
    RemoveActiveKoma,
    /// アクティブコマを指定 index に切り替える。
    SelectKoma { index: usize },
    /// 次のコマをアクティブにする。
    SelectNextKoma,
    /// 前のコマをアクティブにする。
    SelectPreviousKoma,
    /// アクティブコマ中心の表示へ戻す。
    FocusActiveKoma,
    /// 指定サイズで新規ドキュメントを作成する。
    NewDocumentSized { width: usize, height: usize },
}

/// エディタの一過性編集状態 (ツール/色/ペン/ビュー) を変えるセッションコマンド。
///
/// 作品データには触れず、`Document` に同居しているセッション状態を更新する。
/// B5 (BL-073) で `editor-state` クレートへ移設予定。
#[derive(Debug, Clone, PartialEq)]
pub enum SessionCommand {
    /// 現在のアクティブツールを切り替える。
    SetActiveTool { tool: ToolKind },
    /// 登録済みツール ID を指定して現在のアクティブツールを切り替える。
    SelectTool { tool_id: String },
    /// 子ツール ID を指定してアクティブ子ツールを切り替える。
    SelectChildTool { child_id: String },
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
    /// 現在のブラシ色を切り替える。
    SetActiveColor { color: ColorRgba8 },
    /// キャンバス表示倍率を設定する。
    SetViewZoom { zoom: f32 },
    /// キャンバス表示倍率を相対的に変える (ホイール `lines` ノッチ)。
    ///
    /// 倍率 (`view_policy::ZOOM_LINE_BASE.powf(lines)`) と上下限クランプは
    /// ドメイン側 (`view_policy`) が適用する (BL-064)。
    ZoomViewBy { lines: f32 },
    /// キャンバス表示をピクセル量で平行移動する。
    PanView { delta_x: f32, delta_y: f32 },
    /// キャンバス表示を line 量で平行移動する。
    ///
    /// 1 line あたりのピクセル量 (`view_policy::PAN_PIXELS_PER_LINE`) は
    /// ドメイン側が適用する (BL-064)。
    PanViewByLines { x_lines: f32, y_lines: f32 },
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
}
