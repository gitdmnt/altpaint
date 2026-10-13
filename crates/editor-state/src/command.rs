use crate::session::{ColorRgba8, ToolKind};

/// エディタの一過性編集状態 (ツール/色/ペン/ビュー) を変えるセッションコマンド。
///
/// 作品データには触れず、`EditorSession` の状態のみを更新する。適用は
/// [`crate::EditorSession::apply_session_command`] が担う。
#[derive(Debug, Clone, PartialEq)]
pub enum SessionCommand {
    /// 現在のアクティブツールを切り替える。
    SetActiveTool { tool: ToolKind },
    /// 登録済みツール ID を指定して現在のアクティブツールを切り替える。
    ///
    /// `remember_size` が真のとき、切替前のツール/ペンへ現在のペンサイズを退避し、
    /// 切替後のツール/ペンに記憶済みサイズがあれば復元する (ツール別サイズ記憶)。
    /// ドロップダウン経由の選択など記憶不要な経路は偽で発行する。
    SelectTool {
        tool_id: String,
        remember_size: bool,
    },
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
    ///
    /// `remember_size` が真のとき、切替前後のペンでツール別サイズ記憶を退避/復元する。
    SelectNextPenPreset { remember_size: bool },
    /// 前のペンプリセットをアクティブにする。
    ///
    /// `remember_size` が真のとき、切替前後のペンでツール別サイズ記憶を退避/復元する。
    SelectPreviousPenPreset { remember_size: bool },
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
