# ADR 014: PanelTree/PanelNode 撤廃と workspace-layout の HTML パネル化 (Phase 12)

- 作業日時: 2026-05-15
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted (実装完了)

## Context

Phase 10 (ADR 012) で `.altp-panel` DSL を撤廃し HTML+CSS+Wasm DOM mutation に一本化、
Phase 11 (ADR 013) で HTML パネルに 8 ハンドル手動リサイズを導入した。しかし以下の乖離が残存していた:

1. **DSL 時代の中間表現 (`PanelTree` / `PanelNode` / `PanelView`) が `crates/panel-api/` 内に残置**。
   ADR 012 は Decision で `PanelTree` DTO の削除を宣言したが、実際には型と
   `PanelPlugin::panel_tree()` / `view()` のデフォルト実装が残り、HTML パネル経路では空 children を返す
   no-op として消極的に維持されていた。
2. **`builtin.workspace-layout` (パネル表示/非表示管理 UI) のみ Rust ネイティブ実装**。
   `crates/ui-shell/src/workspace.rs::workspace_manager_tree()` が `PanelTree` で UI を構築し、
   `apps/desktop/src/runtime.rs::render_panels()` 経由で毎フレーム GPU に乗せていた。
   これにより:
   - ADR 012 で「DSL 撤廃完了」と書いた文言と実装のズレ
   - ADR 013 で導入した 8 ハンドルリサイズが workspace-layout で動かない不整合
   - PanelTree/PanelNode を消そうとすると workspace 管理 UI が機能を失うブロック
3. **DSL 時代の関連経路 (`tree_query.rs` / `focus.rs` の dropdown / text_input 走査 /
   `TextInputEditorState`) が PanelTree 走査前提のまま残存**。HTML パネル経路では未使用の
   dead code となっていた。

ultrareview からの REQUEST_CHANGES でも上記の乖離が指摘された。alpha 期の方針
「後方互換コードを残さない」に従い、これらを単一 PR で一括撤去する。

## Decision

| 項目 | 決定 |
|---|---|
| `builtin.workspace-layout` の実装形態 | 12 番目の HTML パネルとして再実装 (`crates/builtin-panels/workspace-layout/`) |
| `PanelTree` / `PanelNode` 型 | `crates/panel-api/src/lib.rs` から削除 |
| `PanelView` 型 (DSL 時代の遺物) | 同上、削除 |
| `PanelPlugin::panel_tree` / `view` trait method | trait から削除 |
| `PanelPlugin::handle_event` のデフォルト実装 | `Vec::new()` 返しのみに簡素化 (旧版は `find_actions_in_nodes` で tree を走査) |
| `find_actions_in_nodes` / `find_actions_in_node` | 削除 |
| `panel_runtime::registry::panel_trees()` / `panel_tree_cache` / `rebuild_tree_cache` | 削除 |
| `panel_runtime::registry::panel_views()` | 削除 |
| `panel_runtime` `dispatch_event` / `dispatch_keyboard` の tree 差分検知 | 削除し「event ハンドラが呼ばれたパネルは無条件で `changed_panel_ids` に入れる」no-op 化 |
| `panel_runtime::registry::sync_document_subset` の tree 比較 | 同上、`update` が呼ばれたパネルを無条件で集合に入れる |
| `BuiltinPanelPlugin::panel_tree` impl | 削除 |
| `crates/ui-shell/src/workspace.rs::workspace_manager_tree` / `visible_panels_in_order` / `workspace_panel_actions` / `workspace_panel_entries` | 削除 |
| `crates/ui-shell/src/lib.rs::panel_trees` / `handle_panel_event` の workspace 特殊分岐 | 削除 |
| `crates/ui-shell/src/tree_query.rs` 全体 | 削除 |
| `focus.rs` の `is_dropdown_target` / `text_input_state_for_target` / IME 編集経路 | 削除 (HTML パネル内部完結に統一) |
| `TextInputEditorState` 型 + 関連メソッド (`insert_text_into_focused_input` / `backspace` / `delete` / `move_cursor*` / `set_preedit` / `has_focused_text_input`) | 削除 |
| `apps/desktop/src/runtime/keyboard.rs` の `handle_text_edit_key` / `supports_editing_repeat` / IME 連携 | 削除 |
| 新規 service `workspace_layout.set_panel_visibility` | `crates/panel-api/src/services.rs` に定数追加、`apps/desktop/src/app/services/workspace_layout.rs` で handler 実装 |
| 新規 host snapshot field `workspace.panels_json` | `crates/panel-runtime/src/host_sync.rs` の `build_host_snapshot_cached` に追加。`PanelRuntime::set_workspace_panels_json` 経由で `BuiltinPanelPlugin` に注入 |
| 新規 host API `host::workspace::panels_json()` | `crates/plugin-sdk/src/host.rs` に追加 |

### 新パネル `builtin.workspace-layout` の実装

- 配置: `crates/builtin-panels/workspace-layout/{panel.meta.json, panel.html, panel.css, Cargo.toml, src/lib.rs, builtin_panel_workspace_layout.wasm}`
- DOM: `<ul id="workspace-panel-list">` に `<li><label><input type="checkbox" data-action="altp:activate:set_visibility" data-args='{"value":<NEXT>,"panel_id":"<ID>"}' [checked]/><span>{title}</span></label></li>` を sync_host で構築。`builtin.workspace-layout` 自身は除外。
- ハンドラ: `fn set_visibility(value: i32)` 内で `let panel_id = event_string("panel_id")` し `services::workspace_layout::set_panel_visibility(panel_id, value != 0)` を emit。
- XSS 対策: `dom::html_escape` で panel_id / title をエスケープ。

### `dispatch_event` の changed_panel_ids 検出方針 (事前調査結果)

旧経路は `panel.handle_event` の前後で `panel_tree()` の `PartialEq` 比較を行い、
変化があれば panel_id を `changed_panel_ids` に入れていた。実態としてこの集合は
`apps/desktop/src/app/panel_dispatch.rs` で `mark_runtime_panels_dirty` (no-op) と
`mark_panel_surface_dirty()` を呼ぶトリガーとしてのみ使われている。HTML パネル経路では
`HtmlPanelEngine::render_dirty` が GPU 側の真の dirty 判定を持ち、PanelTree 比較は
情報源として冗長だった。撤廃方針は「event ハンドラが呼ばれた = 何かが起きた」を前提に、
対象パネル ID を**無条件で**集合に入れる no-op 化に統一する。これは PanelTree 比較が
実質提供していた dirty トリガーと同じ粒度を維持しつつ、型依存を排除する。

`dispatch_keyboard` の `handled` 判定は `!panel_actions.is_empty() || persistent_config != previous_config`
に簡素化 (旧版は tree 比較も OR していた)。HTML パネル経路では handler が
emit_command / emit_service / state_patch のいずれかを必ず行うため、検知の漏れは発生しない。

## Consequences

- **得るもの**:
  - `panel-api` の trait 表面が縮小し、新規プラグイン著者が学習すべき API が小さくなる
  - DSL 時代の dead code が一掃され、コードベースが約 800 行縮小
  - workspace-layout でも 8 ハンドルリサイズが利用可能になる
  - ADR 012/013 の文言と実装の整合が取れる
- **失うもの**:
  - DSL 時代の宣言的 IR (`PanelTree` / `PanelNode`) — 使われていない
  - winit IME 経由のテキスト編集経路 — HTML パネル内部完結に統一 (Blitz が IME 入力を扱う)
- **focus 経路の変化**:
  - `focusable_targets` は HTML hit table (`html_panel_hits`) を辿る純粋関数になり、
    GPU 描画前提となる。ユニットテストでフォーカス遷移を検証する場合は
    `update_html_panel_hits` で hit を inject する必要がある。
- **persistence**:
  - workspace の可視性 (`WorkspacePanelState.visible`) はそのまま永続化される
    (既存セッションファイル互換)。
- **サードパーティ HTML パネルへの影響**:
  - `PanelPlugin::panel_tree` / `view` trait method を実装していたパネルはコンパイル不能になる。
    alpha 期の破壊的変更として許容。

## Alternatives Considered

- **HostAction を Wasm から直接 emit する新 ABI 整備** — `SetPanelVisibility` HostAction を
  そのまま流す方が型一貫性があるが、Wasm panel handler は CommandDescriptor (= service request)
  のみを emit する設計に統一済み。新 ABI 追加は複雑度の割に得がない。**却下**。
- **`MovePanel` も workspace_layout サービスとして公開し、ドラッグ並び替え UI を追加** — 本 PR の
  スコープを膨らませる。並び替え UX はオプションとして将来検討。**スコープ外**。
- **`PanelTree` を残し HTML 翻訳の中間表現として再利用** — ADR 012 の方針 (DSL/IR 撤廃) に矛盾し、
  dead code を温存する。**却下**。

## Verification

実行手順:

1. `pwsh scripts/build-ui-wasm.ps1` で 12 個の wasm が出力される (`builtin_panel_workspace_layout.wasm` を含む)。
2. `cargo build --workspace` 全 pass。
3. `cargo test --workspace` で新規追加テストが pass:
   - `panel-runtime` `host_sync::tests::host_sync_emits_workspace_panels_json_in_registered_order`
   - `panel-runtime` `host_sync::tests::host_sync_emits_empty_workspace_panels_json_when_absent`
   - `desktop` `app::tests::service_dispatch_tests::request_service_workspace_layout_set_panel_visibility_toggles_visibility`
   - `desktop` `app::tests::persistence::panel_visibility_round_trip_through_session_save_load`
   - `builtin-panel-workspace-layout` 5 件 (`render_panel_list_*` + `entrypoints_callable_on_native`)
   - `ui-shell` `tests::workspace_panel_entries_include_all_registered_panels`

   なお `desktop` バイナリには本 ADR の範囲外で**ベースライン (commit e6f84f6) 時点から
   既に失敗していた** focus / keyboard 関連の 5 テスト
   (`keyboard_panel_focus_can_activate_app_action`,
   `plugin_keyboard_capture_updates_persistent_config`,
   `plugin_keyboard_shortcut_can_switch_tool`,
   `panel_dispatch_keyboard_path_activates_save_action`,
   `save_and_load_restore_plugin_shortcut_configs`) が残存している。これらは Phase 10/11
   の遺物で、HTML パネル経路の handles_keyboard_event ABI 未整備が根本原因。本 ADR では
   修正対象外とし、follow-up issue で扱う。本 PR は新規 failure をゼロ件しか追加していない
   (テスト結果: 139 passed / 5 failed / 6 ignored、ベースラインと完全一致)。

4. `cargo clippy --workspace --all-targets` でベースライン以上の警告増なし
   (ベースライン 84 → 本 PR 76 件、8 件減)。
5. 実機 `cargo run -p desktop`:
   - workspace-layout パネルが TopLeft anchor で表示され、11 行のチェックボックスリストが描画される
   - `builtin.tool-palette` のチェックを外す → 該当パネルが画面から消える。チェック復帰で再表示
   - 8 ハンドルリサイズが workspace-layout でも機能する
   - アプリ再起動後にチェック状態が永続化されている
6. Grep 検証: `Select-String -Path 'crates\**\*.rs','apps\**\*.rs' -Pattern 'PanelTree|PanelNode|PanelView|TextInputEditorState|tree_query'` が
   コード本体からゼロ件 (ドキュメント・コメント・ADR 引用のみ残る)。

## 関連 ADR / 文書

- ADR 009: DSL → HTML 翻訳器採用 (本 ADR で完全撤去)
- ADR 010: `crates/render` クレート物理削除
- ADR 011: `html-panel` feature gate 完全撤去
- ADR 012: `.altp-panel` DSL 撤去 (本 ADR は post-acceptance note で参照される)
- ADR 013: HTML パネル手動リサイズ (同上)
- `docs/IMPLEMENTATION_STATUS.md` (Phase 12 完了記録は本 PR で追加)
