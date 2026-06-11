# ADR 015: HTML パネルのキーボード ABI 配線と hit テーブルの GPU 非依存化 (Phase 13)

- 作業日時: 2026-06-11
- 作業 Agent: claude-fable-5 (1M context)
- ステータス: Accepted (実装完了)

## Context

Phase 10-12 で全 12 ビルトインパネルを HTML+CSS+Wasm DOM mutation 経路へ統一したが、
ADR 014 が follow-up として明記したとおり、キーボード関連のテスト 5 件がベースラインから
失敗したまま残存していた:

1. `keyboard_panel_focus_can_activate_app_action`
2. `plugin_keyboard_capture_updates_persistent_config`
3. `plugin_keyboard_shortcut_can_switch_tool`
4. `panel_dispatch_keyboard_path_activates_save_action`
5. `save_and_load_restore_plugin_shortcut_configs`

調査の結果、根本原因は 3 つの独立した欠落だった:

1. **`PanelEvent::Keyboard` の Wasm 転送が未配線**。
   `BuiltinPanelPlugin::handle_event` が `Activate` / `SetValue` / `DragValue` / `SetText`
   のみを処理し、`Keyboard` は `_ => Vec::new()` で握り潰されていた。さらに
   `handles_keyboard_event()` がデフォルトの `false` のままで、
   `PanelRuntime::dispatch_keyboard` のループで全パネルがスキップされていた。
   一方 Wasm 側 (`app-actions` / `tool-palette`) には DSL 時代の `keyboard()` handler が
   `panel_handle_keyboard` export として残っており、ホスト側の配線だけが欠けていた。
2. **HTML パネル hit テーブルの更新が GPU 描画ループ専属**。
   `apps/desktop/src/runtime.rs` の RedrawRequested 内、`render_panels()` (要 `install_gpu_context`)
   の後でのみ `update_html_panel_hits` / `update_html_panel_move_handle` /
   `update_html_panel_full_rect` が呼ばれていた。headless (テスト) では hit テーブルが
   常に空となり、`focus_panel_node` → `focusable_targets()` が空列で失敗していた。
   レイアウト解決 (Blitz `resolve`) 自体は GPU 非依存であり、GPU 専属である必然性はなかった。
3. **DSL 初期 state 宣言のデフォルト値が Phase 10 移行で喪失**。
   旧 `.altp-panel` の `config.save_shortcut: string = "Ctrl+S"` 等の初期値宣言は
   DSL パーサーが state に注入していたが、HTML 化後の `init()` は `render_dom()` のみで
   デフォルトショートカットが未設定 (`shortcut_matches` が空文字で常に不一致) だった。

## Decision

| 項目 | 決定 |
|---|---|
| `BuiltinPanelPlugin` に `has_keyboard_handler: bool` フィールド追加 | `load()` 時に `wasm.has_handler("keyboard")` で確定しキャッシュ (trait method が `&self` のため) |
| `BuiltinPanelPlugin::handles_keyboard_event()` | `has_keyboard_handler` を返すよう override |
| `BuiltinPanelPlugin::handle_event` に `PanelEvent::Keyboard` arm 追加 | `dispatch_to_wasm("keyboard", "keyboard", {shortcut, key, repeat})` へ転送。Wasm 側は `event_string("shortcut")` で payload を読む |
| `HtmlPanelEngine::resolve_action_rects(viewport, scale, chrome_height)` 新設 | `on_render` と同一のクランプ規則 (measured_size を viewport / chrome でクランプ) で layout を解決し `collect_action_rects()` を返す。GPU 不要 |
| `PanelRuntime::collect_panel_hits(sized, scale, chrome_height)` 新設 | パネル毎に `resolve_action_rects` を呼ぶ headless 対応の hit 収集 API |
| hit / move handle / full rect テーブルの更新場所 | `prepare_present_frame` (CPU 側、`refresh_html_panel_hit_tables`) へ移動。GPU ループ (runtime.rs) は quad 組み立てのみに縮小 |
| `PanelGpuFrame::hit_regions` / `rendered_this_frame` | 削除 (hit は `collect_panel_hits` へ分離、rendered フラグは使用箇所ゼロの dead code) |
| `render_panels` 内の `collect_action_rects` 呼び出し | 削除 (重複作業の排除) |
| 不可視パネルの hit 掃除 | `refresh_html_panel_hit_tables` に移動し、`remove_html_panel_full_rect` も追加 (従来は full rect が残置されリサイズハンドルが不可視パネルに反応し得た) |
| `HTML_PANEL_CHROME_HEIGHT = 24` | runtime.rs ローカル const から `apps/desktop/src/app/mod.rs` の `pub(crate)` const へ昇格 (present.rs と共有) |
| `PanelPresentation::html_panel_full_rect(panel_id)` getter 新設 | GPU quad の screen rect が hit テーブルと同一の full rect を共有するため |
| `app-actions::init()` にデフォルトショートカット復元 | `new: Ctrl+N` / `save: Ctrl+S` / `save_as: Ctrl+Shift+S` / `open: Ctrl+O` (DSL 初期値と同値) |
| `tool-palette::init()` にデフォルトショートカット復元 | `pen: P` / `eraser: E` / `bucket: G` / `lasso_bucket: Shift+G` / `panel_rect: K` (同上) |
| stylo resolve のグローバル直列化 | `engine.rs` に `STYLE_RESOLVE_LOCK: Mutex<()>` を新設し `resolve_layout` 内の `document.resolve` を直列化 (下記「並列テストの stylo 競合」参照) |
| テストの実ファイル共有を撤廃 | `apps/desktop` のテスト 35 箇所が `DesktopApp::new` 経由で実ユーザーの session / workspace preset パスを共有・汚染していた問題を、`test_app_with_dialogs` (project / session / preset 全パスをテスト毎に一意化) へ統一して解消 |
| synthetic hit-table 注入テストの実経路化 | `overlapping_panel_button_press_*` / `overlapping_panel_drag_*` を synthetic 注入から実 hit テーブル検証へ書き換え (headless で実経路が動くため注入が不要になった) |

### デフォルト値と永続 config の優先順位

`panel_init` (デフォルト設定) → `register_panel` → `restore_persistent_config`
(永続値があれば `config` オブジェクト全体を置換) の順で適用される。
`persistent_config()` は常に config 全体 (デフォルト + ユーザー変更) を返すため、
保存済みプロジェクト/セッションの復元時にデフォルトが欠けることはない。

### hit テーブル更新タイミング

`prepare_present_frame` 内で `sync_dirty_panels` (Wasm DOM mutation 反映) の**直後**に
`refresh_html_panel_hit_tables` を呼ぶ。これにより同一フレーム内の DOM 変更が
hit 矩形に反映されてから GPU 描画 (runtime.rs) が走る。従来の「描画後に hit 更新」
と比べ、hit と描画のフレームずれも解消される。

### 並列テストの stylo 競合 (`STYLE_RESOLVE_LOCK`)

hit テーブル更新を `prepare_present_frame` へ移した結果、並列実行される全 desktop テストが
Blitz layout 解決を行うようになり、`cargo test -p desktop` で毎回 1〜4 件のテストが
**panic メッセージなし** でランダムに失敗する flakiness が発生した。

原因: Blitz の `BaseDocument::resolve` は stylo のスタイル計算をグローバル rayon プール
(`StyleThread#N`) で行うが、**複数ドキュメントの並行 resolve はスレッドセーフでない**。
stylo 内部の `atomic_refcell` が borrow 競合で panic し、rayon が `resume_unwind` で
呼び出し元テストへ伝播する。このとき panic メッセージは「rayon プールを生成したテスト」の
キャプチャバッファへ吸われるため (panic hook は worker 側でのみ発火)、失敗テスト自身の
出力は空になる — これが診断を困難にしていた。

対処: `resolve_layout` 内の `set_viewport` + `resolve` を `STYLE_RESOLVE_LOCK` (グローバル
`Mutex<()>`) で直列化。プロダクションでは resolve は単一 UI スレッドからのみ呼ばれるため
無競合 (実コストゼロ)。回帰テスト `concurrent_resolve_action_rects_is_safe` (8 スレッド ×
20 回の同時 resolve) で固定した。ロック導入前は 5/5 回再現、導入後は 5/5 回通過。

## Consequences

- **得るもの**:
  - キーボードショートカット (ツール切替 / 保存 / ショートカットキャプチャ) が
    HTML パネル経路で全面復活。ベースラインから失敗していた 5 テストが全て通過
  - hit-test / フォーカス巡回が GPU 非依存になり、headless テストで実経路を検証可能
  - GPU ループ (runtime.rs) が約 120 行縮小し、quad 組み立て専属に単純化
  - `PanelGpuFrame` から dead code (`hit_regions` / `rendered_this_frame`) を駆除
- **失うもの / トレードオフ**:
  - `prepare_present_frame` が毎フレーム hit 収集を行う (profiler key `html_panel_hits`)。
    レイアウト解決は dirty 時のみ走り、`collect_action_rects` は DOM クエリ + 矩形計算のみの
    ため軽量だが、パネル数が大幅に増えた場合は要観測
  - Wasm が `panel_handle_keyboard` を export しないパネルはキーボードイベントを受け取れない
    (`handles_keyboard_event = false`)。意図的な仕様 (dispatch ループのスキップ最適化)

## 検証

- `cargo test -p panel-html-experiment --lib`: 31 passed (新規 3 件含む: resolve_action_rects 2 + 並行ストレス 1)
- `cargo test -p panel-runtime --lib`: 12 passed (新規 7 件含む: keyboard 4 + registry 3)
- `cargo test --workspace`: exit 0、全クレート通過。desktop 144 passed / 0 failed / 6 ignored
  (ベースライン 139 passed / 5 failed から失敗ゼロ化)
- `cargo test -p desktop` 並列実行 2 回連続で 144 passed / 0 failed (flakiness 解消の確認)
- 対照実験: dev HEAD (変更前) へ stash で戻した並列実行 2 回はいずれも「キーボード 5 件のみ失敗・flaky ゼロ」
  であり、flakiness が本変更由来 (per-frame layout の並列化) であることを確認した上で
  `STYLE_RESOLVE_LOCK` により解消
- `cargo clippy --workspace --all-targets`: 新規ファイル起因の警告 0 件 (既存警告のみ)
- Wasm 再ビルド: `.\scripts\build-ui-wasm.ps1` で全 12 パネル再生成
