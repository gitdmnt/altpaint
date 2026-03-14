# altpaint 実装状況

## この文書の目的

この文書は、2026-03-14 時点の `altpaint` が**実際にどこまで実装されているか**を短く把握するための現況整理である。

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
| 7-0〜7-2b | Undo/Redo 基盤 | 2026-03-13 |
| 7-3 | PNG export・汎用ジョブ基盤 | 2026-03-13 |
| 7-4 | snapshot handler | 2026-03-14 |
| 7-5 | tool child 構成 | 2026-03-14 |
| 7-5b | TextRenderer trait + text-flow 設計 | 2026-03-14 |
| 7-6 | text-flow plugin + host handler | 2026-03-14 |
| 7-7 | プロファイラ PowerShell スクリプト | 2026-03-14 |
| 7-8 | 最終フェーズ文書整理 | 2026-03-14 |
| 7 | 再編後の機能拡張 | **完了** (2026-03-14) |

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

## 目標アーキテクチャとの残差

| 集中箇所 | 残課題 |
|----------|--------|
| `DesktopApp` | panel/runtime 橋渡しと orchestration がまだ大きい |
| `Document` | tool / pen runtime state をまだ広く持っている |
| `canvas::CanvasRuntime` | tool 実行が host 主導（plugin 主導への移行は未着手） |
| Undo/Redo | `CommandHistory` + canvas 接続は完成済み；さらなる粒度改善は今後の候補 |

## 実務メモ

- 「今どう実装されているか」はコードと `CURRENT_ARCHITECTURE.md` を優先する
- 「どうあるべきか」は `ARCHITECTURE.md` を優先する
- 「次に何を崩さず進めるか」は `ROADMAP.md` を優先する
- フェーズ完了ごとの文書同期は `IMPLEMENTATION_STATUS.md` → `CURRENT_ARCHITECTURE.md` → `MODULE_DEPENDENCIES.md` を最小セットとして固定する
- コード変更後に文書を追記する順序を守り、文書だけを先行させない
- フェーズ0の判断基準として、`canvas` / `panel-runtime` / `plugin-sdk` 系の命名と配置規約を文書で先に固定した
