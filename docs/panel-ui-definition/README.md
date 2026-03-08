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
