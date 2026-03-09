# ui-shell runtime / presentation split memo 2026-03-10

## 目的

`ui-shell` の責務を、依存方向を崩さずに `runtime` 側と `presentation` 側へ段階分離するための実装メモを残す。

## 現在の到達点

2026-03-10 時点で次を先に行った。

- `ui-shell` から `render` 依存を除去した
- panel runtime と canvas render の結合を切った
- panel author の入口は `panel-sdk` に統一したまま維持した

この時点で `ui-shell` はなお単一 crate だが、少なくとも panel runtime 改修と panel presentation 改修を別軸で進めやすくなった。

## 分割方針

### 1. runtime 側

責務:

- panel discovery
- `.altp-panel` load / validate
- DSL evaluation
- Wasm runtime bridge
- host snapshot 同期
- `HostAction` / command mapping
- persistent config
- workspace layout

依存:

- `panel-dsl`
- `panel-schema`
- `plugin-host`
- `plugin-api`
- `serde_json`
- `app-core`

### 2. presentation 側

責務:

- panel tree layout
- hit-test
- focus
- text input editing state
- scroll
- software panel rendering
- viewport 切り出し

依存:

- `plugin-api`
- text 描画 helper
- `app-core` の永続 state には直接依存しない

## 先に切る境界

### runtime に残すもの

- `UiShell::register_panel()`
- `UiShell::load_panel_directory()`
- `UiShell::update()`
- `UiShell::handle_panel_event()`
- `UiShell::handle_keyboard_event()`
- `UiShell::move_panel()`
- `UiShell::set_panel_visibility()`
- `DslPanelPlugin`
- host snapshot / state patch / command descriptor 変換群

### presentation に寄せるもの

- `PanelSurface`
- hit region / focus target / text input editor state
- `UiShell::render_panel_surface()`
- focus / cursor / preedit / scroll 系 API
- tree measurement / node drawing / viewport 切り出し

## 段階計画

1. 型定義と helper を internal module へ移す
2. `UiShell` は facade にとどめる
3. panel performance 最適化は presentation module 内で独立に進める
4. runtime 側の Wasm / DSL 変更は presentation 側に波及させない

## 期待する効果

- panel performance 改善を Wasm runtime 改修から独立して進められる
- panel runtime のテストと panel presentation のテストを分けやすい
- 将来 crate 分離するときの境界が先に固定される
