# 2026 タスク一覧

この文書は、直近の統合タスク一覧への入口である。

現在の最新版は次を参照する。

- [docs/tmp/tasks-2026-03-11.md](tasks-2026-03-11.md)

以下、2026-03-11 版の要点を再掲する。

## 最優先事項

1. `DesktopApp` を薄くし、I/O と event loop 中心へ戻す
2. `canvas` 層を新設し、描画ツール実行と差分生成を host 本体から切り出す
3. `ui-shell` を panel runtime と panel presentation に分離する
4. project / workspace / tool / view / color など非性能領域を plugin 主導へ寄せる

## 直近の具体タスク

### 1. `DesktopApp` の責務削減

対象候補:

- `apps/desktop/src/app/mod.rs`
- `apps/desktop/src/app/commands.rs`
- `apps/desktop/src/app/state.rs`
- `apps/desktop/src/app/present.rs`
- `apps/desktop/src/app/input.rs`

### 2. `canvas` 層の抽出

対象候補:

- `apps/desktop/src/app/drawing.rs`
- `apps/desktop/src/canvas_bridge.rs`
- `crates/app-core/src/painting.rs`
- `crates/app-core/src/document.rs` の一部

### 3. `ui-shell` 分割

runtime 側へ寄せる候補:

- `crates/ui-shell/src/dsl.rs`
- `crates/ui-shell/src/lib.rs` の registry / sync / config 部分

presentation 側に残す候補:

- `crates/ui-shell/src/presentation.rs`
- `crates/ui-shell/src/surface_render.rs`
- `crates/ui-shell/src/focus.rs`
- `crates/ui-shell/src/tree_query.rs`

### 4. plugin-first 化

対象候補:

- `apps/desktop/src/app/commands.rs`
- `crates/storage/src/project_file.rs`
- `crates/storage/src/project_sqlite.rs`
- `crates/desktop-support/src/session.rs`
- `crates/desktop-support/src/workspace_presets.rs`
- `crates/storage/src/tool_catalog.rs`
- `plugins/app-actions/src/lib.rs`
- `plugins/workspace-presets/src/lib.rs`
- `plugins/tool-palette/src/lib.rs`
- `plugins/view-controls/src/lib.rs`
- `plugins/panel-list/src/lib.rs`
- `plugins/color-palette/src/lib.rs`
- `plugins/pen-settings/src/lib.rs`

### 5. 命名整理

- `plugin-api` を panel 専用名へ寄せるか、真に汎用化する
- `panel-sdk` / `panel-macros` を `plugin-sdk` 系へ再編する
- `render` と desktop frame 処理の境界を揃える

### 6. 整理候補

- `plugins/phase6-sample` の移動または削除
- `workspace-persistence` の存続条件見直し
- 文書と実装のズレを継続更新

詳細は [docs/tmp/tasks-2026-03-11.md](tasks-2026-03-11.md) を参照する。
