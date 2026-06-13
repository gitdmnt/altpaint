//! host service request の registry と補助的な GPU/履歴同期処理を扱う。
//!
//! 各サービスハンドラは `features/*` の feature スライスが所有し、`registry` の
//! `SERVICE_HANDLERS` に登録される。`execute_service_request` はその registry を
//! 順に試行する (BL-111)。
//!
//! ここに残るのは feature 横断の補助処理:
//! - `gpu_sync`: 保存前の GPU→CPU ビットマップ読み戻し
//! - `history`: undo/redo の文書/GPU 復元 (paint feature の `EditHistory` を消費)

mod gpu_sync;
mod history;
mod registry;
