# altpaint 現在アーキテクチャ

## この文書の目的

この文書は、2026-06-12 時点の `altpaint` が**コード上で実際にどう分割され、どこに責務が集中しているか**を整理するための現況文書である。

この文書は理想図ではない。現状の事実をまとめる。

- 目標構造は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 実装到達点は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- compile-time 依存は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)

## 現在の全体像

現在の `altpaint` は、次の 4 つの集約点を中心に動いている。

1. `apps/desktop::DesktopApp`
   - desktop host 全体の状態遷移と副作用の統合点
   - GPU ペイントリソース（`GpuCanvasPool` / `GpuBrushDispatch` / `GpuFillDispatch` / `GpuLayerCompositor` / `GpuPenTipCache`）の所有者
2. `app-core::Document`
   - ドメイン状態と編集コマンドの中心
3. `panel-runtime::PanelRuntime`
   - panel registry / `BuiltinPanelPlugin`（HTML+CSS+Wasm）/ host snapshot sync / panel config / hit 収集の中心
4. `ui-shell::PanelPresentation`
   - panel workspace layout / focus / hit-test の中心

描画は Phase 8〜9 で GPU 化が完了している。キャンバスは `gpu-canvas` の compute shader、装飾・overlay は `wgpu_canvas.rs` の専用 quad パイプライン、パネルとステータスバーは `panel-html::HtmlPanelEngine`（Blitz + vello）による GPU 直描画で、CPU ラスタライザ経路は存在しない。

パネルは Phase 10〜13 で `.altp-panel` DSL を撤去し、`crates/builtin-panels/<name>/`（`panel.html` + `panel.css` + `panel.meta.json` + Rust→Wasm）の HTML 経路に全 12 パネルが統一されている。`crates/render` / `crates/panel-dsl` は物理削除済みで存在しない。

workspace は 28 メンバー（ライブラリ 15、ビルトインパネル 12、デスクトップアプリ 1）。

## 現在のプロジェクト構造と責務

### 1. `apps/desktop`

現在の desktop host であり、次を担う。

- `winit` event loop（`src/runtime.rs` の `DesktopRuntime`）
- `wgpu` presenter（`src/wgpu_canvas.rs` の `WgpuPresenter`）
- OS 入力の正規化（`src/runtime/pointer.rs` / `src/runtime/keyboard.rs`）
- `DesktopApp` によるアプリ状態と副作用の統合
- canvas 入力と panel 入力の振り分け
- project / session / workspace preset / tools / pens / builtin panels の起動時読込
- GPU ペイントリソースの所有と paint dispatch（`install_gpu_resources` / `execute_paint_input`）
- `PresentScene`（背景 quad / canvas / overlay quad / panel quad / status quad）の組み立てと提示

主なモジュール:

- `src/main.rs`
- `src/runtime.rs` + `src/runtime/{pointer,keyboard,tests}.rs`
- `src/app/mod.rs`（`DesktopApp` 定義、GPU リソースフィールド、`install_gpu_resources` / `sync_all_layers_to_gpu`）
- `src/app/bootstrap.rs`（session / project / workspace preset 復元、`register_builtin_panels` 呼び出し）
- `src/app/command_router.rs`
- `src/app/panel_dispatch.rs`
- `src/app/panel_config_sync.rs`
- `src/app/io_state.rs`
- `src/app/present_state.rs`
- `src/app/present.rs`（`prepare_present_frame`、dirty panel sync、`refresh_html_panel_hit_tables`）
- `src/app/canvas_frame.rs`（`CanvasFrame`: CPU 側キャンバスフレーム表現。GPU 経路が標準だが width/height は viewport 計算で参照される）
- `src/app/cursor.rs`（リサイズハンドル等のカーソル切替）
- `src/app/services/{mod,project_io,workspace_io,workspace_layout,tool_catalog,export,snapshot,text_render,gpu_sync}.rs`
- `src/app/background_tasks.rs`
- `src/app/snapshot_store.rs`
- `src/app/input.rs`
- `src/app/commands.rs`
- `src/app/state.rs`
- `src/app/drawing.rs`（`canvas::CanvasRuntime` の再公開のみ）
- `src/frame/mod.rs`（`DesktopLayout`）
- `src/frame/geometry.rs`（レイアウト計算と座標変換）
- `src/frame/solid_quad.rs`（`SolidQuad`、pixel→NDC 変換）
- `src/frame/overlay_quad.rs`（`CircleQuad` / `LineQuad`: ブラシプレビュー・lasso・マスク等の overlay DTO）
- `src/frame/status_panel.rs`（ステータスバー専用 `HtmlPanelEngine` ラッパ、`StatusSnapshot`）
- `src/wgpu_canvas.rs`

補足:

- `wgpu_canvas.rs` は `SolidQuadPipeline` / `CircleQuadPipeline` / `LineQuadPipeline` の専用 GPU パイプラインを内部に持ち、`PresentScene { background_quads, canvas_layer, overlay_solid_quads, overlay_circle_quads, overlay_line_quads, panel_quads, foreground_quads, status_quad }` を合成提示する。
- `CanvasLayerSource` は `Cpu` / `Gpu` / `GpuComposite` の 3 系で、通常運転では `GpuLayerCompositor` の合成済みテクスチャを直接提示する。
- `services/gpu_sync.rs::sync_gpu_bitmaps_to_cpu` は保存前にのみ GPU テクスチャを readback して `Document` の CPU bitmap を最新化する（ストローク中は CPU bitmap を書き換えないため）。

### 2. `crates/app-core`

現在のドメイン中核であり、次を担う。

- `Document`
- `Work -> Page -> Panel -> LayerNode`
- `Command`
- `CommandHistory`（`HistoryEntry::BitmapPatch` による undo/redo）
- `CanvasBitmap`
- `CanvasViewTransform`
- `PenPreset` / `ToolDefinition`
- `WorkspaceLayout`
- `WorkspaceUiState` / `PluginConfigs`（project 保存と session 保存で共有する UI 永続化 DTO。旧 `workspace-persistence` クレートから `src/workspace.rs` へ統合）
- `BitmapEdit` / `PaintInput` / compositor などの共有 paint primitive

主なモジュール:

- `src/command.rs`
- `src/coordinates.rs`
- `src/document.rs` + `src/document/{bitmap,layer_ops,pen_state,tests}.rs`
- `src/error.rs`
- `src/history.rs`
- `src/paint_params.rs`
- `src/painting.rs`
- `src/workspace.rs`

補足:

- workspace ローカル依存を持たない最下層クレート。`winit` / `wgpu` / `wasmtime` 非依存。外部依存は `serde` / `serde_json` / `thiserror`（`serde_json` は `PluginConfigs` の統合に伴い追加）。
- `Document` が tool catalog や pen runtime state を広く持つ状態は継続している。

### 3. `crates/canvas`

現在の canvas runtime / input / bitmap op 層であり、次を担う。

- `CanvasRuntime`
- `CanvasInputState`
- view-to-canvas 変換（`view_mapping.rs`）
- canvas gesture state machine（`gesture.rs` / `advance_pointer_gesture`）
- `Document` からの paint context 構築（`context_builder.rs` / `build_paint_context`）
- built-in bitmap paint plugin
- stamp / stroke / flood fill / lasso fill / composite / text の bitmap op
- GPU dispatch 用の stamp 位置計算（`compute_stamp_positions`）
- panel rect preview の render bridge

主なモジュール:

- `src/runtime.rs`
- `src/context.rs`（`ResolvedPaintContext`）
- `src/context_builder.rs`
- `src/registry.rs`（`PaintPluginRegistry` / `default_paint_plugins`）
- `src/input_state.rs`
- `src/view_mapping.rs`
- `src/gesture.rs`
- `src/render_bridge.rs`
- `src/plugins/builtin_bitmap.rs`
- `src/ops/`（stamp / stroke / flood_fill / lasso_fill / composite / text）
- `src/tests/`

補足:

- 依存は `app-core` と `render-types` のみ。
- GPU 経路でも `CanvasRuntime` が `BitmapEdit` 差分（dirty rect の典拠）を計算し、GPU compute shader への dispatch 自体は `apps/desktop` 側（`services/project_io.rs::execute_paint_input`）が行う。

### 4. `crates/gpu-canvas`

Phase 8 で新設された GPU キャンバスペイント層。wgpu compute shader でブラシ・塗りつぶし・レイヤー合成を実行する。

- `GpuCanvasContext`（`Arc<wgpu::Device>` / `Arc<wgpu::Queue>`）
- `GpuCanvasPool` / `GpuLayerTexture`（`(panel_id, layer_index)` → レイヤーテクスチャ、upload / readback）
- `GpuPenTipCache`（ペン先テクスチャキャッシュ）
- `GpuBrushDispatch`（stamp / stroke / erase の compute dispatch）
- `GpuFillDispatch` / `FloodFillOutcome`（flood fill / lasso fill）
- `GpuLayerCompositor` / `CompositeLayerEntry`（レイヤー合成）

主なモジュール:

- `src/gpu.rs`（context / pool / pen tip cache）
- `src/brush.rs`
- `src/fill.rs`
- `src/composite.rs`
- `src/format_check.rs`（`Rgba8Unorm` storage サポート確認）
- `src/shaders/`（`brush_stamp` / `brush_stroke` / `erase_stamp` / `fill_apply` / `flood_fill_step` / `lasso_fill_mark` / `composite_clear` / `layer_composite` の 8 WGSL）

補足:

- Phase 9A で `gpu` feature を撤廃し必須依存となった。実行時は GPU 経路が標準で、`DesktopApp` の GPU フィールドが `None` になるのは headless テスト時のみ。

### 5. `crates/render-types`

純データ DTO 専用の描画計画ライブラリ。依存は `app-core` のみで、`wgpu` / `panel-api` 非依存。

- `CanvasScene` / `FramePlan` / `CanvasPlan` / `OverlayPlan`
- `PixelRect` / `TextureQuad` / `LayerGroup` / `LayerGroupDirtyPlan`
- `CanvasOverlayState` / `PanelNavigatorOverlay` / `PanelNavigatorEntry`
- canvas quad / UV / dirty rect 写像、画面座標 <-> canvas 座標変換
- ブラシプレビュー矩形計算、dirty rect の union

主なモジュール:

- `src/frame_plan.rs` / `src/canvas_plan.rs` / `src/canvas_scene.rs`
- `src/overlay_plan.rs`
- `src/dirty.rs` / `src/brush_preview.rs` / `src/layer_group.rs`
- `src/test_support.rs` / `src/tests/`

補足:

- 旧 `crates/render` の CPU compose / panel rasterize / text 描画は Phase 9C〜9E で GPU 化され、Phase 9F でクレートごと物理削除済み（`docs/adr/010-render-crate-removal.md`）。本クレートが描画計画の唯一の crate である。

### 6. `crates/panel-runtime`

現在の panel runtime 層であり、次を担う。

- `PanelRuntime`（panel registry、dirty panel 管理、GPU frame 管理、event dispatch）
- `BuiltinPanelPlugin`（`HtmlPanelEngine` + `WasmPanelRuntime` を束ねる唯一のパネル実装。`PanelEvent::Keyboard` の Wasm `panel_handle_keyboard` への転送と `handles_keyboard_event()` を持つ — Phase 13）
- host snapshot 同期（`host_sync.rs` の `HostSnapshotCache` / `build_host_snapshot_cached`）
- panel persistent config（`config.rs`）
- `panel.meta.json` のパース（`meta.rs` の `PanelMeta` / `PanelSizeMeta`、`default_size` 必須）
- `CommandDescriptor` → `Command` 変換（`commands.rs`）
- GPU 非依存の hit 収集（`registry.rs::collect_panel_hits` — Phase 13）

主なモジュール:

- `src/lib.rs`
- `src/registry.rs`（`PanelRuntime` / `PanelGpuFrame` / `RuntimeDispatchResult` / `RuntimeKeyboardResult`）
- `src/builtin_plugin.rs`
- `src/host_sync.rs`
- `src/config.rs`
- `src/commands.rs`
- `src/meta.rs`

補足:

- 依存は `app-core` / `panel-api` / `panel-schema` / `plugin-host` / `panel-html`。
- DSL 時代の `dsl_loader.rs` / `dsl_panel.rs` / `dsl_to_html.rs` は Phase 10〜12 で削除済み。

### 7. `crates/panel-html`

HTML パネル描画エンジン（旧名 `panel-html-experiment`、依存最小化リアーキテクトでリネーム）。Phase 9E 以降パネル描画の唯一の経路である。

- `HtmlPanelEngine`（Blitz による HTML/CSS パース・レイアウト、vello による GPU 直描画）
- `resolve_action_rects`（GPU 非依存のレイアウト解決と hit 矩形収集 — Phase 13）
- `PanelSizeConstraints`（CSS min/max 由来のリサイズクランプ — Phase 11）
- `ActionDescriptor` / `AltpKind` / `parse_data_action`（`data-action` 属性のパース）
- `PanelGpuTarget`（パネル毎の GPU テクスチャターゲット）

主なモジュール:

- `src/engine.rs`
- `src/action.rs`
- `src/gpu.rs`

補足:

- workspace ローカル依存なし。外部依存は `blitz-dom` / `blitz-html` / `blitz-paint` / `blitz-traits`、`taffy`、`anyrender_vello`、`vello`、`wgpu`。

### 8. `crates/ui-shell`

現在の panel presentation 層であり、次を担う。

- workspace layout 管理（4 隅アンカー基準の panel 配置、move / visibility / resize）
- focus 管理（`focused_target` / panel フォーカス巡回、`focus_panel_node(panel_id, node_id)` / `focus_next()` / `focus_previous()`）
- HTML panel hit-test（`html_panel_hit_at` / `panel_resize_hit_at` / move handle）
- 登録済みパネル ID と workspace layout の整合（`reconcile_panels(panel_ids: Vec<&'static str>)` — desktop 側が `panel_runtime.panel_static_ids()` を渡す）

主なモジュール:

- `src/lib.rs`（`PanelPresentation`、hit テーブル、リサイズハンドル判定）
- `src/workspace.rs`
- `src/focus.rs`
- `src/tests.rs`

補足:

- DSL 時代の `tree_query.rs` と dropdown / text_input 走査・`TextInputEditorState`・winit IME 編集経路は Phase 12 で削除済み（HTML パネル内部完結に統一）。
- 9E 時代の互換スタブ（`PanelSurface` / `render_panel_surface` / scroll 系 no-op / `handle_panel_event` / `PresentationEventResult` 等）は依存最小化リアーキテクトで削除済み。
- 依存は `app-core` / `panel-api` / `render-types` のみ（`panel-runtime` 非依存）。

### 9. `crates/panel-api`

panel host 契約層である。

- `PanelPlugin` trait（`id` / `title` / `update` / `commands` / `handle_event` / `handles_keyboard_event` / `persistent_config` / `restore_persistent_config` 等）
- `PanelEvent`（`Activate` / `SetValue` / `DragValue` / `SetText` / `Keyboard`）
- `HostAction`（`DispatchCommand` / `RequestService` / `InvokePanelHandler` / `MovePanel` / `SetPanelVisibility`）
- `ResizeEdge`（8 ハンドル: 4 辺 + 4 角 — Phase 11）
- `ServiceRequest` と service 名定数（`src/services.rs`）

補足:

- `PanelTree` / `PanelNode` / `PanelView` は Phase 12（ADR 014）で完全撤去済みで存在しない。

### 10. `crates/plugin-host`

Wasm panel runtime の実行器である。

- `wasmtime`（cache feature 有効）による module load と共有 `Engine`
- host import の公開（state / host_get / event_get / command / diagnostic 系）
- DOM mutation host functions（`src/dom_api.rs`: Blitz `DocumentMutator` を Wasm に公開 — Phase 10）
- Wasm memory 読み書き
- `panel_init` / `panel_handle_event` / `panel_sync_host` / `panel_handle_keyboard` の橋渡し

補足:

- 依存は `panel-schema` と `blitz-dom` / `blitz-html`。panel Wasm runtime に特化しており、一般 plugin host ではない。

### 11. `crates/panel-schema`

host と Wasm panel runtime の共有 DTO である（`src/lib.rs` 単一ファイル）。

- `PanelInitRequest` / `PanelInitResponse`
- `PanelEventRequest`
- `HandlerResult`
- `StatePatch` / `StatePatchOp`
- `CommandDescriptor`
- `Diagnostic`

### 12. `crates/plugin-sdk` / `crates/plugin-macros`

panel 作者向け authoring surface である。

- typed command / service builder（`commands.rs` / `services.rs`）
- host snapshot accessor（`host.rs`）
- DOM mutation API（`dom.rs`: `query_selector` / `set_attribute` / `set_inner_html` 等 — Phase 10）
- runtime helper（`runtime.rs` / `state.rs` / `builder.rs`）
- proc-macro の再 export（`plugin-macros`: `#[panel_init]` / `#[panel_handler]` / `#[panel_sync_host]`）

### 13. `crates/builtin-panels`

ビルトインパネルのローダと 12 パネル定義の置き場である。

- umbrella crate: `register_builtin_panels(runtime, assets_root) -> Vec<String>`（診断）、`BUILTIN_PANELS` 配列（`src/loader.rs`）
- 各パネルは `crates/builtin-panels/<name>/` に `panel.html`（必須）+ `panel.css`（任意）+ `panel.meta.json`（必須、`default_size` 必須）+ Rust→Wasm 実装（`cdylib` + `rlib`）+ コンパイル済み `builtin_panel_<name>.wasm` を同居させる

workspace member のパネル crate（12 個）:

- `app-actions` / `workspace-presets` / `tool-palette` / `view-controls`
- `panel-list` / `layers-panel` / `color-palette` / `pen-settings`
- `job-progress` / `snapshot-panel` / `text-flow` / `workspace-layout`（Phase 12 追加）

補足:

- 各パネル crate の依存は `plugin-sdk` のみ。umbrella crate は `panel-runtime` に依存する。
- Wasm の再ビルドは `scripts/build-ui-wasm.ps1`（または `.sh`）。

### 14. `crates/storage`

project / pen / tool catalog の永続化と読込を担う。

- SQLite ベース project save/load（`rusqlite` bundled）
- format version 管理、page / panel 単位の部分読込
- layer chunk 保存（`rmp-serde` + `zstd`）と current panel snapshot 永続化
- PNG export（`src/export.rs`）
- pen import/export（`pen_exchange.rs` / `pen_format.rs` / `pen_presets.rs`）
- `tools/` カタログ読込（`tool_catalog.rs`）

主なモジュール: `project_file.rs` / `project_sqlite.rs` / `pen_exchange.rs` / `pen_format.rs` / `pen_presets.rs` / `tool_catalog.rs` / `export.rs`

### 15. `crates/desktop-support`

desktop 固有 I/O と補助機能を担う。

- session save/load（`session.rs`）
- native dialog（`dialogs.rs`）
- default path / config（`config.rs`: `default_panel_dir()` は `crates/builtin-panels` を指す）
- profiler（`profiler.rs`）
- canvas template 読込（`templates.rs`）
- workspace preset catalog の読込/保存（`workspace_presets.rs`）

## 現在の runtime flow

### 1. 起動

1. `main.rs` が `DesktopRuntime::run(...)` を呼び、window / GPU / event loop を初期化する
2. `DesktopApp::new(...)` の `bootstrap.rs` が session / project / workspace preset を復元する
3. `register_builtin_panels(&mut panel_runtime, &default_panel_dir())` が `crates/builtin-panels/` 配下 12 パネル（`panel.html` / `panel.css` / `panel.meta.json` / `.wasm`）を登録し、`PanelPresentation` と reconcile する
4. パネルサイズは `panel.meta.json::default_size` を初期値とし、workspace 永続値が SoT として上書きする（Phase 11）
5. `storage` が `tools/` と `pens/` を読み、`Document` へ反映する
6. `canvas::CanvasRuntime` が paint plugin registry を初期化する
7. wgpu 初期化後に `install_gpu_resources(device, queue)` が GPU リソース 5 種を構築し、`sync_all_layers_to_gpu` で全レイヤー bitmap を GPU テクスチャへアップロードする
8. `wgpu_canvas.rs` が最初の提示を行う

### 2. 入力から描画まで

1. OS 入力を `runtime/pointer.rs` と `runtime/keyboard.rs` が正規化する
2. `app/input.rs` が hit テーブルを参照して canvas / panel / panel move / panel resize を振り分ける
3. `canvas::view_mapping` が window/view 座標を canvas 座標へ変換する
4. `canvas::gesture` が down / drag / up を `PaintInput` または panel rect preview へ変換する
5. `canvas::context_builder` が `Document` から paint context を解決する
6. `services/project_io.rs::execute_paint_input` が `CanvasRuntime` で `BitmapEdit` 差分（dirty rect の典拠）を計算し、`GpuBrushDispatch` / `GpuFillDispatch` の compute shader が GPU レイヤーテクスチャへ直接書き込む（ストローク中は CPU bitmap を書き換えない）
7. `GpuLayerCompositor` が dirty 領域を `layer_composite.wgsl` で合成する
8. dirty rect / present flag は `present_state.rs` に蓄積される
9. `present.rs::prepare_present_frame` が dirty panel の sync・HTML パネル hit テーブル更新（`refresh_html_panel_hit_tables`）・差分計画を組み立てる
10. `runtime.rs` が `PresentScene`（背景 solid quad → canvas `GpuComposite` → overlay solid/circle/line quad → panel quad → foreground / status quad）を組み、`WgpuPresenter` が提示する

### 3. パネル

1. `BuiltinPanelPlugin` が `panel.html` + `panel.css` を `HtmlPanelEngine`（Blitz）にロードし、`WasmPanelRuntime`（wasmtime）で `panel_init` を実行する
2. `host_sync.rs::build_host_snapshot_cached` が `Document` 状態を JSON snapshot として同期し、Wasm の `panel_sync_host` が DOM mutation host functions で DOM を更新する
3. pointer 入力は hit テーブル（`PanelRuntime::collect_panel_hits` が `prepare_present_frame` で CPU 更新 — GPU 非依存、Phase 13）→ `panel_dispatch.rs` → `PanelEvent` → Wasm handler → `HandlerResult`（`StatePatch` / `CommandDescriptor`）の経路で流れる
4. keyboard 入力は `PanelEvent::Keyboard` として、`panel_handle_keyboard` を export するパネル（`handles_keyboard_event()` で load 時検出）へ転送される（Phase 13）
5. `CommandDescriptor` は `HostAction::DispatchCommand` / `RequestService` へ変換され、`DesktopApp::execute_host_action` が `Command` または host service handler を実行する
6. パネルリサイズは 8 ハンドル（`ResizeEdge`）ドラッグ → `PanelSizeConstraints`（CSS min/max + 絶対最小 80x60 + viewport クランプ）→ workspace 永続値更新の経路（Phase 11）
7. `HtmlPanelEngine` が vello で `PanelGpuTarget` テクスチャに直描画し、`wgpu_canvas` が `panel_quads` 層で合成する
8. ステータスバーは `frame/status_panel.rs` の専用 `HtmlPanelEngine` が `status_quad` を生成する

### 4. 保存と workspace 状態

- project save/load は `storage`（SQLite）。保存前に `services/gpu_sync.rs` が GPU テクスチャを readback して `Document` を最新化する
- 非同期保存は `background_tasks.rs` がジョブとして起動・回収する
- session save/load と desktop path 管理は `io_state.rs` 経由で `desktop-support`
- workspace preset は `desktop-support::workspace_presets` + `services/workspace_io.rs`
- panel persistent config は `panel-runtime::config`
- 共通 UI 永続化 DTO（`WorkspaceUiState` / `PluginConfigs`）は `app-core::workspace`
- orchestration の中心は依然として `DesktopApp`

## 現在の集中責務

### `DesktopApp`

次が集まっている。

- document / panel runtime / panel presentation / GPU リソースの所有
- project / session / workspace preset I/O
- tool / pen / panel catalog 読込
- canvas runtime と GPU dispatch の橋渡し（`execute_paint_input` に CPU 差分計算と GPU dispatch が同居）
- dirty rect 収集と present 計画組み立て
- panel drag / resize と input state

集中箇所（ファイル単位）:

- `apps/desktop/src/app/mod.rs`: `DesktopApp` 定義、GPU リソースフィールド、`install_gpu_resources` / `sync_all_layers_to_gpu`
- `apps/desktop/src/app/bootstrap.rs`: 起動時の session / project / workspace preset 復元と builtin panel 登録
- `apps/desktop/src/app/command_router.rs`: document command と I/O command の分岐
- `apps/desktop/src/app/panel_dispatch.rs`: panel drag / resize、host action、focus dispatch
- `apps/desktop/src/app/io_state.rs`: `project_path` / `session_path` / `workspace_preset_path` / dialogs / background save queue
- `apps/desktop/src/app/background_tasks.rs`: 非同期 save task の起動と回収
- `apps/desktop/src/app/services/mod.rs`: service request ルータ
- `apps/desktop/src/app/services/project_io.rs`: project save/load と `execute_paint_input`（GPU brush/fill dispatch）
- `apps/desktop/src/app/services/gpu_sync.rs`: 保存前の GPU→CPU readback 同期
- `apps/desktop/src/app/present_state.rs`: dirty rect、present flag、UI 再同期要求
- `apps/desktop/src/app/present.rs`: dirty panel sync、hit テーブル更新、差分 present 計画

### `CanvasRuntime` と GPU ペイント

- bitmap paint plugin registry と `BitmapEdit` 差分計算は `crates/canvas`
- GPU compute dispatch（brush / fill / composite）は `crates/gpu-canvas` の実装を `apps/desktop` が呼び出す

集中箇所（ファイル単位）:

- `crates/canvas/src/runtime.rs`: plugin 実行入口
- `crates/canvas/src/context_builder.rs`: `Document` からの runtime 文脈構築
- `crates/canvas/src/gesture.rs`: canvas gesture state machine
- `crates/canvas/src/ops/*`: stamp / stroke / flood fill / lasso fill / composite / text
- `crates/gpu-canvas/src/{brush,fill,composite}.rs` + `src/shaders/*.wgsl`: GPU ペイント実装
- `apps/desktop/src/app/services/project_io.rs`: CPU 差分計算と GPU dispatch の結合点

### `Document`

次が集中している。

- ドメインモデルと `Command` 適用
- tool / pen runtime 状態

集中箇所（ファイル単位）:

- `crates/app-core/src/document.rs`: document state と tool / pen state
- `crates/app-core/src/painting.rs`: `PaintPluginContext` / `BitmapEdit` / compositor

### `PanelRuntime` / `PanelPresentation`

現在は次のように分担する。

- `PanelRuntime`: panel registry、`BuiltinPanelPlugin`（HTML/Wasm runtime）、host snapshot sync、persistent config、GPU frame、hit 収集
- `PanelPresentation`: workspace layout、focus、hit-test 結果の保持

集中箇所（ファイル単位）:

- `crates/panel-runtime/src/registry.rs`: registry、dirty panel sync、event dispatch、`collect_panel_hits`、GPU frame 管理
- `crates/panel-runtime/src/builtin_plugin.rs`: `HtmlPanelEngine` + Wasm bridge、keyboard 転送、state patch 適用
- `crates/ui-shell/src/workspace.rs`: workspace layout、panel move / visibility / resize
- `crates/ui-shell/src/lib.rs`: `PanelPresentation`、hit-test、focus

## 今後の境界

### 命名と実態のズレ

- `ui-shell`: 名前は shell だが、実態は panel presentation crate である
- `plugin-host`: 一般 plugin host ではなく panel Wasm runtime 専用である
- `DesktopApp`: 単なる app state ではなく desktop host orchestration service に近い

### 現在まだ存在しない明確な境界

- service request の戻り値や双方向 service contract
- tool 実行 plugin と host runtime の安定境界（tool 実行本体は host 側 `canvas` runtime + `apps/desktop` の GPU dispatch が担う）
- GPU paint dispatch の独立層（現在は `apps/desktop/src/app/services/project_io.rs` 内に CPU 差分計算と GPU dispatch が同居する）

### この文書の結論

現在の `altpaint` は、

- `apps/desktop`（orchestration + GPU dispatch + 提示）
- `app-core`（ドメイン）
- `panel-runtime` / `ui-shell`（パネル runtime / presentation）

に大きな責務が集まりながらも、

- `gpu-canvas`（GPU ペイント実装）
- `render-types`（描画計画 DTO）
- `panel-html`（HTML パネルエンジン）
- `plugin-host` / `plugin-sdk`（Wasm 境界）
- `storage` / `desktop-support`（永続化）

へ切り出しが進んだ状態である。今後のリファクタリングでは、`execute_paint_input` の GPU dispatch 分離と tool 実行境界の確立が主なギャップになる。
