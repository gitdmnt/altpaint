# パネル開発ガイド

## この文書の目的

この文書は、`altpaint` のパネル（UI プラグイン）をどこで、どう作り、どうデバッグするかをまとめた実務向けガイドです。

対象は次です。

- 組み込みパネルの配置場所
- Rust SDK（`panel-sdk`）を使った Wasm ハンドラ実装
- パネルアセット（`panel.html` / `panel.css` / `panel.meta.json`）の書き方
- デバッグ用 Wasm の生成方法
- clone 直後のセットアップ

> 用語: 「panel」は UI、「plugin」は将来の外部拡張のための予約語です。現状の組み込み
> パネルはホスト同梱であり、外部 plugin ロードはまだ実装していません。

## 開発場所

組み込みパネルの正規配置は `crates/builtin-panels/<name>/` です。

各パネルは独立クレートを持ち、次を同居させます。

- `Cargo.toml`
- `src/lib.rs`（Rust SDK ベースの Wasm ハンドラ）
- `panel.html`（UI 構造。Blitz が解釈する HTML）
- `panel.css`（スタイル）
- `panel.meta.json`（id / title / default_size / preset 配置）
- 生成物の `.wasm`（`builtin_panel_<name>.wasm`）

現在の 12 パネル:

- `crates/builtin-panels/app-actions/`
- `crates/builtin-panels/color-palette/`
- `crates/builtin-panels/job-progress/`
- `crates/builtin-panels/koma-list/`
- `crates/builtin-panels/layers/`
- `crates/builtin-panels/snapshots/`
- `crates/builtin-panels/text-flow/`
- `crates/builtin-panels/tool-palette/`
- `crates/builtin-panels/tool-settings/`
- `crates/builtin-panels/view-controls/`
- `crates/builtin-panels/workspace-layout/`
- `crates/builtin-panels/workspace-presets/`

## パネルのロード経路

`panel-runtime` の loader（`crates/panel-runtime/src/loader.rs`）が `crates/builtin-panels/<name>/`
配下の `panel.meta.json` / `panel.html` / `panel.css` / `<wasm>` を順に読みます。

- UI 構造: `panel.html`（HTML。`panel-html::HtmlPanelView` が Blitz + parley + vello で GPU 直描画）
- スタイル: `panel.css`
- メタ情報: `panel.meta.json`（id / title / default_size / preset 配置）
- 処理実装: Rust SDK（`panel-sdk`）でビルドした `.wasm`（`panel-wasm-host` の wasmtime が実行）
- ロード単位: クレートフォルダ単位

> 旧 `.altp-panel` DSL は廃止済みです。UI は素の HTML/CSS で書きます。

## フォルダ構成

最小構成は次です。

```text
crates/builtin-panels/my-panel/
  Cargo.toml
  panel.meta.json
  panel.html
  panel.css
  src/
    lib.rs
  builtin_panel_my_panel.wasm
```

## Rust SDK の使い方

Wasm 側は `crates/panel-sdk` を使って実装します。

パネル作者向けの正面入口は `panel-sdk` のみです。`panel-macros`（proc-macro）、
`serde` / `serde_json`、`panel-protocol` の必要面（`RequestDescriptor` / `host_state` /
`names` / `keyboard`）はすべて `panel-sdk` から再公開されるため、これら個別クレートを
直接依存に加える必要はありません。

`Cargo.toml`:

```toml
[package]
name = "builtin-panel-my-panel"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
panel-sdk = { path = "../../panel-sdk" }
```

最小例（`src/lib.rs`）:

```rust
use panel_sdk::{dom::set_text, runtime::emit_request, services};

#[panel_sdk::panel_init]
fn init() {
    set_text("#status", "ready");
}

#[panel_sdk::panel_handler]
fn save_project() {
    // request 発行は単一 API `emit_request`。command / service の区別は
    // host 側 translator registry が静的に振り分ける。
    emit_request(&services::project_io::save_current());
}
```

## `panel.meta.json` の最小例

```json
{
  "id": "builtin.example",
  "title": "Example",
  "default_size": { "width": 240, "height": 320 },
  "preset": {
    "anchor": "top-left",
    "position": { "x": 356, "y": 72 },
    "size": { "width": 320, "height": 240 }
  }
}
```

## `panel.html` の最小例

DOM の `id` で Wasm からの書換え対象を特定し、`data-action="altp:<kind>:<handler>"`
属性でイベントとハンドラ名を結びます。

- `altp:activate:<handler>` — クリック等のアクティベート
- `altp:input:<handler>` — テキスト/数値入力（payload `value` を運ぶ）
- `altp:select:<handler>` — セレクト変更（payload `value` を運ぶ）

```html
<div class="panel">
  <header class="panel-title">Example</header>
  <div class="alt-section">
    <div class="alt-section-title">Example</div>
    <button class="btn" id="example.save" data-action="altp:activate:save_project">Save</button>
    <span id="status"></span>
  </div>
</div>
```

`panel.css` でスタイルを当てます（クラス名は他パネルと揃えると見た目が一貫します）。

## clone 直後のセットアップ

`.wasm` は生成物なので git 管理しません。

clone 後は次を実行します。

```powershell
.\scripts\build-ui-wasm.ps1
```

```bash
bash scripts/build-ui-wasm.sh
```

release 生成したい場合:

```powershell
.\scripts\build-ui-wasm.ps1 -Release
```

このスクリプトは次を行います。

1. `wasm32-unknown-unknown` ターゲットを確認
2. `crates/builtin-panels/` 配下の 12 パネルクレートを Wasm ビルド
3. 各パネルフォルダへ `builtin_panel_<name>.wasm` を配置

## デバッグ起動の流れ

通常の開発手順は次です。

1. `crates/builtin-panels/<name>/src/lib.rs` または `panel.html` / `panel.css` を編集
2. `.\scripts\build-ui-wasm.ps1` を実行
3. `cargo run -p altpaint-desktop` で起動

必要なら先に確認:

- `cargo test -p builtin-panel-<name>`
- `cargo test -p panel-runtime`
- `cargo test -p altpaint-desktop`
- `cargo clippy --workspace --all-targets`

## ハンドラ命名規則

`data-action="altp:activate:save_project"` と書いた場合、`#[panel_sdk::panel_handler]`
を付けた `fn save_project()` が呼ばれます。proc-macro が ABI export wrapper
（`panel_handle_save_project`）を生成しつつ、元の Rust 関数も呼び出し可能なまま残すため、
native テストからは関数を直接呼べます。

- 引数なし: `fn save_project()`
- payload 付き: typed payload を引数に取る（例: `fn edit_new_width(payload: TextValue)`）。
  payload は `panel_sdk::serde::Deserialize` を derive した構造体で受けます（BL-141）。

## request 発行（command / service の単一化）

Wasm 側は `Command` enum を直接知らず、`RequestDescriptor` を `emit_request` で発行します。
command / service の区別は host 側 translator registry が静的に振り分けます。

```rust
use panel_sdk::{runtime::emit_request, services};

emit_request(&services::project_io::save_current());
emit_request(&services::history::undo());
```

`panel_sdk::services::*` は型付きの `RequestDescriptor` ビルダ群です。request 名定数は
`panel_sdk::names::*`（ホスト/パネル共有）にあり、ハードコードしません。

## DOM 書換えヘルパ（`panel_sdk::dom`）

ホスト state や local state を DOM へ反映する水平ヘルパを使います（HTML escape 内蔵）。

- `set_text(selector, text)` / `set_visible(selector, bool)`
- `set_button_active(selector, bool)` / `set_attribute(node, name, value)`
- `set_inner_html(node, html)` / `query_selector(selector) -> Option<Node>`
- `render_options(options, selected)` — `<option>` 列を生成
- `parse_option_list(json, key_field, label_field)` — 構造化 JSON 配列を `(key, label)` へ

## local state（`panel_sdk::state` / `runtime`）

パネルローカル state はキーを型付き定数で宣言します。キーは 2 層化されています（BL-145）。

- `session.*` — セッション内一時状態
- `config.*` — 永続化対象の設定

```rust
use panel_sdk::{
    runtime::{set_state_bool, set_state_string, state_bool, state_string, StatePatchBuffer},
    state,
};

const SHOW_NEW: state::BoolKey = state::bool("show_new");
const NEW_WIDTH: state::StringKey = state::string("new_width");
const NEW_SHORTCUT: state::StringKey = state::string("config.new_shortcut");

set_state_bool(SHOW_NEW, true);
let w = state_string(NEW_WIDTH);

// 複数キーをまとめて反映するなら StatePatchBuffer
let mut batch = StatePatchBuffer::new();
batch.set_string(NEW_WIDTH.as_ref(), "320".to_string());
batch.apply();
```

## host state の読み取りと再描画（`host_section` + `panel_on_host_change`）

ドキュメント本体は host が持ちます。host の現在値は `panel.html` から直接読まず、
`panel_sdk::host_state::*` の typed セクション DTO（`ToolState` / `LayerState` /
`ColorState` / `ViewState` / `DocumentState` / `KomaState` / `SnapshotState` /
`WorkspaceState` / `HistoryState` / `JobsState` 等。BL-142）で取得します。

host state が変化したら `#[panel_sdk::panel_on_host_change]` を付けた lifecycle 関数が
呼ばれるので、そこで typed DTO を読み直して `state.*` と DOM へ反映します（BL-146）。

```rust
use panel_sdk::{
    host_state::{section, ToolState},
    runtime::host_section,
};

fn host_tool() -> Option<ToolState> {
    host_section::<ToolState>(section::TOOL)
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}
```

- `host_section::<T>(section::KEY)` で typed セクションを取得（`section::*` がキー定数）
- `on_host_change` は予約済み lifecycle 名なので、`data-action` には bind しません

## ショートカットレジストリ（`panel_sdk::shortcut`）

「capture → 割当 → マッチ」のショートカット状態機械は SDK の `ShortcutRegistry` に
集約されています（BL-144）。純粋なインメモリ状態機械で、バインディングはパネルの
`config.*` state へ保存し、起動時に復元します。文字列比較規約は
`panel_sdk::keyboard` に従います（独自ハードコード禁止）。

```rust
use panel_sdk::shortcut::{Outcome, ShortcutRegistry};

let mut registry = ShortcutRegistry::new();
registry.define("save", state_string(SAVE_SHORTCUT));
match registry.handle_key(&shortcut) {
    Outcome::Assigned { slot, shortcut } => { /* config へ保存 */ }
    Outcome::Triggered { slot } => { /* slot のアクション実行 */ }
    Outcome::Ignored => {}
}
```

## エントリポイントのスモークテスト（`assert_entrypoints!`）

12 パネルに同型コピペされていた「init / on_host_change / 各 handler を native で
ひと通り呼ぶ」スモークテストは宣言マクロ `assert_entrypoints!` に畳まれています（BL-150）。
各 call は entrypoint への完全な呼び出し式（引数込み）を書きます。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        save_project(),
        edit_new_width(TextValue { value: "320".to_string() }),
        keyboard(KeyEvent::default()),
    });
}
```

## 実装上の注意

- Wasm 側は `Command` enum を直接知らず、`RequestDescriptor` を `emit_request` で発行します
- ドキュメント本体は host が持ち、Wasm は local state（`session.*` / `config.*`）と
  request 発行だけを行います
- host の現在値は `panel.html` から直接読まず、`panel_sdk::host_state::*` の typed
  セクション DTO を `host_section` で取得し、`panel_on_host_change` から反映します
- `on_host_change` / `init` は予約済み lifecycle 名なので `data-action` には bind しません
- `.wasm` を直接編集せず、必ず Rust ソースから再生成します

## 新しい組み込みパネルを足す手順

1. `crates/builtin-panels/<panel-name>/` を作る
2. `Cargo.toml` を追加する（`crate-type = ["cdylib", "rlib"]`、依存は `panel-sdk` のみ）
3. `src/lib.rs` に Rust SDK ベースの handler を書く
4. `panel.html` / `panel.css` / `panel.meta.json` を書く
5. ルート `Cargo.toml` の workspace `members` に追加する
6. `scripts/build-ui-wasm.ps1` の `$panelPackages`（および `.sh` 版）にエントリを追加する
7. `.\scripts\build-ui-wasm.ps1` を実行する
8. `cargo run -p altpaint-desktop` で表示確認する
9. `assert_entrypoints!` テストと文書を更新する

## 関連ファイル

- `crates/builtin-panels/`
- `crates/panel-sdk/src/lib.rs`（SDK 正面入口）
- `crates/panel-runtime/src/loader.rs`（同梱パネル loader）
- `crates/panel-wasm-host/src/lib.rs`（wasmtime 実行器 + DOM mutation host functions）
- `crates/panel-html/`（`HtmlPanelView`）
- `crates/panel-workspace/`（レイアウト・focus・hit-test）
- `scripts/build-ui-wasm.ps1` / `scripts/build-ui-wasm.sh`
</content>
