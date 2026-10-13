//! JSON 設定ファイルのロード結果を 3 状態で区別する共通ローダ。
//!
//! 破損ファイルを既定値で黙って上書きする事故を防ぐため、`Missing` と
//! `Corrupt` を明示的に区別する。`Corrupt` の場合は呼び出し側が既定値で
//! 動作しつつ、元ファイルを温存できる (この層は読み取りのみで書き込まない)。
//!
//! B7 で `desktop-support::json_store` から移管した。project / workspace feature が
//! 共有する。

use std::path::Path;

use serde::de::DeserializeOwned;

/// JSON 設定ファイルのロード結果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JsonLoad<T> {
    /// 正常に読み込めた。
    Loaded(T),
    /// ファイルが存在しない (初回起動など)。
    Missing,
    /// ファイルは存在するが読み取り/パースに失敗した。元ファイルは温存すべき。
    Corrupt,
}

/// `path` を読み込み、`Loaded` / `Missing` / `Corrupt` のいずれかを返す。
///
/// `Corrupt` の場合は `label` を含む診断を標準エラーへ出力する
/// (破損ファイルを黙って既定値で上書きしないため、呼び出し側が温存を選べる)。
pub(crate) fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>, label: &str) -> JsonLoad<T> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return JsonLoad::Missing,
        Err(error) => {
            eprintln!(
                "failed to read {label} file (preserving on disk): {}: {error}",
                path.display()
            );
            return JsonLoad::Corrupt;
        }
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(value) => JsonLoad::Loaded(value),
        Err(error) => {
            eprintln!(
                "failed to parse {label} file (preserving on disk): {}: {error}",
                path.display()
            );
            JsonLoad::Corrupt
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_path(name: &str) -> std::path::PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "altpaint-jsonstore-{name}-{}-{unique}.json",
            std::process::id()
        ))
    }

    #[test]
    fn missing_file_reports_missing() {
        let path = unique_path("missing");
        let _ = std::fs::remove_file(&path);
        let load: JsonLoad<Vec<u32>> = load_json(&path, "test");
        assert_eq!(load, JsonLoad::Missing);
    }

    #[test]
    fn valid_file_reports_loaded() {
        let path = unique_path("valid");
        std::fs::write(&path, b"[1, 2, 3]").expect("write");
        let load: JsonLoad<Vec<u32>> = load_json(&path, "test");
        assert_eq!(load, JsonLoad::Loaded(vec![1, 2, 3]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_reports_corrupt_and_preserves_contents() {
        let path = unique_path("corrupt");
        let raw = b"{ this is not valid json";
        std::fs::write(&path, raw).expect("write");

        let load: JsonLoad<Vec<u32>> = load_json(&path, "test");
        assert_eq!(load, JsonLoad::Corrupt);

        // ローダはファイルへ書き込まない (温存)。
        let after = std::fs::read(&path).expect("read back");
        assert_eq!(after, raw);
        let _ = std::fs::remove_file(&path);
    }
}
