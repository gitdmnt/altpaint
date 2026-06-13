//! 垂直 feature スライス群 (BL-111)。
//!
//! 各スライスは「契約 + ホスト側ハンドラ + 状態 + テスト」を 1 ディレクトリで所有する。
//! B7-part1 では `desktop-support` / `project-store` から移設した永続化・カタログ・
//! export・status_bar を provisional に配置する。残る services/* の features 完全移行と
//! feature 構造の完成は B7-part2 で行う。

pub(crate) mod export;
mod json_store;
pub(crate) mod koma;
pub(crate) mod paint;
pub(crate) mod project;
pub(crate) mod snapshots;
pub(crate) mod status_bar;
pub(crate) mod text;
pub(crate) mod tools;
pub(crate) mod view;
pub(crate) mod workspace;
