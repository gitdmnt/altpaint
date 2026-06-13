/// `Document` の内容そのものを変える純粋なドキュメント変異コマンド。
///
/// レイヤー・コマ・ドキュメント差し替えなど、作品データを書き換える操作だけを表す。
/// I/O (保存・読込・preset 入出力・undo/redo) はホスト側のサービス経路で処理するため
/// ここには含まない。エディタセッション (ツール/色/ペン/ビュー) は
/// [`editor_state::SessionCommand`] が担う。
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
