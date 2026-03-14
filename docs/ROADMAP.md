# altpaint ロードマップ

## 固定する原則

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

---

## 完了フェーズ

| フェーズ | 内容 | 完了時点 |
|----------|------|----------|
| 0 | 境界の固定と作業前提の統一 | 2026-03-11 |
| 1 | `desktopApp` の縮小 | 2026-03-11 |
| 2 | `canvas` 層の新設 | 2026-03-12 |
| 3 | panel runtime / presentation 分離 | 2026-03-12 |
| 4 | plugin-first 化の本格化（`ServiceRequest` 導入） | 2026-03-12 |
| 5 | `render` 中心の画面生成整理 | 2026-03-12 |
| 6 | API 名称と物理配置の整理 | 2026-03-12 |
| 7 | 再編後の機能拡張（Undo/Redo・export・snapshot・text-flow・tool child・profiler） | 2026-03-14 |

詳細は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) を参照。

---

## 並行で継続する横断項目

### パフォーマンス計測

- profiler 維持
- panel / canvas / input のボトルネック観測
- 責務移動後の回帰確認

### テストと回帰防止

- `cargo test` と `cargo clippy --workspace --all-targets` を継続
- panel runtime / canvas runtime / render plan の単体検証を厚くする

### 文書同期

- 現況は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- 目標構造は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 実コードの構造は [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md)

---

## 候補タスク

### [bug] パネル通過時の描画破壊

- **症状**: UIパネルが上を通過するたびにコマの表示がおかしくなる
- **調査起点**: `render::OverlayPlan` / `render::CanvasPlan` の dirty rect 計算、`ui-shell` のパネルサーフェス合成順
- **主な変更箇所**: `crates/render/`, `crates/ui-shell/`

### [bug/performance] 縮小時アンチエイリアス

- **症状**: ズームアウト時にキャンバスのフチがジャギー・線がブツブツ途切れて見える
- **原因候補**: blit / compose パスで縮小時の補間がニアレストネイバーになっている
- **必要作業**:
  - `crates/render/` のソフトウェア合成パスで bilinear / box filter 補間を導入する
  - WGPU サンプラー設定の確認（`FilterMode::Linear` への切り替え）
- **主な変更箇所**: `crates/render/`, `apps/desktop/src/wgpu_canvas.rs`


### [improvement] パネル枠線・内外表示

- **症状**: どのコマを表示しているのかわかりにくい
- **必要作業**:
  - 現在アクティブなパネルに枠線またはオーバーレイ色を描画する
  - `render::OverlayPlan` にパネル境界 overlay の追加が自然な置き場所
  - パネル外領域への暗幕表示もここで対応可
- **主な変更箇所**: `crates/render/src/overlay_plan.rs`, `apps/desktop/src/app/present_state.rs`

### [feature] temp オーバーレイレイヤー

- **目的**: UIとも描画とも関係のない一時的な線・図形をUIレイヤーとキャンバスレイヤーの間に表示する
- **用途**: lasso fill プレビュー・範囲選択ガイド・パネル作成プレビュー枠など
- **現状**: `render::OverlayPlan` は既に存在するが、ツールからの一時描画コマンドを受け取る API がない
- **必要作業**:
  - `OverlayPlan` に一時ポリライン / 矩形のリストを追加する
  - `canvas` / `gesture` からオーバーレイ描画コマンドを発行する API を設ける
  - lasso fill・範囲選択・パネル作成でこの API を使うよう移行する
- **主な変更箇所**: `crates/render/src/overlay_plan.rs`, `crates/canvas/src/gesture/`

### [feature] 描画エンジン: 回転の完全無段階化

- **症状**: 非90度系の回転でキャンバス内容が歪んで見える
- **原因候補**: `CanvasScene` / texture quad / software blit が任意角回転を前提にしていない
- **必要作業**:
  - `render::CanvasScene` の回転モデルを quarter turn 依存から外す
  - dirty rect / UV / hit test / brush preview / lasso overlay の変換を任意角対応に揃える
  - WGPU 表示経路と software 合成経路の両方で同じ回転モデルを使う
- **主な変更箇所**: `crates/render/`, `apps/desktop/src/frame/geometry.rs`, `apps/desktop/src/wgpu_canvas.rs`

### [performance] パネル描画パフォーマンス改善

- **観測している問題**: パネル再構築コストが高い、スクロール時に CPU コピーが発生、文字描画がボトルネック、スライダー fps が低い
- **改善候補**:
  - 可視領域単位のパネルタイル化
  - テキスト計測結果・ノードレイアウト結果のキャッシュ
  - スクロール時の差分 blit / オフセット再利用
  - パネル dirty rect の導入
  - 頻繁に変わらないパネルの静的サーフェス化

### [feature] パネルリサイズ

- **目的**: UIパネルをドラッグで大きくしたり小さくしたりできるようにする
- **現状**: `ui-shell` は4隅アンカー基準で配置するが、実行時サイズ変更のインタラクションはない
- **必要作業**:
  - パネルエッジ / コーナーのヒットテスト追加（`ui-shell::presentation`）
  - リサイズドラッグのジェスチャー状態管理
  - `WorkspaceLayout` / `WorkspaceUiState` にパネルサイズのオーバーライドを追加
  - `workspace-persistence` でサイズを永続化
- **主な変更箇所**: `crates/ui-shell/`, `crates/workspace-persistence/`

### [feature] ツールバープラグイン

- **目的**: ファイル管理・パネル管理・ワークスペース管理などをパネルではなくツールバープラグインとして実装する
- **背景**: 現状これらの機能は `app-actions` / `workspace-presets` / `panel-list` パネルに混在している
- **必要作業**:
  - ツールバー種別（`plugin_type: "toolbar"`）を `.altp-panel` DSL に追加
  - `ui-shell` にツールバーサーフェスレンダリングとレイアウトを追加
  - `panel-runtime` にツールバープラグインの探索・登録経路を追加
  - `apps/desktop` の file / panel-management / workspace-management をツールバープラグインへ移行
- **主な変更箇所**: `crates/panel-dsl/`, `crates/ui-shell/`, `crates/panel-runtime/`, `plugins/`

### [feature] SDK からの render pass 割り込み（filter layer / post effect）

- plugin SDK から render pass に割り込み、filter layer や post effect を差し込める拡張ポイント
- renderer / host ABI / 実行モデルの見直しが必要（単純な SDK API 追加では完結しない）
- **設計作業**:
  - pre-composite / per-layer / post-composite の割り込みポイント定義
  - Wasm effect の安全な呼び出し ABI（timeout / fault isolation / fallback）
  - render pass graph の filter layer 対応と dirty rect キャッシュの effect-aware 再設計
  - filter layer の永続化形式（`app-core` / `storage`）
- **主な変更箇所**: `crates/plugin-host/`, `crates/render/`, `crates/storage/`

### [architecture] 描画レイヤーの物理的分離

- **背景**: UIパネル表示変更のたびにコマ描画が乱れるなど、描画ロジックとUI変更が密結合している
- **方針**: UIレイヤーグループ / 一時描画レイヤー / キャンバスレイヤー / 背景を物理的に分離し、片方の変更が他方へ波及しない構造にする
- **現状**: `render::FramePlan` がすべての平面を一括管理しており、キャンバス・パネル・オーバーレイの再描画トリガーが混在している
- **必要作業**:
  - `render` クレートにレイヤーグループ（UI / temp-overlay / canvas / background）を明確に定義し、合成パスを分離する
  - 各レイヤーグループの dirty rect を独立して管理する
  - UIパネルの再描画がキャンバスレイヤーの再描画をトリガーしない構造にする
- **主な変更箇所**: `crates/render/`, `apps/desktop/src/app/`

### [improvement] イベント駆動パネル再描画（ポーリング廃止）

- **背景**: 毎フレームパネルツリーをスキャンして変更判定している可能性があり、state変更によってレンダリングが発火するReact的設計に変えるべき
- **注意**: レンダリング発火をdirtyにすることと、dirty rect で描画することは別レイヤーの話
- **方針**: パネルのstate変更が直接 dirty flag を立て、フレームループはdirty flagを確認してのみパネルを再描画する
- **現状**: `panel-runtime` の `host_sync.rs` がスナップショット同期を担うが、フレームごとのスキャンがボトルネックになっている可能性がある
- **必要作業**:
  - `PanelRuntime` にstate変更通知API（`mark_dirty(panel_id)` 相当）を追加する
  - `host_sync.rs` でスナップショット差分をstate変更イベントに変換する
  - `render` がdirtyパネルのみ再描画するよう変更する
- **主な変更箇所**: `crates/panel-runtime/`, `crates/render/`, `apps/desktop/src/app/`

### [feature] プラグインホットリロード（開発環境）

- **目的**: 開発中にプラグインの変更を即座に反映できるようにする
- **方針**: 開発環境ではファイル変更を監視し、`.altp-panel` の再パースとWasm再コンパイル → 再ロードを自動実行する（開発環境ではwasm再コンパイルを走らせる前提）
- **現状**: 再読み込みはアプリ再起動が必要
- **必要作業**:
  - `notify` クレートでプラグインディレクトリを監視する（`desktop-support` または `panel-runtime`）
  - ファイル変更イベントで `build-ui-wasm.sh` をバックグラウンド実行する
  - `PanelRuntime` に単一プラグインの再ロード API を追加する（`reload_plugin(name)`）
  - `panel-list` プラグインに手動リロードボタンを追加する（自動 + 手動の両対応）
- **主な変更箇所**: `crates/panel-runtime/`, `crates/desktop-support/`, `plugins/panel-list/`

### [improvement] プラグインUI改善（各パネル）

#### 全体

- 各ボタンを右クリックでコンテキストダイアログを開ける機能
- `<section>` タグを全体的に削除する（DSL的に不要）
- flaticon.com の SVG でアイコンを差し替える
- テキストラベルをなるべく減らす
- ショートカット設定ボタンをウィンドウヘッダー相当エリアに移して小さくする
- 選択中パネルをUIレイヤー内で最前面に描画する

#### カラーホイール（`plugins/color-palette`）

- 「カラー」見出しを削除
- プレビューの代わりに一時カラーパレット（10色）をクリック / ショートカットで切り替え
- カラーホイール自体を大きくする
- HSVとカラーコードを並列表示し、クリックでコピーできるようにする

#### 筆設定（`plugins/pen-settings`）

- 「現在のツール」項目にはペン名とストロークプレビューのみ表示し、それ以外は削除
- スライダーと数値入力欄を横並びにする
- 「サイズ」と「pen width」が別の値を表示している問題を修正する（バグ）

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
