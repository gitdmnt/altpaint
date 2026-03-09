# altpaint 実装状況

## この文書の目的

この文書は、2026-03-10 時点の `altpaint` が**実際にどこまで実装済みか**を短く把握するための現況メモである。

依存関係の詳細は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)、
設計原則は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) を参照する。

## 現在の要約

`altpaint` は現在、次を持つ。

- Cargo workspace による multi-crate 構成
- `winit` + `wgpu` による単一ウィンドウ desktop host
- 単一 `Document` を中心にした最小編集モデル
- 複数ラスタレイヤー、blend mode、簡易マスク、パン/ズーム
- マウス / touch / wheel によるキャンバス操作
- dirty rect を使う差分提示
- project save/load
- session save/load
- `plugins/` 配下の `.altp-panel` + Wasm panel の再帰ロード
- built-in panel 群の UI DSL + Rust SDK + Wasm 実装
- panel local state / host snapshot / persistent config
- 外部ペンプリセット読込
- 実行時 profiler とタイトル表示

## workspace 現況

### 中核クレート

- `app-core`
- `render`
- `storage`
- `desktop-support`
- `plugin-api`
- `ui-shell`
- `plugin-host`
- `panel-dsl`
- `panel-schema`
- `panel-sdk`
- `panel-macros`
- `apps/desktop`

### 組み込みパネル crate

- `plugins/app-actions`
- `plugins/tool-palette`
- `plugins/layers-panel`
- `plugins/color-palette`
- `plugins/pen-settings`
- `plugins/job-progress`
- `plugins/snapshot-panel`

## 実装済みの主要領域

### 1. ドメインモデル

`app-core` には次がある。

- `Document`
- `Command`
- `CanvasBitmap`
- `RasterLayer`
- `BlendMode`
- `CanvasViewTransform`
- `PenPreset`
- `WorkspaceLayout`

現状の中心は、`Document::apply_command(...)` を通じて編集状態を変える形である。

### 2. デスクトップホスト

`apps/desktop` には次がある。

- `DesktopRuntime` による `winit` event loop
- `DesktopApp` による状態遷移と副作用統合
- `wgpu` presenter
- base frame / canvas texture / overlay frame の 3 層提示
- pointer / keyboard / IME の処理
- panel と canvas の入力ルーティング

### 3. パネル基盤

現在の panel stack は次で構成される。

- `plugin-api`: `PanelTree`, `PanelNode`, `PanelEvent`, `HostAction`
- `panel-dsl`: `.altp-panel` parser / validator / normalized IR
- `panel-schema`: Wasm runtime DTO
- `panel-sdk`: plugin author API
- `panel-macros`: safe export macro
- `plugin-host`: `wasmtime` ベース runtime
- `ui-shell`: panel runtime 統合、panel draw、focus、text input、persistent config

### 4. 永続化

`storage` には次がある。

- project save/load
- `format_version` 管理
- `WorkspaceLayout` 永続化
- `plugin_configs` 永続化
- pen preset 読込

`desktop-support` には次がある。

- session save/load
- native dialog
- desktop config
- profiler

### 5. built-in panels

現在の標準 panel は次である。

- `builtin.app-actions`
- `builtin.tool-palette`
- `builtin.layers-panel`
- `builtin.color-palette`
- `builtin.pen-settings`
- `builtin.job-progress`
- `builtin.snapshot-panel`

これらは `plugins/` 配下に `.altp-panel` と Rust/Wasm 実装を同居させる構成で揃っている。

補足:

- `builtin.layers-panel` は、レイヤー追加/削除、縦並び一覧からの選択、ドラッグ&ドロップ並べ替え、レイヤー名変更、合成モード選択 dropdown に対応した

## runtime と依存関係の現況

### 依存関係の読み方

依存関係の正本は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) に移した。

重要な点だけここに再掲する。

- `app-core` はローカル依存を持たない
- `render`, `storage`, `desktop-support`, `plugin-api` は `app-core` に依存する
- `ui-shell` は `panel-*`, `plugin-host`, `plugin-api`, `render`, `app-core` に依存する
- `apps/desktop` は desktop host 全体を束ねる
- built-in panel crate は `panel-sdk` のみへ依存する

### 現時点での実装上の特徴

1. `ui-shell` が panel runtime の中心である
2. `render` はまだ薄く、描画 orchestration の多くは `apps/desktop` にある
3. `plugin-host` は `ui-shell` の内側で使われる
4. project 保存と session 保存は既に分離されている

## 到達済みフェーズの整理

### 完了済み

- フェーズ0: 最小契約
- フェーズ1: 最小起動ループ
- フェーズ2: 最小描画ループ
- フェーズ3: 保存と再読込
- フェーズ4: パネル中間表現
- フェーズ5: 標準パネルの host 描画
- フェーズ6: panel 基盤 crate と UI DSL parser
- フェーズ7: built-in panel の UI DSL + Wasm 移植

### 最小到達済み

- フェーズ8: 外部 Wasm panel runtime の基盤
- フェーズ9: 実用寄りキャンバス機能の最小形

## 既知の現在地

### 強い点

- host 主導の desktop runtime が一周している
- panel DSL + Wasm の最小垂直スライスが通っている
- built-in panel を file-based plugin へ寄せられている
- dirty rect と三層提示により、最低限の差分更新構造がある

### まだ薄い点

- `render` は将来構想に比べると責務が少ない
- panel permission は宣言に比べ検証がまだ薄い
- jobs / snapshot / export はまだ最小プレースホルダ寄りである
- Undo/Redo や高度なドキュメント操作は未実装である

## いま読むべき関連文書

- 依存関係を追いたいとき
  - [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
- 設計原則を確認したいとき
  - [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 描画責務を確認したいとき
  - [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)
- 今後の順番を確認したいとき
  - [docs/ROADMAP.md](docs/ROADMAP.md)

## 実務メモ

- 「今どう実装されているか」はコードが正本
- 「依存関係はどう整理されているか」は `MODULE_DEPENDENCIES.md` を優先
- 「どういう境界を守るべきか」は `ARCHITECTURE.md` を優先
