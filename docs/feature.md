1. イラストの色分析機能があったらいいな〜

### [bug] 外部ペン読み込み不動作 — プランG

- **対象テスト**: tool.pen_import
- **原因**: プランA の `.wasm` 再ビルドで解消するか、`pen_import` サービスハンドラが未実装か調査が必要
- **修正**: 再ビルド後も失敗する場合は `apps/desktop/src/app/services/pen_import.rs` を追加してファイルピッカー → `storage::external_brush` parse → Document 追加のフローを実装
- **完了条件**: 外部ペンファイルを選択するダイアログが開き、選択したペンがパレットに追加される



## 候補タスク

> 以下の項目は 2026-03-15 に対応完了。詳細は `docs/IMPLEMENTATION_STATUS.md` を参照。
> - ~~[bug] パネル通過時の描画破壊~~ → 修正済み
> - ~~[bug/performance] 縮小時アンチエイリアス~~ → 修正済み
> - ~~[architecture] 描画レイヤーの物理的分離~~ → L3/L4 分離・LayerGroupDirtyPlan 完了
> - ~~[improvement] イベント駆動パネル再描画~~ → mark_dirty API 完了
> - ~~[improvement] パネル枠線・内外表示~~ → compose_active_panel_border 完了
> - ~~[improvement] pen-settings スライダー横並び・サイズ表示バグ~~ → 完了

### 実装推奨順序

アーキテクチャ依存の観点から次の順序を推奨する。

1. ~~`[architecture] 描画レイヤーの物理的分離`~~ — **完了 (2026-03-15)**
2. ~~`[improvement] イベント駆動パネル再描画`~~ — **完了 (2026-03-15)**
3. `[performance] パネル描画パフォーマンス改善` — dirty rect 独立管理が整ったので着手可能
4. 残りのタスクは上記と独立して着手可能

---

### ~~[architecture] 描画レイヤーの物理的分離~~ — 完了 (2026-03-15)

- **状態**: **完了** — overlay 単層を L3 TempOverlay / L4 UiPanel に分割。`LayerGroupDirtyPlan` 導入で各層が独立した dirty rect を持つ
- **優先度**: 高（`[improvement] イベント駆動パネル再描画` / `[performance] パネル描画パフォーマンス改善` / `[feature] temp オーバーレイレイヤー` の前提）
- **依存**: なし
- **背景**: UIパネル表示変更のたびにコマ描画が乱れるなど、描画ロジックとUI変更が密結合している
- **方針**: UIレイヤーグループ / 一時描画レイヤー / キャンバスレイヤー / 背景を物理的に分離し、片方の変更が他方へ波及しない構造にする
- **現状**: `render::FramePlan` がすべての平面を一括管理しており、キャンバス・パネル・オーバーレイの再描画トリガーが混在している
- **必要作業**:
  - `render` クレートにレイヤーグループ（UI / temp-overlay / canvas / background）を明確に定義し、合成パスを分離する
  - 各レイヤーグループの dirty rect を独立して管理する
  - UIパネルの再描画がキャンバスレイヤーの再描画をトリガーしない構造にする
- **完了条件**: `crates/render/` にレイヤーグループ別の dirty 判定ユニットテストが通る。UIパネル移動後にキャンバスレイヤーが再描画されないことをテストで確認できる
- **主な変更箇所**: `crates/render/`, `apps/desktop/src/app/`

### ~~[improvement] イベント駆動パネル再描画（ポーリング廃止）~~ — 完了 (2026-03-15)

- **状態**: **完了** — `PanelRuntime.mark_dirty()` / `mark_all_dirty()` / `sync_dirty_panels()` 実装済み。フレームループに全パネルスキャンなし
- **優先度**: 高
- **依存**: `[architecture] 描画レイヤーの物理的分離`（先行することが望ましい）
- **背景**: 毎フレームパネルツリーをスキャンして変更判定している可能性があり、state変更によってレンダリングが発火するReact的設計に変えるべき
- **注意**: レンダリング発火をdirtyにすることと、dirty rect で描画することは別レイヤーの話
- **方針**: パネルのstate変更が直接 dirty flag を立て、フレームループはdirty flagを確認してのみパネルを再描画する
- **現状**: `panel-runtime` の `host_sync.rs` がスナップショット同期を担うが、フレームごとのスキャンがボトルネックになっている可能性がある
- **必要作業**:
  - `PanelRuntime` にstate変更通知API（`mark_dirty(panel_id)` 相当）を追加する
  - `host_sync.rs` でスナップショット差分をstate変更イベントに変換する
  - `render` がdirtyパネルのみ再描画するよう変更する
- **完了条件**: フレームループに全パネルスキャンのコードがない。`mark_dirty` を呼ばない限り対象パネルが再描画されないことをユニットテストで確認できる
- **主な変更箇所**: `crates/panel-runtime/`, `crates/render/`, `apps/desktop/src/app/`

### [performance] パネル描画パフォーマンス改善

- **優先度**: 中
- **依存**: `[improvement] イベント駆動パネル再描画` 完了後に計測・改善するのが望ましい
- **観測している問題**: パネル再構築コストが高い、スクロール時に CPU コピーが発生、文字描画がボトルネック、スライダー fps が低い
- **改善候補（推奨着手順）**:
  1. テキスト計測結果・ノードレイアウト結果のキャッシュ（最も費用対効果が高い）
  2. パネル dirty rect の導入（`[architecture]` タスク完了後）
  3. 頻繁に変わらないパネルの静的サーフェス化
  4. スクロール時の差分 blit / オフセット再利用
  5. 可視領域単位のパネルタイル化（大きなパネルが実装されてから）
- **完了条件**: スライダー操作時に 60fps を維持することを profiler で確認できる

### ~~[improvement] パネル枠線・内外表示~~ — 完了 (2026-03-15)

- **状態**: **完了** — `CanvasOverlayState.active_ui_panel_rect` + `compose_active_panel_border()` 実装済み。アクティブパネルに水色枠線表示
- **優先度**: 中
- **依存**: なし（`render::OverlayPlan` は既に存在する）
- **症状**: どのコマを表示しているのかわかりにくい
- **スコープ**: アクティブパネルへの枠線描画を主とする。パネル外領域の暗幕表示は独立タスクとして別途判断する
- **必要作業**:
  - 現在アクティブなパネルに枠線（1〜2px ボーダー）を描画する
  - `render::OverlayPlan` にパネル境界 overlay を追加する（自然な置き場所）
- **完了条件**: アクティブパネルが視覚的に識別できる枠線が表示され、パネル切り替え時に更新される
- **主な変更箇所**: `crates/render/src/overlay_plan.rs`, `apps/desktop/src/app/present_state.rs`

### [feature] temp オーバーレイレイヤー

- **優先度**: 中
- **依存**: なし（`render::OverlayPlan` は既に存在するため独立実装可能。`[architecture]` 完了後に整合性が高まる）
- **目的**: UIとも描画とも関係のない一時的な線・図形をUIレイヤーとキャンバスレイヤーの間に表示する
- **用途**: lasso fill プレビュー・範囲選択ガイド・パネル作成プレビュー枠など
- **現状**: `render::OverlayPlan` は既に存在するが、ツールからの一時描画コマンドを受け取る API がない
- **必要作業**:
  - `OverlayPlan` に一時ポリライン / 矩形のリストを追加する
  - `canvas` / `gesture` からオーバーレイ描画コマンドを発行する API を設ける
  - lasso fill・範囲選択・パネル作成でこの API を使うよう移行する
- **完了条件**: lasso fill 操作中に選択領域のアウトラインが画面表示され、操作完了後に消える
- **主な変更箇所**: `crates/render/src/overlay_plan.rs`, `crates/canvas/src/gesture/`

### [feature] パネルリサイズ

- **優先度**: 中
- **依存**: なし
- **目的**: UIパネルをドラッグで大きくしたり小さくしたりできるようにする
- **現状**: `ui-shell` は4隅アンカー基準で配置するが、実行時サイズ変更のインタラクションはない
- **必要作業**:
  - パネルエッジ / コーナーのヒットテスト追加（`ui-shell::presentation`）
  - リサイズドラッグのジェスチャー状態管理
  - `WorkspaceLayout` / `WorkspaceUiState` にパネルサイズのオーバーライドを追加（アンカー位置は変えず幅・高さのみ上書き）
  - `workspace-persistence` でサイズを永続化
- **完了条件**: 1つのパネルを端ドラッグで横幅変更でき、アプリ再起動後もサイズが保持される
- **主な変更箇所**: `crates/ui-shell/`, `crates/workspace-persistence/`

### [feature] 描画エンジン: 回転の完全無段階化

- **優先度**: 中（現状は90度単位なら動作する）
- **依存**: なし
- **症状**: 非90度系の回転でキャンバス内容が歪んで見える
- **原因候補**: `CanvasScene` / texture quad / software blit が任意角回転を前提にしていない
- **リスク**: 変更範囲が広い（dirty rect / UV / hit test / brush preview / lasso overlay すべてに影響）。段階実装を推奨
- **必要作業（MVP）**:
  - `render::CanvasScene` の GPU 表示経路（texture quad）を任意角対応にする
  - software 合成経路（CPU compose）も同じ回転モデルを使うよう統一する
- **必要作業（完全対応）**:
  - dirty rect の変換を任意角対応にする
  - hit test を任意角対応にする（四角近似から正確な逆変換へ）
  - brush preview・lasso overlay を任意角対応に揃える
- **完了条件（MVP）**: 45度回転時にキャンバス内容が歪まず表示される。dirty rect は四角近似のまま許容
- **主な変更箇所**: `crates/render/`, `apps/desktop/src/frame/geometry.rs`, `apps/desktop/src/wgpu_canvas.rs`

### [feature] ツールバープラグイン

- **優先度**: 低
- **依存**: なし
- **目的**: ファイル管理・パネル管理・ワークスペース管理などをパネルではなくツールバープラグインとして実装する
- **背景**: 現状これらの機能は `app-actions` / `workspace-presets` / `panel-list` パネルに混在している
- **実装フェーズ**:
  - フェーズA（基盤）: ツールバー種別（`plugin_type: "toolbar"`）を `.altp-panel` DSL に追加し、`ui-shell` にツールバーサーフェスレンダリングとレイアウトを追加する。`panel-runtime` にツールバープラグインの探索・登録経路を追加する
  - フェーズB（移行）: `apps/desktop` の file / panel-management / workspace-management 機能をツールバープラグインへ移行する。フェーズAの動作確認後に着手する
- **完了条件（フェーズA）**: 空のツールバープラグインが画面端に配置され、クリックイベントを受け取れる
- **主な変更箇所**: `crates/panel-dsl/`, `crates/ui-shell/`, `crates/panel-runtime/`, `plugins/`

### [feature] SDK からの render pass 割り込み（filter layer / post effect）

- **優先度**: 低（設計コストが高い）
- **依存**: `[architecture] 描画レイヤーの物理的分離`
- **概要**: plugin SDK から render pass に割り込み、filter layer や post effect を差し込める拡張ポイント
- **注意**: renderer / host ABI / 実行モデルの見直しが必要。単純な SDK API 追加では完結しない
- **実装フェーズ**:
  - フェーズA（設計）: 以下の設計を `docs/adr/` に記録する。実装はフェーズA承認後に着手する
    - pre-composite / per-layer / post-composite の割り込みポイント定義
    - Wasm effect の安全な呼び出し ABI（timeout / fault isolation / fallback）
    - render pass graph の filter layer 対応と dirty rect キャッシュの effect-aware 再設計
    - filter layer の永続化形式（`app-core` / `storage`）
  - フェーズB（実装）: フェーズA 設計承認後に着手
- **完了条件（フェーズA）**: ADR に設計が記録され、レビュー済みである
- **主な変更箇所**: `crates/plugin-host/`, `crates/render/`, `crates/storage/`

### [feature] プラグインホットリロード（開発環境）

- **優先度**: 低（開発者向け機能）
- **依存**: なし
- **目的**: 開発中にプラグインの変更を即座に反映できるようにする
- **方針**: 開発環境ではファイル変更を監視し、`.altp-panel` の再パースとWasm再コンパイル → 再ロードを自動実行する（開発環境ではwasm再コンパイルを走らせる前提）
- **現状**: 再読み込みはアプリ再起動が必要
- **必要作業**:
  - `notify` クレートでプラグインディレクトリを監視する（OS依存のファイル監視は `desktop-support` が担当）
  - ファイル変更イベントで `build-ui-wasm.sh` をバックグラウンドサブプロセスとして実行する（`cfg(debug_assertions)` または feature flag でリリースビルドから除外する）
  - `PanelRuntime` に単一プラグインの再ロード API を追加する（`reload_plugin(name)`）
  - `panel-list` プラグインに手動リロードボタンを追加する（自動 + 手動の両対応）
- **リスク**: サブプロセス実行はプラットフォーム差異が出やすい。初期実装は手動リロードボタンのみでもよい
- **完了条件**: `panel-list` の手動リロードボタンでプラグインが再ロードされ、アプリ再起動なしに変更が反映される
- **主な変更箇所**: `crates/panel-runtime/`, `crates/desktop-support/`, `plugins/panel-list/`

### [improvement] プラグインUI改善（各パネル）

- **優先度**: 低〜中（プラグイン別に独立して着手可能）
- **依存**: なし（「全体」のDSL変更が必要な項目を除き、各パネルは独立して改善可能）

#### 全体（DSL/インフラ変更が必要なもの）

- 各ボタンを右クリックでコンテキストダイアログを開ける機能（`panel-dsl` 拡張が必要）
- `<section>` タグを全体的に削除する（DSL的に不要）
- ショートカット設定ボタンをウィンドウヘッダー相当エリアに移して小さくする
- 選択中パネルをUIレイヤー内で最前面に描画する

#### 全体（個別プラグイン変更のみで完結するもの）

- flaticon.com の SVG でアイコンを差し替える
- テキストラベルをなるべく減らす

#### カラーホイール（`plugins/color-palette`）

- 「カラー」見出しを削除
- プレビューの代わりに一時カラーパレット（10色）をクリック / ショートカットで切り替え
- カラーホイール自体を大きくする
- HSVとカラーコードを並列表示し、クリックでコピーできるようにする

#### 筆設定（`plugins/pen-settings`）

- ~~「現在のツール」項目にはペン名とストロークプレビューのみ表示し、それ以外は削除~~ — **完了 (2026-03-15)**
- ~~スライダーと数値入力欄を横並びにする~~ — **完了 (2026-03-15)**
- ~~**[bug]** 「サイズ」と「pen width」が別の値を表示している問題を修正する~~ — **完了 (2026-03-15)**（`display_value` フィールド追加で対応）
- ストロークプレビュー表示（未実装）

#### ツール（`plugins/tool-palette`）

- アイコン表示で小さくまとめる（現状はテキストリスト）
- 子ツールこそリストで実装する

#### 表示（`plugins/view-controls`）

- キャンバスプレビューにPowerPoint風の変形ハンドルを付けて操作できるUIに刷新
- 前コマ / 次コマ / 中央表示を別プラグインとして分離する

#### レイヤー（`plugins/layers-panel`）

- ページ / パネル番号はパネルタイトル部分に表示する
- 追加/削除・マスク切り替え・合成モード切り替えを同じ行に並列する
- レイヤー表示切り替えを各レイヤー行の左側に目アイコンとして実装する
