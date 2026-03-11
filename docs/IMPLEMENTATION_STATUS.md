# altpaint 実装状況

## この文書の目的

この文書は、2026-03-11 時点の `altpaint` が**実際にどこまで実装されているか**を短く把握するための現況整理である。

この文書は理想図ではなく現況の要約であり、次と役割を分ける。

- 現在の構造: [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md)
- 目標構造: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 依存関係の事実: [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
- 今後の順序: [docs/ROADMAP.md](docs/ROADMAP.md)

## 現在の要約

`altpaint` は現在、次を持つ。

- Cargo workspace による multi-crate 構成
- `winit` + `wgpu` による単一ウィンドウ desktop host
- `Document` / `Command` を中心にした編集モデル
- 複数ラスタレイヤー、blend mode、簡易 mask、pan / zoom / rotation / flip
- dirty rect を使う差分提示
- マウス / touch / wheel / keyboard を含む入力処理
- `.altp-panel` + Rust/Wasm による built-in panel 実装
- `plugins/` 配下 panel の再帰ロード
- `tools/` 配下 tool 定義の再帰ロード
- `pens/` 配下の外部ペン preset 読込
- panel local state / host snapshot / persistent config
- 4隅アンカー基準の workspace panel 配置
- workspace preset の読込 / 切替 / 保存 / 書き出し
- SQLite ベース project save/load
- page / panel 単位の project index / 部分ロード
- layer bitmap の chunk 保存と current panel snapshot 永続化
- session save/load
- profiler と実行時間計測

## 現在の workspace 構成

### 中核 crate

- `app-core`
- `canvas`
- `render`
- `storage`
- `desktop-support`
- `plugin-api`
- `ui-shell`
- `workspace-persistence`
- `plugin-host`
- `panel-dsl`
- `panel-schema`
- `panel-sdk`
- `panel-macros`
- `apps/desktop`

### workspace member の built-in panel plugin

- `plugins/app-actions`
- `plugins/workspace-presets`
- `plugins/tool-palette`
- `plugins/view-controls`
- `plugins/panel-list`
- `plugins/layers-panel`
- `plugins/color-palette`
- `plugins/pen-settings`
- `plugins/job-progress`
- `plugins/snapshot-panel`

補足:

- `plugins/phase6-sample` は存在するが、workspace member ではない。

## 実装済みの主要領域

### 1. desktop host

`apps/desktop` には次がある。

- `DesktopRuntime` による `winit` event loop
- `WgpuPresenter` による base / canvas / overlay の三層提示
- `DesktopApp` による document / UI / I/O / present の統合
- `apps/desktop/src/app/bootstrap.rs` / `command_router.rs` / `panel_dispatch.rs` / `present_state.rs` / `background_tasks.rs` / `io_state.rs` / `services.rs` への責務分割
- pointer / keyboard / IME の処理
- panel と canvas の入力ルーティング
- 起動時の `plugins/` / `tools/` / `pens/` の読込

補足:

- `DesktopApp` は依然として orchestration の中心だが、constructor / command routing / panel dispatch / I/O state / workspace preset 操作は module 分割済みである。
- `apps/desktop/src/app/drawing.rs` は薄い wrapper になり、built-in paint plugin 実行本体は `crates/canvas` へ移った。

### 2. ドメインと document モデル

`app-core` には次がある。

- `Document`
- `Work`, `Page`, `Panel`, `LayerNode`, `RasterLayer`
- `Command`
- `CanvasBitmap`
- `CanvasViewTransform`
- `PenPreset`
- `ToolDefinition`
- `WorkspaceLayout`
- `BitmapEdit` / `PaintInput` / compositor などの共有 paint primitive

現状の状態変更の中心は `Document::apply_command(...)` である。

補足:

- `Document::resolve_paint_plugin_context(...)` は削除され、paint context の組み立ては `canvas::context_builder` へ移った。

### 3. canvas runtime

`canvas` には次がある。

- `CanvasRuntime`
- `CanvasInputState`
- `CanvasPointerEvent` と view-to-canvas 変換
- `advance_pointer_gesture(...)` による gesture state machine
- `build_paint_context(...)` による `Document` からの runtime 文脈構築
- built-in bitmap paint plugin
- stamp / stroke / flood fill / lasso fill の bitmap op
- `panel_creation_preview_bounds(...)` による render bridge

### 4. 描画と表示計画

`render` には次がある。

- `RenderFrame`
- `CanvasScene`
- canvas quad / UV / dirty rect 写像
- 画面座標 <-> canvas 座標変換
- ブラシプレビュー矩形計算
- floating panel layer のラスタライズ
- panel hit region 生成
- panel 描画用 text 計測 / 描画

補足:

- 画面生成ロジックの一部は `render` に寄っているが、desktop 固有の frame 合成と最終提示 orchestration はまだ `apps/desktop` に厚く残る。

### 4. panel 基盤

現在の panel stack は次で構成される。

- `plugin-api`: `PanelTree`, `PanelNode`, `PanelEvent`, `HostAction`
- `panel-dsl`: `.altp-panel` parser / validator / normalized IR
- `panel-schema`: host-Wasm 間 DTO
- `panel-sdk`: panel 作者向け SDK
- `panel-macros`: panel export 用 proc-macro
- `plugin-host`: `wasmtime` ベース runtime
- `ui-shell`: panel runtime と presentation の統合点

### 5. 永続化

`storage` には次がある。

- SQLite ベース project save/load
- `format_version` 管理
- page / panel 単位の部分ロード API
- layer bitmap の chunk 保存
- current panel snapshot 永続化
- full / delta save mode の差し込み余地
- pen preset 読込
- external brush parse / export module
- `tools/` カタログ読込

`desktop-support` には次がある。

- session save/load
- native dialog
- desktop config
- profiler
- canvas template 読込
- workspace preset catalog の読込 / 保存

`workspace-persistence` には次がある。

- project / session で共有する `WorkspaceUiState`
- `plugin_configs`

### 6. built-in panel 群

現在の built-in panel は次である。

- `builtin.app-actions`
- `builtin.workspace-presets`
- `builtin.tool-palette`
- `builtin.view-controls`
- `builtin.panel-list`
- `builtin.layers-panel`
- `builtin.color-palette`
- `builtin.pen-settings`
- `builtin.job-progress`
- `builtin.snapshot-panel`

補足:

- これらは `plugins/` 配下に `.altp-panel` と Rust/Wasm 実装を同居させる構成で揃っている。

### 7. ツールとペン

現在のツール系は次の構成で動いている。

- `storage::tool_catalog` が `tools/` から tool 定義を読む
- `Document` が active tool と設定を保持する
- `tool-palette` と `pen-settings` が host snapshot を読む
- paint plugin 実行は `canvas::CanvasRuntime` が担当する
- `storage` が外部ペン preset を読み、`AltPaintPen` 正規化 format を扱う

補足:

- ツール UI は plugin 化されているが、project / workspace I/O はまだ plugin-first 化の途中である。

## runtime と依存関係の現況

### 現在の実行上の中心

現在の実装は、主に次の 4 点へ責務が集中している。

1. `apps/desktop::DesktopApp`
2. `app-core::Document`
3. `canvas::CanvasRuntime`
4. `ui-shell::UiShell`

### 現在の特徴

1. `ui-shell` は panel runtime と presentation の両方を持つ
2. `render` は canvas 表示計算と panel rasterize を持つが、最終提示の中心ではまだない
3. project 保存と session 保存は分離されている
4. built-in panel は file-based plugin 構成へかなり寄っている
5. tool UI は plugin 化されているが、tool 実行はまだ完全には plugin-first ではない

## 到達済みの状態

### 明確に到達済み

- desktop host と GPU 提示の最小実用ループ
- multi-crate 構成
- panel DSL + Wasm panel の最小垂直スライス
- built-in panel 群の plugin 化
- dirty rect ベースの canvas / panel 更新
- SQLite project 形式の導入
- workspace preset と session 復元

### 最小実用到達済み

- 実用寄りの canvas 編集機能の最小形
- 外部ペン preset 読込
- panel 配置保存と復元
- project index / 部分ロード

## 既知の現在地

### 強い点

- host 主導の desktop runtime が一周している
- `render`、panel runtime、storage が独立 crate として成立している
- `canvas` が独立 crate として成立し、desktop から bitmap op と gesture state machine を切り離せた
- built-in panel の file-based plugin 化が進んでいる
- project / session / workspace preset が一応つながっている

### まだ途中の点

- `DesktopApp` に責務が集まりすぎている
- `Document` が tool / pen runtime state をまだ広く抱えている
- `ui-shell` が runtime と presentation を兼務している
- project / workspace I/O は plugin 主導ではない
- tool catalog / pen setting の plugin-first 化はまだ途中である

## いま読むべき関連文書

- 現在の構造を把握したいとき
  - [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md)
- 目標構造を確認したいとき
  - [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 依存関係を追いたいとき
  - [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
- 今後の順序を確認したいとき
  - [docs/ROADMAP.md](docs/ROADMAP.md)
- リファクタリング候補を見たいとき
  - [docs/tmp/tasks-2026-03-11.md](docs/tmp/tasks-2026-03-11.md)

## 実務メモ

- 「今どう実装されているか」はコードと `CURRENT_ARCHITECTURE.md` を優先する
- 「どうあるべきか」は `ARCHITECTURE.md` を優先する
- 「次に何を崩さず進めるか」は `ROADMAP.md` を優先する
- フェーズ完了ごとの文書同期は `IMPLEMENTATION_STATUS.md` → `CURRENT_ARCHITECTURE.md` → `MODULE_DEPENDENCIES.md` を最小セットとして固定する
- コード変更後に文書を追記する順序を守り、文書だけを先行させない
- フェーズ0の判断基準として、`canvas` / `panel-runtime` / `plugin-sdk` 系の命名と配置規約を文書で先に固定した
