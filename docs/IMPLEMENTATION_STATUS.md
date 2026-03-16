# altpaint 実装状況

## この文書の目的

この文書は、2026-03-15 時点の `altpaint` が**実際にどこまで実装されているか**を短く把握するための現況整理である。

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
- **Undo/Redo 基盤 (フェーズ7-0〜7-2b)**: replay 方式 `CommandHistory`・`BitmapEditRecord`・
  `CanvasRuntime::replay_paint_record`・`execute_undo()`/`execute_redo()` サービスハンドラ・
  host snapshot への `can_undo`/`can_redo` 反映・app-actions パネルの undo/redo ボタン
- **テキスト描画基盤 (フェーズ7-5b〜7-6)**: `TextRenderer` trait・`Font8x8Renderer`・`render_text_to_bitmap_edit` canvas op・`text_render.render_to_layer` service・`plugins/text-flow` panel plugin・host handler
- **レンダー層分離 (2026-03-15)**: overlay 単層を L3 TempOverlay（canvas ブラシ/lasso）と L4 UiPanel（フローティング UI）に分割。`compose_temp_overlay_frame` / `compose_ui_panel_frame` / `LayerGroupDirtyPlan` 導入。各層が独立した dirty rect で更新され、不要な CPU 合成を削減。
- **起動時間改善 (2026-03-16)**: `plugin-host` の `WasmPanelRuntime::load` が Panel 毎に `Engine::default()` を生成し wasmtime JIT が 11 回フルコンパイルして起動に 4+ 秒かかっていた問題を修正。wasmtime `cache` feature を有効化し `Config::cache_config_load_default()` を使うことでコンパイル済みモジュールをディスクキャッシュし、2 回目以降の起動を大幅短縮。
- **ズーム/パン後のフレームスパイク修正 (2026-03-16)**: ズーム/パン操作完了時に `ui_sync_panels` が ~129ms かかりフレームが固まる問題を修正。原因は `build_host_snapshot()` が毎回 `serde_json::to_string(&pen_presets)` / `serde_json::to_string(&tool_catalog)` 等を再シリアライズしていたこと（debug ビルドで特に顕著）。`HostSnapshotCache` を `DslPanelPlugin` に持たせ、pen 数・active_tool_id・layer 数などの変化キーが変わった時だけ再シリアライズするよう改善。ズーム/パン操作後は view のみが変化するため高価なシリアライズをスキップ。`ui_sync_panels` avg: 129ms → **2.3ms**（98% 削減）。

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
- `plugins/text-flow`

補足:

- `tools/experimental/phase6-sample` へ DSL/WAT sample を移し、既定 `plugins/` 探索対象から外した。

## 実装済みの主要領域

### 1. desktop host

`apps/desktop` には次がある。

- `DesktopRuntime` による `winit` event loop
- `WgpuPresenter` による base / canvas / temp_overlay / ui_panel の四層提示
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
- `LayerGroupDirtyPlan` による L1/L3/L4 レイヤー独立 dirty rect 管理
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
| 7-0〜7-2b | Undo/Redo 基盤 | 2026-03-13 |
| 7-3 | PNG export・汎用ジョブ基盤 | 2026-03-13 |
| 7-4 | snapshot handler | 2026-03-14 |
| 7-5 | tool child 構成 | 2026-03-14 |
| 7-5b | TextRenderer trait + text-flow 設計 | 2026-03-14 |
| 7-6 | text-flow plugin + host handler | 2026-03-14 |
| 7-7 | プロファイラ PowerShell スクリプト | 2026-03-14 |
| 7-8 | 最終フェーズ文書整理 | 2026-03-14 |
| 7 | 再編後の機能拡張 | **完了** (2026-03-14) |
| 7-bugfix | ブラックボックステスト結果に基づくバグ修正 | 2026-03-15 |
| 7-testfix | テスト安定化（テスト競合・OOM・期待値修正） | 2026-03-15 |

## フェーズ7 実装完了内容 (2026-03-14)

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

### 実装済み（フェーズ7-3: PNG export・汎用バックグラウンドジョブ基盤）

- `crates/storage/src/export.rs` — `export_active_panel_as_png` PNG 書き出し関数（テスト 3 件）
- `apps/desktop/src/app/background_tasks.rs` — `BackgroundJob`/`JobKind` 汎用ジョブ型
- `apps/desktop/src/app/services/export.rs` — `export.image` service handler
- `crates/panel-api/src/lib.rs` — `PanelPlugin::update` に `active_jobs: usize` 追加
- `crates/panel-runtime/src/host_sync.rs` — host snapshot `jobs.active` を実値に更新
- `crates/panel-runtime/src/registry.rs` — `sync_document` 系に `active_jobs` 追加
- `crates/desktop-support/src/dialogs.rs` — `pick_save_image_path` default メソッド追加

### 実装済み（フェーズ7-4: snapshot handler）

- `apps/desktop/src/app/snapshot_store.rs` — `SnapshotStore`/`SnapshotEntry`（push / get / len / entries / 容量制限 20）
- `apps/desktop/src/app/services/snapshot.rs` — `SNAPSHOT_CREATE` / `SNAPSHOT_RESTORE` service handler
- `apps/desktop/src/app/mod.rs` — `DesktopApp.snapshots: SnapshotStore` フィールド追加
- `apps/desktop/src/app/services/mod.rs` — `handle_snapshot_service_request` をルーターに追加
- `crates/panel-api/src/lib.rs` — `PanelPlugin::update` に `snapshot_count: usize` 追加
- `crates/panel-runtime/src/registry.rs` — `sync_document` / `sync_document_panels` / `sync_document_subset` に `snapshot_count` 追加
- `crates/panel-runtime/src/dsl_panel.rs` — `update` シグネチャ更新
- `crates/panel-runtime/src/host_sync.rs` — `build_host_snapshot` に `snapshot_count` 追加、`snapshot.count` / `snapshot.storage_status` を実値に更新
- `apps/desktop/src/app/present.rs` — `snapshot_count` を取得して渡す
- `apps/desktop/src/app/bootstrap.rs` — 初回 `sync_document` に `snapshot_count=0` を渡す
- `crates/plugin-sdk/src/host.rs` — `host::snapshot::count()` getter 追加
- `plugins/snapshot-panel/panel.altp-panel` — `snapshot_count` state / Create Snapshot ボタン追加
- `plugins/snapshot-panel/src/lib.rs` — `snapshot_count` 同期・`create_snapshot` handler 追加

### 実装済み（フェーズ7-5b: TextRenderer trait + text-flow 設計）

- `.context/text-flow-design.md` — 設計ドキュメント新設（TextRenderer trait 契約、Font8x8Renderer、canvas op API、out-of-scope）
- `crates/canvas/src/ops/text.rs` — 新設
  - `TextRenderer` trait（`render(&str, font_size, color) -> TextRenderOutput`）
  - `TextRenderOutput`（pixels / width / height）
  - `Font8x8Renderer` — `font8x8::BASIC_FONTS` を用いた 8×8 bitmap スケールレンダラ
  - `render_text_to_bitmap_edit(text, font_size, color, x, y)` — デフォルト実装
  - `render_text_to_bitmap_edit_with(text, font_size, color, x, y, renderer)` — 差し込み可能なオーバーロード
  - 単体テスト 5 件（全パス）
- `crates/canvas/src/ops/mod.rs` — `pub mod text;` 追加
- `crates/canvas/Cargo.toml` — `font8x8 = { workspace = true }` 追加

### 実装済み（フェーズ7-6: text-flow plugin + host handler）

- `crates/panel-api/src/services.rs` — `TEXT_RENDER_TO_LAYER` service 名追加
- `crates/plugin-sdk/src/services.rs` — `services::text_render::render_to_layer()` descriptor builder 追加
- `plugins/text-flow/Cargo.toml` — workspace member として新設（`cdylib` + `rlib`、`plugin-sdk` 依存）
- `plugins/text-flow/panel.altp-panel` — Panel DSL 新設（テキスト入力・フォントサイズ・カラー・座標・描画ボタン）
- `plugins/text-flow/src/lib.rs` — plugin handler 新設
  - `init()` / `sync_host()`
  - `update_text()` / `update_font_size()` / `update_x()` / `update_y()`
  - `render_text()` — `emit_service(&services::text_render::render_to_layer(...))` 発行
- `apps/desktop/src/app/services/text_render.rs` — host handler 新設
  - `handle_text_render_service_request` — service request ルーター
  - `render_text_to_active_layer` — `render_text_to_bitmap_edit` → `apply_bitmap_edits_to_active_layer` → dirty rect 更新
  - `parse_color_hex` 補助関数
  - テスト 2 件（空テキスト / 有効テキスト）
- `apps/desktop/src/app/services/mod.rs` — `mod text_render;` および `handle_text_render_service_request` 呼び出し追加
- `Cargo.toml`（workspace） — `"plugins/text-flow"` を members に追加

### 実装済み（フェーズ7-7: プロファイラスクリプト）

- `scripts/profile-render.ps1` — `cargo test -p render` を N 回実行し、タイミング JSON を `logs/` へ出力
- `scripts/profile-canvas.ps1` — `cargo test -p canvas` を N 回実行し、タイミング JSON を `logs/` へ出力
- `scripts/profile-panels.ps1` — `cargo test -p panel-runtime` を N 回実行し、イテレーション別タイミング・avg/min/max JSON を `logs/` へ出力

### 実装済み（フェーズ7-5: tool child 構成）

- `crates/app-core/src/document.rs` — `ToolDefinition.children: Vec<ToolDefinition>` 追加（`#[serde(default)]`）、`Document.active_child_tool_id: String` フィールド追加、`active_child_tool_definition()` / `child_tool_definition()` メソッド追加
- `crates/app-core/src/command.rs` — `Command::SelectChildTool { child_id: String }` 追加
- `crates/app-core/src/document.rs` — `SelectChildTool` / `SelectTool` / `SetActiveTool` apply 実装（child reset）
- `crates/panel-runtime/src/host_sync.rs` — `active_child_tool_id` / `active_child_tool_label` / `child_tools_json` を tool JSON へ追加
- `crates/plugin-sdk/src/host.rs` — `host::tool::active_child_tool_id()` / `active_child_tool_label()` / `child_tools_json()` getter 追加
- `crates/plugin-sdk/src/commands.rs` — `commands::tool::select_child_tool()` descriptor builder 追加
- `crates/panel-runtime/src/dsl_panel.rs` — `"tool.select_child"` → `Command::SelectChildTool` DSL マッピング追加
- `apps/desktop/src/app/command_router.rs` — `Command::SelectChildTool` を tool panel sync 経路に追加
- `crates/storage/src/project_sqlite.rs` — `Document` 構造体初期化に `active_child_tool_id: String::new()` 追加
- `tools/builtin/pen.altp-tool.json` — `children` 配列（通常・乗算）追加
- `plugins/tool-palette/src/lib.rs` — `ACTIVE_CHILD_TOOL_ID` / `ACTIVE_CHILD_TOOL_LABEL` / `CHILD_TOOLS_JSON` 定数・sync 追加、`select_child_tool` handler 追加
- `plugins/tool-palette/panel.altp-panel` — 子ツール表示セクション追加
- `apps/desktop/src/app/tests/commands.rs` — `execute_command_select_child_tool_updates_active_child_tool_id` テスト追加
- `apps/desktop/src/app/tests/service_dispatch_tests.rs` — `snapshot_create_service_increases_snapshot_count` / `snapshot_restore_service_restores_document` テスト追加

### 実装済み（フェーズ7-8: 最終フェーズ文書整理）

- `docs/IMPLEMENTATION_STATUS.md` — フェーズ7全サブフェーズの実装記録を追加、進行中→完了に更新
- `docs/CURRENT_ARCHITECTURE.md` — 2026-03-14 に更新、`services/export.rs` / `services/snapshot.rs` / `services/text_render.rs`・`snapshot_store.rs`・`history.rs`・`canvas/ops/text`・`plugins/text-flow` を反映
- `docs/MODULE_DEPENDENCIES.md` — 2026-03-14 に更新、`plugins/text-flow` を組み込みパネル crate 一覧と依存グラフに追加
- `docs/ROADMAP.md` — フェーズ7の完了条件に ✓ を追記、完了宣言を追加

## バグ修正 (2026-03-15)

### Undo/Redo 修正（BitmapPatch 方式への移行）

フェーズ7-0〜7-2b で実装した replay 方式の Undo/Redo に 2 件のバグがあった。

**問題1**: StrokeSegment ごとに個別の `HistoryEntry` が積まれており、1 ストローク = 多数の undo ステップになっていた。

**問題2**: FloodFill/LassoFill の replay が broken。undo 後に `composited_bitmap` が透明になる → redo で再実行すると透明キャンバスに塗るため見た目が変わる。

**修正内容**:

- `crates/app-core/src/history.rs`
  - `HistoryEntry` に `BitmapPatch` variant 追加（panel_id / layer_index / dirty / before: CanvasBitmap / after: CanvasBitmap）
  - `BitmapOp` は後方互換のため残置（新規生成しない）

- `crates/app-core/src/document/bitmap.rs`
  - `CanvasBitmap::extract_region(start_x, start_y, width, height)` — 矩形部分ビットマップ抽出

- `crates/app-core/src/document/layer_ops.rs`
  - `Document::clone_panel_layer_bitmap(panel_id, layer_index)` — ストローク開始前スナップショット取得
  - `Document::capture_panel_layer_region(panel_id, layer_index, dirty)` — パネルローカル座標系で dirty 領域取得
  - `Document::restore_panel_layer_region(panel_id, layer_index, x, y, bitmap)` — undo/redo 時の領域書き戻し（recomposite + page dirty rect 返却）

- `apps/desktop/src/app/mod.rs`
  - `PendingStroke` 構造体（panel_id / layer_index / before_layer / dirty）追加
  - `DesktopApp.pending_stroke: Option<PendingStroke>` フィールド追加

- `apps/desktop/src/app/services/project_io.rs`
  - `execute_paint_input`: Stamp/StrokeSegment は `PendingStroke` でバッチ化（ストローク開始時にレイヤー丸ごとキャプチャ、dirty rect 蓄積）
  - `execute_paint_input`: FloodFill/LassoFill は即時 `BitmapPatch` 生成
  - `commit_stroke_to_history`: ポインタ Up 後に `before_layer.extract_region` → `capture_panel_layer_region` → `BitmapPatch` を push

- `apps/desktop/src/app/input.rs`
  - `CanvasPointerAction::Up` 時に `commit_stroke_to_history()` 呼び出し追加

- `apps/desktop/src/app/services/mod.rs`
  - `execute_undo` / `execute_redo`: `BitmapPatch` variant で `restore_panel_layer_region` を呼ぶ実装に刷新
  - undo/redo 後に `sync_ui_from_document()` を呼び出すよう修正

### パネルボタン 2 回目クリック無効バグ修正

**問題**: layer visibility / blend mode ボタンを 2 回目以降クリックしたとき反応しない。

**根本原因**: 既にフォーカス済みのボタンを再クリックすると `begin_panel_interaction` が `false` を返す → canvas 処理へフォールスルー → `canvas_input.is_drawing = true` → pointer Up がキャンバス up として処理される → `handle_panel_pointer` が呼ばれない。

**修正内容**:

- `apps/desktop/src/app/panel_dispatch.rs`
  - `PanelEvent::Activate` ハンドラで `refresh_panel_surface_if_changed(changed)` の戻り値を返す代わりに `true` を返すよう変更
  - パネルボタンにヒットした場合は常に処理済みとしてキャンバスへのフォールスルーを防ぐ

### pen-settings ボタン非表示バグ修正 (DSL `||` 演算子非対応)

**問題**: `<when test={state.supports_pressure || state.supports_antialias || state.supports_stabilization}>` が常に `false` になる。

**根本原因**: `evaluate_expression` が `||` / `&&` を未サポート。式が `state.` で始まるため `lookup_json_path` に `"supports_pressure || state.supports_antialias || ..."` というパスを渡し、存在しない → `Null` → `false`。

**修正内容**:

- `crates/panel-runtime/src/dsl_panel.rs`
  - `evaluate_expression` に `||` / `&&` チェックを追加（`!=` / `==` より前にチェックして低優先度として動作させる）
  - テスト `expression_evaluator_supports_or_and_and_operators` 追加

### lasso bucket ギザギザエッジ修正 (point_in_polygon バグ)

**問題**: 斜め辺を持つラッソ選択の塗りつぶし結果がギザギザになる。

**根本原因**: `point_in_polygon` の交点 x 計算式の分母に `.abs()` が誤って付いていた。上向き辺（`y2 < y1`）では `y2-y1` が負になるが、`.abs()` で正に変換されると交点 x の符号が逆転し内部/外部判定が誤る。

**修正内容**:

- `crates/canvas/src/ops/mod.rs`
  - `point_in_polygon` の `(y2 - y1).abs().max(f32::EPSILON)` を `(y2 - y1)` に修正
  - (`(y1 > y) != (y2 > y)` 条件により horizontal 辺では除算されないため除算ゼロの危険なし)
  - テスト `lasso_fill_triangular_region_diagonal_edges` 追加（斜め辺を持つ三角形の塗りつぶし）

## バグ修正 (2026-03-15) — ROADMAP 候補タスク

### [bug/performance] 縮小時アンチエイリアス修正

**問題**: ズームアウト時にキャンバスのフチがジャギー・線がブツブツ途切れる。

**根本原因**: `crates/render/src/compose.rs` の CPU 合成パスがニアレストネイバーのみ。GPU 側（`wgpu_canvas.rs`）は既に `FilterMode::Linear` だが、CPU 合成バッファが先に粗くなっていた。

**修正内容**:

- `crates/render/src/compose.rs`
  - `blit_canvas_with_transform()`: scale < 1.0 の場合に bilinear 補間パスを追加（scale >= 1.0 は既存の `build_source_axis_runs` パスを維持）
  - `blit_scaled_rgba_region()`: destination が source より小さい（縮小）場合に bilinear 補間パスを追加
  - bilinear アルゴリズム: 4近傍ピクセルの双線形加重平均。境界は端ピクセルへ clamp。

- `crates/render/src/tests/dirty_tests.rs`
  - `blit_canvas_with_transform_bilinear_at_zoom_out`: 8x8 チェッカーパターンを scale=0.5 で 4x4 に描画→出力がグレー（ニアレストネイバーなら純白）
  - `blit_scaled_rgba_region_bilinear_at_scale_down`: 4x4 チェッカーパターンを 2x2 に縮小→出力がグレー

---

### [bug] パネル通過時の描画破壊修正

**問題**: UIパネルが上を通過するたびにキャンバスのコマ表示が崩れる。

**根本原因**: パネル移動・非表示時に `mark_panel_surface_dirty()` のみ呼ばれ、`append_canvas_host_dirty_rect()` が呼ばれていなかった。パネルが通過した領域のキャンバス背景が再描画されなかった。

**修正内容**:

- `apps/desktop/src/app/panel_dispatch.rs`
  - `drag_panel_interaction()` の `PanelDragState::Move` ブランチ: `move_panel_to()` の前に `panel_presentation.panel_rect()` で以前の矩形をキャプチャし、変更後に `append_canvas_host_dirty_rect(rect)` を呼ぶ
  - `execute_host_action()` の `HostAction::MovePanel`: 同様に `panel_rect()` キャプチャ + `append_canvas_host_dirty_rect()`
  - `execute_host_action()` の `HostAction::SetPanelVisibility`: 同様に `panel_rect()` キャプチャ + `append_canvas_host_dirty_rect()`

- `apps/desktop/src/app/tests/panel_dispatch_tests.rs`
  - `drag_panel_move_marks_canvas_host_dirty`: パネルドラッグ移動後に `pending_canvas_host_dirty_rect` が `Some` になることを検証

---

## イベント駆動パネル再描画 (2026-03-15)

### 実装済み

- `crates/panel-runtime/src/registry.rs`
  - `dirty_panels: BTreeSet<String>` フィールド追加
  - `mark_dirty(panel_id: &str)` — 指定パネルを dirty としてマーク（未登録 ID は無視）
  - `mark_all_dirty()` — 全登録パネルを dirty としてマーク
  - `has_dirty_panels() -> bool` — dirty パネルが存在するか確認
  - `dirty_panel_count() -> usize` — dirty パネル件数
  - `sync_dirty_panels(document, ...) -> BTreeSet<String>` — dirty パネルのみ `update()` を呼び、変更セットを返す。呼び出し後 dirty 集合はクリアされる
  - `register_panel` で登録時に自動 dirty マーク
  - `sync_document` / `sync_document_panels` を削除（全呼び出し元を新 API へ移行）

- `apps/desktop/src/app/mod.rs`
  - `needs_ui_sync: bool` / `ui_sync_panel_ids: BTreeSet<String>` フィールド削除

- `apps/desktop/src/app/present_state.rs`
  - `sync_ui_from_document()` → `panel_runtime.mark_all_dirty()` + `mark_panel_surface_dirty()`
  - `sync_ui_from_document_panels(ids)` → 各 id に `panel_runtime.mark_dirty(id)` + `mark_panel_surface_dirty()`

- `apps/desktop/src/app/present.rs`
  - `if self.needs_ui_sync { ... }` ブロックを `if self.panel_runtime.has_dirty_panels() { ... }` に置き換え
  - `sync_document` / `sync_document_panels` の条件分岐を `sync_dirty_panels` の単一呼び出しに簡略化
  - フレームループに全パネルスキャンのコードが存在しない状態を実現

- `apps/desktop/src/app/bootstrap.rs`
  - `panel_runtime.sync_document(...)` を `mark_all_dirty() + sync_dirty_panels(...)` に変更

- `crates/panel-runtime/src/tests.rs`
  - `sync_dirty_panels_skips_panels_not_marked_dirty` — dirty でないパネルは再描画されないことを検証
  - `mark_dirty_causes_only_that_panel_to_be_synced` — mark_dirty した Panel だけが sync されることを検証
  - `mark_all_dirty_marks_every_registered_panel` — mark_all_dirty が全パネルを対象にすることを検証
  - `mark_dirty_unknown_panel_id_is_ignored` — 未登録 ID を mark_dirty しても dirty 集合に追加されないことを検証

---

## アクティブパネル枠線表示 (2026-03-15)

### 実装済み

- `crates/render/src/overlay_plan.rs`
  - `CanvasOverlayState` に `active_ui_panel_rect: Option<PixelRect>` フィールド追加
    （フォーカス中の UI パネルの画面座標矩形。`Some` のとき枠線を描画する）

- `crates/render/src/compose.rs`
  - `ACTIVE_UI_PANEL_BORDER: [u8; 4] = [0x42, 0xa5, 0xf5, 0xff]` 定数追加（水色）
  - `compose_active_panel_border(frame, overlay, dirty_rect)` — L4 フレームへパネル枠線を描画するパブリック関数追加
    - `overlay.active_ui_panel_rect` が `None` のときは何もしない
    - `dirty_rect` によるクリップ差分描画に対応

- `crates/render/src/lib.rs`
  - `compose_active_panel_border` を公開 API としてエクスポート

- `apps/desktop/src/app/present.rs`
  - 全体再構築パス・差分更新パス両方の `overlay_state` に `active_ui_panel_rect` を設定
    （`panel_presentation.focused_target()` → `panel_presentation.panel_rect(panel_id)` で取得）
  - `compose_ui_panel_frame` / `compose_ui_panel_region` 呼び出し後に `compose_active_panel_border` を呼ぶよう変更

- `crates/render/src/tests/overlay_tests.rs`
  - `compose_active_panel_border_draws_border_when_rect_is_some` — 枠線色が描画されることを検証
  - `compose_active_panel_border_no_op_when_rect_is_none` — rect が None のとき何も描画されないことを検証

---

## pen-settings UI 改善 (2026-03-15)

### 実装済み

- `plugins/pen-settings/panel.altp-panel`
  - 「現在のツール」セクション: `active_tool_label` / `pen_name` のみ表示し、ID・管轄 plugin・描画 plugin のデバッグ情報を削除
  - 「太さ」セクション: `<row>` タグでスライダーと数値入力欄を横並びに変更。冗長な `pen_name` / `tool_label: size px` テキストを削除
  - 「描画特性」セクション: 手ぶれ補正の冗長テキスト (`手ぶれ補正: {state.stabilization}%`) を削除（スライダーラベルに表示済みのため）

- `crates/panel-runtime/src/dsl_panel.rs`
  - テスト `row_layout_produces_slider_and_input_as_children` 追加: `<row>` タグが `PanelNode::Slider` と `PanelNode::TextInput` の 2 子を正しく生成することを検証

---

## pen-settings サイズ表示バグ修正 (2026-03-15)

### 問題

`pen-settings` パネルのサイズスライダーが内部の対数スケール位置（0〜1000）をラベルに表示していたため、
「サイズ: 250」と「Pen Width: 10px」のように 2 つの数値が食い違って表示されていた。

### 修正内容

- `crates/panel-api/src/lib.rs`
  - `PanelNode::Slider` に `display_value: Option<usize>` フィールド追加
    （`Some` のとき内部スライダー位置の代わりにこの値をラベルに表示する）

- `crates/panel-runtime/src/dsl_panel.rs`
  - `"slider"` DSL 要素のパース時に `display_value` 属性を読み取るよう追加

- `crates/render/src/panel.rs`
  - `PanelNode::Slider` レンダリング時に `display_value` が `Some` のときはその値を使用
    （`"{label}: {shown_value}"` 形式で表示）

- `plugins/pen-settings/panel.altp-panel`
  - サイズスライダーに `display_value={state.size}` を追加
    → スライダーラベルが「サイズ: 10」と表示され、テキストの「Pen Width: 10px」と一致する

---

## 目標アーキテクチャとの残差

| 集中箇所 | 残課題 |
|----------|--------|
| `DesktopApp` | panel/runtime 橋渡しと orchestration がまだ大きい |
| `Document` | tool / pen runtime state をまだ広く持っている |
| `canvas::CanvasRuntime` | tool 実行が host 主導（plugin 主導への移行は未着手） |
| Undo/Redo | BitmapPatch 方式で stroke / flood fill 両方対応済み |

## 実務メモ

- 「今どう実装されているか」はコードと `CURRENT_ARCHITECTURE.md` を優先する
- 「どうあるべきか」は `ARCHITECTURE.md` を優先する
- 「次に何を崩さず進めるか」は `ROADMAP.md` を優先する
- フェーズ完了ごとの文書同期は `IMPLEMENTATION_STATUS.md` → `CURRENT_ARCHITECTURE.md` → `MODULE_DEPENDENCIES.md` を最小セットとして固定する
- コード変更後に文書を追記する順序を守り、文書だけを先行させない
- フェーズ0の判断基準として、`canvas` / `panel-runtime` / `plugin-sdk` 系の命名と配置規約を文書で先に固定した

## 7-bugfix バグ修正内容 (2026-03-15)

ブラックボックステスト結果 JSON に基づき次のバグを修正した。

### 修正済みバグ

- **BitmapPatch Undo/Redo**: `execute_undo()` / `execute_redo()` を replay 方式 (`HistoryEntry::BitmapOp`) で実装。`BitmapEditRecord` を記録し、`CanvasRuntime::default()` で再生することで正しく元に戻せるようにした。
- **パネルボタン 2 回目クリック**: `pen-settings` パネルの `||` DSL 演算子対応（`panel-dsl` のパーサー修正）。
- **lasso bucket の `point_in_polygon`**: 外積判定の符号バグを修正。
- **app.save のコマンド戻り値**: `app.save` ボタンは `emit_service` 経由で保存するため `dispatch_panel_event_with_command` は `Some(Command::Noop)` を返す。テストの期待値を `SaveProject` → `Noop` に修正し、`pending_jobs.len() == 1` で保存ジョブのキューを検証するよう変更した（`commands.rs` / `panel_dispatch_tests.rs`）。
- **workspace preset テストの競合**: `/tmp/altpaint-test.altp.json` を複数テストが共有していたため、キーボードテストが書き込んだプロジェクト状態が workspace preset テストに干渉していた。`unique_test_path("preset-project")` / `unique_test_path("preset-session")` で競合を解消した（`persistence.rs`）。
- **layers-panel DSL 回帰**: `plugins/layers-panel/panel.altp-panel` から `<text>{state.title}</text>` が誤って削除されており、`desktop_app_replaces_builtin_panels_with_phase7_dsl_variants` が失敗していた。行を復元した。

## 7-testfix テスト安定化 (2026-03-15)

### 修正内容

- **`evicts_oldest_when_full` OOM**: `SnapshotStore` のテストが `Document::default()` (2894×4093 = ~47MB) を `MAX_SNAPSHOTS+1 = 21` 個生成し、合計 ~1GB でOOMkillされていた。テスト内の `make_doc()` を `Document::new(1, 1)` に変更して解消した（`snapshot_store.rs`）。
