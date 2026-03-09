# altpaint パネル Wasm Runtime 設計

## この文書の役割

この文書は、フェーズ6後半からフェーズ8にかけて導入する Wasm runtime 側の正本である。

対象は次である。

- Wasm モジュールの責務
- `wasmtime` 実行方針
- ABI の最小形
- DTO と command descriptor の境界
- Rust SDK / schema crate 方針
- plugin author が依存すべき API

UI 構文と `.altp-panel` の設計は [docs/panel-ui-definition/ui-dsl.md](docs/panel-ui-definition/ui-dsl.md) を正本とする。

## 2026-03-09 時点の実装状況

フェーズ6後半の最小 runtime 接続は実装済みである。

- `crates/panel-schema` / `crates/panel-sdk` / `crates/plugin-host` は追加済み
- `plugin-host` は `wasmtime` で sample Wasm/WAT module をロードできる
- `ui-shell` は handler 実行結果の state patch / command descriptor / diagnostics を反映できる
- 標準パネル6種は Rust SDK + Wasm 版へ移行済みである
- `crates/panel-macros` を追加し、plugin 作者は `#[panel_sdk::panel_init]` / `#[panel_sdk::panel_sync_host]` / `#[panel_sdk::panel_handler]` で安全に export を宣言できる
- `crates/panel-sdk` は `commands::*` と `state::*Key` を公開し、生の command 名や state path 文字列を Rust 側から追い出し始めている

ただし、現時点の ABI は**フェーズ6向けの最小実装**であり、将来の外部 plugin 公開用にそのまま固定する段階ではない。

- 現行 export は `panel_init` / `panel_sync_host` / `panel_handle_<handler_name>` 命名である
- host import を通じて state patch / command descriptor / diagnostics を収集している
- bytes DTO ベースの単一 `panel_handle_event` ABI は今後の安定化対象である

重要なのは、**ABI が暫定でも SDK は暫定でなくてよい** という点である。
plugin 作者に unsafe / `extern` / export 名を露出したままフェーズ9へ進むべきではない。

## 結論

処理側は次の方針で固定する。

- 処理は Rust などから Wasm へコンパイルする
- 実行は `wasmtime` を第一候補とする
- Wasm は host 内部型を直接触らない
- Wasm は `Command` ではなく command descriptor を返す
- plugin 作者には host 内部 crate ではなく SDK crate を使わせる
- host が権限、state 適用、`Command` 変換を握る

さらに、plugin 作者が日常的に書く Rust コードについて、次も固定する。

- handler export は SDK attribute macro で宣言する
- command 発行は型付き helper を優先する
- state 参照は typed key を優先する
- `unsafe` と `extern "C"` は SDK 実装へ閉じ込める

## Wasm の責務

Wasm は Rust 等からコンパイルする処理モジュールである。

持つ責務:

- handler 実装
- local state 更新ルール
- command descriptor 生成
- diagnostics 生成

持たない責務:

- UI 描画
- native widget 生成
- host 内部型の直接操作
- `wgpu` / OS / native widget 直接操作

## 実行基盤

第一候補は次である。

- target: `wasm32-wasip1`
- runtime: `wasmtime`

デフォルト権限:

- filesystem なし
- network なし
- env なし

## フェーズ6時点の最小 export

フェーズ6で実装した最小 export は次である。

- `panel_init`
- `panel_sync_host`
- `panel_handle_<handler_name>`

フェーズ6では、host が handler ごとの export を呼び出し、Wasm から host import を通じて次を収集する。

- state patch
- command descriptors
- diagnostics

`panel_dispose` と bytes DTO ベースの単一 event ABI は、今後の安定化対象として残している。

## 現状仕様への批判的整理

レビュワーのコメントを踏まえると、現状仕様には次の事実がある。

1. ABI の最小実装自体は妥当だが、そのまま plugin 作者へ露出してはいけない
2. `CommandDescriptor` と state path は境界で文字列化されるため、完全なコンパイル時検査はできない
3. それでも、Rust 側の authoring experience はかなり改善できる

このため、仕様としては次の 3 層を分けて扱う。

### 1. ABI 層

- host と Wasm runtime の低レイヤ境界
- ここでは `extern` / export 名 / import 関数 / DTO などの文字列化を許容する
- ただし、これは SDK 内部実装であり、plugin 作者の主戦場ではない

### 2. SDK 層

- plugin 作者が直接触る層
- 安全な Rust 関数、attribute macro、typed helper を置く
- 今回の改訂ではここを優先的に安定化する

### 3. DSL / manifest 層

- `.altp-panel` 上の handler binding や state 名宣言を置く層
- 別ファイルである都合上、名前文字列は残る
- validator や将来の codegen によって不整合を減らす

この整理により、**「文字列ベースの境界が存在する」ことと「plugin 作者が文字列だらけのコードを書く」ことを切り分ける**。

## 次の段階で実装する crate

次の段階では、まず Wasm runtime と DSL をつなぐ基盤 crate を追加する。

想定する最小構成:

- `crates/panel-dsl`
  - UI DSL parser / AST / validator / normalized IR
- `crates/panel-schema`
  - ABI DTO
  - state patch DTO
  - command descriptor DTO
  - diagnostics DTO
- `crates/panel-sdk`
  - Rust から Wasm handler を書くための helper
- `crates/plugin-host`
  - `wasmtime` 実行
  - Wasm module ロード
  - DTO encode / decode

この段階では、外部 plugin 公開より先に、**組み込み panel を将来この基盤へ移せること**を成功条件に置く。

## ABI 表

フェーズ6時点の実装 ABI は次である。

| Export                        | 入力                  | 出力                         | 役割                                            |
| ----------------------------- | --------------------- | ---------------------------- | ----------------------------------------------- |
| `panel_init`                  | なし                  | host import 経由の patch 群  | 初期 state の最小セットアップ                   |
| `panel_sync_host`             | なし                  | host import 経由の patch 群  | host snapshot を panel local state へ同期する   |
| `panel_handle_<handler_name>` | なし または `i32` 1つ | host import 経由の result 群 | UI event に応じて state patch / command を返却 |

将来の安定化候補 ABI は次である。

| Export               | 入力                 | 出力                                      | 役割                                     |
| -------------------- | -------------------- | ----------------------------------------- | ---------------------------------------- |
| `panel_init`         | 初期化 payload bytes | 初期 state bytes または diagnostics bytes | 初期 state と runtime 初期化             |
| `panel_handle_event` | event payload bytes  | handler result bytes                      | state patch と command descriptor の返却 |
| `panel_dispose`      | なしまたは空 bytes   | なし                                      | 後始末                                   |

### ABI 設計で固定すること

ビルトイン移植を前提に、次は最初から固定しておく。

- UI 側から見える handler 名は文字列で扱う
- Wasm 側は `Command` enum を知らない
- host は DTO を decode して `Command` へ変換する
- built-in / external のどちらも同じ handler result 形式を使う
- built-in だからという理由で別 ABI を作らない
- ただし plugin 作者には `panel_sync_host` や `panel_handle_<handler_name>` を直接書かせない

### 将来の `panel_handle_event` 入力 DTO

| フィールド       | 型       | 説明                               |
| ---------------- | -------- | ---------------------------------- |
| `handler_name`   | `string` | UI DSL 側で bind された handler 名 |
| `event_kind`     | `string` | `click` / `change` など            |
| `event_payload`  | object   | イベント固有 payload               |
| `state_snapshot` | object   | 現在の panel local state           |
| `host_snapshot`  | object   | host が渡す読み取り専用 snapshot   |

### 将来の `panel_handle_event` 出力 DTO

| フィールド    | 型    | 説明                          |
| ------------- | ----- | ----------------------------- |
| `state_patch` | array | local state に対する patch 列 |
| `commands`    | array | command descriptor 列         |
| `diagnostics` | array | エラー、警告、補足情報        |

ビルトイン移植を見据え、将来的には次の補助項目も許容できる形にしておく。

| フィールド   | 型    | 説明                                          |
| ------------ | ----- | --------------------------------------------- |
| `view_hints` | array | host が描画最適化やフォーカス補助に使うヒント |

### command descriptor DTO

| フィールド | 型       | 説明                  |
| ---------- | -------- | --------------------- |
| `name`     | `string` | 例: `tool.set_active` |
| `payload`  | object   | schema 準拠 payload   |

### state patch DTO

| フィールド | 型       | 説明               |
| ---------- | -------- | ------------------ |
| `op`       | `string` | `set` / `toggle`   |
| `path`     | `string` | 例: `selectedTool` |
| `value`    | any      | `set` 時の値       |

## イベント処理の流れ

1. host が `PanelEvent` を作る
2. UI 定義から handler 名を解決する
3. host が `panel_handle_<handler_name>` を呼ぶ
4. Wasm が state patch と command descriptors を返す
5. host が state を適用する
6. host が command descriptor を `Command` に変換する
7. host が panel を再評価し `PanelTree` を更新する

この流れは、次の次の段階でビルトイン panel を移植した後も変えない。

つまり `app-actions` や `tool-palette` も、最終的にはこの流れに乗せる。

## コマンド境界

Wasm は `Command` を直接返さない。
返すのは command descriptor である。

例:

- `tool.set_active` + `{ tool: "brush" }`
- `tool.set_color` + `{ color: "#1E88E5" }`
- `project.save` + `{}`

host がこれを `Command` に変換する。

### Rust SDK での command 構築方針

plugin 作者が次のようなコードを書くことは、今後は推奨しない。

```rust
command("tool.set_active").string("tool", "brush")
```

この API は escape hatch として残してよいが、第一選択肢は型付き helper とする。

```rust
panel_sdk::commands::tool::set_active(panel_sdk::commands::Tool::Brush)
panel_sdk::commands::project::save()
panel_sdk::commands::project::new_sized(320, 240)
```

これにより、少なくとも Rust 側では command 名・payload key・代表的な enum 値の typo をコンパイル時に減らせる。

## state patch 境界

Wasm は document 本体を直接更新しない。
返すのは panel ローカル state 用の patch だけである。

例:

- `StatePatch::set("selectedTool", "brush")`
- `StatePatch::toggle("showAdvanced")`
- `StatePatch::set_bool("expanded", true)`

重要なのは次である。

- document 正本は host 側が持つ
- Wasm は local state patch を返すだけ
- document 更新は `Command` を経由する

## SDK と schema の方針

Wasm を手で書かせる前提は採らない。
Rust などからコンパイルする前提で設計する。

### 重要な原則

plugin 作者には host 内部 crate を直接依存させない。

直接依存させないもの:

- `app-core`
- `ui-shell`
- `desktop`
- `plugin-host`

代わりに、安定した SDK を1枚挟む。

### 推奨 crate 構成

- workspace 上の想定 crate は次とする。
  - `crates/panel-schema`
  - `crates/panel-sdk`
  - `crates/panel-macros`
- 公開 package 名は必要なら次のように付けてもよい。
  - `altpaint-panel-schema`
  - `altpaint-panel-sdk`
  - `altpaint-panel-macros`

### plugin 作者が使う型

- `EventContext`
- `HostSnapshot`
- `StatePatch`
- `CommandDescriptor`
- `HandlerResult`

加えて、現時点で plugin 作者が日常的に使う表面 API は次である。

- `#[panel_sdk::panel_init]`
- `#[panel_sdk::panel_handler]`
- `panel_sdk::commands::*`
- `panel_sdk::host::*`
- `panel_sdk::state::*Key`
- `panel_sdk::runtime::{emit_command, state_i32, state_string, set_state_bool, set_state_i32, ...}`

### SDK が提供すべき helper

- `commands::tool::set_active(Tool::Brush)`
- `commands::tool::set_color_rgb(RgbColor::new(...))`
- `commands::project::save()`
- `commands::project::new_sized(width, height)`
- `host::tool::pen_name()`
- `host::document::title()`
- `StatePatch::set("selectedTool", "brush")`
- `StatePatch::toggle("showAdvanced")`
- `state::bool("show_new")`
- `state::string("new_width")`

重要なのは、plugin 作者が `.altp-panel` から `host.*` を直接読むのではなく、Wasm handler 内で `panel_sdk::host::*` を使って取得し、その値を local state へ mirror することだ。

escape hatch としては、必要に応じて従来の `command("...")` builder も残してよい。

ビルトイン移植後に必要な helper は、少なくとも次を含む。

- `.bool("value", true)`
- `.color("color", "#1E88E5")`
- `StatePatch::replace("selected_id", "layer-1")`

### SDK サンプル

```rust
use panel_sdk::{
  commands::{self, Tool},
  runtime::emit_command,
};

#[panel_sdk::panel_handler]
fn activate_brush() {
  emit_command(&commands::tool::set_active(Tool::Brush));
}
```

## ビルトイン移植を前提にした実装方針

その次の段階では、既存ビルトインプラグインを UI DSL + Wasm へ再構成する。

ここで重要なのは、built-in 専用の近道を増やしすぎないことだ。

守るべき方針:

- built-in も `.altp-panel` + Wasm module の組で持つ
- built-in も `panel-schema` / `panel-sdk` を使う
- built-in も handler result DTO を返す
- host だけが `Command` を適用する

### 移植順の推奨

1. `app-actions`
2. `tool-palette`
3. `color-palette`
4. `layers-panel`
5. `job-progress`
6. `snapshot-panel`

### 各 panel で最低限必要な Wasm 能力

| Panel            | 必要な能力                                |
| ---------------- | ----------------------------------------- |
| `app-actions`    | command descriptor 返却                   |
| `tool-palette`   | active tool に応じた state / command 切替 |
| `color-palette`  | color payload 返却                        |
| `layers-panel`   | host snapshot の配列参照と選択 command    |
| `job-progress`   | 読み取り専用 diagnostics / snapshot 表示  |
| `snapshot-panel` | 読み取り専用リスト表示                    |

## 依存ライブラリの許容方針

Rust から Wasm を作る以上、plugin 作者が crates.io のライブラリを使うこと自体は許容してよい。

ただし、MVP では次を制約として明文化した方がよい。

- `wasm32-wasip1` でビルド可能であること
- ネイティブ動的ライブラリ依存を持たないこと
- host 権限なしでは filesystem / network を前提にしないこと
- 重い計算を UI イベント同期処理へ持ち込まないこと

推奨依存の例:

- `serde`
- `thiserror`
- `smallvec`
- `indexmap`

慎重に扱うべき依存の例:

- スレッド前提 crate
- ネイティブ FFI 前提 crate
- 大きなランタイム依存
- OS ファイルシステムを当然視する crate

## validation と安全性

最低限必要な確認:

- `runtime.wasm` が存在する
- UI から参照される handler 名が解決できる
- Wasm が返しうる command descriptor が schema に一致する
- 権限宣言と host snapshot 参照が整合する

また、host 側では次を守る。

- capability をデフォルト deny にする
- エラー時は panel 全体を落とさず隔離する
- diagnostics を panel 表示へ出せるようにする

## フェーズ別の実装順

### フェーズ6後半

- `crates/panel-schema` / `crates/panel-sdk` / `crates/plugin-host` の最小追加
- `runtime { wasm: ... }` の導入
- handler binding
- `wasmtime` 実行の最小接続
- state patch 反映
- command descriptor 反映

この範囲は sample panel 動作まで含めて実装済みである。

### フェーズ7

- 既存ビルトイン panel の UI DSL + Wasm 移植
- built-in panel の command 経路検証
- host snapshot 参照の不足分補完
- 外部 plugin 化に向けた ABI 安定化

### フェーズ8

- 権限管理の本格化
- 外部 Wasm panel ロード
- エラー隔離
- SDK / schema の整備拡張

## MVP スコープ

MVP では次で十分である。

- runtime: `wasmtime`
- handler: `panel_init` と `panel_handle_<handler_name>`
- payload: まずは export 引数最小 + host import 経由
- state: local patch のみ
- command: descriptor 経由
- 権限: deny by default を設計原則とし、本格 enforcement は後続フェーズ
- SDK: Rust 向け最小版を1つ用意

さらに、次の次の段階の built-in 移植に向けて次を満たす。

- built-in panel が同一 ABI を使って動かせる
- built-in 専用 ABI を追加しない
- SDK だけで最初の 3 panel を記述できる

## 結論

Wasm 側は、**host 内部型から切り離した処理 runtime** として設計するのがよい。

- 実行は `wasmtime`
- 境界は DTO と command descriptor
- plugin 作者には SDK を経由させる
- state は patch で返す
- document 更新は host が `Command` 化して担う

この方針であれば、Rust からの Wasm 開発体験を確保しつつ、host 側の ABI と権限モデルも守りやすい。
