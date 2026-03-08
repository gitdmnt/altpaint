use crate::document::ColorRgba8;
use crate::document::ToolKind;

/// アプリケーション状態を変更するための最小コマンド列挙型。
///
/// フェーズ0ではまだ実際の編集機能を持たないため、将来の変更経路を
/// 先に固定するためのプレースホルダとして最小コマンドだけを定義する。
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// 現在のブラシ色を切り替える。
    SetActiveColor { color: ColorRgba8 },
    /// 新規ドキュメントを作成する。
    NewDocument,
    /// 現在のドキュメントを保存する。
    SaveProject,
    /// 既定パスからドキュメントを読み込む。
    LoadProject,
}
