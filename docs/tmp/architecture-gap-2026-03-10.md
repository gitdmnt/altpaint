# architecture gap 2026-03-10

## 目的

現在の実装と理想的な責務分割の差分を、Issue 下書きとして残す。

---

## Issue 1: canvas 表示計算が `apps/desktop` に残りすぎている

### 現在

- `apps/desktop/src/frame.rs`
- `apps/desktop/src/canvas_bridge.rs`
- `apps/desktop/src/app/present.rs`

が次を持っている。

- dirty rect の表示先写像
- 画面座標 <-> canvas 座標変換
- パン/ズーム時の露出背景計算
- ブラシプレビュー矩形
- canvas quad 計算

### 理想

`render` が canvas scene 計画を持ち、desktop は OS/GPU 提示だけを担当する。

### 完了条件

- `render` に canvas scene / transform 計算 API を追加する
- `apps/desktop` から canvas 幾何計算を段階的に移す
- `apps/desktop` は `winit` / `wgpu` / desktop chrome に集中する

---

## Issue 2: `ui-shell` が panel runtime と panel presentation を同時に抱えている

### 現在

`ui-shell` が次を同時に持っている。

- panel discovery
- DSL evaluation
- Wasm runtime bridge
- host snapshot 同期
- command mapping
- persistent config
- layout / hit-test / focus / text input / scroll
- software panel rendering

### 理想

少なくとも次の2方向へ分割できる構造にする。

1. panel runtime 層
   - DSL / Wasm / state / host snapshot / command mapping
2. panel presentation 層
   - tree layout / hit-test / focus / text input / rendering

### 完了条件

- `ui-shell` の内部責務を runtime 側と presentation 側に分離する設計メモを作る
- 依存方向を崩さずに内部モジュールまたは新 crate へ段階分離する
- panel performance 改善を runtime 実装から独立して進められる形にする

---

## Issue 3: project 永続化と session 永続化の境界は正しいが、重複データがある

### 現在

- `storage` は project file に `workspace_layout` と `plugin_configs` を保存する
- `desktop-support` は session file にも `workspace_layout` と `plugin_configs` を保存する

### 理想

ownership は維持しつつ、重複するシリアライズ形を整理する。

- project 保存: 共有・持ち運び可能な作品状態
- session 保存: 起動補助とローカル復元状態

### 完了条件

- project と session で何を保存対象にするかを明文化する
- 必要なら共有 DTO / helper を導入する
- `desktop-support` が desktop policy を持ち、`storage` が project format を持つ構造を維持する

---

## Issue 4: `panel-sdk` と `panel-macros` の関係を文書と API 上で明確化する

### 現在

- 物理的には別 crate
- plugin 作者は `panel-sdk` 経由で macro を使える
- ただし文脈によっては「別物」に見えやすい

### 理想

plugin 作者に対しては `panel-sdk` を唯一の正面入口として見せる。

### 完了条件

- 文書で「物理分離 / 論理一体」を明記する
- サンプルコードは `panel_sdk::...` 表記へ統一する
- `panel-macros` の直接依存を plugin 作者へ要求しない

---

## Issue 5: `CanvasViewTransform` の ownership と render 責務の境界を固定する

### 現在

- `CanvasViewTransform` は `app-core::Document` にある
- 実際の表示計算は主に `apps/desktop` 側にある

### 理想

- user-visible state は `app-core`
- その state から導く表示ロジックは `render`

### 完了条件

- この分割を `ARCHITECTURE.md` / `RENDERING-ENGINE.md` に明記する
- 将来 `render` へ移す対象関数群を洗い出す
- renderer 専用派生データと永続 state を分けて扱う