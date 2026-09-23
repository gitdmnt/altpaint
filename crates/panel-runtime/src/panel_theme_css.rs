//! 全ビルトインパネルが共有する単一テーマ CSS (ADR 019)。
//!
//! Blitz は CSS `var()` を解決しないため、テーマは共通クラスをここへ集約して統一する。
//! 色は presenter `theme.rs` と一致させる (背景 `#181818`)。各 `panel.css` は
//! パネル固有ルールのみを持ち、共通クラス (`.panel`/`.btn`/...) はここを唯一の定義とする。

/// 全パネル共通のベース CSS。
///
/// - `.panel` からは常時グレー枠 + `border-radius` を撤廃した。アクティブ強調は
///   presenter が描画する選択枠 (`ACTIVE_PANEL_BORDER`) に委ね、二重枠を解消する。
/// - 枠線色は単一値 `#2a3140` へ統一 (従来の `#2a3140`/`#434c5d`/`#4d5a70` を集約)。
/// - `min-width` は単一基準 `240px`。
pub(crate) const PANEL_BASE_CSS: &str = r#"
.panel {
  padding: 12px;
  background: #181818;
  color: #f3f7ff;
  font-family: sans-serif;
  font-size: 13px;
  min-width: 240px;
}
.panel-title {
  font-size: 14px;
  font-weight: bold;
  padding: 4px 0 8px;
  border-bottom: 1px solid #2a3140;
  margin-bottom: 8px;
}
.alt-section { margin-top: 10px; }
.alt-section > .alt-section-title {
  margin: 0 0 6px;
  font-size: 11px;
  color: #8aa2c0;
  text-transform: uppercase;
}
hr { border: none; border-top: 1px solid #2a3140; margin: 8px 0; }
label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 6px 0;
  font-size: 11px;
  color: #8aa2c0;
}
label > span { font-size: 11px; color: #8aa2c0; }
select,
input[type="text"],
input[type="number"] {
  background: #232a36;
  color: #f3f7ff;
  border: 1px solid #4d5a70;
  border-radius: 3px;
  padding: 4px 6px;
}
input[type="range"] { width: 100%; }
.btn {
  padding: 6px 10px;
  background: #273540;
  color: #f3f7ff;
  border: 1px solid #4d5a70;
  border-radius: 4px;
  font-size: 13px;
  margin-bottom: 2px;
}
.btn.active { background: #3b4f63; border-color: #6f80a3; }
.btn[disabled] { opacity: 0.45; }
.row { display: flex; gap: 6px; }
.muted { color: #8aa2c0; font-size: 11px; }
p { margin: 4px 0; }
[hidden] { display: none; }
"#;

/// パネル固有 CSS の前に共有ベース CSS を差し込んで結合する。
///
/// 単一スタイルシート内で後方のルールが同一詳細度で優先されるため、パネル固有
/// ルール (末尾) が必要に応じてベース (先頭) を上書きする。
pub(crate) fn compose_panel_css(user_css: &str) -> String {
    format!("{PANEL_BASE_CSS}\n{user_css}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn builtin_panels_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("builtin-panels")
    }

    #[test]
    fn compose_prepends_base_and_keeps_user_css() {
        let composed = compose_panel_css(".color-preview { height: 40px; }");
        assert!(composed.contains(".panel"), "base rules must be present");
        assert!(
            composed.contains(".color-preview { height: 40px; }"),
            "user css must be preserved"
        );
        // ベースが先、ユーザーが後 (後勝ちで上書き可能)。
        let base_at = composed.find(".panel").unwrap();
        let user_at = composed.find(".color-preview").unwrap();
        assert!(base_at < user_at, "base must precede user css");
    }

    #[test]
    fn base_uses_canonical_palette_matching_presenter_theme() {
        // presenter theme.rs の APP_BACKGROUND (#181818) と一致。
        assert!(PANEL_BASE_CSS.contains("#181818"));
        // 旧ハードコード背景 (#181c24) は残さない。
        assert!(!PANEL_BASE_CSS.contains("#181c24"));
        // 枠線色は単一値へ統一され、旧 3 値のうち非採用の 2 値は base に無い。
        assert!(PANEL_BASE_CSS.contains("#2a3140"));
        assert!(!PANEL_BASE_CSS.contains("#434c5d"));
    }

    #[test]
    fn builtin_panel_meta_sizes_are_consistent() {
        // ADR 019 Phase 4: default_size == preset.size (preset が size を持つ場合)、
        // かつ default 幅は共有ベース CSS の min-width (240) 以上。
        const MIN_WIDTH: u64 = 240;
        let dir = builtin_panels_dir();
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("builtin-panels dir readable") {
            let entry = entry.expect("dir entry");
            if !entry.file_type().expect("file type").is_dir() {
                continue;
            }
            let meta_path = entry.path().join("panel.meta.json");
            let Ok(raw) = std::fs::read_to_string(&meta_path) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let v: serde_json::Value = serde_json::from_str(&raw).expect("valid meta json");
            let dw = v["default_size"]["width"].as_u64().expect("default width");
            let dh = v["default_size"]["height"].as_u64().expect("default height");
            if dw < MIN_WIDTH {
                offenders.push(format!("{name}: default width {dw} < min {MIN_WIDTH}"));
            }
            if let Some(size) = v.get("preset").and_then(|p| p.get("size")) {
                let pw = size["width"].as_u64().expect("preset width");
                let ph = size["height"].as_u64().expect("preset height");
                if (pw, ph) != (dw, dh) {
                    offenders.push(format!(
                        "{name}: preset {pw}x{ph} != default {dw}x{dh}"
                    ));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "builtin panel meta sizes must be consistent: {offenders:?}"
        );
    }

    #[test]
    fn builtin_panel_html_has_no_decorative_hr() {
        // 装飾目的の <hr> 区切りは撤廃 (セクション余白で区切る, ADR 019 Phase 3)。
        let dir = builtin_panels_dir();
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("builtin-panels dir readable") {
            let entry = entry.expect("dir entry");
            if !entry.file_type().expect("file type").is_dir() {
                continue;
            }
            let html_path = entry.path().join("panel.html");
            let Ok(html) = std::fs::read_to_string(&html_path) else {
                continue;
            };
            if html.contains("<hr") {
                offenders.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        assert!(
            offenders.is_empty(),
            "panel.html must not use decorative <hr>: {offenders:?}"
        );
    }

    #[test]
    fn builtin_panel_css_files_do_not_redefine_shared_blocks() {
        // 共通ブロック (.panel / .panel-title) は base の唯一定義。各 panel.css が
        // これらを再定義していないこと (テーマ重複ハードコードの排除を固定)。
        let dir = builtin_panels_dir();
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("builtin-panels dir readable") {
            let entry = entry.expect("dir entry");
            if !entry.file_type().expect("file type").is_dir() {
                continue;
            }
            let css_path = entry.path().join("panel.css");
            let Ok(css) = std::fs::read_to_string(&css_path) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if css.contains(".panel ") || css.contains(".panel{") || css.contains(".panel-title") {
                offenders.push(format!("{name}: redefines shared .panel/.panel-title block"));
            }
            if css.contains("#181c24") {
                offenders.push(format!("{name}: keeps old hardcoded background #181c24"));
            }
        }
        assert!(
            offenders.is_empty(),
            "panel.css must not duplicate shared theme blocks: {offenders:?}"
        );
    }
}
