# altpaint パネル Wasm Runtime 設計

## この文書の役割

この文書は、フェーズ6後半からフェーズ7にかけて導入する Wasm runtime 側の正本である。

対象は次である。

- Wasm モジュールの責務
- `wasmtime` 実行方針
- ABI の最小形
- DTO と command descriptor の境界
- Rust SDK / schema crate 方針
- plugin author が依存すべき API

UI 構文と `.altp-panel` の設計は [docs/panel-ui-definition/ui-dsl.md](docs/panel-ui-definition/ui-dsl.md) を正本とする。

## 結論

処理側は次の方針で固定する。

- 処理は Rust などから Wasm へコンパイルする
- 実行は `wasmtime` を第一候補とする
- Wasm は host 内部型を直接触らない
- Wasm は `Command` ではなく command descriptor を返す
- plugin 作者には host 内部 crate ではなく SDK crate を使わせる
- host が権限、state 適用、`Command` 変換を握る

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

## 最小 export

MVP では export を少数に絞る。

- `panel_init`
- `panel_handle_event`
- `panel_dispose`

入出力は、まずはシリアライズ済み bytes で十分である。

入力:

- handler 名
- event payload
- current state snapshot
- host snapshot

出力:

- state patch
- command descriptors
- diagnostics

内部形式は最初は JSON でもよいが、DTO は host から分離しておく。

## ABI 表

MVP の ABI は次を最小形とする。

| Export               | 入力                 | 出力                                      | 役割                                     |
| -------------------- | -------------------- | ----------------------------------------- | ---------------------------------------- |
| `panel_init`         | 初期化 payload bytes | 初期 state bytes または diagnostics bytes | 初期 state と runtime 初期化             |
| `panel_handle_event` | event payload bytes  | handler result bytes                      | state patch と command descriptor の返却 |
| `panel_dispose`      | なしまたは空 bytes   | なし                                      | 後始末                                   |

### `panel_handle_event` 入力 DTO

| フィールド       | 型       | 説明                               |
| ---------------- | -------- | ---------------------------------- |
| `handler_name`   | `string` | UI DSL 側で bind された handler 名 |
| `event_kind`     | `string` | `click` / `change` など            |
| `event_payload`  | object   | イベント固有 payload               |
| `state_snapshot` | object   | 現在の panel local state           |
| `host_snapshot`  | object   | host が渡す読み取り専用 snapshot   |

### `panel_handle_event` 出力 DTO

| フィールド    | 型    | 説明                          |
| ------------- | ----- | ----------------------------- |
| `state_patch` | array | local state に対する patch 列 |
| `commands`    | array | command descriptor 列         |
| `diagnostics` | array | エラー、警告、補足情報        |

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
3. host が `panel_handle_event` を呼ぶ
4. Wasm が state patch と command descriptors を返す
5. host が state を適用する
6. host が command descriptor を `Command` に変換する
7. host が panel を再評価し `PanelTree` を更新する

## コマンド境界

Wasm は `Command` を直接返さない。
返すのは command descriptor である。

例:

- `tool.set_active` + `{ tool: "brush" }`
- `tool.set_color` + `{ color: "#1E88E5" }`
- `project.save` + `{}`

host がこれを `Command` に変換する。

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

- `altpaint-panel-schema`
  - host / Wasm 間の共有 DTO
- `altpaint-panel-sdk`
  - plugin 作者向け高水準 API
- `altpaint-panel-macros`
  - 必要なら macro 補助

### plugin 作者が使う型

- `EventContext`
- `HostSnapshot`
- `StatePatch`
- `CommandDescriptor`
- `HandlerResult`

### SDK が提供すべき helper

- `command("tool.set_active")`
- `.string("tool", "brush")`
- `StatePatch::set("selectedTool", "brush")`
- `StatePatch::toggle("showAdvanced")`

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

- `runtime { wasm: ... }` の導入
- handler binding
- `wasmtime` 実行の最小接続
- state patch 反映
- command descriptor 反映

### フェーズ7

- `plugin-host` 導入
- 権限管理
- 外部 Wasm panel ロード
- エラー隔離
- SDK / schema の整備

## MVP スコープ

MVP では次で十分である。

- runtime: `wasmtime`
- handler: `panel_handle_event`
- payload: bytes ベース
- state: local patch のみ
- command: descriptor 経由
- 権限: deny by default
- SDK: Rust 向け最小版を1つ用意

## 結論

Wasm 側は、**host 内部型から切り離した処理 runtime** として設計するのがよい。

- 実行は `wasmtime`
- 境界は DTO と command descriptor
- plugin 作者には SDK を経由させる
- state は patch で返す
- document 更新は host が `Command` 化して担う

この方針であれば、Rust からの Wasm 開発体験を確保しつつ、host 側の ABI と権限モデルも守りやすい。
