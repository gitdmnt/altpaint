# プラグイン開発ガイド

## この文書の目的

この文書は、`altpaint` のパネル系プラグインをどこで、どう作り、どうデバッグするかをまとめた実務向けガイドです。

対象は次です。

- 組み込みパネルの配置場所
- Rust SDK を使った Wasm ハンドラ実装
- `.altp-panel` の書き方
- デバッグ用 Wasm の生成方法
- clone 直後のセットアップ

## 開発場所

プラグイン開発用の正規配置は `plugins/` です。

各プラグインは独立フォルダを持ち、次を同居させます。

- `Cargo.toml`
- `src/lib.rs`
- `panel.altp-panel`
- 生成物の `.wasm`

例:

- `plugins/app-actions/`
- `plugins/tool-palette/`
- `plugins/color-palette/`
- `plugins/layers/`
- `plugins/job-progress/`
- `plugins/snapshots/`
- `tools/experimental/phase6-sample/`

## なぜ `plugins/` に置くのか

`apps/desktop/` はホストアプリ本体です。

一方、パネルは将来的に built-in と external を近づけたいので、開発時点からホスト本体の外へ寄せます。

このため、組み込みパネルも `plugins/` に置き、次の形を揃えます。

- UI 定義: `.altp-panel`
- 処理実装: Rust SDK + Wasm
- ロード単位: フォルダ単位

## フォルダ構成

最小構成は次です。

```text
plugins/my-panel/
  Cargo.toml
  panel.altp-panel
  src/
    lib.rs
  my_panel.wasm
```

`ui-shell` は `plugins/` を再帰探索し、見つけた `.altp-panel` をロードします。

## Rust SDK の使い方

Wasm 側は `crates/plugin-sdk` を使って実装します。

plugin 作者向けの正面入口は `plugin-sdk` のみで、`plugin-macros` も `plugin-sdk` から再 export されます。

依存は次です。

```toml
[dependencies]
plugin-sdk = { path = "../../crates/plugin-sdk" }
```

最小例:

```rust
use panel_sdk::{runtime::emit_request, services};

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_handler]
fn save_project() {
  // B9 P27: request 発行は単一 API `emit_request`。command/service の区別は
  // host 側 translator registry が静的に振り分ける。
  emit_request(&services::project_io::save_current());
}
```

## `.altp-panel` の最小例

```text
panel {
  id: "builtin.example"
  title: "Example"
  version: 1
}

permissions {
  read.document
  write.command
}

runtime {
  wasm: "example.wasm"
}

state {
}

view {
  <column gap=8 padding=8>
    <section title="Example">
      <button id="example.save" on:click="save_project">Save</button>
    </section>
  </column>
}
```

`runtime.wasm` は同じフォルダ内の Wasm ファイル名を指します。

## clone 直後のセットアップ

`.wasm` は生成物なので git 管理しません。

clone 後は次を実行します。

```powershell
./scripts/build-ui-wasm.ps1
```

release 生成したい場合:

```powershell
./scripts/build-ui-wasm.ps1 -Release
```

このスクリプトは次を行います。

1. `wasm32-unknown-unknown` ターゲットを確認
2. `plugins/` 配下の組み込みパネル crate を Wasm ビルド
3. 各プラグインフォルダへ `.wasm` を配置
4. 実験用 DSL/WAT sample は `tools/experimental/phase6-sample/` に保持する

## デバッグ起動の流れ

通常の開発手順は次です。

1. `plugins/<name>/src/lib.rs` または `panel.altp-panel` を編集
2. `./scripts/build-ui-wasm.ps1` を実行
3. `cargo run` で起動

必要なら先に確認:

- `cargo test -p panel-workspace`
- `cargo test -p altpaint-desktop`
- `cargo clippy --workspace --all-targets`

## ハンドラ命名規則


`.altp-panel` で `on:click="save_project"` と書いた場合、Wasm 側では次の export が生成されます。

- `panel_handle_save_project`

`on:change="set_red"` の場合は次です。

- `panel_handle_set_red`

## 現在使える主な runtime helper

`panel-sdk::runtime` では少なくとも次を使えます（B9 BL-147 で `abi` / `state` / `events` / `diagnostics` に分割済み）。

- `emit_request(&RequestDescriptor)`（request 発行の単一 API。B9 P27。旧 `emit_command` / `emit_service` / `emit_*_descriptor` は撤去）
- `set_state_bool(...)` / `set_state_string(...)` / `state_bool(...)` / `state_string(...)`（state キーは 2 層化 `session.*` / `config.*`。B9 BL-145）
- `info(...)` / `warn(...)` / `error(...)`

DOM 書換えは `panel-sdk::dom` の水平ヘルパ（B9 BL-143）を使います。

- `set_text(selector, text)` / `set_visible(selector, bool)` / `set_button_active(selector, bool)` / `set_slider(selector, value, display_selector)`
- `render_options(options, selected)` / `render_action_list(items)`（いずれも HTML escape 内蔵）

host state 変化時の再描画 lifecycle として次も使えます。

- `#[panel_sdk::panel_on_host_change]`（B9 P26 / BL-146: 旧 `#[plugin_sdk::panel_sync_host]`）。host 値は `.altp-panel` から直接読まず `panel_sdk::host_state::*` の typed セクション DTO（`ToolState` / `LayerState` 等。B9 BL-142）で取得して `state.*` へ反映します

## 現在使える主な UI ノード

現時点で実装済みの代表例:

- `column`
- `row`
- `section`
- `text`
- `button`
- `toggle`
- `slider`
- `color-preview`
- `when`
- `separator`
- `spacer`

## 実装上の注意

- Wasm 側は `Command` enum を直接知らず、`RequestDescriptor` を `emit_request` で発行します
- ドキュメント本体は host が持ち、Wasm は local state と request 発行だけを行います
- host の現在値は `.altp-panel` から直接読まず、`panel_sdk::host_state::*` の typed セクション DTO（B9 BL-142）で取得して `#[panel_sdk::panel_on_host_change] fn on_host_change()`（B9 P26）から `state.*` へ反映します
- `on_host_change` は予約済み lifecycle 名なので、`.altp-panel` の `on:click` / `on:change` には bind しません
- `.wasm` を直接編集せず、必ず Rust ソースか `.wat` から再生成します

## 新しい組み込みパネルを足す手順

1. `plugins/<panel-name>/` を作る
2. `Cargo.toml` を追加する
3. `src/lib.rs` に Rust SDK ベースの handler を書く
4. `panel.altp-panel` を書く
5. 必要なら `Cargo.toml` の workspace member に含まれていることを確認する
6. `./scripts/build-ui-wasm.ps1` を実行する
7. `cargo run` で表示確認する
8. テストと文書を更新する

## 関連ファイル

- `plugins/`
- `scripts/build-ui-wasm.ps1`
- `crates/plugin-sdk/src/lib.rs`
- `crates/plugin-host/src/lib.rs`
- `crates/ui-shell/src/lib.rs`
- `docs/panel-ui-definition/ui-dsl.md`
- `docs/panel-ui-definition/wasm-runtime.md`
