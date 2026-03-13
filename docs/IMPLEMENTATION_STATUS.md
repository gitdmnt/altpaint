# altpaint 実装状況

## この文書の目的

この文書は、2026-03-13 時点の `altpaint` が**実際にどこまで実装されているか**を短く把握するための現況整理である。

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
- `panel-api`
- `panel-runtime`
- `ui-shell`
- `workspace-persistence`
- `plugin-host`
- `panel-dsl`
- `panel-schema`
- `plugin-macros`
- `plugin-sdk`
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

- `tools/experimental/phase6-sample` へ DSL/WAT sample を移し、既定 `plugins/` 探索対象から外した。

## 実装済みの主要領域

### 1. desktop host

`apps/desktop` には次がある。

- `DesktopRuntime` による `winit` event loop
- `WgpuPresenter` による base / canvas / overlay の三層提示
- `DesktopApp` による document / UI / I/O / present の統合
- `apps/desktop/src/app/bootstrap.rs` / `command_router.rs` / `panel_dispatch.rs` / `present_state.rs` / `background_tasks.rs` / `io_state.rs` / `services/` への責務分割
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
- `FramePlan` / `CanvasPlan` / `OverlayPlan` / `PanelPlan`
- canvas quad / UV / dirty rect 写像
- 画面座標 <-> canvas 座標変換
- ブラシプレビュー矩形計算
- dirty rect の union とブラシプレビュー dirty 計算
- base / overlay / panel / status の CPU compose
- floating panel layer のラスタライズ
- panel hit region 生成
- panel 描画用 text 計測 / 描画

補足:

- フェーズ5完了により、frame compose と dirty rect 判断の中核は `render` に移った。
- `apps/desktop/src/app/present.rs` は panel refresh と `FramePlan` 組み立て、`wgpu_canvas.rs` は最終 GPU 提示に寄った。

### 4. panel 基盤

現在の panel stack は次で構成される。

- `panel-api`: `PanelTree`, `PanelNode`, `PanelEvent`, `HostAction`, `ServiceRequest`
- `panel-dsl`: `.altp-panel` parser / validator / normalized IR
- `panel-schema`: host-Wasm 間 DTO
- `plugin-sdk`: plugin 作者向け SDK、typed service request builder、macro 再 export
- `plugin-macros`: `plugin-sdk` が再 export する proc-macro 実装
- `plugin-host`: `wasmtime` ベース runtime
- `panel-runtime`: panel discovery / DSL-Wasm bridge / host snapshot sync / persistent config
- `ui-shell`: panel presentation / workspace layout / focus / hit-test / surface render

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
- `app-actions` / `workspace-presets` / `view-controls` / `panel-list` が host service request を発行する
- paint plugin 実行は `canvas::CanvasRuntime` が担当する
- `storage` が外部ペン preset を読み、`AltPaintPen` 正規化 format を扱う

補足:

- project / workspace / tool catalog reload / view / panel navigation は service request 経由で host へ届く。

## runtime と依存関係の現況

### 現在の実行上の中心

現在の実装は、主に次の 4 点へ責務が集中している。

1. `apps/desktop::DesktopApp`
2. `app-core::Document`
3. `canvas::CanvasRuntime`
4. `panel-runtime::PanelRuntime` と `ui-shell::PanelPresentation`

### 現在の特徴

1. panel runtime は `panel-runtime`、panel presentation は `ui-shell` に分離された
2. `render` は canvas 表示計算と panel rasterize を持つが、最終提示の中心ではまだない
3. project 保存と session 保存は分離されている
4. built-in panel は file-based plugin 構成へかなり寄っている
5. project / workspace / tool catalog reload / view / panel navigation は plugin-first service API 経由になったが、tool 実行本体は依然として host 側 `canvas` runtime が担う

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
- `render`、`panel-runtime`、storage が独立 crate として成立している
- `render` が frame plan / dirty plan / compose の中心として成立した
- `canvas` が独立 crate として成立し、desktop から bitmap op と gesture state machine を切り離せた
- built-in panel の file-based plugin 化が進んでいる
- project / session / workspace preset が一応つながっている

### まだ途中の点

- `DesktopApp` は `bootstrap` / `command_router` / `panel_dispatch` / `io_state` / `services/` / `present_state` / `background_tasks` に分割され、フレーム compose も `render` へ移ったが、subsystem orchestration と panel/runtime 橋渡しは依然として大きい
- `Document` が tool / pen runtime state をまだ広く抱えている
- `DesktopApp` が `PanelRuntime` と `PanelPresentation` の orchestration をまだ厚く抱えている
- tool 実行本体は plugin 主導ではなく `canvas` runtime 主導である
- tool catalog 一覧取得や tool 実行 API の一般化はまだ途中である

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

## フェーズ完了履歴

| フェーズ | 内容 | 完了時点 |
|----------|------|----------|
| 0 | 境界の固定と作業前提の統一 | 2026-03-11 |
| 1 | `desktopApp` の縮小 | 2026-03-11 |
| 2 | `canvas` 層の新設 | 2026-03-12 |
| 3 | panel runtime / presentation 分離 | 2026-03-12 |
| 4 | plugin-first 化の本格化 | 2026-03-12 |
| 5 | `render` 中心の画面生成整理 | 2026-03-12 |
| 6 | API 名称と物理配置の整理 | 2026-03-12 |
| 7 | 再編後の機能拡張 | **進行中** (2026-03-13〜) |

## フェーズ7 進行状況 (2026-03-13 時点)

### 実装済み（Undo/Redo 基盤）

- `crates/app-core/src/history.rs` 新設
  - `CommandHistory`（push / undo / redo / clear / past_entries）
  - `HistoryEntry::BitmapOp(BitmapEditRecord)`
  - `DEFAULT_HISTORY_CAPACITY = 50`
  - 全操作の単体テスト完備

- `crates/app-core/src/painting.rs` 追加型
  - `BitmapEditOperation`（Stamp / StrokeSegment / FloodFill / LassoFill）
  - `BitmapEditRecord`（panel_id / layer_index / operation / pen_snapshot / color_snapshot / tool_id）
  - `BitmapEditOperation::from_paint_input` / `to_paint_input` 変換

- `crates/canvas/src/edit_record.rs` 新設（`app_core` からの re-export）

- `crates/panel-api/src/services.rs`
  - `HISTORY_UNDO` / `HISTORY_REDO` service 名追加
  - `SNAPSHOT_CREATE` / `SNAPSHOT_RESTORE` / `EXPORT_IMAGE` service 名追加

- `crates/plugin-sdk/src/services.rs`
  - `history::undo()` / `history::redo()` descriptor builder 追加
  - `snapshot::create()` / `snapshot::restore()` / `export_image::export()` descriptor builder 追加

### 未実装（フェーズ7残項目）

- `canvas::CanvasRuntime` への undo 接続（`HISTORY_UNDO` → replay 経路）
- `apps/desktop` 側の `HISTORY_UNDO` / `HISTORY_REDO` service handler
- export job / snapshot handler（フェーズ7-3 / 7-4）
- tool child 構成 / text-flow / 高度な tool plugin 構成

## 目標アーキテクチャとの残差

| 集中箇所 | 残課題 |
|----------|--------|
| `DesktopApp` | panel/runtime 橋渡しと orchestration がまだ大きい |
| `Document` | tool / pen runtime state をまだ広く持っている |
| `canvas::CanvasRuntime` | tool 実行が host 主導（plugin 主導への移行は未着手） |
| Undo/Redo | `CommandHistory` 基盤はできたが canvas への接続が未実装 |

## 実務メモ

- 「今どう実装されているか」はコードと `CURRENT_ARCHITECTURE.md` を優先する
- 「どうあるべきか」は `ARCHITECTURE.md` を優先する
- 「次に何を崩さず進めるか」は `ROADMAP.md` を優先する
- フェーズ完了ごとの文書同期は `IMPLEMENTATION_STATUS.md` → `CURRENT_ARCHITECTURE.md` → `MODULE_DEPENDENCIES.md` を最小セットとして固定する
- コード変更後に文書を追記する順序を守り、文書だけを先行させない
- フェーズ0の判断基準として、`canvas` / `panel-runtime` / `plugin-sdk` 系の命名と配置規約を文書で先に固定した
