//! インメモリのドキュメントスナップショット管理。

use app_core::Document;

/// スナップショットの最大保持件数。
pub(crate) const MAX_SNAPSHOTS: usize = 20;

/// 名前付きドキュメントスナップショット。
#[derive(Debug, Clone)]
pub(crate) struct SnapshotEntry {
    /// スナップショットの一意 ID（単調増加の整数文字列）。
    pub(crate) id: String,
    /// ユーザー定義のラベル。
    pub(crate) label: String,
    /// 採取時点の Document クローン。
    pub(crate) document: Document,
}

/// スナップショット一覧と採番カウンタを保持する。
#[derive(Debug, Default)]
pub(crate) struct SnapshotStore {
    entries: Vec<SnapshotEntry>,
    next_id: u64,
}

impl SnapshotStore {
    /// スナップショットを追加する。
    ///
    /// `MAX_SNAPSHOTS` を超えた場合は最も古いエントリを破棄する。
    pub(crate) fn push(&mut self, label: impl Into<String>, document: Document) -> String {
        let id = self.next_id.to_string();
        self.next_id += 1;
        if self.entries.len() >= MAX_SNAPSHOTS {
            self.entries.remove(0);
        }
        self.entries.push(SnapshotEntry {
            id: id.clone(),
            label: label.into(),
            document,
        });
        id
    }

    /// ID でスナップショットを検索して返す。
    pub(crate) fn get(&self, id: &str) -> Option<&SnapshotEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// 保持しているスナップショット数を返す。
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// 保持しているスナップショットの一覧を返す（ID・ラベルのみ）。
    pub(crate) fn entries(&self) -> &[SnapshotEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Document;

    fn make_doc() -> Document {
        // MAX_SNAPSHOTS 分のクローンを保持するため、メモリを節約して最小サイズを使う。
        Document::new(1, 1)
    }

    /// push したスナップショットを ID で取り出せることを確認する。
    #[test]
    fn push_and_get() {
        let mut store = SnapshotStore::default();
        let id = store.push("test", make_doc());
        assert_eq!(store.len(), 1);
        let entry = store.get(&id).expect("should find entry");
        assert_eq!(entry.label, "test");
    }

    /// MAX_SNAPSHOTS 超過時に最古エントリが破棄されることを確認する。
    #[test]
    fn evicts_oldest_when_full() {
        let mut store = SnapshotStore::default();
        let first_id = store.push("first", make_doc());
        for i in 0..MAX_SNAPSHOTS {
            store.push(format!("snap{i}"), make_doc());
        }
        assert_eq!(store.len(), MAX_SNAPSHOTS);
        assert!(store.get(&first_id).is_none());
    }

    /// 存在しない ID の場合 None を返すことを確認する。
    #[test]
    fn get_unknown_id_returns_none() {
        let store = SnapshotStore::default();
        assert!(store.get("999").is_none());
    }
}
