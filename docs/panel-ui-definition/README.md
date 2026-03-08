# altpaint パネル UI 定義

## このフォルダの役割

このフォルダは、フェーズ6以降のパネル UI 定義関連文書をまとめた正本である。

対象は次の2系統である。

- UI DSL
- Wasm runtime / SDK / ABI

以前はトップレベルの `PANEL_*` 文書に分散していたが、関連文書が増えて見通しが悪くなったため、このフォルダへ集約した。

## 読み分け

### UI 構文を詰めたいとき

次を読む。

- `.altp-panel` の構造
- JSX 風構文
- state schema
- handler binding
- validation
- EBNF

参照先: [docs/panel-ui-definition/ui-dsl.md](docs/panel-ui-definition/ui-dsl.md)

### Wasm 実行モデルを詰めたいとき

次を読む。

- `wasmtime` 実行方針
- export / input / output
- ABI
- command descriptor
- state patch
- SDK / schema crate

参照先: [docs/panel-ui-definition/wasm-runtime.md](docs/panel-ui-definition/wasm-runtime.md)

## このフォルダで固定する判断

- UI は JSX 風 DSL で記述する
- 処理は Rust などから Wasm へコンパイルする
- 実行は `wasmtime` を第一候補とする
- host が描画、入力配送、権限管理、`Command` 変換を握る
- host と plugin の境界は DTO と command descriptor に限定する

## 段階的導入計画

このフォルダの文書は、次の 3 段階で導入する前提で読む。

### 段階1: 基盤 crate と parser の導入

次の段階では、まず土台を作る。

- `crates/panel-dsl`
	- lexer
	- parser
	- AST
	- validator
	- normalized IR
- `crates/panel-schema`
	- host / Wasm 間の共有 DTO
- `crates/panel-sdk`
	- Rust から Wasm を書くための最小 SDK
- `ui-shell` / `desktop`
	- `.altp-panel` の探索
	- ロード / 再ロード
	- parser 接続

この段階では、まず static panel と最小 handler binding が成立すればよい。

### 段階2: 既存ビルトインプラグインの再構成

その次の段階では、既存のビルトインプラグインを **UI DSL + Wasm** へ移す。

前提は次である。

- 組み込みパネルも外部パネル候補と同じ中間表現へ寄せる
- UI は `.altp-panel` で表現する
- 処理は Rust 実装を Wasm へコンパイルして載せる
- host は従来どおり `Command` と描画の主導権を握る

### 段階3: 外部 Wasm パネルの一般化

ビルトイン移植後に、外部ロード、権限、隔離、エラー処理を本格化する。

## ビルトイン移植の優先順

移植順は次を基本とする。

1. `app-actions`
2. `tool-palette`
3. `color-palette`
4. `layers-panel`
5. `job-progress`
6. `snapshot-panel`

理由:

- 最初の 3 つは `Command` 発行と active 表示の最小確認に向く
- `layers-panel` は host snapshot 参照と反復表示を検証しやすい
- `job-progress` と `snapshot-panel` は読み取り専用 panel として後追いで移しやすい

## このフォルダの文書を読むときの注意

ここで書く UI DSL / Wasm runtime は、最初から全機能を一度に入れる想定ではない。

重要なのは次である。

- 次の段階では crate と parser を成立させる
- その次の段階で既存ビルトインを新基盤へ移植する
- したがって仕様は「ビルトイン移植に必要な最小集合」を優先して詰める
