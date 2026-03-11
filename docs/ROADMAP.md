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
- [docs/tmp/tasks-2026.md](docs/tmp/tasks-2026.md) の追従更新
- crate / module の命名方針整理

### 完了条件

- 「どこへ置くべきか」を文書で即答できる
- 新規実装が `DesktopApp` / `UiShell` / `Document` へ無秩序に集中しない

## フェーズ1: `desktopApp` の縮小

### 目的

`desktopApp` を host orchestration へ戻し、アプリ本体の肥大化を止める。

### 実装するもの

- `apps/desktop/src/app/mod.rs` の責務棚卸し
- input / command / present / state / drawing の分割計画
- project / session / workspace preset orchestration の service 化
- render plan と canvas runtime 呼び出しの外出し

### 完了条件

- `desktopApp` が主に event loop / I/O / 呼び出し順制御を担う
- canvas 実行や panel runtime 内部事情が `desktopApp` に残りすぎない

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

- キャンバス処理の説明を `canvas` を中心に行える
- `Document` は状態保持と command 適用へ寄る
- `desktopApp` は canvas 実行器ではなくなる

## フェーズ3: panel runtime / presentation 分離

### 目的

`ui-shell` の集中責務を減らし、plugin runtime と UI 表示系を分離する。

### 実装するもの

- panel runtime 側の独立境界
- DSL / Wasm bridge の切り出し
- panel config / state patch / host snapshot 同期の再配置
- presentation 側の layout / hit-test / focus / text input の整理

### 完了条件

- runtime 修正が presentation 修正へ不要に波及しない
- panel 描画改善を Wasm bridge 改修から独立して進められる

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

- 非性能領域の新機能を基本 plugin 側へ置ける
- host は plugin のための runtime と service provider として振る舞う

## フェーズ5: `render` 中心の画面生成整理

### 目的

画面生成責務を `render` へ寄せ、desktop 固有の最終提示との境界を明確にする。

### 実装するもの

- render plan の再配置
- desktop frame 計算の再棚卸し
- canvas / overlay / panel layer の責務整理
- dirty rect / quad / overlay 更新判断の統合

### 完了条件

- 画面生成ロジックの中心が `render` にある
- `desktopApp` は GPU 所有と最終提示に集中できる

## フェーズ6: API 名称と物理配置の整理

### 目的

名前と実態のズレを減らし、今後の理解コストを下げる。

### 実装するもの

- `plugin-api` の再定義または改名
- `panel-sdk` / `panel-macros` の `plugin-sdk` 系再編
- sample / tmp / legacy 的資産の再配置
- 文書名と実装名の同期

### 完了条件

- crate 名を見たときに責務を誤解しにくい
- plugin 作者向け入口が一つに見える

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

- 新機能追加時に配置先で迷わない
- plugin と host の境界を壊さず拡張できる

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
- 次の作業候補は `docs/tmp/tasks-2026.md`

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
