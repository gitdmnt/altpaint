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
    /// アクティブレイヤーを安定 id (`RasterLayer.id`) で切り替える (BL-148)。
    ///
    /// 表示順 index ではなく安定 id を使うことで、UI 並べ替えで意味が変わる index 依存
    /// (パネル側の二重 index 反転) を解消する。未知 id は無視 (no-op)。
    SelectLayer { id: super::LayerNodeId },
    /// アクティブレイヤー名を変更する。
    RenameActiveLayer { name: String },
    /// レイヤー順を安定 id 間で移動する (BL-148)。`from_id` のレイヤーを `to_id` の
    /// 位置へ移す。未知 id は無視 (no-op)。
    MoveLayer {
        from_id: super::LayerNodeId,
        to_id: super::LayerNodeId,
    },
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
