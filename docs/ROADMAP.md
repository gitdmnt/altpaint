# altpaint 新ロードマップ

## この文書の役割

この文書は、2026-03-11 時点の実装到達点と [docs/tmp/tasks-2026.md](docs/tmp/tasks-2026.md) の整理結果を前提に、`altpaint` を**plugin-first / host-owned-performance** 方針へ再編するための新しいロードマップである。

旧ロードマップのように「最初から何もない前提」で積み上げるのではなく、**すでにある実装をどう整理し直すか**を中心に定める。

## 固定する原則

今後の設計では次を固定する。

1. 性能が強く要求される処理はアプリ本体に組み込む
2. それ以外の機能は原則 plugin として実装する
3. `desktopApp` は起動、ランタイム I/O、event loop を担う
4. `app-core` はアプリの中核ドメインを担う
5. `render` は画面生成を担う
6. `canvas` はキャンバス処理を担う
7. `ui-shell` は plugin API 提供と panel 統合を担う
8. `plugin-host` は plugin panel runtime を担う
9. `panel-dsl` は panel 定義ファイルの parse を担う
10. `plugin-sdk` は plugin 作者向け SDK を担い、macro はその authoring surface 配下に置く

## 現在すでにある基盤

2026-03-11 時点で、次は既にある。

- `winit` + `wgpu` による desktop host
- `Document` / `Command` 中心の編集モデル
- 複数レイヤー、ビュー変換、回転、dirty rect
- `render` の canvas scene 計画と panel rasterize
- `.altp-panel` + Wasm panel runtime
- `storage` の SQLite project save/load
- session / workspace preset / panel config の永続化
- built-in panel 群の plugin 化
- `tools/` / `pens/` 読込

問題は「基盤がないこと」ではなく、**責務の置き場所がまだ途中であること**にある。

## 再編の大目標

このロードマップでは、次の 4 本柱で進める。

1. `desktopApp` を薄くする
2. `canvas` 層を独立させる
3. `ui-shell` を runtime / presentation に分離する
4. 非性能領域を plugin 主導へ寄せる

---

## フェーズ0: 境界の固定と作業前提の統一

### 目的

今後の移動先を先に固定し、場当たり的な責務追加を止める。

### 実装するもの

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) の目標構造を正本化
- [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md) の現況整理維持
- [docs/tmp/tasks-2026-03-11.md](docs/tmp/tasks-2026-03-11.md) の追従更新
- crate / module の命名方針整理
- `crates/canvas` / `crates/panel-runtime` / `plugin-sdk` 系の命名を先に固定

### 完了条件

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) に責務表、配置草案、配置規約がある
- [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md) に集中箇所とテスト棚卸しがある
- [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) に依存禁止事項と将来配置図がある
- [Cargo.toml](../Cargo.toml) に `crates/canvas` / `crates/panel-runtime` の計画コメントがある
- 新規実装が `DesktopApp` / `UiShell` / `Document` へ無秩序に集中しない判断基準が文書化されている

## フェーズ1: `desktopApp` の縮小

### 目的

`desktopApp` を host orchestration へ戻し、アプリ本体の肥大化を止める。

### 実装するもの

- `apps/desktop/src/app/mod.rs` の責務棚卸し
- input / command / present / state / drawing の分割計画
- project / session / workspace preset orchestration の service 化
- render plan と canvas runtime 呼び出しの外出し

### 完了条件

- `apps/desktop/src/app/services.rs`、`apps/desktop/src/app/io_state.rs`、`apps/desktop/src/app/bootstrap.rs`、`apps/desktop/src/app/command_router.rs`、`apps/desktop/src/app/panel_dispatch.rs`、`apps/desktop/src/app/present_state.rs`、`apps/desktop/src/app/background_tasks.rs` が存在する
- `apps/desktop/src/app/mod.rs` は constructor / type 定義 / 薄い公開 API 中心になる
- `apps/desktop/src/app/tests/` が bootstrap / command router / panel dispatch など module 単位へ分かれる
- canvas 実行や panel runtime 内部事情が `DesktopApp` 本体に残りすぎない

## フェーズ2: `canvas` 層の新設

### 目的

現在分散しているキャンバス処理を一つの責務へまとめる。

### 実装するもの

- `canvas` crate または同等の独立層
- tool 実行ランタイム
- canvas 入力解釈
- bitmap 差分生成と適用補助
- built-in paint 実装の desktop からの移動

### 移行元の代表例

- `apps/desktop/src/app/drawing.rs`
- `apps/desktop/src/canvas_bridge.rs`
- `crates/app-core/src/painting.rs`
- `crates/app-core/src/document.rs` の一部

### 完了条件

- `crates/canvas/Cargo.toml` と `crates/canvas/src/lib.rs` が存在する
- `apps/desktop/src/app/drawing.rs` が削除済みまたは thin wrapper 化されている
- `apps/desktop/src/canvas_bridge.rs` の主要ロジックが `crates/canvas` へ移る
- `crates/app-core/src/document.rs` の paint runtime 文脈解決が `canvas` 側へ移る

## フェーズ3: panel runtime / presentation 分離

### 目的

`ui-shell` の集中責務を減らし、plugin runtime と UI 表示系を分離する。

### 実装するもの

- panel runtime 側の独立境界
- DSL / Wasm bridge の切り出し
- panel config / state patch / host snapshot 同期の再配置
- presentation 側の layout / hit-test / focus / text input の整理

### 完了条件

- `crates/panel-runtime/Cargo.toml` と `crates/panel-runtime/src/lib.rs` が存在する
- `crates/ui-shell/src/presentation/` 配下に layout / hit-test / focus / text input の module がある
- `crates/ui-shell/src/dsl.rs` や registry/runtime sync の主要責務が `crates/panel-runtime` へ移る
- `apps/desktop` が runtime 詳細ではなく facade 経由で panel system を使う

## フェーズ4: plugin-first 化の本格化

### 目的

性能非依存の機能を host 直書きから plugin 主導へ寄せる。

### plugin 化を進める対象

- project ファイルの読込/保存
- workspace の読込/保存
- workspace panel 配置管理
- ツール一覧の読込と表示
- ツールパラメータと親子ツール管理
- view 移動
- panel 一覧と表示切替
- color palette

### 実装するもの

- host service API
- plugin からの I/O request 境界
- tool plugin 実行 API
- 安定した command / service descriptor

### 完了条件

- `apps/desktop/src/app/services/project_io.rs`、`workspace_io.rs`、`tool_catalog.rs` などの service handler が存在する
- `plugins/app-actions`、`plugins/workspace-presets`、`plugins/view-controls`、`plugins/panel-list` が host 固有分岐ではなく service request を使う
- project / workspace / tool catalog の主要 I/O が command 列挙直書きから service 指向へ寄る

## フェーズ5: `render` 中心の画面生成整理

### 目的

画面生成責務を `render` へ寄せ、desktop 固有の最終提示との境界を明確にする。

### 実装するもの

- render plan の再配置
- desktop frame 計算の再棚卸し
- canvas / overlay / panel layer の責務整理
- dirty rect / quad / overlay 更新判断の統合

### 完了条件

- `crates/render/src/frame_plan.rs`、`canvas_plan.rs`、`overlay_plan.rs`、`panel_plan.rs`、`dirty.rs` が存在する
- `apps/desktop/src/app/present.rs` が frame compose 本体ではなく plan 組み立て / presenter 呼び出し中心になる
- `apps/desktop/src/frame/` には desktop 固有の presenter 入力変換だけが残る

## フェーズ6: API 名称と物理配置の整理

### 目的

名前と実態のズレを減らし、今後の理解コストを下げる。

### 実装するもの

- `plugin-api` の再定義または改名
- `panel-sdk` / `panel-macros` の `plugin-sdk` 系再編
- sample / tmp / legacy 的資産の再配置
- 文書名と実装名の同期

### 完了条件

- `plugin-api` の rename または shim 方針がコードで表現されている
- `panel-sdk` / `panel-macros` が `plugin-sdk` 系の re-export または rename へ移行している
- `plugins/*` と `apps/desktop` の import が新名称へ追従している
- `plugins/phase6-sample` や `docs/tmp/*` の恒久配置が整理されている

## フェーズ7: 再編後の機能拡張

### 目的

責務整理後に、機能追加を迷いなく進められる状態へ入る。

### 優先候補

- Undo/Redo
- 高度な document 操作
- 非同期 job と export
- snapshot / branch
- テキスト流し込み
- 高度な tool plugin / child tool 構成

### 完了条件

- `crates/app-core/src/history.rs`、`crates/canvas` の undo 対応、export service、snapshot 拡張など主要機能の受け皿 module が実在する
- 新機能が `apps/desktop` / `ui-shell` / `Document` へ逆流せず、決めた境界に沿って追加されている

---

## 並行で継続する横断項目

### 1. パフォーマンス計測

- profiler 維持
- panel / canvas / input のボトルネック観測
- 責務移動後の回帰確認

### 2. テストと回帰防止

- refactor 前に境界ごとのテストを増やす
- `cargo test` と `cargo clippy --workspace --all-targets` を継続
- panel runtime / canvas runtime / render plan の単体検証を厚くする

### 3. 文書同期

- 現況は `IMPLEMENTATION_STATUS.md`
- 理想は `ARCHITECTURE.md`
- 実コードの構造は `CURRENT_ARCHITECTURE.md`
- 次の作業候補は `docs/tmp/tasks-2026-03-11.md`

## 当面の優先順位

直近は次の順に進める。

1. `desktopApp` の責務棚卸し
2. `canvas` 境界の定義と built-in paint 実装の切り出し
3. `ui-shell` の runtime / presentation 分離
4. plugin-first 化のための host service API 設計
5. `render` への画面生成責務の再配置

## この文書の結論

今の `altpaint` に必要なのは、機能を無差別に足すことではない。

必要なのは、

- 高性能経路を host に残し
- それ以外を plugin へ委譲し
- 現在肥大化している責務を適切な層へ移すこと

である。

この再編が終わると、以後の機能追加は今よりかなり自然になる。
