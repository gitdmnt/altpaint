# ADR 013: HTML パネル手動リサイズと自動サイズ撤去

## Status

Accepted (Phase 11, 2026-05-12, Claude Opus 4.7)

## Context

Phase 10 (ADR 012) で `.altp-panel` DSL を撤廃し、HTML+CSS+Wasm DOM mutation 経路
(`HtmlPanelEngine` + `builtin-panels`) に一本化した。しかし
`HtmlPanelEngine` には DOM レイアウト結果からパネルサイズを自動で算出して
workspace_layout へ書き戻す **自動サイズ追従** が残っており、ユーザーがパネル
サイズを記憶・操作する手段が存在しなかった。具体的には:

- `engine.rs::measure_intrinsic` が body content size を測って measured_size に
  代入する経路
- `on_render` 内で `compute_body_content_size` 結果に応じて measured_size を
  毎フレーム更新し、`pending_size_change` フラグを経由して `take_size_change` で
  上位へ流出させる経路
- `panel-runtime::registry::pending_panel_size_changes` を経由して
  `apps/desktop/src/runtime.rs` が `panel_presentation.set_panel_size` を毎フレーム
  呼び出して workspace を上書き

この自動経路はユーザーの操作意図 (手動でパネルサイズを保持したい) と整合せず、
また workspace 永続値とコンテンツ自然サイズの優先順位が曖昧だった。

## Decision

- `HtmlPanelEngine` の自動サイズ追従経路 (`measure_intrinsic` /
  `compute_body_content_size` / `pending_size_change` / `take_size_change`) を
  **完全撤去**する。
- パネルサイズの単一権威を `workspace_layout.panels[*].size` に移し、起動時に
  `apps/desktop/src/app/bootstrap.rs::apply_ui_state_to_panel_system` が
  `panel.meta.json` の必須フィールド `default_size: { width, height }` で
  初期化する。`WorkspacePanelSize::default()` (300x220) には依存しない。
- 全 4 辺 (N/E/S/W) + 4 角 (NW/NE/SE/SW) の 8 ハンドルでドラッグリサイズ可能とする。
  - ハンドル厚: 辺 6px / 角 12x12px (角優先)
  - タイトルバー上端 6px は N edge として扱う (移動より優先)
  - 最小サイズはパネル root 要素の **CSS `min-width` / `min-height` を尊重** し、
    指定が無い軸は絶対最小 80x60 を適用。最大サイズも **CSS `max-width` /
    `max-height` を尊重** し、指定が無い軸は viewport サイズで上限化。
    `%` 単位や `auto` は制約なし扱い。CSS 制約は taffy::Style の
    `min_size` / `max_size` フィールドから px 値を抽出する。
  - 左 / 上ハンドル時は `set_position_from_absolute` で anchor を再計算し
    対辺の screen 座標を固定 (TopRight/BottomLeft/BottomRight anchor 維持)
- リサイズ中は毎フレーム `restore_panel_size` で engine の `measured_size` に
  追従させ、永続化 (`persist_session_state`) は drag end でのみ実施。
- `viewport > measured_size` のクランプは `on_render` 内の **描画用 local 変数** で
  行い `measured_size` 自体は変更しない (ウィンドウ縮小→復元時の往復不変)。
- カーソルアイコンは hover/active 時に winit `Window::set_cursor` で edge 別に
  切替える (NS/EW/NWSE/NESW)。
- `panel.meta.json` の `default_size` を必須化したことで、新規パネル追加時の
  初期サイズの SoT が一本化される。

## Consequences

- ADR 008 系の "intrinsic 測定で自動追従" 方針を破棄。
- `panel.html` / `panel.css` は固定 viewport 前提で書き、はみ出しは
  CSS `overflow: auto` で対処する規約となる。
- `HtmlPanelEngine` の API 表面が縮小: `take_size_change` / `pending_size_change` /
  `measure_intrinsic` / `compute_body_content_size` 削除。`on_load` のシグネチャを
  `Option<(u32, u32)>` から `(u32, u32)` に変更し必須化。
- `HtmlPanelEngine::root_size_constraints()` / `PanelRuntime::panel_size_constraints()`
  / `PanelSizeConstraints` 型を新規追加し、リサイズ時のクランプで CSS 制約を尊重。
  panel-html-experiment が taffy 0.10.1 (`grid` feature) を直接 dep に追加。
- `panel-runtime::PanelRuntime::pending_panel_size_changes` /
  `take_panel_size_changes` 削除、`apps/desktop/src/runtime.rs` の毎フレーム
  size 書き戻しループも削除 (約 9 行)。
- 起動時の workspace_layout 復元順序が変わる:
  `default_panel_state` / `reconcile_workspace_layout` で size=None で挿入され、
  bootstrap で meta default が注入される。
- 既存セッションファイル (size を永続化済み) はそのまま尊重される。
- `panel.meta.json` のスキーマに `default_size` が必須として追加されたため、
  サードパーティ HTML パネル作者は同フィールドを追加する必要がある (alpha 期の
  破壊的変更として許容)。
- 信頼境界: リサイズ計算は純粋関数 `compute_resized_rect` に切り出され、
  edge 別の幾何学的振る舞いを単体テストで検証可能。

## Alternatives Considered

- 自動サイズを残し手動を上書き優先 → 経路二重化で alpha 期方針 (後方互換コードを
  残さない) に反する。**却下**。
- リサイズ UI を 4 角のみ → エッジドラッグの利便性が落ちる。**却下**。
- `meta.default_size` を Option にして Rust 定数 fallback → SoT が二重化。
  **却下**。
- リサイズ中は engine 追従せず drag end のみ反映 → ドラッグ中の描画が古い
  viewport のままになり CSS layout (text wrap 等) が追従しない違和感。**却下**。
