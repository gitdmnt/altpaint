# altpaint 現在アーキテクチャ

## この文書の目的

この文書は、2026-03-11 時点の `altpaint` が**コード上で実際にどう分割され、どこに責務が集中しているか**を整理するための現況文書である。

この文書は理想図ではない。現状の事実をまとめる。

- 目標構造は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 実装到達点は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- compile-time 依存は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)

## 現在の全体像

現在の `altpaint` は、次の 3 つの集約点を中心に動いている。

1. `apps/desktop::DesktopApp`
   - desktop host 全体の状態遷移と副作用の統合点
2. `app-core::Document`
   - ドメイン状態と編集コマンドの中心
3. `ui-shell::UiShell`
   - panel runtime と panel presentation の統合点

このため、crate は分かれているが、実際の責務はまだ太い単位に集中している。

## 現在のプロジェクト構造と責務

### 1. `apps/desktop`

現在の desktop host であり、次を担う。

- `winit` event loop
- `wgpu` presenter
- OS 入力の正規化
- `DesktopApp` によるアプリ状態と副作用の統合
- canvas 入力と panel 入力の振り分け
- project / session / workspace preset / tools / pens / panels の起動時読込
- base / canvas / overlay の三層提示
- `canvas` runtime と `UiShell` の orchestration

主なモジュール:

- `src/main.rs`
- `src/runtime.rs`
- `src/runtime/pointer.rs`
- `src/runtime/keyboard.rs`
- `src/app/mod.rs`
- `src/app/bootstrap.rs`
- `src/app/command_router.rs`
- `src/app/panel_dispatch.rs`
- `src/app/services.rs`
- `src/app/background_tasks.rs`
- `src/app/input.rs`
- `src/app/commands.rs`
- `src/app/state.rs`
- `src/app/present.rs`
- `src/app/drawing.rs`
- `src/frame/`
- `src/wgpu_canvas.rs`

補足:

- `DesktopApp` は単なる状態コンテナではなく、現状では host orchestration の中心である。
- `src/app/drawing.rs` は `canvas::CanvasRuntime` を再公開するだけの薄い wrapper になった。

### 2. `crates/app-core`

現在のドメイン中核であり、次を担う。

- `Document`
- `Work -> Page -> Panel -> LayerNode`
- `Command`
- `CanvasBitmap`
- `CanvasViewTransform`
- `PenPreset`
- `ToolDefinition`
- `WorkspaceLayout`
- `BitmapEdit` / `PaintInput` / compositor などの共有 paint primitive

主なモジュール:

- `src/command.rs`
- `src/document.rs`
- `src/document/bitmap.rs`
- `src/document/layer_ops.rs`
- `src/document/pen_state.rs`
- `src/painting.rs`
- `src/workspace.rs`

補足:

- ドメイン純度は高いが、`Document` が tool catalog や pen runtime state を広く持っている。
- paint context 構築そのものは `canvas::context_builder` へ移った。

### 3. `crates/canvas`

現在の canvas runtime / input / bitmap op 層であり、次を担う。

- `CanvasRuntime`
- `CanvasInputState`
- view-to-canvas 変換
- canvas gesture state machine
- `Document` からの paint context 構築
- built-in bitmap paint plugin
- stamp / stroke / flood fill / lasso fill
- panel rect preview の render bridge

主なモジュール:

- `src/runtime.rs`
- `src/context_builder.rs`
- `src/input_state.rs`
- `src/view_mapping.rs`
- `src/gesture.rs`
- `src/render_bridge.rs`
- `src/plugins/builtin_bitmap.rs`
- `src/ops/`

### 4. `crates/render`

現在の描画計画・表示幾何・panel software rasterize 層であり、次を担う。

- `RenderFrame`
- `CanvasScene`
- canvas quad / UV / dirty rect 写像
- 画面座標 <-> canvas 座標変換
- ブラシプレビュー矩形計算
- floating panel layer のラスタライズ
- panel hit region 生成
- panel 描画用 text 計測・描画

主なモジュール:

- `src/lib.rs`
- `src/panel.rs`
- `src/text.rs`

補足:

- `render` は育っているが、最終提示戦略や desktop 固有フレーム合成はまだ `apps/desktop` に強く残る。
- 名前の期待ほど「描画のすべて」を持っているわけではない。

### 4. `crates/ui-shell`

現在の panel runtime 統合層であり、次を担う。

- panel registry
- `.altp-panel` の再帰読込
- DSL panel の構築
- Wasm panel runtime の呼び出し
- host snapshot 同期
- panel local state / persistent config 管理
- workspace layout 管理
- focus / scroll / text input
- panel surface の構築
- panel event 解釈

主なモジュール:

- `src/lib.rs`
- `src/dsl.rs`
- `src/presentation.rs`
- `src/surface_render.rs`
- `src/workspace.rs`
- `src/focus.rs`
- `src/tree_query.rs`

補足:

- 名前は `ui-shell` だが、実態は panel runtime と panel presentation の両方を持つ統合層である。
- ここが現在もっとも責務が集中している境界の一つである。

### 5. `crates/plugin-api`

現在は汎用 plugin API というより、panel host 契約層である。

- `PanelPlugin`
- `PanelTree`
- `PanelNode`
- `PanelEvent`
- `HostAction`

補足:

- 命名は広いが、実態は panel 向け契約が中心である。

### 6. `crates/plugin-host`

Wasm panel runtime の実行器である。

- `wasmtime` による module load
- host import の公開
- Wasm memory 読み書き
- `panel_init` / `panel_handle_event` / `panel_sync_host` の橋渡し

補足:

- 一般 plugin host ではなく、現時点では panel Wasm runtime に特化している。

### 7. `crates/panel-dsl`

`.altp-panel` の parser / validator / normalized IR 層である。

- AST
- parser
- validator
- file load
- handler binding 抽出

### 8. `crates/panel-schema`

host と Wasm panel runtime の共有 DTO である。

- `PanelInitRequest`
- `PanelInitResponse`
- `PanelEventRequest`
- `HandlerResult`
- `StatePatch`
- `CommandDescriptor`
- `Diagnostic`

### 9. `crates/panel-sdk` / `crates/panel-macros`

panel 作者向け authoring surface である。

- typed command builder
- host snapshot accessor
- runtime helper
- panel export macro

補足:

- 論理的には一つの authoring surface だが、Rust の都合で proc-macro は別 crate である。

### 10. `crates/storage`

project / pen / tool catalog の永続化と読込を担う。

- project save/load
- SQLite backend
- format version
- page / panel 単位の部分読込
- layer chunk 保存
- current panel snapshot 永続化
- pen import/export
- `tools/` カタログ読込

主なモジュール:

- `src/project_file.rs`
- `src/project_sqlite.rs`
- `src/pen_exchange.rs`
- `src/pen_format.rs`
- `src/pen_presets.rs`
- `src/tool_catalog.rs`

### 11. `crates/desktop-support`

desktop 固有 I/O と補助機能を担う。

- session save/load
- native dialog
- default path / config
- profiler
- canvas template 読込
- workspace preset catalog の読込/保存

### 12. `crates/workspace-persistence`

project 保存と session 保存で共有する UI 永続化 DTO を持つ。

- `WorkspaceUiState`
- `PluginConfigs`

### 13. `plugins/*`

現在の built-in panel 群である。

workspace member として存在するもの:

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

- 各 plugin は基本的に `panel.altp-panel` と Rust/Wasm 実装を同居させている。
- `plugins/phase6-sample` は残っているが、workspace member ではない。

## 目標 crate 配置草案との差分

現状コードを基準に見たとき、移動先として固定したい境界は次である。

| 論理名          | 現在の主配置                               | 目標配置               | 現状メモ                              |
| --------------- | ------------------------------------------ | ---------------------- | ------------------------------------- |
| `desktopApp`    | `apps/desktop`                             | `apps/desktop`         | 現在は orchestration 以外も広く抱える |
| `app-core`      | `crates/app-core`                          | `crates/app-core`      | `Document` に tool / pen state が残る |
| `render`        | `crates/render`                            | `crates/render`        | desktop 固有 compose がまだ外に残る   |
| `canvas`        | `crates/canvas`                            | `crates/canvas`        | runtime / gesture / bitmap op を集約  |
| `ui-shell`      | `crates/ui-shell`                          | `crates/ui-shell`      | runtime と presentation が同居中      |
| `panel-runtime` | `crates/ui-shell` 内部に内包               | `crates/panel-runtime` | フェーズ3で切り出し候補               |
| `plugin-host`   | `crates/plugin-host`                       | `crates/plugin-host`   | panel Wasm runtime 専用               |
| `panel-dsl`     | `crates/panel-dsl`                         | `crates/panel-dsl`     | 純粋 parse 層として維持しやすい       |
| `plugin-sdk`    | `crates/panel-sdk` + `crates/panel-macros` | `crates/plugin-sdk` 系 | 物理名と論理名がまだずれる            |

## 現在の runtime flow

### 1. 起動

1. `apps/desktop` が window / GPU / event loop を初期化する
2. `DesktopApp::new(...)` が session / project / workspace preset を読む
3. `UiShell` が `plugins/` 以下の `.altp-panel` を再帰ロードする
4. `storage` が `tools/` と `pens/` を読み、`Document` へ反映する
5. `canvas::CanvasRuntime` が paint plugin registry を初期化する
6. `render` と `wgpu_canvas` が最初の提示を行う

### 2. 入力から描画まで

1. OS 入力を `runtime/pointer.rs` と `runtime/keyboard.rs` が正規化する
2. `app/input.rs` が canvas / panel / panel move を振り分ける
3. `canvas::view_mapping` が window/view 座標を canvas 座標へ変換する
4. `canvas::gesture` が down / drag / up を `PaintInput` または panel rect preview へ変換する
5. `canvas::context_builder` が `Document` から paint context を解決する
6. `canvas::runtime` が built-in plugin を呼んで bitmap 差分を作る
7. 差分を `Document` に適用する
8. `app/present.rs` と `wgpu_canvas.rs` が dirty rect ベースで再提示する

### 3. パネル

1. `panel-dsl` が `.altp-panel` を parse / validate する
2. `plugin-host` が Wasm panel runtime を起動する
3. `UiShell` が host snapshot を panel 側へ同期する
4. panel event は `PanelEvent` と `HostAction` に変換される
5. `DesktopApp` が `Command` または host side effect として処理する
6. `render` が panel surface と hit region を作る

### 4. 保存と workspace 状態

- project save/load は主に `storage`
- session save/load と desktop path 管理は `desktop-support`
- 共通 UI 永続化 DTO は `workspace-persistence`
- ただし orchestration の中心は依然として `DesktopApp`

## 現在の集中責務

### `DesktopApp`

次が集まりすぎている。

- document と UI shell の所有
- project / session / workspace preset I/O
- tool / pen / panel catalog 読込
- canvas runtime と panel runtime の橋渡し
- dirty rect と present planning
- panel drag と input state

集中箇所（ファイル単位）:

- `apps/desktop/src/app/mod.rs`: `DesktopApp` 定義、constructor wiring、薄い公開 API が残る
- `apps/desktop/src/app/bootstrap.rs`: 起動時の session / project / workspace preset 復元が集約される
- `apps/desktop/src/app/command_router.rs`: document command と I/O command の分岐が集約される
- `apps/desktop/src/app/panel_dispatch.rs`: panel drag、host action、focus / text input dispatch が集約される
- `apps/desktop/src/app/io_state.rs`: `project_path` / `session_path` / `workspace_preset_path` / dialogs / background save queue をまとめる
- `apps/desktop/src/app/background_tasks.rs`: 非同期 save task の起動と回収を扱う
- `apps/desktop/src/app/services.rs`: workspace preset 操作、tool / pen reload、pen import、status 生成を扱う
- `apps/desktop/src/app/present_state.rs`: dirty rect、present flag、UI 再同期要求が集約される
- `apps/desktop/src/app/input.rs`: window→canvas 変換と panel/canvas ルーティングが残る
- `apps/desktop/src/app/present.rs`: dirty rect、panel surface refresh、frame compose 指示が集中する

### `CanvasRuntime`

次が `canvas` に集約された。

- bitmap paint plugin registry
- `Document` 由来の paint context 構築
- stroke / fill / erase / lasso の bitmap op
- lasso / panel rect / drag の input state machine

集中箇所（ファイル単位）:

- `crates/canvas/src/runtime.rs`: plugin 実行入口
- `crates/canvas/src/context_builder.rs`: `Document` からの runtime 文脈構築
- `crates/canvas/src/gesture.rs`: canvas gesture state machine
- `crates/canvas/src/plugins/builtin_bitmap.rs`: built-in bitmap plugin
- `crates/canvas/src/ops/*`: stamp / stroke / flood fill / lasso fill / composite

### `Document`

次が集中している。

- ドメインモデル
- `Command` 適用
- tool / pen runtime 状態

集中箇所（ファイル単位）:

- `crates/app-core/src/document.rs`: document state と tool / pen state が集中する
- `crates/app-core/src/painting.rs`: `PaintPluginContext`、`BitmapEdit`、compositor が集中する

### `UiShell`

次が同居している。

- panel discovery
- DSL / Wasm runtime
- state patch / host sync
- workspace layout
- focus / scroll / text input
- panel surface 生成

集中箇所（ファイル単位）:

- `crates/ui-shell/src/lib.rs`: registry、update、persistent config、公開 facade が集中する
- `crates/ui-shell/src/dsl.rs`: panel file 探索、DSL 評価、Wasm bridge、state patch 適用が集中する
- `crates/ui-shell/src/workspace.rs`: workspace layout、panel move/visibility、manager tree が集中する

## 命名と実態のズレ

### `ui-shell`

名前より実態は「panel runtime 統合層」である。

### `render`

名前ほど描画責務が集約されておらず、desktop 側に最終提示と host frame 生成が残る。

### `plugin-api`

汎用 plugin API というより panel host API である。

### `DesktopApp`

単なる app state ではなく、desktop host orchestration service に近い。

## 現在まだ存在しない明確な境界

次の責務は重要だが、まだ独立した crate / module として確立していない。

- `canvas` と呼べる独立層
- `panel-runtime` と呼べる独立層
- project / workspace を plugin 主導で扱う一般 plugin runtime
- panel runtime と panel presentation の明確な分離
- tool 実行 plugin と host runtime の安定境界

## 既存テストの責務境界棚卸し

### `apps/desktop/src/app/tests/*`

- `bootstrap_tests.rs`: 起動復元と session 起点の project 復元を分離して検証し始めた
- `command_router_tests.rs`: document command と I/O command の経路を薄く固定し始めた
- `commands.rs`: command dispatch、shortcut、preset 同期、built-in panel 置換確認をまとめて抱えている
- `interaction.rs`: canvas 入力、panel 入力、drag、color wheel、present 前提の振る舞いをまとめて抱えている
- `panel_dispatch_tests.rs`: panel dispatch と drag source 更新の回帰を分離し始めた
- `persistence.rs`: project save/load、workspace layout 復元、plugin config 永続化、差分 present 検証をまとめて抱えている

今後 crate / module 単位へ移したい代表例:

- canvas 座標変換、eraser/stroke/fill の期待値テスト → 将来 `crates/canvas`
- panel dispatch、host action 適用、focus/control activation → `apps/desktop` 内の `panel_dispatch` / `command_router`
- dirty rect と panel 差分 compose の検証 → `crates/render`
- workspace preset config 同期や bootstrap 復元 → `apps/desktop` 内の `bootstrap` / `services`

### `crates/ui-shell/src/tests.rs`

- 現在は runtime bridge と presentation の両方を 1 ファイルで検証している
- DSL 読込、Wasm bridge、focus、dropdown、text input、surface render が同居している

今後分離したい代表例:

- DSL/Wasm/host snapshot sync → 将来 `crates/panel-runtime`
- hit-test、focus、text input、surface render → `crates/ui-shell/src/presentation/*`

### `crates/panel-sdk/src/tests.rs`

- 現在は command builder、typed host accessor、runtime helper、macro export を 1 つの作者向け回帰集合で検証している
- authoring surface の回帰テストとしては妥当だが、将来 `plugin-sdk` 再編時は command / services / runtime helper を分割しやすい構成へ寄せる

## この文書の結論

現在の `altpaint` は、

- `apps/desktop`
- `app-core`
- `ui-shell`

の 3 点に大きな責務が集まりながらも、

- `render`
- `panel-dsl`
- `plugin-host`
- `panel-sdk`
- `storage`

へ責務を切り出す土台は既にできている状態である。

今後のリファクタリングでは、まずこの現況を前提に、目標構造とのギャップを段階的に埋める必要がある。
