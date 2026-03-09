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
- `plugins/layers-panel/`
- `plugins/job-progress/`
- `plugins/snapshot-panel/`
- `plugins/phase6-sample/`

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

Wasm 側は `crates/panel-sdk` を使って実装します。

依存は次です。

```toml
[dependencies]
panel-sdk = { path = "../../crates/panel-sdk" }
```

最小例:

```rust
use panel_sdk::{command, runtime::emit_command_descriptor};

#[unsafe(no_mangle)]
pub extern "C" fn panel_init() {}

#[unsafe(no_mangle)]
pub extern "C" fn panel_handle_save_project() {
    emit_command_descriptor(&command("project.save").build());
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
4. `plugins/phase6-sample/phase6-sample.wat` から `phase6-sample.wasm` を生成

## デバッグ起動の流れ

通常の開発手順は次です。

1. `plugins/<name>/src/lib.rs` または `panel.altp-panel` を編集
2. `./scripts/build-ui-wasm.ps1` を実行
3. `cargo run` で起動

必要なら先に確認:

- `cargo test -p ui-shell`
- `cargo test -p desktop`
- `cargo clippy --workspace --all-targets`

## ハンドラ命名規則

`.altp-panel` で `on:click="save_project"` と書いた場合、Wasm 側では次の export が必要です。

- `panel_handle_save_project`

`on:change="set_red"` の場合は次です。

- `panel_handle_set_red`

## 現在使える主な runtime helper

`panel-sdk::runtime` では少なくとも次を使えます。

- `emit_command_descriptor(...)`
- `toggle_state(...)`
- `set_state_bool(...)`
- `state_i32(...)`
- `info(...)`
- `warn(...)`
- `error(...)`

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

- Wasm 側は `Command` enum を直接知らず、command descriptor を返します
- ドキュメント本体は host が持ち、Wasm は local state と command 発行だけを行います
- `state` の既定値に `{host.*}` を使うと、host snapshot 由来の値で同期できます
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
- `crates/panel-sdk/src/lib.rs`
- `crates/plugin-host/src/lib.rs`
- `crates/ui-shell/src/lib.rs`
- `docs/panel-ui-definition/ui-dsl.md`
- `docs/panel-ui-definition/wasm-runtime.md`
