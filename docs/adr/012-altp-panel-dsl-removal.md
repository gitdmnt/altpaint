# ADR 012: `.altp-panel` DSL 撤去と HTML/CSS/Wasm-DOM 直接記述化 (Phase 10)

- 作業日時: 2026-05-03
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted (実装完了)

## コンテキスト

altpaint は Phase 9E (ADR 009) で DSL パネルを `dsl_to_html::translate_panel_tree` 経由で
HTML+CSS に翻訳し、Phase 9G (ADR 011) で `HtmlPanelEngine` を唯一の描画経路に統合した。
この時点で `.altp-panel` DSL は **HTML 翻訳の中間表現** にすぎず、DSL→HTML 翻訳器・
`PanelTree` DTO・DSL パーサがコードベースに残ったまま機能のない肥大要因となっていた。

Phase 10 では DSL を完全撤去し、各パネルを **`panel.html` + `panel.css` + Wasm ハンドラ**
で直接記述する形に移行する。Wasm 経路 (wasmtime + plugin-host + plugin-sdk + plugin-macros +
panel-schema) は維持し、Wasm の役割を「`PanelTree` を返す」から「Blitz DOM を直接 mutate する」
に転換する。サードパーティ Wasm パネル拡張点も温存される。

## 決定

| 項目 | 決定 |
|---|---|
| 移行方式 | **一括スイッチ**: 全 11 パネルを単一フェーズで HTML+CSS+DOM mutation 経路へ移植 + panel-dsl 即時削除 |
| 同梱パネル集約先 | `crates/builtin-panels/` (新クレート) |
| HTML/CSS テンプレート保持 | 外部ファイル `panel.html` / `panel.css` を起動時にランタイム読込 |
| Wasm の state→UI 反映 | Wasm が `dom` host module 経由で Blitz `HtmlDocument` を直接 mutate |
| DSL 翻訳器 | 削除 (`crates/panel-runtime/src/dsl_to_html.rs`) |
| `PanelTree` DTO | 削除 (`crates/panel-api/src/lib.rs`) |
| `panel-dsl` クレート | 削除 |
| `panel-schema::HandlerResult::panel_tree` フィールド | 削除 |
| `plugin-host` / `plugin-sdk` / `plugin-macros` / `panel-schema` (panel_tree 以外) / `wasmtime` | 維持 |
| `plugins/` ディレクトリ | 全削除 |
| 並行稼働期間 | **ゼロ** (旧 DSL 経路を残さない) |

## DOM mutation API 設計

Blitz `DocumentMutator` / `BaseDocument` の API を **同名・同シグネチャで Wasm に公開**する。
合成 API (`set_text` / `add_class` / `clear_children` 等) は提供しない。著者は Blitz primitive を
直接組み合わせて HTML/CSS を mutate する。

### plugin-host 側 host functions (`crates/plugin-host/src/dom_api.rs`)

- `dom.query_selector(selector_ptr, selector_len) -> u64`
- `dom.query_selector_all(selector_ptr, selector_len) -> u64` (iter handle)
- `dom.iter_next(handle) -> u64`
- `dom.iter_drop(handle)`
- `dom.get_attribute_len(node_id, name_ptr, name_len) -> i32`
- `dom.get_attribute_copy(node_id, name_ptr, name_len, buf_ptr, buf_cap) -> i32`
- `dom.set_attribute(node_id, name_ptr, name_len, value_ptr, value_len)`
- `dom.clear_attribute(node_id, name_ptr, name_len)`
- `dom.create_text_node(text_ptr, text_len) -> u64`
- `dom.append_children(parent_id, children_ptr, count)`
- `dom.remove_and_drop_all_children(node_id)`
- `dom.set_inner_html(node_id, html_ptr, html_len)`

NodeId は host 側 NodeId+1 を i64 として渡す (0 = None)。
DOM context は `WasmPanelRuntime::call_with_dom(&mut HtmlDocument, |rt| ...)` のスコープでのみ
有効。スコープ外で host function を呼ぶと diagnostic エラー。

### plugin-sdk 側 (`crates/plugin-sdk/src/dom.rs`)

Blitz と同名の free function 群 + `html_escape`:

```rust
pub fn query_selector(selector: &str) -> Option<NodeId>;
pub fn query_selector_all(selector: &str) -> impl Iterator<Item = NodeId>;
pub fn get_attribute(node: NodeId, name: &str) -> Option<String>;
pub fn set_attribute(node: NodeId, name: &str, value: &str);
pub fn clear_attribute(node: NodeId, name: &str);
pub fn create_text_node(text: &str) -> NodeId;
pub fn append_children(parent: NodeId, children: &[NodeId]);
pub fn remove_and_drop_all_children(node: NodeId);
pub fn set_inner_html(node: NodeId, html: &str);
pub fn html_escape(input: &str) -> String;
```

### 信頼境界

`set_inner_html` は Wasm 内文字列を Blitz の HTML パーサに通す。host snapshot 由来文字列を
埋め込む場合は **必ず `html_escape` を経由する** ことを著者規約とし、各パネルの単体テストで
XSS 境界を検証する。

## 着手済み (foundation)

2026-05-03 着地時点で:

- `crates/builtin-panels/` クレート新設、workspace member 登録
- `crates/plugin-host/src/dom_api.rs` 12 host function 実装
- `crates/plugin-host/src/lib.rs::WasmPanelRuntime::call_with_dom` / `panel_init` 追加
- `crates/plugin-sdk/src/dom.rs` Wasm ABI シム + `html_escape` 公開
- `crates/panel-html-experiment/src/engine.rs` に `document_mut` / `mark_mutated` 公開、
  `HtmlProvider` 自動 install (set_inner_html を Wasm から動かすため)
- `BuiltinPanelPlugin` (`builtin-panels::builtin_plugin`) の PanelPlugin 実装スケルトン
- `crates/plugin-host/tests/dom_api.rs` で `set_attribute` / `set_inner_html` を Wasm から呼ぶ統合テスト
- `crates/plugin-sdk` の `html_escape` 単体テスト (XSS 境界含む)

## 残作業

### パネル移植 (依存少→多 順、各パネル 1 コミット)

1. view-controls
2. job-progress
3. panel-list
4. tool-palette
5. snapshot-panel
6. pen-settings
7. workspace-presets
8. app-actions
9. layers-panel
10. text-flow
11. color-palette

各コミット内容:

- `crates/builtin-panels/<name>/{panel.html, panel.css, panel.meta.json, Cargo.toml, src/lib.rs}` 新設
- `plugins/<name>/` 削除
- workspace `Cargo.toml` の members から `plugins/<name>` 除外、`crates/builtin-panels/<name>` 追加
- `register_builtin_panels` に追加
- handler 単体テスト + DOM 状態スナップショットテスト

### DSL 経路削除 (11 パネル全て移植完了後の同一 PR 内)

- `crates/panel-dsl/` 全体削除
- `crates/panel-runtime/src/{dsl_panel,dsl_loader,dsl_to_html}.rs` 削除
- `crates/panel-runtime/src/host_sync.rs` の DSL 専用部分削除
- `crates/panel-runtime/src/html_panel.rs` の `apply_bindings` 経路削除
- `panel-api::PanelTree` / `PanelNode` / `PanelTreeBuilder` 削除
- `panel-schema::HandlerResult::panel_tree` フィールド削除
- `panel-html-experiment::AltpKind` / `altp:` プレフィックス parse 経路削除
- `panel-html-experiment::apply_bindings` (data-bind-* 経路) 削除
- workspace `Cargo.toml` の members から `panel-dsl` 除外
- `apps/desktop/src/app/bootstrap.rs` の `load_panel_directory` 呼出を `register_builtin_panels` に置換
- `apps/desktop/src/app/panel_dispatch.rs` の DSL 関連分岐削除
- `scripts/build-ui-wasm.{sh,ps1}` を `crates/builtin-panels/<name>/` パスに更新
- `plugins/` ディレクトリ削除

### 検証

- workspace 内 `panel_tree\|PanelTree\|altp:\|data-bind` のヒットなし
- `cargo build --workspace` / `cargo test --workspace` / `cargo clippy --workspace --all-targets` 通過
- `cargo run -p desktop` で全 11 パネルが起動・操作可能
- XSS 境界テスト (layer 名に `<script>` 等を含めて HTML 注入が起きない)

## 関連 ADR / 文書

- ADR 007: HTML panel experiment (`html-panel` feature 導入)
- ADR 008: HTML panel dynamic size and engine consolidation
- ADR 009: DSL → HTML 翻訳器採用 (Phase 10 で **superseded**)
- ADR 010: `crates/render` クレート物理削除 (Phase 9F)
- ADR 011: `html-panel` feature gate 完全撤去 (Phase 9G)
- `docs/IMPLEMENTATION_STATUS.md` Phase 10 完了記録 (移植完了時に追加)

## 代替案と却下理由

- **漸進移行 (10A→10B→10C 並行稼働)**: 後方互換コードが各フェーズで残存し alpha 期方針に反する。**却下**
- **DSL を維持し HTML 翻訳器のみ削除**: 中間表現が残り肥大化。
- **`data-bind-*` 経路に統一**: 動的リスト (layer / preset) の表現に独自テンプレート言語が必要、複雑化。

## トレードオフ

- 失う: DSL の宣言的記述、DSL→HTML 自動翻訳
- 得る: 中間表現除去、HTML/CSS による直接著述、DOM mutation の表現力 (動的リスト等が素直に書ける)
- 著者は Blitz `DocumentMutator` を直接学ぶ必要がある。合成 API を持たないため Wasm 側で
  HTML 文字列組立 + `set_inner_html` を多用するパターンに収束する見込み

## Post-Acceptance Note (2026-05-15)

本 ADR の Decision で「`PanelTree` DTO 削除」「`PanelNode` 削除」を宣言したが、Phase 10 着地時点では
`crates/panel-api/src/lib.rs` 内に `PanelTree` / `PanelNode` / `PanelView` 型と
`PanelPlugin::panel_tree()` / `view()` のデフォルト実装が**残存**していた。`builtin.workspace-layout`
パネル (パネル表示/非表示管理 UI) が Rust ネイティブ実装 (`crates/ui-shell/src/workspace.rs`
`workspace_manager_tree()`) で構築する `PanelTree` を介して GPU 描画する経路を通っていたためである。

この乖離は ADR 014 で正式に解消する: workspace-layout を 12 番目の HTML パネルとして実装し直し、
PanelTree / PanelNode / PanelView と関連 trait method、`tree_query`、focus 経路の dropdown /
text_input 走査、`TextInputEditorState` を一括撤去する。
