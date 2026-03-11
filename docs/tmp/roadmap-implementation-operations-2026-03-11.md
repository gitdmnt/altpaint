# ROADMAP 実装操作タスクリスト 2026-03-11

## この文書の目的

この文書は、[docs/ROADMAP.md](../ROADMAP.md) を**実装操作の順番**へ落とし込んだ作業計画である。

ここでは「何を目指すか」ではなく、次を固定する。

- どのフェーズで
- どのファイルに
- どの種類の編集・追加・移動・削除を行うか
- どの順番で進めるか

この文書は**上から順番に実行する前提**で書く。
後ろのフェーズを先に始めない。

## この文書の前提

- 最小実装で止めない
- 各フェーズでは、その責務範囲で必要な整理をまとめて行う
- 既存の責務集中を温存するような暫定回避を増やさない
- 変更のたびにテスト・`clippy`・関連文書更新まで含める
- `target/` は読まない、編集しない
- ファイル構造の最適化とサイズ削減が目的なので不要なファイルはどんどん消してコンパクトにする

## フェーズ実行ルール

各タスクでは、必要に応じて次を含める。

1. 既存ファイルの責務棚卸し
2. 新規 module / crate / test file 作成
3. 呼び出し側の import / dependency 更新
4. 旧 API の削除
5. テスト追加・更新
6. `docs/IMPLEMENTATION_STATUS.md` / `docs/CURRENT_ARCHITECTURE.md` / `docs/MODULE_DEPENDENCIES.md` の追従

---

## フェーズ0: 境界の固定と作業前提の統一

このフェーズではコードを大きく動かす前に、**移動先と命名方針を文書と workspace 定義で固定**する。

### フェーズ0完了判定

- `docs/ARCHITECTURE.md` に責務表、配置草案、ファイル配置規約がある
- `docs/CURRENT_ARCHITECTURE.md` に集中箇所とテスト棚卸しがある
- `docs/MODULE_DEPENDENCIES.md` に依存禁止事項と将来配置図がある
- `Cargo.toml` に `crates/canvas` / `crates/panel-runtime` の計画コメントがある

### 0-1. 目標 crate 配置の草案を文書へ追加する

- 編集:
	- `docs/ARCHITECTURE.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
- 操作:
	1. `desktopApp` / `app-core` / `render` / `canvas` / `ui-shell` / `plugin-host` / `panel-dsl` / `plugin-sdk` の責務表を追記する。
	2. 現在存在しない `canvas`、必要なら `panel-runtime` の想定位置を図示する。
	3. 「どこへ置くべきか」の判断基準を crate ごとに箇条書きで明記する。

### 0-2. 現在の責務集中箇所をファイル単位で列挙する

- 編集:
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/tmp/tasks-2026-03-11.md`
- 操作:
	1. `apps/desktop/src/app/mod.rs`、`apps/desktop/src/app/input.rs`、`apps/desktop/src/app/present.rs`、`apps/desktop/src/app/drawing.rs` を `DesktopApp` 集中箇所として明記する。
	2. `crates/ui-shell/src/lib.rs`、`crates/ui-shell/src/dsl.rs`、`crates/ui-shell/src/workspace.rs` を `ui-shell` 集中箇所として明記する。
	3. `crates/app-core/src/document.rs`、`crates/app-core/src/painting.rs` を `Document` 集中箇所として明記する。

### 0-3. 追加予定 crate 名を workspace 計画へ反映する

- 編集:
	- `Cargo.toml`
	- `docs/ROADMAP.md`
	- `docs/tmp/tasks-2026-03-11.md`
- 操作:
	1. `canvas` crate を追加する計画を `Cargo.toml` コメントまたは別文書側で明示する。
	2. `panel-runtime` を作る場合の命名案を `docs/ROADMAP.md` と同期する。
	3. `panel-sdk` / `panel-macros` を将来 `plugin-sdk` 系へ寄せる方針を明示する。

### 0-4. 依存方向の禁止事項を明文化する

- 編集:
	- `docs/MODULE_DEPENDENCIES.md`
	- `docs/ARCHITECTURE.md`
- 操作:
	1. `app-core` から `desktop` / `wgpu` / `plugin-host` を参照しないことを明記する。
	2. `render` に project I/O を入れないことを明記する。
	3. `ui-shell` presentation 側に Wasm runtime 詳細を持ち込まないことを明記する。
	4. `canvas` に panel runtime を入れないことを明記する。

### 0-5. フェーズごとの完了条件を操作レベルへ書き換える

- 編集:
	- `docs/ROADMAP.md`
	- `docs/tmp/roadmap-implementation-operations-2026-03-11.md`
- 操作:
	1. 各フェーズの完了条件を「責務の説明」ではなく「ファイル移動が完了している状態」で追記する。
	2. 例として `apps/desktop/src/app/drawing.rs` が削除または wrapper 化されている、のような具体条件へ寄せる。

### 0-6. 既存テストの責務境界を棚卸しする

- 編集:
	- `apps/desktop/src/app/tests/*`
	- `crates/ui-shell/src/tests.rs`
	- `crates/panel-sdk/src/tests.rs`
- 操作:
	1. 現在のテストが `DesktopApp` 経由でしか検証できない領域を洗い出す。
	2. 今後 crate 単位へ移すべきテストを TODO コメントではなく文書へ一覧化する。

### 0-7. 今後の新規ファイル配置規約を固定する

- 編集:
	- `AGENTS.md`
	- `docs/ARCHITECTURE.md`
- 操作:
	1. `runtime/`、`presentation/`、`services/`、`ops/`、`tests/` の使い分けを追記する。
	2. `lib.rs` に巨大実装を戻さない方針を追記する。

### 0-8. フェーズ移行時に毎回更新する文書を固定する

- 編集:
	- `AGENTS.md`
	- `docs/IMPLEMENTATION_STATUS.md`
- 操作:
	1. フェーズ完了ごとに更新する文書を `IMPLEMENTATION_STATUS` / `CURRENT_ARCHITECTURE` / `MODULE_DEPENDENCIES` に固定する。
	2. 「コードが正本」の原則に従い、文書更新はコード変更直後に行う運用を追記する。

---

## フェーズ1: `desktopApp` の縮小

このフェーズでは `DesktopApp` を**event loop と orchestration の薄い入口**へ戻す。

### フェーズ1完了判定

- `apps/desktop/src/app/mod.rs` が constructor と薄い公開 API 中心になっている
- `services.rs`、`io_state.rs`、`bootstrap.rs`、`command_router.rs`、`panel_dispatch.rs`、`present_state.rs`、`background_tasks.rs` が存在する
- `apps/desktop/src/app/tests/` が module 単位へ分割されている

### 1-1. `DesktopApp` のフィールドを責務別に分割する

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/state.rs`
- 新規作成:
	- `apps/desktop/src/app/services.rs`
	- `apps/desktop/src/app/io_state.rs`
- 操作:
	1. `DesktopApp` の field を document / UI / present / persistence / canvas interaction / panel interaction に分類する。
	2. `project_path`、`session_path`、`workspace_preset_path`、`dialogs`、`pending_save_tasks` を `io_state.rs` 側 struct へ移す。
	3. panel drag / press / dirty flag 群を `state.rs` 側専用 struct にまとめる。

### 1-2. 初期化処理を constructor から service 化する

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/services.rs`
- 操作:
	1. `new_with_dialogs_session_path_and_workspace_preset_path(...)` 内の session 読込、project 読込、panel directory 読込、workspace preset 読込を private helper へ分割する。
	2. `DesktopApp` constructor には wiring と初期 service 呼び出しだけを残す。

### 1-3. project / session / preset 起動復元を orchestration module へ出す

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/commands.rs`
- 新規作成:
	- `apps/desktop/src/app/bootstrap.rs`
- 操作:
	1. 起動時の project 優先順位決定ロジックを `bootstrap.rs` へ移す。
	2. workspace layout / plugin config 復元ロジックを `bootstrap.rs` へ移す。
	3. `mod.rs` から復元ロジックの詳細分岐を削る。

### 1-4. template / workspace preset 反映処理を UI 更新コードから分離する

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/commands.rs`
- 新規作成:
	- `apps/desktop/src/app/panel_config_sync.rs`
- 操作:
	1. `refresh_new_document_templates()` と `refresh_workspace_presets()` の panel config 書き込みを `panel_config_sync.rs` へ移す。
	2. `builtin.app-actions` と `builtin.workspace-presets` の config 生成ロジックを `DesktopApp` 本体から除去する。

### 1-5. command 実行を I/O command と domain command に分ける

- 編集:
	- `apps/desktop/src/app/commands.rs`
	- `apps/desktop/src/app/mod.rs`
- 新規作成:
	- `apps/desktop/src/app/command_router.rs`
- 操作:
	1. `execute_command()` の `match` を project I/O、workspace I/O、pen/tool reload、pure document command に分割する。
	2. `Command::SaveProject*` / `LoadProject*` 系は I/O ルータへ退避する。
	3. `other => self.execute_document_command(other)` 以外の副作用分岐を減らす。

### 1-6. background task 管理を専用 module へ出す

- 編集:
	- `apps/desktop/src/app/state.rs`
	- `apps/desktop/src/app/commands.rs`
- 新規作成:
	- `apps/desktop/src/app/background_tasks.rs`
- 操作:
	1. `PendingSaveTask` と `poll_background_tasks()` を `background_tasks.rs` へ移す。
	2. 将来 export/job に使える汎用 task result 型を定義する。

### 1-7. パネルイベント中継を `DesktopApp` から薄く切る

- 編集:
	- `apps/desktop/src/app/input.rs`
	- `apps/desktop/src/app/commands.rs`
- 新規作成:
	- `apps/desktop/src/app/panel_dispatch.rs`
- 操作:
	1. `dispatch_panel_event(...)`、`execute_host_action(...)`、`activate_panel_control(...)` の相互依存を `panel_dispatch.rs` に集約する。
	2. `DesktopApp` は panel runtime の詳細を知らず、`UiShell` と `HostAction` 適用をつなぐだけにする。

### 1-8. present 更新指示の組み立てを app 本体から切り離す

- 編集:
	- `apps/desktop/src/app/present.rs`
	- `apps/desktop/src/app/state.rs`
- 新規作成:
	- `apps/desktop/src/app/present_state.rs`
- 操作:
	1. dirty rect、transform update、full rebuild flag を `present_state.rs` に集約する。
	2. `DesktopApp` 本体の field を状態 object 経由参照へ置き換える。

### 1-9. `mod.rs` から大きな private helper 群を退避する

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/services.rs`
	- `apps/desktop/src/app/bootstrap.rs`
	- `apps/desktop/src/app/panel_config_sync.rs`
- 操作:
	1. `reload_tool_catalog_into_document`、`reload_pen_presets_into_document`、status 文字列生成などの helper を責務別 module へ移す。
	2. `mod.rs` は type 定義、constructor、module 宣言、薄い public API だけに絞る。

### 1-10. `DesktopApp` 用テストを module 単位へ分解する

- 編集:
	- `apps/desktop/src/app/tests/*`
- 新規作成:
	- `apps/desktop/src/app/tests/bootstrap_tests.rs`
	- `apps/desktop/src/app/tests/command_router_tests.rs`
	- `apps/desktop/src/app/tests/panel_dispatch_tests.rs`
- 操作:
	1. 既存 app テストを constructor / I/O / panel dispatch / present state ごとに分ける。
	2. `mod.rs` 直結テストを減らし、分割後 module ごとのテストへ寄せる。

### 1-11. `apps/desktop/src/main.rs` と `runtime.rs` から app 内部知識を減らす

- 編集:
	- `apps/desktop/src/main.rs`
	- `apps/desktop/src/runtime.rs`
	- `apps/desktop/src/runtime/*`
- 操作:
	1. runtime 側が `DesktopApp` 内部 field を前提にしないよう、呼び出し API を整理する。
	2. event loop 側は `handle_*` と `prepare_present_frame()` の呼び出しに集中させる。

### 1-12. フェーズ1完了後の文書同期

- 編集:
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
- 操作:
	1. `DesktopApp` が保持する責務一覧を更新する。
	2. 追加した module 群を current architecture に追記する。

---

## フェーズ2: `canvas` 層の新設

このフェーズでは、分散している描画ツール実行・入力解釈・差分生成を `canvas` crate に集約する。

### フェーズ2完了判定

- `crates/canvas` crate が workspace member として存在する
- `apps/desktop/src/app/drawing.rs` が削除済みまたは thin wrapper 化されている
- canvas runtime / input / ops / tests が `crates/canvas` 配下へ移っている

### 2-1. workspace に `canvas` crate を追加する

- 編集:
	- `Cargo.toml`
- 新規作成:
	- `crates/canvas/Cargo.toml`
	- `crates/canvas/src/lib.rs`
- 操作:
	1. workspace member に `crates/canvas` を追加する。
	2. `app-core`、`render`、`apps/desktop` から必要な依存だけを参照する最小 Cargo 設定を作る。

### 2-2. `canvas` crate の基礎 module を作る

- 新規作成:
	- `crates/canvas/src/runtime.rs`
	- `crates/canvas/src/input.rs`
	- `crates/canvas/src/ops.rs`
	- `crates/canvas/src/registry.rs`
	- `crates/canvas/src/context.rs`
	- `crates/canvas/src/tests.rs`
- 操作:
	1. tool 実行入口、input 解釈、bitmap op、plugin registry、context builder を分ける。
	2. `lib.rs` は公開 API の再 export だけに寄せる。

### 2-3. built-in bitmap paint 実装を `apps/desktop` から移動する

- 編集:
	- `apps/desktop/src/app/drawing.rs`
	- `apps/desktop/Cargo.toml`
	- `crates/canvas/Cargo.toml`
- 新規作成:
	- `crates/canvas/src/plugins/builtin_bitmap.rs`
	- `crates/canvas/src/plugins/mod.rs`
- 操作:
	1. `BuiltinBitmapPaintPlugin`、`EraseComposite`、stamp / stroke / flood fill / lasso fill 実装を `crates/canvas/src/plugins/builtin_bitmap.rs` へ移す。
	2. `apps/desktop/src/app/drawing.rs` は削除するか、`canvas` 呼び出し wrapper のみへ縮小する。
	3. registry 初期化は `canvas::default_paint_plugins()` 側へ移す。

### 2-4. `CanvasInputState` と view-to-canvas 変換を `canvas` へ移す

- 編集:
	- `apps/desktop/src/canvas_bridge.rs`
	- `apps/desktop/src/app/input.rs`
- 新規作成:
	- `crates/canvas/src/input_state.rs`
	- `crates/canvas/src/view_mapping.rs`
- 操作:
	1. `CanvasInputState`、`CanvasPointerEvent`、`map_view_to_canvas_with_transform(...)` を `canvas` crate へ移す。
	2. `apps/desktop` 側は window 座標を `canvas` 用入力へ変換して渡すだけにする。

### 2-5. `PaintPluginContext` 解決を `Document` から切り出す

- 編集:
	- `crates/app-core/src/document.rs`
	- `crates/app-core/src/painting.rs`
	- `crates/app-core/src/lib.rs`
- 新規作成:
	- `crates/canvas/src/context_builder.rs`
- 操作:
	1. `Document::resolve_paint_plugin_context(...)` 相当を `canvas` 側 builder へ移す。
	2. `Document` には paint runtime 文脈生成ではなく、参照用 getter と pure state だけを残す。
	3. 必要なら `app-core` に `CanvasRuntimeSnapshot` 的な読み取り専用 struct を追加する。

### 2-6. `execute_paint_input(...)` を `canvas` runtime 呼び出しへ置き換える

- 編集:
	- `apps/desktop/src/app/commands.rs`
	- `apps/desktop/src/app/input.rs`
	- `apps/desktop/src/app/mod.rs`
- 操作:
	1. `DesktopApp::execute_paint_input(...)` 内の plugin 選択と edit 生成を `canvas::runtime` 呼び出しへ置き換える。
	2. `DesktopApp` は生成済み `BitmapEdit` の適用だけを受け取る形にする。

### 2-7. canvas op を tool 種別ごとに module 分割する

- 新規作成:
	- `crates/canvas/src/ops/stamp.rs`
	- `crates/canvas/src/ops/stroke.rs`
	- `crates/canvas/src/ops/flood_fill.rs`
	- `crates/canvas/src/ops/lasso_fill.rs`
	- `crates/canvas/src/ops/composite.rs`
	- `crates/canvas/src/ops/mod.rs`
- 操作:
	1. `drawing.rs` 由来のロジックを 1 ファイル巨大実装に戻さず、op ごとに分ける。
	2. 将来 plugin 置換しやすいよう、tool ごとに entry 関数を分離する。

### 2-8. `app-core` のツール状態を pure state へ寄せる

- 編集:
	- `crates/app-core/src/document.rs`
	- `crates/app-core/src/painting.rs`
- 操作:
	1. `ToolDefinition` を「catalog 定義」と「runtime 選択状態」の責務で整理する。
	2. runtime 実行依存の処理を `app-core` から除く。
	3. `Document` には active tool id / active pen id / tool settings 値だけを残す方向へ寄せる。

### 2-9. `apps/desktop/src/app/input.rs` の canvas 分岐を縮小する

- 編集:
	- `apps/desktop/src/app/input.rs`
- 新規作成:
	- `crates/canvas/src/gesture.rs`
- 操作:
	1. `handle_canvas_pointer(...)` 系ロジックから tool 種別ごとの分岐を `canvas::gesture` 側へ寄せる。
	2. lasso point 蓄積、drag/stamp/up/down の入力状態遷移を `canvas` 側 state machine へ移す。

### 2-10. `render` と `canvas` の責務境界を明示する adapter を導入する

- 編集:
	- `crates/render/src/lib.rs`
	- `crates/canvas/src/lib.rs`
	- `apps/desktop/src/app/present.rs`
- 新規作成:
	- `crates/canvas/src/render_bridge.rs`
- 操作:
	1. `render` が必要とする canvas bitmap / dirty rect / overlay 情報を `canvas` から取得する adapter を定義する。
	2. `apps/desktop` が canvas 内部型を直接つなぎ込まないようにする。

### 2-11. `canvas` crate の単体テストを先に厚くする

- 新規作成:
	- `crates/canvas/src/tests/stamp_tests.rs`
	- `crates/canvas/src/tests/stroke_tests.rs`
	- `crates/canvas/src/tests/fill_tests.rs`
	- `crates/canvas/src/tests/context_tests.rs`
	- `crates/canvas/src/tests/input_tests.rs`
- 操作:
	1. ブラシ、消しゴム、bucket、lasso fill、pressure、spacing の期待値を crate 単位で固定する。
	2. `DesktopApp` を経由しない再現テストへ置き換える。

### 2-12. フェーズ2完了後の cleanup と文書同期

- 編集:
	- `apps/desktop/src/app/drawing.rs`
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
- 操作:
	1. `drawing.rs` が不要なら削除する。
	2. `canvas` crate の追加と責務を文書へ反映する。

---

## フェーズ3: panel runtime / presentation 分離

このフェーズでは `ui-shell` を、**runtime 仲介**と**presentation** に分ける。

### フェーズ3完了判定

- `crates/panel-runtime` crate が存在する
- `crates/ui-shell/src/presentation/` 配下へ presentation module が整理されている
- `crates/ui-shell/src/dsl.rs` 由来の runtime 責務が `panel-runtime` へ移っている

### 3-1. `ui-shell` 内の責務を runtime / presentation へ再分類する

- 編集:
	- `crates/ui-shell/src/lib.rs`
	- `crates/ui-shell/src/dsl.rs`
	- `crates/ui-shell/src/workspace.rs`
	- `crates/ui-shell/src/presentation.rs`
	- `crates/ui-shell/src/surface_render.rs`
	- `crates/ui-shell/src/focus.rs`
	- `crates/ui-shell/src/tree_query.rs`
- 操作:
	1. 各 module の公開関数を runtime 寄りと presentation 寄りに分類する。
	2. 依存先に `plugin_host` / `panel_dsl` / `render` のどれが入っているかで一次分類する。

### 3-2. `panel-runtime` crate を新設する

- 編集:
	- `Cargo.toml`
- 新規作成:
	- `crates/panel-runtime/Cargo.toml`
	- `crates/panel-runtime/src/lib.rs`
	- `crates/panel-runtime/src/registry.rs`
	- `crates/panel-runtime/src/dsl_loader.rs`
	- `crates/panel-runtime/src/runtime_bridge.rs`
	- `crates/panel-runtime/src/host_sync.rs`
	- `crates/panel-runtime/src/config.rs`
- 操作:
	1. `plugin-host`、`panel-dsl`、`plugin-api`、`panel-schema` を参照する runtime 層を切り出す。
	2. `UiShell` から panel discovery と Wasm bridge を除く準備をする。

### 3-3. DSL 読込と panel 登録処理を `panel-runtime` へ移す

- 編集:
	- `crates/ui-shell/src/dsl.rs`
	- `crates/ui-shell/src/lib.rs`
- 新規作成:
	- `crates/panel-runtime/src/dsl_panel.rs`
- 操作:
	1. `collect_panel_files_recursive(...)`、`DslPanelPlugin::from_definition(...)`、directory load 流れを `panel-runtime` へ移す。
	2. `UiShell::load_panel_directory(...)` は runtime service を呼ぶだけにする。

### 3-4. `UiShell` の panel registry 所有を runtime service へ移す

- 編集:
	- `crates/ui-shell/src/lib.rs`
- 新規作成:
	- `crates/panel-runtime/src/registry_state.rs`
- 操作:
	1. `panels`、`loaded_panel_ids`、`panel_tree_cache` のうち runtime 側状態を `panel-runtime` へ移す。
	2. `UiShell` は presentation に必要な snapshot を受け取る形に変える。

### 3-5. host snapshot 同期と persistent config 管理を runtime 側へ移す

- 編集:
	- `crates/ui-shell/src/lib.rs`
	- `crates/ui-shell/src/workspace.rs`
- 新規作成:
	- `crates/panel-runtime/src/snapshot.rs`
	- `crates/panel-runtime/src/persistent_config.rs`
- 操作:
	1. `update(...)` / `update_panels(...)` の document 同期処理を runtime service 化する。
	2. `persistent_panel_configs` の所有者を `UiShell` から runtime 側へ寄せる。

### 3-6. presentation 側を surface / layout / focus / input に分ける

- 編集:
	- `crates/ui-shell/src/presentation.rs`
	- `crates/ui-shell/src/surface_render.rs`
	- `crates/ui-shell/src/focus.rs`
	- `crates/ui-shell/src/tree_query.rs`
- 新規作成:
	- `crates/ui-shell/src/presentation/layout.rs`
	- `crates/ui-shell/src/presentation/hit_test.rs`
	- `crates/ui-shell/src/presentation/focus.rs`
	- `crates/ui-shell/src/presentation/text_input.rs`
	- `crates/ui-shell/src/presentation/mod.rs`
- 操作:
	1. panel surface 生成、hit-test、focus、text input を submodule に分ける。
	2. `lib.rs` に戻さず presentation namespace 配下へ整理する。

### 3-7. `UiShell` 公開 API を runtime facade + presentation facade に分ける

- 編集:
	- `crates/ui-shell/src/lib.rs`
- 新規作成:
	- `crates/ui-shell/src/runtime_facade.rs`
	- `crates/ui-shell/src/presentation_facade.rs`
- 操作:
	1. panel event 受理と panel surface 描画を別 facade に分ける。
	2. `DesktopApp` からは `UiShell` 1 型を保ってもよいが、内部では合成にする。

### 3-8. `apps/desktop` 側依存を新境界へ付け替える

- 編集:
	- `apps/desktop/Cargo.toml`
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/input.rs`
	- `apps/desktop/src/app/present.rs`
	- `apps/desktop/src/app/commands.rs`
- 操作:
	1. `UiShell` へ直結していた runtime 詳細呼び出しを facade 経由に置き換える。
	2. `panel-runtime` crate 追加に伴う依存更新を行う。

### 3-9. runtime/presentation 分離テストを追加する

- 新規作成:
	- `crates/panel-runtime/src/tests.rs`
	- `crates/ui-shell/src/tests/runtime_facade_tests.rs`
	- `crates/ui-shell/src/tests/presentation_tests.rs`
- 操作:
	1. DSL ロード、host sync、panel config 復元、focus、hit-test を別々にテストする。
	2. Wasm bridge の差し替え可能な fake runtime を用意する。

### 3-10. フェーズ3完了後の cleanup と文書同期

- 編集:
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
	- `docs/ARCHITECTURE.md`
- 操作:
	1. `ui-shell` の責務説明を presentation 中心へ更新する。
	2. `panel-runtime` を採用した場合は依存図と runtime flow を更新する。

---

## フェーズ4: plugin-first 化の本格化

このフェーズでは、性能非依存の I/O・workspace・view・tool catalog を host 直書きから plugin 主導へ寄せる。

### フェーズ4完了判定

- `apps/desktop/src/app/services/` 配下に project / workspace / tool catalog service handler がある
- `plugins/app-actions`、`plugins/workspace-presets`、`plugins/view-controls`、`plugins/panel-list` が service request ベースで動く
- 上位 I/O フローが host command 直書きから service 指向へ寄っている

### 4-1. host service API の最小セットを追加する

- 編集:
	- `crates/plugin-api/src/lib.rs`
	- `crates/panel-sdk/src/host.rs`
	- `crates/panel-sdk/src/commands.rs`
	- `crates/panel-sdk/src/runtime.rs`
- 新規作成:
	- `crates/plugin-api/src/services.rs`
	- `crates/panel-sdk/src/services.rs`
- 操作:
	1. project save/load、workspace preset load/save、tool catalog reload、view update 用 service descriptor を追加する。
	2. `HostAction` に直接個別分岐を増やすのではなく service request へ寄せる。

### 4-2. `DesktopApp` の project save/load 分岐を service handler 化する

- 編集:
	- `apps/desktop/src/app/commands.rs`
	- `apps/desktop/src/app/panel_dispatch.rs`
- 新規作成:
	- `apps/desktop/src/app/services/project_io.rs`
- 操作:
	1. `save_project_to_current_path()`、`save_project_as()`、`load_project()` を project I/O service handler に移す。
	2. plugin から service request を受けて実行する形へ切り替える。

### 4-3. workspace preset / session 操作を service handler 化する

- 編集:
	- `apps/desktop/src/app/mod.rs`
	- `apps/desktop/src/app/commands.rs`
	- `crates/desktop-support/src/session.rs`
	- `crates/desktop-support/src/workspace_presets.rs`
- 新規作成:
	- `apps/desktop/src/app/services/workspace_io.rs`
- 操作:
	1. workspace preset 読込/保存/書き出しの orchestration を service 化する。
	2. session 永続化も project save と別 service に切り分ける。

### 4-4. `plugins/app-actions` を host command 列挙型依存から service 指向へ寄せる

- 編集:
	- `plugins/app-actions/src/lib.rs`
	- `plugins/app-actions/panel.altp-panel`
- 操作:
	1. new/save/open 系 UI を service request ベースへ更新する。
	2. panel 側で path 収集・template 選択・保存モード選択を組み立てられるようにする。

### 4-5. `plugins/workspace-presets` を workspace service API へ載せ替える

- 編集:
	- `plugins/workspace-presets/src/lib.rs`
	- `plugins/workspace-presets/panel.altp-panel`
- 操作:
	1. preset 一覧取得、適用、保存、書き出しを service request 化する。
	2. host 固有 command 名に依存する分岐を削る。

### 4-6. `plugins/view-controls` と `plugins/panel-list` を stable host API へ揃える

- 編集:
	- `plugins/view-controls/src/lib.rs`
	- `plugins/panel-list/src/lib.rs`
	- `plugins/view-controls/panel.altp-panel`
	- `plugins/panel-list/panel.altp-panel`
- 操作:
	1. view 移動、zoom、rotation、panel visibility 変更を安定 service / command descriptor 経由へ寄せる。
	2. plugin 固有の host action 直接組み立てを減らす。

### 4-7. `plugins/color-palette` と `plugins/pen-settings` の state 取得経路を整理する

- 編集:
	- `plugins/color-palette/src/lib.rs`
	- `plugins/pen-settings/src/lib.rs`
- 操作:
	1. host snapshot の読み取りフィールドを明示的 API 経由へ置き換える。
	2. color / pen state を plugin が必要最小限で受け取るようにする。

### 4-8. tool catalog 読込を plugin 主導へ寄せる

- 編集:
	- `crates/storage/src/tool_catalog.rs`
	- `apps/desktop/src/app/mod.rs`
	- `plugins/tool-palette/src/lib.rs`
- 新規作成:
	- `apps/desktop/src/app/services/tool_catalog.rs`
- 操作:
	1. 起動時 host 自動読込だけでなく、plugin request で catalog 再読込できるようにする。
	2. `tool-palette` が service で tool list を取得できる API を追加する。

### 4-9. project / workspace / tool service の統合テストを追加する

- 新規作成:
	- `apps/desktop/src/app/tests/service_project_io_tests.rs`
	- `apps/desktop/src/app/tests/service_workspace_io_tests.rs`
	- `apps/desktop/src/app/tests/service_tool_catalog_tests.rs`
	- `plugins/app-actions/tests.rs`
	- `plugins/workspace-presets/tests.rs`
- 操作:
	1. plugin request から host service 実行までの往復をテストする。
	2. file dialog fake と temporary path を使った再現テストを追加する。

### 4-10. `storage` と `desktop-support` を低レベル実装へ再定義する

- 編集:
	- `crates/storage/src/lib.rs`
	- `crates/storage/src/project_file.rs`
	- `crates/storage/src/project_sqlite.rs`
	- `crates/desktop-support/src/lib.rs`
	- `crates/desktop-support/src/session.rs`
	- `crates/desktop-support/src/workspace_presets.rs`
- 操作:
	1. これらの crate の責務説明を「意味論を持つ上位機能」から「serializer / low-level I/O」へ寄せる。
	2. 上位フローは service handler 側へ移した状態にする。

### 4-11. フェーズ4完了後の文書同期

- 編集:
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/ARCHITECTURE.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
- 操作:
	1. plugin-first 化した領域を列挙し直す。
	2. host service API の責務と plugin 側責務の境界を明記する。

---

## フェーズ5: `render` 中心の画面生成整理

このフェーズでは画面生成の中心を `render` へ移し、`apps/desktop` は GPU 所有と最終提示へ寄せる。

### フェーズ5完了判定

- `crates/render/src/frame_plan.rs` など plan module が存在する
- `apps/desktop/src/app/present.rs` が compose 本体を持たない
- `apps/desktop/src/frame/` には desktop 固有の presenter 変換だけが残る

### 5-1. `apps/desktop/src/app/present.rs` の compose 責務を棚卸しする

- 編集:
	- `apps/desktop/src/app/present.rs`
	- `apps/desktop/src/frame/*`
	- `crates/render/src/lib.rs`
- 操作:
	1. layout 計算、base compose、overlay compose、status compose、dirty rect union を分類する。
	2. `render` へ移す関数と `desktop frame` に残す関数を決める。

### 5-2. render plan 型を `render` crate に追加する

- 新規作成:
	- `crates/render/src/frame_plan.rs`
	- `crates/render/src/canvas_plan.rs`
	- `crates/render/src/overlay_plan.rs`
	- `crates/render/src/panel_plan.rs`
- 編集:
	- `crates/render/src/lib.rs`
- 操作:
	1. base / canvas / overlay / panel の plan 型を `render` 側に定義する。
	2. `DesktopApp` は plan 生成結果を受け取り、presenter へ渡すだけにする。

### 5-3. `compose_base_frame` / `compose_overlay_frame` を `render` へ寄せる

- 編集:
	- `apps/desktop/src/frame/*`
	- `crates/render/src/lib.rs`
- 新規作成:
	- `crates/render/src/compose.rs`
- 操作:
	1. frame compose 実装を `render::compose` へ移す。
	2. `apps/desktop/src/frame` には presenter 入力変換と OS/window 依存だけを残す。

### 5-4. status / overlay / brush preview 計算を `render` へ統合する

- 編集:
	- `apps/desktop/src/app/present.rs`
	- `apps/desktop/src/frame/mod.rs`
	- `crates/render/src/lib.rs`
	- `crates/render/src/panel.rs`
- 新規作成:
	- `crates/render/src/status.rs`
	- `crates/render/src/brush_preview.rs`
- 操作:
	1. status text bounds、brush preview rect、overlay bounds 計算を `render` 側へ移す。
	2. window レイアウト座標から pixel rect を返す API として整理する。

### 5-5. dirty rect 判断を `render` 側に集約する

- 編集:
	- `apps/desktop/src/app/present_state.rs`
	- `apps/desktop/src/app/present.rs`
	- `crates/render/src/lib.rs`
- 新規作成:
	- `crates/render/src/dirty.rs`
- 操作:
	1. panel dirty、canvas dirty、status dirty、overlay dirty の union と再描画判断を `render::dirty` へ寄せる。
	2. `DesktopApp` は dirty flag 収集だけにし、最終 dirty plan 生成は `render` に委譲する。

### 5-6. `DesktopLayout` と `render` の座標型を整合させる

- 編集:
	- `apps/desktop/src/frame/*`
	- `crates/render/src/lib.rs`
	- `crates/render/src/panel.rs`
- 操作:
	1. `Rect` / `PixelRect` / canvas host rect など重複型を整理する。
	2. layout から compose までの型変換を最小にする。

### 5-7. present テストを `render` 中心へ移す

- 新規作成:
	- `crates/render/src/tests/frame_plan_tests.rs`
	- `crates/render/src/tests/dirty_tests.rs`
	- `crates/render/src/tests/overlay_tests.rs`
- 編集:
	- `apps/desktop/src/app/tests/*`
- 操作:
	1. 画面生成ロジックの検証を `DesktopApp` テストから `render` テストへ移す。
	2. `apps/desktop` 側には integration 相当だけを残す。

### 5-8. `wgpu_canvas.rs` と presenter 側 API を plan 消費型へ更新する

- 編集:
	- `apps/desktop/src/wgpu_canvas.rs`
	- `apps/desktop/src/runtime.rs`
	- `apps/desktop/src/frame/*`
- 操作:
	1. presenter が `RenderFrame` や dirty plan を受け取って描画する形へ整理する。
	2. app 本体が raw pixel buffer 操作に深入りしないようにする。

### 5-9. フェーズ5完了後の文書同期

- 編集:
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
	- `docs/RENDERING-ENGINE.md`
- 操作:
	1. 画面生成責務の中心を `render` に更新する。
	2. dirty rect / overlay / panel compose の所在を文書へ反映する。

---

## フェーズ6: API 名称と物理配置の整理

このフェーズでは命名と実態のズレを減らす。

### フェーズ6完了判定

- `plugin-api` の rename か shim が導入されている
- `panel-sdk` / `panel-macros` が `plugin-sdk` 系へ移行している
- 旧名への依存が workspace 全体で整理されている

### 6-1. `plugin-api` の再命名方針を確定する

- 編集:
	- `crates/plugin-api/Cargo.toml`
	- `crates/plugin-api/src/lib.rs`
	- `Cargo.toml`
- 新規作成候補:
	- `crates/panel-api/Cargo.toml`
	- `crates/panel-api/src/lib.rs`
- 操作:
	1. `plugin-api` を panel 契約専用と認めるなら `panel-api` へ rename する。
	2. 即 rename が大きい場合は shim crate を追加し、移行期間を設ける。

### 6-2. 参照側の crate 名を一括更新する

- 編集:
	- `crates/ui-shell/Cargo.toml`
	- `crates/panel-runtime/Cargo.toml`
	- `crates/panel-sdk/Cargo.toml`
	- `plugins/*/Cargo.toml`
	- `apps/desktop/Cargo.toml`
- 操作:
	1. rename または shim に応じて dependency 名を更新する。
	2. `use plugin_api::...` を新名へ置換する。

### 6-3. `panel-sdk` / `panel-macros` を `plugin-sdk` 系へ再編する

- 編集:
	- `crates/panel-sdk/Cargo.toml`
	- `crates/panel-macros/Cargo.toml`
	- `Cargo.toml`
- 新規作成候補:
	- `crates/plugin-sdk/Cargo.toml`
	- `crates/plugin-sdk/src/lib.rs`
	- `crates/plugin-sdk-macros/Cargo.toml`
- 操作:
	1. authoring surface の論理名を `plugin-sdk` へ寄せる。
	2. 移行期間は re-export で吸収し、plugin 実装の import を段階更新する。

### 6-4. built-in plugin 群の dependency 名を追従更新する

- 編集:
	- `plugins/app-actions/Cargo.toml`
	- `plugins/workspace-presets/Cargo.toml`
	- `plugins/tool-palette/Cargo.toml`
	- `plugins/view-controls/Cargo.toml`
	- `plugins/panel-list/Cargo.toml`
	- `plugins/color-palette/Cargo.toml`
	- `plugins/pen-settings/Cargo.toml`
	- `plugins/*/src/lib.rs`
- 操作:
	1. SDK 名・API 名変更に追従して imports を更新する。
	2. deprecated path を残す場合は段階的 warning コメントを付ける。

### 6-5. sample / tmp / legacy 的資産を整理する

- 編集/移動:
	- `plugins/phase6-sample`
	- `docs/tmp/*`
	- `docs/builtin-plugins/*`
- 操作:
	1. `plugins/phase6-sample` を `tools/experimental/` または `docs/examples/` 相当へ移すか削除する。
	2. 継続参照する一時文書だけを `docs/tmp/` に残し、恒久文書は `docs/` 直下へ移す。

### 6-6. workspace member 名と docs 内表記を同期する

- 編集:
	- `Cargo.toml`
	- `docs/ARCHITECTURE.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
	- `docs/IMPLEMENTATION_STATUS.md`
- 操作:
	1. crate 名変更後の表記揺れを一掃する。
	2. 旧名と新名の対応表を一時的に追記する。

### 6-7. フェーズ6完了後の cleanup

- 編集:
	- rename 対象各 crate の `lib.rs`
	- workspace 全体の import 文
- 操作:
	1. re-export だけの暫定互換層が不要になったら削除する。
	2. `cargo test` と `cargo clippy --workspace --all-targets` を通した状態で旧名残骸を掃除する。

---

## フェーズ7: 再編後の機能拡張

このフェーズでは、整理後の境界を壊さずに拡張機能を追加する。

### フェーズ7完了判定

- Undo/Redo、export、snapshot などの受け皿 module が追加されている
- 新機能が定義済み境界に沿って実装され、`DesktopApp` / `UiShell` / `Document` へ逆流していない

### 7-1. Undo/Redo を command stack として追加する

- 編集:
	- `crates/app-core/src/command.rs`
	- `crates/app-core/src/document.rs`
	- `apps/desktop/src/app/command_router.rs`
	- `plugins/app-actions/src/lib.rs`
- 新規作成:
	- `crates/app-core/src/history.rs`
	- `crates/app-core/src/tests/history_tests.rs`
- 操作:
	1. `Document` の変更適用結果から undo record を構築する。
	2. UI は plugin から undo/redo を呼べるようにする。

### 7-2. canvas runtime と Undo/Redo の接続を実装する

- 編集:
	- `crates/canvas/src/runtime.rs`
	- `crates/canvas/src/context.rs`
	- `crates/canvas/src/tests/*`
- 操作:
	1. `BitmapEdit` 適用時に逆差分または再適用情報を残す。
	2. stroke / fill / erase のまとまり単位で undo できるようにする。

### 7-3. 非同期 job と export 経路を追加する

- 編集:
	- `apps/desktop/src/app/background_tasks.rs`
	- `apps/desktop/src/app/services/project_io.rs`
	- `plugins/job-progress/src/lib.rs`
- 新規作成:
	- `crates/storage/src/export.rs`
	- `apps/desktop/src/app/services/export.rs`
- 操作:
	1. PNG などの export job を background task 化する。
	2. job-progress panel が task 状態を監視できるようにする。

### 7-4. snapshot / branch の document 拡張を行う

- 編集:
	- `crates/app-core/src/document.rs`
	- `crates/workspace-persistence/src/lib.rs`
	- `plugins/snapshot-panel/src/lib.rs`
- 新規作成:
	- `crates/app-core/src/snapshot.rs`
	- `crates/app-core/src/tests/snapshot_tests.rs`
- 操作:
	1. snapshot メタデータと参照関係を `app-core` に追加する。
	2. snapshot-panel plugin から作成・復元・一覧表示できるようにする。

### 7-5. 高度な tool plugin / child tool 構成を導入する

- 編集:
	- `crates/canvas/src/registry.rs`
	- `crates/storage/src/tool_catalog.rs`
	- `plugins/tool-palette/src/lib.rs`
	- `plugins/pen-settings/src/lib.rs`
- 新規作成:
	- `crates/canvas/src/tool_runtime.rs`
	- `crates/canvas/src/tool_manifest.rs`
- 操作:
	1. tool catalog に child tool と parameter file の概念を追加する。
	2. tool-palette と pen-settings から階層表示・選択ができるようにする。

### 7-6. テキスト流し込み機能を plugin + host service で追加する

- 編集:
	- `crates/plugin-api/src/services.rs`
	- `crates/panel-sdk/src/services.rs`
	- `crates/canvas/src/runtime.rs`
- 新規作成:
	- `plugins/text-flow/`
	- `crates/canvas/src/ops/text.rs`
- 操作:
	1. text input と canvas 配置の service API を追加する。
	2. 文字列から bitmap 差分を生成する `canvas` op を追加する。

### 7-7. render / canvas / panel runtime の回帰計測を常設化する

- 編集:
	- `crates/desktop-support/src/profiler/*`
	- `apps/desktop/src/runtime.rs`
	- `logs/*` の生成フローを扱う scripts
- 新規作成:
	- `scripts/profile-render.ps1`
	- `scripts/profile-canvas.ps1`
	- `scripts/profile-panels.ps1`
- 操作:
	1. 主要経路ごとの計測スクリプトを追加する。
	2. フェーズ5以降の性能回帰を継続監視できるようにする。

### 7-8. 最終フェーズの文書整理

- 編集:
	- `docs/IMPLEMENTATION_STATUS.md`
	- `docs/CURRENT_ARCHITECTURE.md`
	- `docs/ARCHITECTURE.md`
	- `docs/MODULE_DEPENDENCIES.md`
	- `docs/ROADMAP.md`
- 操作:
	1. 途中フェーズ向けの暫定表現を消す。
	2. 実装済み構造と今後の拡張余地を最終形へ更新する。

---

## 各フェーズ共通の実施チェック

各フェーズの末尾で必ず次を行う。

1. 追加・移動した module / crate の unit test を追加する
2. integration に残すべきテストだけを `apps/desktop` 側へ残す
3. `cargo test` を通す
4. `cargo clippy --workspace --all-targets` を通す
5. 変更した crate の責務説明を `docs/CURRENT_ARCHITECTURE.md` に反映する
6. 依存関係が変わったら `docs/MODULE_DEPENDENCIES.md` を更新する
7. 到達済み状態が変わったら `docs/IMPLEMENTATION_STATUS.md` を更新する

## この文書の使い方

- 実装時はこの文書を上から順に実行する
- 各タスクは「対象ファイルを読んでから編集」に従う
- 途中で構造変更が発生したら、この文書自体も同じコミット系列で更新する
- 後ろのフェーズに出てくるタスクでも、前倒しで必要になったものは**前フェーズの責務を壊さない範囲で**のみ取り込む
