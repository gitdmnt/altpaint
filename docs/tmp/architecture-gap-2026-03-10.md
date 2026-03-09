# architecture gap 2026-03-10

## 目的

現在の実装と理想的な責務分割の差分を、Issue 下書きとして残す。

## 2026-03-10 チェック結果

責務境界と主要ファイルサイズを実コード基準で再確認した。

### 責務境界チェック

- Issue 1: **概ね解消**
   - `render` に `CanvasScene` / `prepare_canvas_scene(...)` / dirty 写像 / view 座標変換 / 露出背景計算がある
   - `apps/desktop` 側は `render` の API を呼ぶ薄い橋渡しへ寄った
- Issue 2: **部分解消**
   - `ui-shell` から `presentation.rs` を切り出し、`render` 依存も除去した
   - ただし runtime / presentation の facade 本体がなお `ui-shell/src/lib.rs` に大きく残る
- Issue 3: **解消**
   - `workspace-persistence::WorkspaceUiState` を導入し、project / session の UI 永続化 DTO を共通化した
- Issue 4: **解消**
   - `panel-sdk` の crate doc と plugin 開発文書で、作者の正面入口を `panel-sdk` に統一した
- Issue 5: **解消**
   - `CanvasViewTransform` は `app-core` の state、表示計算は `render` という境界を文書化済み

### 主要ファイルサイズチェック

| ファイル                              | 行数 | 所見                                                       |
| ------------------------------------- | ---: | ---------------------------------------------------------- |
| `crates/ui-shell/src/lib.rs`          | 4064 | 依然として大きい。Issue 2 の継続対象                       |
| `apps/desktop/src/frame.rs`           | 1136 | desktop 固有合成としては許容だが、将来は更なる分割余地あり |
| `crates/panel-sdk/src/lib.rs`         |  683 | SDK surface と helper 集約のため中規模。直ちに問題ではない |
| `crates/render/src/lib.rs`            |  465 | canvas scene API を吸収した結果として妥当                  |
| `apps/desktop/src/app/state.rs`       |  383 | `DesktopApp` 状態集約として妥当                            |
| `crates/ui-shell/src/presentation.rs` |  145 | 切り出し済み。更なる presentation 分離の起点として妥当     |

結論として、**責務境界の主論点は Issue 2 に収束した**。今後は `ui-shell` の runtime / presentation 分離を、責務境界だけでなくファイルサイズ面でも進める。

---

## Issue 1: canvas 表示計算が `apps/desktop` に残りすぎている

### ステータス

解消。

### 現在

- `render` が `CanvasScene` と transform 計画 API を持つ
- `apps/desktop` は `render` の API を使って canvas 表示を準備する

### 理想

`render` が canvas scene 計画を持ち、desktop は OS/GPU 提示だけを担当する。

### 完了条件

- [x] `render` に canvas scene / transform 計算 API を追加する
- [x] `apps/desktop` から canvas 幾何計算を段階的に移す
- [x] `apps/desktop` は `winit` / `wgpu` / desktop chrome に集中する

### メモ

- `apps/desktop/src/frame.rs` はなお 1136 行あり大きいが、Issue 1 の主眼だった canvas 幾何の ownership は `render` 側へ寄った

---

## Issue 2: `ui-shell` が panel runtime と panel presentation を同時に抱えている

### ステータス

部分解消。継続対象。

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

- [x] `ui-shell` の内部責務を runtime 側と presentation 側に分離する設計メモを作る
- [x] 依存方向を崩さずに内部モジュールまたは新 crate へ段階分離する
- [ ] panel performance 改善を runtime 実装から独立して進められる形にする

### 追加観測

- `crates/ui-shell/src/presentation.rs` を追加し、hit-test / focus / `PanelSurface` など presentation 側の型を分離した
- `ui-shell` から `render` 依存は外れた
- ただし `crates/ui-shell/src/lib.rs` は **4064 行**あり、runtime facade と presentation 実装の両方がまだ大きく残る

### 次アクション

- `render_panel_surface()` 周辺を presentation module へさらに移す
- host snapshot / state patch / command mapping を runtime module として切り出す
- `ui-shell/src/lib.rs` を 1000 行未満へ段階縮小することを目安にする

---

## Issue 3: project 永続化と session 永続化の境界は正しいが、重複データがある

### ステータス

解消。

### 現在

- `storage` は project format を持つ
- `desktop-support` は desktop session policy を持つ
- 共有 UI 永続化 DTO は `workspace-persistence::WorkspaceUiState` に集約した

### 理想

ownership は維持しつつ、重複するシリアライズ形を整理する。

- project 保存: 共有・持ち運び可能な作品状態
- session 保存: 起動補助とローカル復元状態

### 完了条件

- [x] project と session で何を保存対象にするかを明文化する
- [x] 必要なら共有 DTO / helper を導入する
- [x] `desktop-support` が desktop policy を持ち、`storage` が project format を持つ構造を維持する

---

## Issue 4: `panel-sdk` と `panel-macros` の関係を文書と API 上で明確化する

### ステータス

解消。

### 現在

- 物理的には別 crate
- plugin 作者は `panel-sdk` 経由で macro を使える
- ただし文脈によっては「別物」に見えやすい

### 理想

plugin 作者に対しては `panel-sdk` を唯一の正面入口として見せる。

### 完了条件

- [x] 文書で「物理分離 / 論理一体」を明記する
- [x] サンプルコードは `panel_sdk::...` 表記へ統一する
- [x] `panel-macros` の直接依存を plugin 作者へ要求しない

---

## Issue 5: `CanvasViewTransform` の ownership と render 責務の境界を固定する

### ステータス

解消。

### 現在

- `CanvasViewTransform` は `app-core::Document` にある
- 表示計算は `render` の canvas scene API へ寄せた

### 理想

- user-visible state は `app-core`
- その state から導く表示ロジックは `render`

### 完了条件

- [x] この分割を `ARCHITECTURE.md` / `RENDERING-ENGINE.md` に明記する
- [x] 将来 `render` へ移す対象関数群を洗い出す
- [x] renderer 専用派生データと永続 state を分けて扱う