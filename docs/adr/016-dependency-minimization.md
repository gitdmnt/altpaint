# ADR 016: クレート依存構造の最小化リアーキテクト

- 作業日時: 2026-06-12
- 作業 Agent: claude-fable-5 (1M context)
- ステータス: Accepted (実装完了)

## Context

クレート構造が段階的リファクタリング (Phase 9-13) の通過点を引きずったまま複雑化していた。
`cargo metadata` 実測と全クレート横断の使用箇所調査により、次の問題を確定した。

1. **replay 方式 undo の残骸が dead code**。`CanvasRuntime::replay_paint_record` は呼び出し元ゼロ。
   `execute_paint_input` が返す `BitmapEditRecord` は唯一の呼び出し元
   (`services/project_io.rs`) で `let Some((edits, _))` と破棄。`HistoryEntry::BitmapOp` は
   no-op arm とテストのみ。undo 履歴は永続化されないため互換性の制約なし。
2. **ui-shell の約半分が Phase 9E/9F 撤去後の互換スタブ**。1×1 ダミーを返す
   `render_panel_surface`、常に 0 を返す `max_panel_scroll_offset` (→ `scroll_panels` は
   常に no-op)、no-op の `mark_runtime_panels_dirty` / `mark_*_content_dirty`、定数を返す
   `last_panel_*` 5 メソッド、常に同一の定数を返す `handle_panel_event` /
   `PresentationEventResult`。このダミーが `DesktopApp.panel_surface` →
   `render_types::PanelSurfaceSource` → `FramePlan.panel_surface` → `PanelPlan` (読み手ゼロ)
   → 無意味なプロファイラ計測 (1×1 バッファの面積記録) として全層に配線されていた。
3. **ui-shell → panel-runtime 依存の実体は `panel_static_ids()` 1 メソッド**。
   focus 系 3 関数の `_runtime` パラメータは未使用。
4. **未使用依存 7 件** (cargo machete + 手動確認): gpu-canvas/storage の anyhow、
   tool-palette の serde、panel-html-experiment の anyrender / serde / tracing、
   workspace ルートの ab_glyph。
5. **workspace-persistence は 57 行 2 型の DTO クレート**。利用側 3 クレートは全て
   app-core にも依存済みで、本体の `WorkspaceLayout` も app-core にある。
6. **panel-html-experiment は Phase 9E 以降パネル描画の唯一の正式経路**なのに
   experiment という名前のまま (MODULE_DEPENDENCIES.md のリファクタリング候補 #2)。

## Decision

| 項目 | 決定 |
|---|---|
| replay undo 残骸 | `HistoryEntry::BitmapOp` / `BitmapEditRecord` / `BitmapEditOperation` / `replay_paint_record` / `PaintResult` / `canvas::edit_record` / `past_entries()` を削除。`execute_paint_input` は `Option<Vec<BitmapEdit>>` を返す |
| ui-shell スタブ群 | `presentation.rs` / `surface_render.rs` をファイルごと削除。`PanelSurface` / scroll 状態 / no-op・定数スタブ / `handle_panel_event` / `PresentationEventResult` を削除。`FocusTarget` は focus.rs へ移動 |
| ui-shell → panel-runtime 依存 | 切断。`reconcile_runtime_panels(&PanelRuntime)` → `reconcile_panels(panel_ids: Vec<&'static str>)`。desktop 側が `panel_runtime.panel_static_ids()` を渡す。focus 系から runtime 引数を除去 |
| render-types のダミー型 | `PanelPlan` / `PanelSurfaceSource` / `FramePlan.panel_surface` / `panel_plan()` を削除 (読み手ゼロ) |
| desktop のダミー配線 | `DesktopApp.panel_surface` / `panel_surface_*` プロファイラ計測 / `scroll_panel_surface` / `DesktopLayout.panel_host_rect`・`panel_surface_rect` を削除。`needs_panel_surface_refresh` → `needs_panel_reconcile`、`mark_panel_surface_dirty` → `request_panel_reconcile` に改名 (実態への命名一致)。新計測キーは `panel_reconcile` |
| パネル上ホイール | `pointer.rs` でキャンバスへのフォールスルー防止のみ (従来も `scroll_panels` が常に false を返しており等価)。スクロールは Engine 内部で完結 |
| workspace-persistence | クレート削除。`WorkspaceUiState` / `PluginConfigs` を `app-core::workspace` へ統合 (app-core に serde_json を追加)。エッジ 4 本削減 |
| panel-html-experiment | `panel-html` にリネーム (ディレクトリ / パッケージ名 / Rust 名)。機能不変 |
| 未使用依存 7 件 | Cargo.toml から削除。machete クリーン化 |
| 小物 dead code | `SnapshotStore::entries()` / `StatusPanel::last_snapshot()`・`measured_size()` / panel-html `collect_text()` を削除。`StatusPanel::gpu_target()` はテスト使用のみのため `#[cfg(test)]` 化 |
| 陳腐化資産 | `tools/experimental/phase6-sample` (DSL/WAT サンプル、参照ゼロ) と `apps/desktop/ARCH_GREP.md` (撤去済み feature gate に言及する stale な分析スナップショット) を削除 |

### 採用しなかった案

- **`SnapshotEntry.label` の削除**: clippy が "never read" を報告するが、label は
  `SNAPSHOT_CREATE` サービスでパネルから実際に渡される稼働中の契約。削除すると
  サービス ABI が変わり「動作変更ゼロ」の受け入れ条件に反するため残置 (将来の
  スナップショット一覧 UI で読み手がつく見込み)。
- **desktop → panel-api / panel-html 直接依存の撤去**: 実使用があり (service dispatch /
  status bar)、panel-runtime 経由の再エクスポートに変えるのは間接化が増えるだけ。
- **panel-schema と panel-api の統合**: panel-schema は Wasm 側 plugin-sdk と共有。
  統合すると Wasm プラグインに app-core が混入する。
- **pen_exchange / pen_format の削除**: `services/tool_catalog.rs` から配線済みの稼働機能。

## Consequences

- workspace メンバー 29 → 28 (ライブラリ 15 + ビルトインパネル 12 + アプリ 1)、
  クレート間エッジ 47 → 42 (ui-shell→panel-runtime 1 本 + workspace-persistence 関連 4 本)
- ui-shell の依存は app-core / panel-api / render-types のみとなり、
  presentation 層が runtime 層から完全に分離された
- `FramePlan` はキャンバス合成専用となり、パネル面の概念が型レベルから消えた
- 検証結果: `cargo test --workspace` 412 passed / 0 failed / 8 ignored
  (ベースライン 415 との差分 3 件は削除した dead 機能のテスト)、
  clippy 警告 84 → 79、cargo machete クリーン、アプリ起動 12 秒スモーク確認
  (クラッシュ・エラー出力なし)
- 文書更新: CLAUDE.md / MODULE_DEPENDENCIES.md / IMPLEMENTATION_STATUS.md /
  CURRENT_ARCHITECTURE.md / RENDERING-ENGINE.md / ROADMAP.md / feature.md
