# altpaint 実装状況

## この文書の目的

この文書は、2026-05-15 時点の `altpaint` が**実際にどこまで実装されているか**を短く把握するための現況整理である。

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
- HTML+CSS + Wasm DOM mutation による built-in panel 実装 (Phase 10 で `.altp-panel` DSL を撤去、Phase 12 で workspace-layout も HTML 化し全 12 パネル統一)
- `crates/builtin-panels/` 配下 panel の再帰ロード
- `tools/` 配下 tool 定義の再帰ロード
- `pens/` 配下の外部ペン preset 読込
- panel local state / host snapshot / persistent config
- 4隅アンカー基準の workspace panel 配置
- 全 4 辺 + 4 角の 8 ハンドルによる手動パネルリサイズ (Phase 11、anchor 維持・最小 80x60・viewport クランプ・edge 別カーソル切替)
- パネルサイズの単一権威化: `panel.meta.json::default_size` を初期値とし、workspace 永続値を SoT とする (Phase 11 で自動サイズ追従撤去)
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
- **2026-03-16 ブラックボックステスト バグ修正**:
  - **プランB（ストローク性能）**: `crates/canvas/src/ops/stroke.rs` に `MAX_STAMP_STEPS = 64` を追加。大きなブラシで spacing が小さい場合に 1 セグメントあたりのスタンプ数を上限に制限し CPU 過負荷を防止。
  - **プランC（lasso プレビュー）**: `apps/desktop/src/app/input.rs` の `LassoPreviewChanged` ハンドラが `false` を返していたバグを修正（`true` へ変更）。ラッソドラッグ中に L3 temp overlay dirty rect が設定されず `request_redraw` が呼ばれなかった問題を解消。
  - **プランD（レイヤー表示順序）**: `crates/panel-runtime/src/host_sync.rs` の `layers_json` 生成を `.iter().rev()` に変更（UI の先頭 = 前面レイヤー）。`active_layer_ui_index` 変換を追加。`plugins/layers-panel/src/lib.rs` の `handle_layer_list` に UI index → model index 変換を追加。
  - **プランE（レイヤー名変更 UI）**: `plugins/layers-panel/panel.altp-panel` に確定ボタン追加・`rename_text` state 追加。`src/lib.rs` に `RENAME_BUF`（`thread_local! RefCell<String>`）、`update_rename_text` / `confirm_rename` ハンドラを追加。
  - **プランF（visibility/blend_mode パフォーマンス）**: `apps/desktop/src/app/command_router.rs` で `SetActiveLayerBlendMode` / `ToggleActiveLayerVisibility` を独立した match arm に分離し、`refresh_canvas_frame_region(panel_bounds)` + `append_canvas_dirty_rect(panel_bounds)` による差分更新に変更。全体 recomposite を回避。
  - **プランA（Wasm 再ビルド）**: `.\scripts\build-ui-wasm.ps1` で全 11 プラグインの `.wasm` を再ビルド。app-actions・undo/redo・panel_rect 等の export エラーを解消。
- **Phase 9F 完了 (2026-04-29)**: `crates/render/` クレート物理削除。`RenderFrame` を `apps/desktop/src/app/canvas_frame.rs::CanvasFrame` へ吸収、`PresentScene` から dummy 化されていた `base_layer` (L1) と `ui_panel_layer` (L4) を撤去、`html_panel_quads` を `panel_quads` にリネーム。`PresentTimings` から `base_upload`/`ui_panel_upload` 系フィールド削除。さらに dead code 撤去として `PanelHitKind`/`PanelHitRegion`/`PanelSurface::hit_regions` 一式、`PanelDragState::Control` ヴァリアント、`refresh_canvas_frame_region`、`panel_surface_hit_regions` profiler value を削除。HTML パネル hit-test を `html_panel_hit_at`/`html_panel_move_handle_at` に統一し、関連テストを synthetic hit-table 注入で書き直し。最終ベースライン: 127 passed / 0 failed / 6 ignored、clippy 警告 83 件 (着手前と同数)。詳細: `docs/adr/010-render-crate-removal.md`。
- **Phase 9G 完了 (2026-05-02)**: `html-panel` feature gate を完全撤去。Phase 9E で CPU パネルラスタライザを撤去した結果、`HtmlPanelEngine` 経路がパネル描画の唯一の手段となっていたが、`apps/desktop` の `html-panel` feature が default OFF のまま放置されており、`cargo run -p desktop` (feature 指定なし) では `panel_quads = &[]` / `status_quad = None` となり全パネル＋ステータスバーが画面から消える状態だった。`apps/desktop/Cargo.toml` と `crates/panel-runtime/Cargo.toml` から `[features]` テーブル削除、`panel-html-experiment` / `keyboard-types` を必須依存へ昇格、`apps/desktop` 内 14 箇所と `panel-runtime` 内 23 箇所の `cfg(feature = "html-panel")` / `cfg(not(feature = "html-panel"))` 分岐を完全撤去。clippy 警告 83 → 70 件 (13 件減)。あわせて `crates/panel-runtime/src/dsl_to_html.rs` の `PanelNode::Section` 翻訳を `<details><summary>` から `<div class="alt-section">` へ切り替え (Blitz/stylo がネストした `<details>` の primary style 解決に失敗して panic する潜在バグ回避)。詳細: `docs/adr/011-html-panel-feature-removal.md`。
- **Phase 12 完了 (2026-05-15)**: ADR 014 — `PanelTree` / `PanelNode` / `PanelView` 型と `PanelPlugin::panel_tree()` / `view()` trait method を完全撤去 (ADR 012 で宣言済みだったが残置されていた DSL 時代の中間表現 / dead code を一括清算)。並行して、唯一 Rust ネイティブ実装で残っていた `builtin.workspace-layout` (パネル表示/非表示管理 UI) を 12 番目の HTML+CSS+Wasm パネルとして再実装し、HTML 経路へ完全統一。新規サービス `workspace_layout.set_panel_visibility` と新規 host snapshot field `workspace.panels_json` を追加し、Wasm パネル handler がチェック切替で可視性を制御する経路を整備。同時に DSL 時代の `tree_query.rs` / `focus.rs` の dropdown / text_input 走査 / `TextInputEditorState` / winit IME 編集経路を撤去 (HTML パネル内部完結に統一)。約 800 行縮小、clippy 警告 84 → 76 件 (8 件減)、テスト 139 passed / 5 failed (failure はベースライン e6f84f6 と完全一致、新規 failure ゼロ)。詳細: `docs/adr/014-paneltree-removal-and-workspace-layout-html.md`。

## 現在の workspace 構成

### 中核 crate

- `app-core`
- `canvas`
- `render-types`
- `storage`
- `desktop-support`
- `panel-api`
- `panel-runtime`
- `ui-shell`
- `workspace-persistence`
- `plugin-host`
- `panel-schema`
- `plugin-macros`
- `plugin-sdk`
- `panel-html-experiment`
- `builtin-panels`
- `apps/desktop`

### workspace member の built-in panel plugin (Phase 10 で `crates/builtin-panels/` 配下に移行)

- `crates/builtin-panels/app-actions`
- `crates/builtin-panels/workspace-presets`
- `crates/builtin-panels/tool-palette`
- `crates/builtin-panels/view-controls`
- `crates/builtin-panels/panel-list`
- `crates/builtin-panels/layers-panel`
- `crates/builtin-panels/color-palette`
- `crates/builtin-panels/pen-settings`
- `crates/builtin-panels/job-progress`
- `crates/builtin-panels/snapshot-panel`
- `crates/builtin-panels/text-flow`
- `crates/builtin-panels/workspace-layout` (Phase 12 で追加、ADR 014)

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

- `panel-api`: `PanelEvent`, `HostAction`, `ServiceRequest` (Phase 12 で `PanelTree` / `PanelNode` / `PanelView` を完全撤去)
- `panel-schema`: host-Wasm 間 DTO
- `plugin-sdk`: plugin 作者向け SDK、typed service request builder、macro 再 export、`dom` モジュール (DOM mutation API)
- `plugin-macros`: `plugin-sdk` が再 export する proc-macro 実装
- `plugin-host`: `wasmtime` ベース runtime + `dom` host functions (Blitz `DocumentMutator` を Wasm に公開)
- `panel-html-experiment`: `HtmlPanelEngine` (Blitz HTML/CSS + parley + vello)
- `panel-runtime`: `BuiltinPanelPlugin` / panel registry / host snapshot sync / persistent config
- `builtin-panels`: 同梱 12 パネル定義 (HTML+CSS+Wasm) と `register_builtin_panels` orchestration
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

- これらは `crates/builtin-panels/<name>/` 配下に `panel.html` + `panel.css` + `panel.meta.json` + Rust/Wasm 実装を同居させる構成で揃っている (Phase 10 で `.altp-panel` DSL から移行)。

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

## Phase 8A 完了 (2026-03-16)

### GPU キャンバスクレートの新設

- **`crates/gpu-canvas/`** 新設
  - `GpuCanvasContext { device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue> }`
  - `GpuLayerTexture`: 1 レイヤー 1 `wgpu::Texture` (Format: `Rgba8Unorm`, Usage: `STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC | COPY_DST`)
  - `GpuCanvasPool`: `(panel_id, layer_index)` → `GpuLayerTexture` マップ。`create_layer_texture()` / `upload_cpu_bitmap()` / `get()`
  - `GpuPenTipCache`: ペン先テクスチャキャッシュ。`upload_from_preset()` でビットマップペン先を GPU テクスチャとしてアップロード
  - `supports_rgba8unorm_storage()`: アダプターが `Rgba8Unorm` の `STORAGE_READ_WRITE` をサポートするか確認（Phase 8A では `Rgba8Unorm` 固定を採用）
  - テスト: CPU 単体テスト 1 件 + GPU スモークテスト 3 件

- **`apps/desktop/src/wgpu_canvas.rs`**: `device`/`queue` を `Arc<wgpu::Device>`/`Arc<wgpu::Queue>` に変更。`device()` / `queue()` getter を追加

- **`apps/desktop/Cargo.toml`**: `gpu-canvas = { optional = true }` と `[features] gpu = ["dep:gpu-canvas"]` を追加

- **`apps/desktop/src/app/mod.rs`**: `#[cfg(feature = "gpu")] gpu_canvas_pool: Option<GpuCanvasPool>` / `gpu_pen_tip_cache: Option<GpuPenTipCache>` を `DesktopApp` に追加（初期値 None）

- **`apps/desktop/src/app/command_router.rs`**: ペン切り替え時に `#[cfg(feature = "gpu")] upload_active_pen_tip_to_gpu_cache()` を呼ぶ

- **`docs/adr/003-gpu-canvas-migration.md`**: ADR 作成 (2026-03-16, claude-sonnet-4-6)

---

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

---

## Phase 8B 完了 (2026-04-25) — GPU リソース初期化・ブラシ dispatch 実接続

作業モデル: claude-sonnet-4-6

### 実装内容

- **`apps/desktop/src/app/mod.rs`**
  - `#[cfg(feature = "gpu")] srgb_view_supported: bool` フィールド追加（初期値 `false`）
  - `install_gpu_resources(device, queue, srgb_view_supported)`: `GpuCanvasPool` / `GpuPenTipCache` / `GpuBrushDispatch` を初期化し、`sync_all_layers_to_gpu()` および `upload_active_pen_tip_to_gpu_cache()` を実行
  - `sync_all_layers_to_gpu()`: 全 Page × Panel × RasterLayer の CPU ビットマップを GPU テクスチャへアップロード。中間 Vec で borrow 競合を回避

- **`apps/desktop/src/app/command_router.rs`**
  - `AddRasterLayer | RemoveActiveLayer | SelectLayer | MoveLayer` / `AddPanel | SelectPanel | NewDocumentSized` の各 arm に `#[cfg(feature = "gpu")] self.sync_all_layers_to_gpu()` を追加

- **`apps/desktop/src/app/services/mod.rs`**
  - `execute_undo` / `execute_redo` の全 `true` 返却 arm に `#[cfg(feature = "gpu")] self.sync_all_layers_to_gpu()` を追加

- **`apps/desktop/src/app/services/project_io.rs`**
  - `load_project` 成功パスの末尾に `#[cfg(feature = "gpu")] self.sync_all_layers_to_gpu()` を追加

- **`apps/desktop/src/runtime.rs`**
  - `WgpuPresenter::new` 完了直後に `#[cfg(feature = "gpu")] self.app.install_gpu_resources(...)` を呼び出す

- **テスト (`apps/desktop/src/app/tests/gpu_tests.rs`)**
  - `install_gpu_resources_sets_all_gpu_fields_to_some`
  - `sync_all_layers_to_gpu_creates_textures_for_all_layers`

---

## Phase 8C 完了 (2026-04-25) — GPU テクスチャを表示の正本へ切り替え（単一レイヤー）

作業モデル: claude-sonnet-4-6

### 実装内容

- **`crates/gpu-canvas/src/gpu.rs`**
  - `GpuLayerTexture::create` の `TextureDescriptor` に `view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb]` を追加
  - `GpuLayerTexture::create_srgb_view()` 追加: `Rgba8UnormSrgb` フォーマットの `TextureView` を返す
  - `GpuCanvasPool::get_view(panel_id, layer_index)` 追加: `get()` → `create_srgb_view()` のショートカット

- **`apps/desktop/src/wgpu_canvas.rs`**
  - `CanvasLayerSource<'a>` enum 追加 (`Cpu(TextureSource<'a>)` / `#[cfg(feature = "gpu")] Gpu { panel_id, layer_index, width, height }`)。`&'a str` により `Copy` を維持
  - `CanvasLayer<'a>.source` を `CanvasLayerSource<'a>` に変更
  - `GpuBindGroupCache` 構造体追加 (bind_group / uniform_buffer / panel_id / layer_index / width / height)
  - `WgpuPresenter` に `canvas_gpu_bind_group_cache: Option<GpuBindGroupCache>` と `srgb_canvas_view_supported: bool` フィールド追加
  - `WgpuPresenter::new()` で `supports_rgba8unorm_storage(&adapter)` を評価して `srgb_canvas_view_supported` を設定
  - `WgpuPresenter::render()` に `#[cfg(feature = "gpu")] gpu_canvas_pool: Option<&GpuCanvasPool>` 引数を追加
  - `render()` 内で `CanvasLayerSource::Gpu` 時は `update_gpu_canvas_bind_group()` → キャッシュ bind group で描画
  - `update_gpu_canvas_bind_group()` を追加: `(panel_id, layer_index, width, height)` 変化時のみ bind group を再生成し uniform buffer を更新

- **`apps/desktop/src/app/mod.rs`**
  - `should_use_gpu_canvas_source()`: pool 有効 + `srgb_view_supported` + アクティブパネルが単一レイヤー + テクスチャ存在 → `true`
  - `gpu_canvas_pool()`: `Option<&GpuCanvasPool>` getter

- **`apps/desktop/src/app/present_state.rs`**
  - `apply_bitmap_edits`: `#[cfg(feature = "gpu")]` で GPU 経路有効時は `refresh_canvas_frame_region` をスキップし `append_canvas_dirty_rect` のみを実行（CPU ビットマップ更新は Phase 8D まで維持）

- **`apps/desktop/src/runtime.rs`**
  - `canvas_layer` 構築を `should_use_gpu_canvas_source()` で分岐: `true` → `CanvasLayerSource::Gpu { ... }`、`false` → `CanvasLayerSource::Cpu { ... }`
  - `presenter.render(scene, #[cfg(feature = "gpu")] self.app.gpu_canvas_pool())` へ変更

- **テスト (`apps/desktop/src/app/tests/gpu_tests.rs`)**
  - `should_use_gpu_canvas_source_false_without_resources`
  - `should_use_gpu_canvas_source_false_if_srgb_not_supported`
  - `should_use_gpu_canvas_source_true_for_single_layer_with_resources`
  - `should_use_gpu_canvas_source_false_for_multi_layer`
  - `layer_count_change_updates_gpu_source_decision`

### 設計制約

- 複数レイヤー時は CPU 経路を継続（Phase 8E で GPU 合成予定）
- `srgb_view_supported = false` の場合は CPU フォールバック（ハードウェア互換性の保障）
- CPU ペイントパス (`apply_bitmap_edits`) は Phase 8D まで維持

---

## Phase 8G: HTML パネル枠サイズの動的化と責務再分離 (2026-04-25) 完了

作業モデル: claude-opus-4-7 (1M context)

### 経緯

Phase 8F で HTML パネル GPU 直描画統合は完了したが、描画枠サイズが `WorkspacePanelSize::default()` の固定値で決まっており、HTML コンテンツ自然サイズと一致しない問題が残っていた。あわせて `HtmlPanelEngine` / `HtmlPanelPlugin` / `apps/desktop/runtime.rs` 間で「枠サイズ・dirty・GPU target・chrome 描画」の責務が散らばっており、枠の動的化と同時に責務再分離を行った。

### 実装内容

- **`crates/panel-html-experiment/src/engine.rs`** に状態とライフサイクルを集約
  - `measured_size: (u32, u32)` — パネルの権威サイズ。`on_load` で初期化、`on_render` で intrinsic 結果に応じて更新
  - `layout_dirty` / `render_dirty` / `pending_size_change` フラグを Engine 内で管理（Blitz の `has_changes()` バグを回避するため自前トラック継続）
  - `gpu_target: Option<PanelGpuTarget>` — Plugin から移管。`on_render` 内でリサイズ判定して再生成
  - 公開 API: `on_load(restored_size)` / `measured_size()` / `measure_intrinsic(max_w)` / `on_host_snapshot(snapshot)` / `on_input(UiEvent)` / `on_render(device, queue, renderer, scene_buf, viewport, scale, chrome)` / `take_size_change()`
  - `measure_intrinsic` は viewport (max_w, 8192) で resolve → `<body>` の `final_layout.content_size` から自然サイズを取得
  - `on_input` は `EventDriver::handle_ui_event` 経由で `:hover` / `<details>` / pointer click を Blitz に流す
  - `on_render` は viewport クランプ → resolve → content size 比較 → measured_size 変化なら GPU target 再作成 + `pending_size_change` 立て

- **`crates/panel-runtime/src/html_panel.rs`** を panel-api 接合層として薄く再構成
  - `gpu_target` / `render_dirty` / chrome 描画 / リサイズ判定はすべて Engine に委譲
  - `load(directory, restored_size: Option<(u32, u32)>)` / `from_parts(id, title, html, css, restored_size)` でロード時に永続値を流せる
  - `forward_input(UiEvent)` / `measured_size()` / `take_size_change()` / `restore_size((w, h))` を新設
  - 残った責務: ファイル I/O、host snapshot 構築（Document → JSON）、`data-action` → `HostAction` 翻訳、`PanelPlugin` trait 実装

- **`crates/panel-runtime/src/registry.rs`**
  - `html_measured_sizes() -> Vec<(String, u32, u32)>` — HTML パネル毎の現在の measured_size
  - `forward_html_input(panel_id, UiEvent) -> bool` — 指定パネルへ入力転送
  - `take_html_size_changes() -> Vec<(String, (u32, u32))>` — `render_html_panels` 中に発生したサイズ変化を吸い取り（take セマンティクス）
  - `restore_html_panel_size(panel_id, (w, h))` — 起動時 restore 用
  - `render_html_panels` 内で `take_size_change()` を吸い取り `pending_html_size_changes` に蓄積

- **`crates/ui-shell/src/lib.rs` / `workspace.rs`**
  - `set_panel_size(panel_id, w, h)` — workspace_layout の panel size を書き換える（永続化に流す）
  - `html_panel_at(x, y) -> Option<(panel_id, local_x, local_y)>` — HTML パネル領域ヒット（chrome 除外）

- **`apps/desktop/src/runtime.rs`**
  - HTML quad entry 構築から `PixelRect { width: 300, height: 220 }` 固定 fallback を削除
  - サイズは `panel_runtime.html_measured_sizes()` から取得、位置のみ `panel_rect_in_viewport` から取得
  - `render_html_panels` 後に `take_html_size_changes()` を吸い取って `set_panel_size` で workspace_layout を更新（既存 persistence 経路に乗る）

- **`apps/desktop/src/runtime/pointer.rs`**
  - `handle_mouse_cursor_moved` / `handle_mouse_button` で HTML パネル領域内の pointer を `forward_html_input` に転送（PointerDown/Up/Move）
  - `:hover` / `<details>` 開閉 / `<button>` click が動的に動くようになった

- **`apps/desktop/src/app/bootstrap.rs`**
  - `apply_ui_state_to_panel_system` 内で workspace_layout に永続化された HTML パネル size を `restore_html_panel_size` 経由で Engine に流し込む

### 受け入れ結果

- HTML パネル起動直後の枠が `panel.html` のコンテンツ自然サイズに一致（300x220 固定撤廃）
- 永続化された `WorkspacePanelSize` が起動時に復元
- `:hover` / `<details>` / button click が動的に動作
- viewport 上限クランプ（コンテンツが viewport を超えてもパネルは画面内に収まる）
- chrome 領域への pointer は move handle へ、body 領域は HTML エンジンへ正しく振り分け
- `cargo test -p panel-html-experiment --lib`: 27 passed
- `cargo test -p panel-runtime --features html-panel --lib`: 26 passed
- `cargo test -p desktop --features html-panel --bins`: 110 passed
- `cargo clippy --workspace --all-targets --features html-panel`: 新規コード起因 0 error

### スコープ外

- 手動リサイズハンドル UI（角ドラッグ等）
- IME / キーボード入力の Blitz 転送
- スクロール対応（viewport クランプのみ）
- `<select>` ドロップダウン（popup レイヤ責務の設計が別途必要なため次フェーズ）

---

## Phase 8F: HTML パネル GPU 直描画統合 (2026-04-25) 完了

作業モデル: claude-opus-4-7 (1M context)
関連 ADR: [docs/adr/007-html-panel-experiment.md](adr/007-html-panel-experiment.md)

### 経緯

第 1 稿で自作 HTML/CSS サブセット → 第 2 稿で Blitz + Vello-CPU → **第 3 稿（本フェーズ）で Blitz + Vello-GPU 直描画**に進化。Phase 8 系「CPU-GPU 通信最小化」と整合する形で、HTML パネルが CPU readback ゼロで実画面に CSS 反映される。

### 実装内容

- **`crates/panel-html-experiment/`** — `blitz-* 0.3.0-alpha.2` + `anyrender 0.8` + `anyrender_vello 0.8` + `vello 0.8` へアップグレード（`anyrender_vello_cpu` 削除）
  - `engine.rs` — `build_scene(&mut vello::Scene, w, h, scale)` で `blitz_paint::paint_scene` を呼んで Vello シーン構築。CPU pixel API は完全削除。`collect_action_rects()` で CSS layout 後の絶対座標を返す。`document_dirty()` は自前トラック
  - `gpu.rs` — `PanelGpuTarget`: パネル毎の `wgpu::Texture`（`Rgba8Unorm` + `STORAGE_BINDING|TEXTURE_BINDING|COPY_SRC|COPY_DST` + `view_formats=[Rgba8UnormSrgb]`）
  - `binding.rs` / `action.rs` は維持（DOM 非依存純粋ロジック）
  - テスト 21 件（GPU 必須 1 件）

- **`crates/panel-runtime/src/html_panel.rs`**
  - `render_gpu(device, queue, &mut Renderer, &mut Scene, w, h, scale) -> RenderOutcome<'_>` で altpaint 所有テクスチャに直描画
  - `RenderOutcome::{Rendered, Skipped}` enum で dirty 制御をテスタブル化
  - `panel_tree()` は空ツリーを返し DSL レンダ経路から自動除外
  - `handle_event` を Override し `Activate` を `<button data-action="...">` で解決
  - テスト 17 件（GPU 必須 3 件: red pixel readback / Skipped 判定 / resize 再生成）

- **`crates/panel-runtime/src/registry.rs`**
  - `install_gpu_context(Arc<Device>, Arc<Queue>)` で `vello::Renderer` を集約構築（`AaSupport::area_only`、失敗時はログ）
  - `html_panel_ids()` / `render_html_panels(&[(id, w, h)], scale) -> Vec<HtmlPanelGpuFrame>`

- **`crates/panel-api/src/lib.rs`** — `PanelPlugin::as_any_mut` を default `None` で追加（後方互換、既存実装変更不要）

- **`apps/desktop/src/wgpu_canvas.rs`** — `PresentScene::html_panel_quads: &[GpuPanelQuad<'_>]` 追加。bind_group キャッシュを panel_id ベースで保持し、L4 ui_panel_layer 直後にクワッド描画

- **`apps/desktop/src/runtime.rs`** — WgpuPresenter 構築直後に `install_gpu_context`、各フレームで `render_html_panels` を呼んで `GpuPanelQuad` 配列を構築

- **`crates/ui-shell/src/surface_render.rs`** — `collect_floating_panels` で empty children を除外（HTML パネルを DSL 経路から自動除外）

- **`apps/desktop/Cargo.toml`** — `html-panel = ["panel-runtime/html-panel"]` feature 公開

- **`plugins/app-actions/`** — `panel.html` / `panel.css` を最終形に。`data-action` は service 統一

### ベースライン比測定（Windows / clean release build）

| 項目 | ベースライン | 第 2 稿 (CPU 経路) | **第 3 稿 (GPU 直)** |
|------|-------------|-------------------|---------------------|
| ビルド時間 | 117s | 335s (+186%) | **215s (+84%)** |
| バイナリサイズ | 21.97 MiB | 29.43 MiB (+34%) | **36.46 MiB (+66%)** |

GPU 直描画では CPU→GPU 通信ゼロを実現する代わりに vello GPU シェーダ・blitz-paint 0.3-alpha 等で +6.7 MiB を追加。一方ビルド時間は anyrender_vello_cpu 廃止で大幅短縮。

### 受け入れ結果

- `cargo test --workspace`（default features）: **378 passed / 0 failed**
- `cargo test --workspace --features desktop/html-panel`: **391 passed / 0 failed**（テキスト描画関連のテスト 5 件追加）
- `cargo clippy -p panel-html-experiment -p panel-runtime --features panel-runtime/html-panel --all-targets`: 新規コード起因 0 warning
- `cargo build -p desktop --release --features html-panel`: 成功（3m35s, 36.46 MiB）
- CSS 反映: `gpu_html_panel_renders_red_pixel_when_css_red_background` で texture readback により実 pixel レベルで検証
- テキスト描画: `ascii_text_renders_dark_pixels_in_text_rect` / `japanese_text_renders_dark_pixels` / `full_panel_html_renders_visible_text` / `panel_background_color_is_preserved` / `ascii_text_emits_glyph_run_in_scene` で texture 上の glyph 出現を pixel レベルで検証

### Phase 8F 修正: テキスト描画 (2026-04-25)

`crates/panel-html-experiment/Cargo.toml` で `blitz-dom = { default-features = false }` としていたために、`system_fonts` feature（`parley/system` を有効化）が抜け落ちていた。これにより parley が `Collection { system_fonts: false }` で動作し、Windows / dwrite 経由のフォントロードが行われず glyph runs が空のまま vello scene に積まれず、結果として「枠は描画されるがテキストは一切出ない」状態になっていた。

修正: `features = ["system_fonts"]` を明示。これだけで日本語含む system font 経路が開通し、`panel.html` のテキストが画面に出るようになる。

GPU テスト群が並列実行で wgpu Adapter / Device 生成競合により flaky だった問題は、`Mutex<()>` ベースの `gpu_test_lock()` で本モジュール内 GPU テストを直列化する形で解消した。

### スコープ外（次々フェーズ）

- HiDPI（scale != 1.0）対応
- HTML パネル数増加時のテクスチャアトラス化
- HTML パネルのドラッグ移動
- 他ビルトインパネルの HTML 化
- `<script>` / JS 実行
- vello::Renderer 構築コストの遅延化

---

## Phase 8E 完了 (2026-04-25) — GPU 塗りつぶし + レイヤー合成 + BlendMode::Custom 削除

作業モデル: claude-opus-4-7 (1M context)

### 実装内容

- **`crates/gpu-canvas/src/fill.rs`** 新設
  - `GpuFillDispatch::new` / `dispatch_flood_fill(source, target, seed, fill_rgba)` / `dispatch_lasso_fill(target, polygon, aabb, fill_rgba)`
  - `FloodFillOutcome { iterations, pixels_changed }` 公開
  - Ping-pong マスクテクスチャ (Rgba8Unorm) による iterative region growing。32 iter ごとに atomic counter readback で早期収束検出

- **`crates/gpu-canvas/src/shaders/`** 追加シェーダー
  - `flood_fill_step.wgsl` — 4-connect で同色ピクセルを 1 ステップ拡張
  - `fill_apply.wgsl` — mark 1.0 のピクセルに source-over で fill_color を書き込む
  - `lasso_fill_mark.wgsl` — point-in-polygon (ray casting) によるポリゴン内部判定
  - `layer_composite.wgsl` — 単一レイヤーを composite テクスチャへ source-over / multiply / screen / add でブレンド
  - `composite_clear.wgsl` — dirty 領域の透明クリア

- **`crates/gpu-canvas/src/composite.rs`** 新設
  - `GpuLayerCompositor::new` / `recomposite(composite, layers, dirty)`
  - `CompositeLayerEntry { color, mask, blend_code, visible }`
  - Bottom → top に 1 layer ずつ dispatch する iterative compositor。`visible == false` はスキップ

- **`crates/gpu-canvas/src/gpu.rs`** 拡張
  - `GpuCanvasPool::ensure_composite_texture(panel_id, w, h)` — panel 毎の合成テクスチャを遅延作成（同サイズなら no-op）
  - `get_composite` / `get_composite_view` / `upload_mask` / `get_mask` / `remove_mask` / `read_back_composite`
  - 内部ヘルパー `read_back_texture` を追加して `read_back_full` と共有

- **`crates/app-core/src/document.rs`**
  - `BlendMode::Custom(String)` variant を完全削除
  - `BlendMode::parse_name` は未知文字列で `Some(Normal)` を返す（後方互換、保存済み Custom は Normal に落ちる）
  - `BlendMode::gpu_code() -> u32` (Normal=0, Multiply=1, Screen=2, Add=3) 追加
  - `crates/app-core/src/document/layer_ops.rs` から `CustomBlendFormula` / `BlendExpr*` / `BlendExprParser` を完全削除
  - `crates/app-core/src/document/tests.rs` の custom blend 2 テストを `parse_name_falls_back_to_normal_for_unknown_strings` / `gpu_code_matches_shader_switch_codes` に置き換え

- **`apps/desktop/src/wgpu_canvas.rs`**
  - `CanvasLayerSource::GpuComposite { panel_id, width, height }` variant 追加
  - `GpuBindGroupCache` に `kind: GpuBindGroupKind { Single, Composite }` フィールド追加
  - `update_gpu_canvas_bind_group` が `Gpu` / `GpuComposite` 両方に対応

- **`apps/desktop/src/app/mod.rs`**
  - `gpu_fill: Option<GpuFillDispatch>` / `gpu_compositor: Option<GpuLayerCompositor>` フィールド追加
  - `install_gpu_resources` に両者を初期化し、`recomposite_all_panels` を最後に呼ぶ
  - `sync_all_layers_to_gpu` がレイヤー本体に加え mask / composite テクスチャも同期
  - `should_use_gpu_canvas_source` を `canvas_layer_source_kind() -> Option<GpuCanvasSourceKind>` に拡張
  - `recomposite_panel(panel_id, dirty)` / `recomposite_all_panels()` 追加

- **`apps/desktop/src/runtime.rs`**
  - `CanvasLayerSource` の組み立てを `GpuCanvasSourceKind::Single/Composite` で分岐

- **`apps/desktop/src/app/services/project_io.rs`**
  - `execute_gpu_fill(panel_id, layer_index, input, edits)` 新設: `PaintInput::FloodFill` / `LassoFill` を GPU dispatch に振り分け、`GpuPatchSnapshot` を Undo 履歴に push
  - Stroke 経路完了時にも `recomposite_panel` を発火

- **`apps/desktop/src/app/command_router.rs`**
  - レイヤー系 Command / パネル系 Command の完了時に `recomposite_all_panels` を追加
  - `SetActiveLayerBlendMode` / `ToggleActiveLayerVisibility` はパネル内ローカル dirty で `recomposite_panel` を呼ぶ

- **`apps/desktop/src/app/services/mod.rs`**
  - `execute_undo` / `execute_redo` の `GpuBitmapPatch` ハンドラで復元後に `recomposite_panel(panel_id, dirty)` を発火

- **`crates/storage/src/project_sqlite.rs`**
  - `BlendMode::Custom(_)` シリアライズ分岐を削除

### テスト

- **CPU ユニット**: `BlendMode::gpu_code` / `parse_name` 未知文字列 → Normal の動作、params バイトレイアウト 3 種（flood_fill / fill_apply / lasso_mark / composite / composite_clear）
- **GPU smoke**（`try_init_device` / `supports_rgba8unorm_storage` で未対応環境はスキップ）:
  - `gpu_flood_fill_fills_connected_region_only`
  - `gpu_lasso_fill_triangle_paints_interior`
  - `gpu_layer_compositor_single_layer_passthrough`
  - `gpu_layer_compositor_invisible_layer_is_skipped`
- 実機で 21/21 の gpu-canvas テストが通ることを確認

### 設計制約

- `BlendMode::Custom` は完全削除（ユーザー決定 — alpha 期間のため後方互換不要）。保存済み Custom 設定はロード時に Normal へ格下げ
- FloodFill の seed 色取得は composite テクスチャがあればそこから、なければ active layer 自身から（単一レイヤーでは等価）
- FloodFill の収束は 32 iter ごとの atomic readback + 最大 iter 上限 `FLOOD_FILL_ITERATION_CAP = 8192`
- Composite テクスチャは panel ごとに 1 枚。レイヤー数・panel サイズが変わらなければ再利用する
- `gpu` feature 無効ビルドでは従来の CPU 合成経路 (`composite_panel_bitmap_region` / `compose_canvas_host_region`) がそのまま動作（runtime 分岐で生存） — *Phase 9A で `gpu` feature 自体が撤廃され、本記述は履歴情報。現在は GPU 経路一本*

---

## Phase 9A 完了 (2026-04-26) — `gpu` feature default-on 化、CPU フォールバック削除

作業モデル: claude-opus-4-7 (1M context)

### 実装内容

- **`apps/desktop/Cargo.toml`**
  - `gpu` feature を完全削除（`gpu-canvas` を必須依存化、`optional = true` 撤廃）

- **`crates/gpu-canvas/Cargo.toml` / `src/lib.rs` / `src/tests.rs`**
  - `gpu` feature を撤廃し `wgpu` を必須依存化
  - 全モジュール (`format_check` / `brush` / `composite` / `fill` / `gpu`) の `#[cfg(feature = "gpu")]` を削除

- **`apps/desktop/src/`**
  - 9 ファイル (mod.rs / wgpu_canvas.rs / runtime.rs / present_state.rs / services/mod.rs / services/project_io.rs / services/text_render.rs / command_router.rs / background_tasks.rs / tests/mod.rs) から `#[cfg(feature = "gpu")]` / `#[cfg(not(feature = "gpu"))]` を全削除
  - GPU フィールド (`gpu_canvas_pool` / `gpu_pen_tip_cache` / `gpu_brush` / `gpu_fill` / `gpu_compositor`) は無条件で保持
  - `srgb_view_supported` フィールド削除 / `install_gpu_resources` 引数から除去 / `canvas_layer_source_kind()` の sRGB 分岐削除
  - `WgpuPresenter` から `srgb_canvas_view_supported` フィールド + 公開 getter 削除（`format_check::supports_rgba8unorm_storage` 呼び出しと診断ログは存続）

- **`apps/desktop/src/app/present_state.rs::apply_bitmap_edits`**
  - GPU/CPU 分岐を撤廃し `append_canvas_dirty_rect(dirty)` のみ実行する形へ単純化

- **キャンバス用 `refresh_canvas_frame_region` 呼び出しの削除** (4 箇所)
  - `services/mod.rs::execute_undo` / `execute_redo`、`services/text_render.rs::render_text_to_active_layer`、`command_router.rs` の SetActiveLayerBlendMode/ToggleActiveLayerVisibility 分岐
  - 関数定義は `crates/render/` 削除 (Phase 9F) と一括除去するため `#[allow(dead_code)]` 付きで存続

- **テスト**
  - `should_use_gpu_canvas_source_false_if_srgb_not_supported` 削除（前提が消滅）
  - `try_init_device` を `HighPerformance` + `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` opt-in / `supports_rgba8unorm_storage` ガード付きへ更新
  - `should_use_gpu_canvas_source_false_for_multi_layer` → `should_use_gpu_canvas_source_true_for_multi_layer_via_composite` に書き換え（GPU 常時有効では composite 経由で `true` を維持）
  - `layer_count_change_updates_gpu_source_decision` → `layer_count_change_switches_gpu_source_kind`（Single → Composite 切替を直接検証）
  - `interaction.rs:1409` のコメント更新

### 設計制約

- alpha 期間として、Rgba8Unorm STORAGE_READ_WRITE 非対応の旧 GPU を切り捨て (ROADMAP / ADR-006 既出)
- `should_use_gpu_canvas_source` はテスト専用関数になったため `#[cfg(test)]` 化
- `wgpu_canvas` 内の CPU `CanvasLayerSource::Cpu` 経路と CPU `canvas_frame` 自体は存続（runtime fallback）。完全な駆除は Phase 9F で `crates/render/` 削除と一括対応
- text_render 経路は CPU bitmap → GPU 同期が未配線。Phase 9A スコープ外として既存挙動を維持

### 検証

- `cargo build -p desktop` 通過（警告 2 件、いずれも既存の snapshot_store 由来）
- `cargo build -p desktop --features html-panel` 通過
- `cargo check --release --workspace` 通過
- `cargo test --workspace` 通過（116 desktop tests + 全クレート ok）
- `cargo clippy --workspace --all-targets` 警告増加なし
- `feature = "gpu"` の cfg 参照が本流コードから 0 件
- `srgb_view_supported` の参照が 0 件
- キャンバス用 `refresh_canvas_frame_region` 呼び出しが 0 件（定義のみ存続）

## Phase 9B 完了 (2026-04-26) — `render-types` クレート抽出

### 背景
Phase 9A 完了後、`render` クレートには「純データ DTO + CPU 合成 + パネル CPU
ラスタ + テキスト描画」が同居していた。Phase 9C/9D (装飾・overlay GPU 化) と
9E (DSL パネル GPU 化) を並列着手するため、純データ DTO を分離して CPU 実装
側を触らずに consume できる形を作る必要があった。

### 作業内容
- 新規クレート `crates/render-types/` を作成 (依存: `app-core` のみ)
- 移動した型・関数 (12 シンボル + 派生関数群):
  - `PixelRect`, `TextureQuad`, `CanvasScene`, `prepare_canvas_scene`
  - `FramePlan`, `CanvasPlan`, `CanvasCompositeSource`, `OverlayPlan`,
    `PanelPlan`, `PanelSurfaceSource`
  - `CanvasOverlayState`, `PanelNavigatorOverlay`, `PanelNavigatorEntry`
  - `LayerGroupDirtyPlan`, `LayerGroup`
  - `union_dirty_rect`, `union_optional_rect`, `brush_preview_dirty_rect`
  - `map_canvas_dirty_to_display_with_transform`,
    `map_view_to_canvas_with_transform`, `canvas_texture_quad`,
    `canvas_drawn_rect`, `brush_preview_rect`,
    `brush_preview_rect_for_diameter`, `map_canvas_point_to_display`,
    `exposed_canvas_background_rect`,
    `exposed_canvas_background_rect_from_scenes`
- 既存テスト (15 件) を `render-types/src/tests/` に同伴移動
- `render`/`canvas`/`ui-shell`/`apps/desktop` の Cargo.toml と use 文を一括
  更新 (互換 re-export なし)

### 残ったもの (`render` クレート)
- `RenderFrame`, `RenderContext` (Phase 9F で削除予定)
- `compose.rs` 全 (Phase 9C/9D で GPU 化)
- `panel.rs` 全 + UI 型 (Phase 9E で GPU 化)
- `text.rs` 全 (Phase 9C/9E で parley/cosmic-text 置換予定)
- `status.rs` 全 (Phase 9C で GPU 化)

### 9B でやらなかったこと
- panel.rs UI 型 (PanelHitKind/PanelHitRegion/FloatingPanel/PanelRenderState
  等) の移動 — 9E で GPU 直描画化と一緒に panel-runtime か render-types の
  どちらに置くか確定させるため保留

### 検証
- `cargo test --workspace` 通過 (render-types 単独 14 件 + 既存全テスト緑)
- `cargo build --workspace` 通過
- `cargo clippy --workspace --all-targets` 警告 83 件 (ベースライン同数)
- `cargo tree -p render-types` で wgpu/vello/panel-api/fontdb/ab_glyph 非依存
  を確認 (依存は app-core のみ)
- `use render::(PixelRect|FramePlan|CanvasPlan|OverlayPlan|PanelPlan|...)` の
  workspace 内参照が 0 件

## Phase 9C-1 完了 (2026-04-26) — Solid Quad パイプライン + 矩形 GPU 化

### 背景
Phase 9C は「ステータス / デスクトップ背景 / アクティブパネル枠」を GPU 直
描画化するサブフェーズ。9C-1 ではテキストを除く単色矩形 (背景・キャンバス
枠 fill・キャンバスホスト枠線・アクティブ UI パネル枠線) を GPU で塗る基盤
を整備し、`crates/render/` の対応する CPU 関数を撤去した。9C-2 でステータス
テキストを GPU (parley + Vello) 化したら L1 `background_frame` 自体を廃止する。

### 作業内容
- 新規モジュール `apps/desktop/src/frame/solid_quad.rs`:
  - `SolidQuad { rect, color }` DTO
  - `pixel_rect_to_ndc(rect, w, h) -> [f32; 4]` (wgpu Y 反転を吸収)
  - `build_background_solid_quads(window, host, display) -> Vec<SolidQuad>`
    (ウィンドウ背景 + キャンバス領域 + 4 マージン + ホスト枠線 4 矩形)
  - `build_foreground_solid_quads(active_rect)` (アクティブ枠線 4 矩形)
  - 純関数テスト 11 件 (full screen / quadrant / margins / 枠線 / None ケース)
- `apps/desktop/src/wgpu_canvas.rs`:
  - 専用 `SolidQuadPipeline` を新設 (32 バイト uniform: rect_ndc + color)
  - `PresentScene` に `background_quads: &[SolidQuad]` と
    `foreground_quads: &[SolidQuad]` フィールドを追加
  - レンダーパス内で L0 (background_quads) → L1〜L5 → L6 (foreground_quads)
    の順で描画
- `apps/desktop/src/app/present_state.rs`:
  - `background_solid_quads()` / `foreground_solid_quads()` accessor を追加
- `apps/desktop/src/app/present.rs`:
  - `compose_active_panel_border` 呼び出し 3 箇所を削除
  - `clear_canvas_host_region` 経由の `pending_background_dirty_rect`
    再描画ループを撤去 (GPU L0 が毎フレーム塗るため)
- `crates/render/src/compose.rs`:
  - `compose_background_frame` を「テキスト専用」最小実装へ縮約
    (window fill / canvas host fill / 枠線描画コードを削除)
  - `compose_status_region` の `fill_rect(APP_BACKGROUND)` を透明クリアに変更
  - 削除した関数: `compose_active_panel_border`,
    `compose_desktop_frame`, `compose_canvas_host_region`,
    `clear_canvas_host_region`, `fill_canvas_host_background`,
    `blit_canvas_with_transform`, `stroke_rect_region`
- `crates/render/src/lib.rs`: 削除関数の re-export を撤去
- `crates/desktop-support/src/config.rs`: `ACTIVE_UI_PANEL_BORDER` 定数を
  追加 (旧 compose.rs 内の private const から昇格)
- 既存テストの整理:
  - `crates/render/src/tests/overlay_tests.rs`:
    `compose_status_region_*` / `compose_active_panel_border_*` テスト削除
  - `crates/render/src/tests/frame_plan_tests.rs`:
    `compose_desktop_frame_writes_panel_and_canvas_regions` テスト削除
  - `crates/render/src/tests/dirty_tests.rs`:
    `blit_canvas_with_transform_bilinear_at_zoom_out` テスト削除 (CPU 経路廃止)
  - `apps/desktop/src/app/tests/interaction.rs::pan_view_updates_canvas_without_status_recompose`:
    `compose_dirty_canvas_base` profiler 期待値と
    `background_dirty_rect.is_some()` を反転 (GPU 化により dirty 不要)

### 採用判断
- 単色矩形は **専用 solid-quad パイプライン** (Vello は使用せず)
  - 理由: 単色 1 矩形に Vello はオーバーヘッド大 / 既存
    `PRESENT_SHADER` はテクスチャサンプル前提で色固定描画と相性悪い
- 枠線は **4 矩形分解** (top/bottom/left/right) で 1px を表現
  - 理由: shader 分岐より CPU 側で 4 quad に分けた方がシェーダが単純
- NDC 変換ヘルパは `apps/desktop/src/frame/solid_quad.rs` に配置し
  `render-types` を描画関心事で汚染しない

### 残ったもの (Phase 9C-2 で対応)
- `compose_background_frame` (テキスト専用、9C-2 で削除)
- `compose_status_region` (CPU テキスト経路、9C-2 で parley + Vello に置換)
- L1 `background_frame: RenderFrame` フィールド (9C-2 で廃止)
- `pending_background_dirty_rect` / `LayerGroup::background` (9C-2 で削除)
- `crates/render/src/text.rs::draw_text_rgba` (パネル CPU ラスタは 9E)
- `crates/render/src/status.rs::status_text_bounds` (9C-2 で parley 経路に統一)

### 検証
- `cargo test -p render` → 10 件 通過
- `cargo test -p desktop` → 127 件 通過 (parallel テストの flaky failure
  4 件は test_threads=1 で全て通過する pre-existing collision)
- `cargo clippy --workspace --all-targets` → 警告 83 件 (ベースライン同数)
- `cargo build --release -p desktop` → コンパイル成功 (WGSL も valid)

## Phase 9D 完了 (2026-04-26) — L3 一時オーバーレイ完全 GPU 化

### 背景
Phase 9C-1 で L0 (背景・キャンバス枠・アクティブ UI パネル枠) を solid quad
GPU パイプラインへ移した。Phase 9D は L3 (`temp_overlay_layer`) 担当。
`crates/render/src/compose.rs` に残っていた overlay 関連 5 サブ要素 (brush
preview / active panel mask / lasso preview / panel creation preview /
panel navigator) を全て GPU 直描画化し、`compose_temp_overlay_frame` /
`compose_temp_overlay_region` および補助関数 (draw_canvas_overlay,
draw_active_panel_mask, draw_brush_preview, draw_lasso_preview,
draw_overlay_line, draw_panel_creation_preview, draw_panel_navigator) を撤去。
`stroke_rect` 補助関数および overlay private 定数も削除。

### 作業内容
- 新規モジュール `apps/desktop/src/frame/overlay_quad.rs`:
  - `CircleQuad { center_px, radius, thickness, color }` DTO
  - `LineQuad { start_px, end_px, thickness, color }` DTO
  - `build_overlay_solid_quads(plan, overlay) -> Vec<SolidQuad>`
    (active panel mask / panel creation preview / panel navigator を
     AABB 単色矩形に展開)
  - `build_overlay_circle_quads(plan, overlay) -> Vec<CircleQuad>`
    (ブラシプレビュー円リング)
  - `build_overlay_line_quads(plan, overlay) -> Vec<LineQuad>`
    (ラッソ線分カプセル)
  - 純関数テスト 6 件 (空入力 / active mask / brush preview / lasso /
     panel navigator / panel creation preview)
- `apps/desktop/src/wgpu_canvas.rs`:
  - 新規 `CircleQuadPipeline` (SDF 円リング、64B uniform)
  - 新規 `LineQuadPipeline` (カプセル SDF、80B uniform)
  - WGSL `CIRCLE_QUAD_SHADER` / `LINE_QUAD_SHADER` 追加
  - `PresentScene` のフィールド変更:
    - 削除: `temp_overlay_layer: FrameLayer<'a>`
    - 追加: `overlay_solid_quads`, `overlay_circle_quads`, `overlay_line_quads`
  - `WgpuPresenter::temp_overlay_layer: Option<UploadedLayerTexture>` 削除
  - レンダーパス順序: L0 → L1 → L2 → **L3a overlay_solid → L3b overlay_circle
    → L3c overlay_line** → L4 → L5 → L6
  - solid quad slot プールを `[background; overlay_solid; foreground]` の
    3 連結に拡張し、各レンジを描画
- `crates/render/src/compose.rs`:
  - 削除関数: `compose_temp_overlay_frame`, `compose_temp_overlay_region`,
    `draw_canvas_overlay`, `draw_active_panel_mask`, `draw_brush_preview`,
    `draw_lasso_preview`, `draw_overlay_line`, `draw_panel_creation_preview`,
    `draw_panel_navigator`, `stroke_rect`
  - 削除 private 定数: `PANEL_NAVIGATOR_*`, `ACTIVE_PANEL_*`, `PANEL_PREVIEW_*`
  - `fill_rect` は `compose_status_region` で継続使用するため残置
- `crates/render/src/lib.rs`: 削除関数の re-export を撤去
- `crates/render-types/`:
  - `OverlayPlan` 型を削除 (新ビルダが直接 `CanvasOverlayState` を消費)
  - `FramePlan::overlay_plan()` メソッド削除
  - `lib.rs` の re-export 整理
- `crates/desktop-support/src/config.rs`: 11 個の overlay 色定数を昇格
  (`ACTIVE_PANEL_*`, `PANEL_PREVIEW_*`, `PANEL_NAVIGATOR_*`,
   `BRUSH_PREVIEW_RING`, `LASSO_LINE`)
- `crates/desktop-support/src/profiler/types.rs`: `PresentTimings` から
  `temp_overlay_upload` / `temp_overlay_upload_bytes` フィールド削除
- `apps/desktop/src/app/`:
  - `mod.rs`: `temp_overlay_frame: Option<RenderFrame>` フィールド削除
  - `present.rs`: full rebuild と dirty パスから L3 CPU 合成経路を削除。
    `pending_temp_overlay_dirty_rect` は redraw シグナルとして残存
  - `present_state.rs`: `temp_overlay_frame()` accessor 削除、
    `overlay_quads(window_w, window_h) -> (Vec<SolidQuad>, Vec<CircleQuad>,
    Vec<LineQuad>)` 追加
- `apps/desktop/src/runtime.rs`: PresentScene へ overlay quad 配列を供給
- 既存テストの整理:
  - `crates/render/src/tests/frame_plan_tests.rs` から
    `overlay_frame_draws_panel_navigator_when_multiple_panels_exist` 削除
  - `crates/render-types/src/tests/overlay_plan_tests.rs` ファイル削除
  - `apps/desktop/src/app/tests/interaction.rs::overlapping_panel_and_canvas_overlay_updates_union_dirty_rects`:
    `compose_dirty_overlay` profiler 期待アサーションを削除

### 採用判断
- Phase 9C-1 と同様、SDF プリミティブごとに小型パイプライン
  (`CircleQuadPipeline` / `LineQuadPipeline`) を並べる方針を採用
  - 円とカプセルは数学的に異なる SDF (中心距離 vs. 線分距離)。1 本に
    まとめると分岐が増え、将来の拡張 (アロー線分など) にも不利
  - vello は overhead が大きいためスキップ
- AABB 単色矩形 (mask / fill / stroke / navigator) は既存
  `SolidQuadPipeline` の slot プールを単一連結 `Vec` に拡張して再利用
- 線幅・リング太さは CPU 経路の現行値を踏襲 (ブラシリング 1.0px、
  ラッソ線 1.25px) し、SDF 1px フェザリングで AA を実装
- `pending_temp_overlay_dirty_rect` は redraw シグナルとして残存
  (テストの hover/lasso 振る舞い検証を維持)

### 残ったもの (Phase 9C-2 / 9E / 9F で対応)
- `compose_background_frame` / `compose_status_region` (Phase 9C-2 で削除)
- L1 `background_frame: RenderFrame` フィールド (Phase 9C-2 で廃止)
- `crates/render/src/panel.rs` (Phase 9E)
- `crates/render/src/text.rs::draw_text_rgba` (Phase 9C-2 / 9E)
- `crates/render/` クレート完全削除 (Phase 9F)

### 検証
- `cargo test -p render-types` → 13 件 通過
- `cargo test -p render` → 9 件 通過
- `cargo test -p desktop` → 133 件 通過 (うち 6 件は `#[ignore]`)
- `cargo test --workspace` → 全クレート通過
- `cargo clippy --workspace --all-targets` → 警告 83 件 (ベースライン同数、
  新規警告なし)
- `cargo build --release -p desktop` → コンパイル成功 (WGSL も valid)

## Phase 9E 完了 (2026-04-26) — DSL パネル / ステータスバー GPU 化 + テスト基準再設定

Phase 9C-2 / 9D 完了時点で残っていた CPU 経路は次の三つだった:
- DSL `.altp-panel` 9 個の CPU ラスタライザ (`render::panel::rasterize_panel_layer`)
- ステータスバー CPU テキスト (`render::status::compose_status_region`)
- font8x8 / ab_glyph / fontdb によるテキストレンダリング (`render::text`)

Phase 9E は 5 サブフェーズで上記をすべて駆除し、DSL パネルを Phase 8F の
`HtmlPanelEngine` (Blitz HTML/CSS + parley + vello) 経路に乗せ替えた。

### サブフェーズ別の作業内容

- **9E-1**: `panel-runtime/src/dsl_to_html.rs` を新規追加。`PanelTree → (html, css)`
  の純関数翻訳器を実装。PanelNode 11 種すべて (Column / Row / Section / Text /
  Button / Slider / Dropdown / TextInput / LayerList / ColorPreview / ColorWheel)
  を HTML プリミティブへ写像。`data-action` ペイロード仕様を確定 (`alt:slider:<id>`,
  `alt:select:<id>`, `alt:input:<id>`, `alt:layer-select`, `alt:color:<id>`)。
- **9E-2**: `DslPanelPlugin` に `HtmlPanelEngine` を内蔵し `render_gpu()` /
  `gpu_target()` / `forward_input()` / `collect_action_rects()` を追加。CPU 経路と
  並走可能な状態にし、フォーカス / details 開閉状態 / Tab 順 を翻訳再生成で保持。
- **9E-3**: `ui-shell::surface_render::rebuild_panel_bitmaps` 系を削除し L4
  `ui_panel_layer` を 1×1 dummy 化。`render::panel::rasterize_panel_layer` /
  `measure_panel_size` / `draw_node` および private 描画関数群、`RasterizedPanelLayer`
  / `FloatingPanel` / `PanelRenderState` / `PanelFocusTarget` / `PanelTextInputState`
  型を削除。`PanelHitKind` / `PanelHitRegion` のみ ui-shell 互換型として存続。
- **9E-4**: `apps/desktop/src/frame/status_panel.rs` を新規追加し `StatusPanel`
  (`HtmlPanelEngine` 内蔵) を導入。`crates/render/src/text.rs` (font8x8 / ab_glyph
  / fontdb 一式) と `crates/render/src/status.rs` を完全削除。`render` の依存から
  `font8x8` / `ab_glyph` / `fontdb` を除去。
- **9E-5**: ピクセル比較系テストを実装非依存アサート (色矩形 / 暗色ピクセル数 /
  DOM 構造) に書き換える方針を確立。`crates/render-types/src/test_support.rs` を
  新規追加し `find_dark_pixels` / `find_color_in_rect` を共通化 (`vello::Scene`
  glyph run カウントは vello 直接依存となるため module docs にのみパターン記載)。
  9E-3 で `#[ignore]` した 4 件を整理 (3 件削除 / 1 件書き換え PASS 化)。
  ピクセル比較系テスト自体は 9E-4 までで既に存在しなくなっており追加書き換えは
  不要だった。

### 撤去した関数 / 型 / 依存の総括

- 撤去関数 (9E-3 / 9E-4 で削除済み):
  - `render::panel::rasterize_panel_layer`
  - `render::panel::measure_panel_size`
  - `render::panel::draw_node` および 11 種の private ノード描画関数
  - `render::status::compose_status_region`
  - `render::status::status_text_bounds`
  - `render::text::draw_text_rgba`
  - `render::text::wrap_text_lines`
  - `render::text::measure_text_width`
  - `ui-shell::surface_render::rebuild_panel_bitmaps`
  - `ui-shell::surface_render::compose_panel_surface[_incremental]`
- 撤去型:
  - `RasterizedPanelLayer` / `FloatingPanel` / `PanelRenderState`
  - `PanelFocusTarget` / `PanelTextInputState` / `MeasuredPanelSize`
- 撤去依存 (`crates/render/Cargo.toml`):
  - `font8x8` / `ab_glyph` / `fontdb`

### 採用判断 (詳細は ADR 009)

- 翻訳経路: **案 B (DSL → HTML 翻訳器)** を採用 (案 A: Blitz スタイル直接構築 /
  案 C: 専用 GPU パイプライン)
- 工数最小化のため `HtmlPanelEngine` (Phase 8F 既存) を全面再利用
- color-wheel は `<input type="color">` で alpha 期間の妥協を許容 (post-alpha で
  カスタム widget 化予定)
- LayerList の多選択 / D&D は現行 CPU 実装非サポートを踏襲 (将来 Phase で個別検討)
- Slider / Dropdown / TextInput / ColorWheel の `alt:*` 配線は 9E-3 で未完了
  (Phase 9F 以降で配線)

### 9E スコープ縮小事項 (Phase 9F に移送)

- `PresentScene::base_layer` / `ui_panel_layer` の **型自体** の物理削除
- `html_panel_quads` → `panel_quads` リネーム
- `crates/render/` クレート物理削除

これらは render クレート削除と同じレイヤー整理タスクで PR 粒度を揃えるため
Phase 9F に集約する。9E 完了時点では「中身が dummy / 1×1 テクスチャ」状態。

### 検証 (9E-5 完了時)

- `cargo test --workspace` → 113 passed / 17 failed / 6 ignored
  (失敗 17 件はすべて pre-existing: builtin.app-actions Wasm hits 未取得・
  shortcut 解決失敗・ICU 言語データ欠落など。dev HEAD baseline と同数)
- `cargo clippy --workspace --all-targets` → 警告 83 件 (baseline と同数、新規 0)
- `cargo build --release -p desktop` → 成功
- `crates/render-types/src/test_support.rs` の単体テスト 2 件追加 通過
- `app::tests::interaction::focus_refresh_does_not_trigger_ui_update` を弱検証に
  書き換えて PASS 化 (baseline では `#[ignore]`)

### Phase 9F 着手準備

- `crates/render` 残存表面: `RenderFrame` / `RenderContext` / `compose_*` / `blit_*`
  / `fill_rgba_block` / `scroll_canvas_region` / `PanelHitKind` / `PanelHitRegion`
- すべて Phase 9F でクレート物理削除と同期して撤去予定。`PresentScene::base_layer`
  / `ui_panel_layer` 型と `html_panel_quads` → `panel_quads` リネームも 9F で対応。
