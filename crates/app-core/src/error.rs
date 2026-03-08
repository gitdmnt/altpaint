use thiserror::Error;

/// `app-core` 内で発生する最小エラー型。
///
/// 現段階ではドキュメント整合性に関するエラーのみを定義し、
/// 後続フェーズで保存やコマンド適用に関する失敗を追加していく。
#[derive(Debug, Error)]
pub enum CoreError {
    /// ドキュメントの構造が前提を満たさない場合に返すエラー。
    #[error("invalid document state: {0}")]
    InvalidDocumentState(&'static str),
}
