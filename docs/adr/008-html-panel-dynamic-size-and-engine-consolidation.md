# ADR 008: HTML パネル枠サイズの動的化と Engine への責務集約

- 作業日時: 2026-04-25
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted

## コンテキスト

ADR 007 で導入した HTML パネル GPU 直描画統合（Phase 8F）以降、HTML パネルの描画枠サイズは `WorkspacePanelSize::default()` 固定（300x220 fallback）で決まっており、HTML コンテンツの自然サイズと一致しない問題があった。同時に「枠サイズ・dirty・GPU target・chrome 描画」の責務が `HtmlPanelEngine` / `HtmlPanelPlugin` / `apps/desktop/runtime.rs` の 3 層に分散していた。

`<details>` / `:hover` / `<select>` などのユーザー操作で HTML レイアウトサイズが動的に変わるケースに追従させたい、というのが今回の駆動要件。

## 決定

1. **「コンテンツ駆動サイズ」モデルを採用**: パネルの権威サイズは HTML レイアウト計算結果（`<body>` の `final_layout.content_size`）。`WorkspacePanelSize` は永続化された初期値としてのみ使う。
2. **`HtmlPanelEngine` に責務を集約**: `measured_size`, `layout_dirty`, `render_dirty`, `gpu_target`, chrome 描画、UI イベント転送をすべて Engine 内に閉じる。`HtmlPanelPlugin` は `panel-api` 接合層として薄いラッパに退ける。
3. **入力経路を実配線**: Pointer 系イベントは `panel_runtime::forward_html_input` 経由で `EventDriver::handle_ui_event` に流し、`:hover` / `<details>` 開閉 / `<button>` click を動かす。`<select>` は popup レイヤ責務の設計が別途必要なためスコープ外。
4. **永続化は既存経路に乗せる**: `take_html_size_changes()` で吸い取った変化を `panel_presentation.set_panel_size()` 経由で `workspace_layout` に書き戻す。session/project の workspace persistence がそのまま再利用される。
5. **幅は MaxContent で初回測定 → 以降は viewport クランプのみ**: 改行による振動を避けるため、幅は `measure_intrinsic` で確定後固定とし、コンテンツ変化に追従するのは高さ。`<details>` 等で幅も伸びるケースは `on_render` の都度再 measure する設計で吸収（毎フレーム resolve は元々 dirty 駆動なのでコスト負担は許容範囲）。

### Blitz 0.3.0-alpha API 制約への対処

- intrinsic size 専用 API は無いため、巨大 viewport (8192) で resolve → `<body>` の `final_layout.content_size` を読む方式を採用。`<body>` 直下の bbox を走査する初期実装は inline 要素が拾えず却下。
- `BaseDocument::has_changes()` は実装バグで使えない。Engine 内で `pending_mutation` / `layout_dirty` を自前トラックする方針を継続。
- `:hover` / `<details>` 開閉は `BaseDocument` の API 自動 reflow で発火しないため、`EventDriver::handle_ui_event` を呼ぶ経路を確立し、その後で `layout_dirty = true` を立てる。

## 影響

- `HtmlPanelPlugin::load(directory)` のシグネチャを `load(directory, restored_size: Option<(u32, u32)>)` に拡張（破壊的変更）。`from_parts` も同様。
- `panel_runtime::render_html_panels` の呼び出し側（`apps/desktop/runtime.rs`）は `panel_rect_in_viewport` のサイズ部分を使わなくなった。固定 fallback (300x220) を削除。
- `panel-html-experiment` クレートに `keyboard-types` を dev-dep として追加（テストでの `BlitzPointerEvent` 構築用）。
- 本番経路で `apps/desktop` が `panel-html-experiment` と `keyboard-types` を `html-panel` feature 経由で参照（pointer event の構築のため）。

## 受け入れ条件と結果

| 条件 | 結果 |
|------|------|
| 起動直後の枠がコンテンツ自然サイズ | OK（300x220 固定撤廃） |
| 永続化サイズの restore | OK（`apply_ui_state_to_panel_system` で `restore_html_panel_size`） |
| `<details>` クリックで枠の高さ追従 | OK（`forward_html_input` + `take_html_size_changes`） |
| `:hover` でスタイル切替 | OK（PointerMove を Blitz に転送） |
| viewport 上限クランプ | OK（`on_render` 内で `min(measured, viewport)`） |
| chrome 領域 pointer の振り分け | OK（`html_panel_at` が body 領域のみ返す） |
| `runtime.rs` の HTML 専用 fallback 削除 | OK |
| 既存 GPU 統合テスト pass | OK（panel-runtime 26 passed, desktop 110 passed） |

## 代替案と却下理由

- **`taffy::compute_root_layout` を直接呼んで MaxContent を測る**: 公開されているが `BaseDocument` 内部の resolve 後処理（サブドキュメント viewport 連動）がスキップされる。`set_viewport(8192) → resolve` 方式の方が安全。
- **`HtmlPanelHost` を新設して Engine と Plugin の中間層に挟む**: ユーザーとの対話で「Engine に集約」が選ばれた。中間層を増やすと境界が増えて responsibility がぼやける。
- **`<select>` ドロップダウン対応を含める**: popup を別レイヤで描画するか、HTML パネルの矩形外まで拡張するかの設計判断が必要で、本タスクのスコープを大きく超える。受け入れ条件と次フェーズに切り出した。
- **`render_html_panels` 戻り値型を破壊的変更して size 変化を含める**: 既存呼び出し側への影響が広いため却下。`take_html_size_changes()` を別 API として用意し、戻り値型は据え置き。

## 関連文書

- ADR 007: HTML パネル実験（GPU 直描画統合）
- `docs/IMPLEMENTATION_STATUS.md` Phase 8G セクション
- 計画文書: `.context/plan-html-panel-dynamic-size-2026-04-25.md`
