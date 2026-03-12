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

## 2026-03-09 改訂で固定したこと

今回の改訂では、フェーズ9へ進む前提として「plugin 作者が unsafe / ABI / 文字列 command 名を直接扱わない」ことを明文化した。

固定した判断は次である。

- plugin 作者が触る正面 API は `crates/plugin-sdk` に集約する
- `extern "C"` / `#[unsafe(no_mangle)]` / host import は SDK 内部へ閉じ込める
- Rust 側の handler export は `#[plugin_sdk::panel_init]` / `#[plugin_sdk::panel_handler]` で宣言する
- Rust 側の command 発行は `plugin_sdk::commands::*` の型付き helper を優先し、生の `"tool.set_active"` 文字列は escape hatch 扱いにする
- Rust 側の local state 参照は `plugin_sdk::state::*Key` を通し、状態パス文字列の重複を減らす
- UI DSL 上の handler binding は依然として文字列だが、これは UI 定義の manifest 面に閉じ込め、Rust 実装側へは漏らさない

つまり、**文字列ベースの境界は DTO / DSL 側へ寄せ、plugin 本体の Rust コードはなるべく型付き API で書けるようにする**。

## レビュワーコメントに対する整理

レビュワーの指摘のうち、次は妥当である。

- 現行 ABI はフェーズ6向けの暫定形であり、安定 ABI と見なすべきではない
- plugin 作者へ unsafe / export 名 / host import を露出させるべきではない
- `Command` 名や state path の typo が、境界で文字列化されるとコンパイル時検査から漏れやすい

一方で、次は補足が必要である。

- handler 名の文字列化を完全にゼロにはできない
	- UI DSL が別ファイルである以上、manifest 面には名前解決点が残る
	- したがって、Rust 側を型安全化しつつ、DSL 側は validator と将来の生成支援で補強するのが現実的である
- ABI の bytes DTO 化は将来方針として妥当だが、SDK の安全化はそれより前に進めるべきである
	- ABI が暫定であっても、plugin 作者に見せる API は今すぐ安定化できる

このため、本フォルダの文書では **「ABI はまだ発展途上だが、SDK は先に安全化する」** という立場を採る。

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
- `crates/plugin-sdk`
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
