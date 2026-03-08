# altpaint 実装状況メモ

## この文書の目的

この文書は、`docs/SKETCH.md`、`docs/ARCHITECTURE.md`、`docs/ROADMAP.md` に基づいて進めた実装作業の現状を、後から参照しやすい形でまとめるための実装ログ兼コンテキスト文書である。

役割は以下。

- ここまでに何を決め、何を実装したかを手早く把握する
- 現在のコードベースがどのフェーズまで進んでいるかを確認する
- 直近の性能改善で何が効いたのかを把握する
- 次に着手する候補を整理する

設計原則や責務分割そのものは `docs/ARCHITECTURE.md`、実装順序は `docs/ROADMAP.md` を正として、この文書では「実際にどう進んだか」と「今どこにいるか」に集中する。

## 現在の要約

2026-03-08 時点で、`altpaint` は以下まで到達している。

- Cargo workspace 構成がある
- 最小クレートとして `app-core`、`render`、`ui-shell`、`plugin-api`、`storage`、`builtin-plugins`、`apps/desktop` がある
- 単一ウィンドウのデスクトップアプリが起動する
- 単一ページ、単一コマ、単一ラスタレイヤーの最小 `Document` がある
- 白いキャンバスを灰色背景の上に表示できる
- マウス入力で黒い点・線ストロークを描ける
- キャンバス表示位置と入力座標変換が一致している
- 実行時プロファイラで描画コストを区間別に計測できる
- dirty rect による差分転送で、描画コストを大幅に削減済みである
- JSONベースの最小保存形式で `Document` を保存/読込できる
- フォーマットバージョン付きで保存し、未知バージョンを拒否できる
- `desktop` から新規作成・保存・読込・起動時自動読込ができる
- `ui-shell` が内部標準プラグインを自動登録できる
- 読み取り専用の最小 `layers-panel` 内部プラグインがある
- `tool-palette` と `layers-panel` の最小可視UIが `desktop` にある
- `plugin-api` に `PanelUi` / `PanelUiNode` を追加し、パネルUI記述を中間表現として扱える
- `plugin-api` に `PanelUi` / `PanelUiNode` を追加し、パネルUI記述をホスト非依存の中間表現として扱える
- `ui-shell` が `PanelUi` を `SlintPanelModel` へ変換し、`desktop` がそれを Slint UI にバインドできる
- `desktop` でツールパレット行のクリックから `Command` を実行し、アクティブツール切替へ反映できる
- ブラシと消しゴムの最小ツール切替がある
- `builtin.app-actions` パネルを追加し、`new` / `save` / `load` をパネル側から `Command` として発行できる
- `save` / `load` / `new` / `tool switch` が `DesktopApp::execute_command(...)` 経由に統一された
- `desktop` は Slint ベースの単一ウィンドウ構成へ移行し、サイドパネルとキャンバス操作面を同居させた
- `desktop` のキャンバス操作面から pointer 操作を `Command` へ変換し、`Document` 更新と再描画へ接続できる
- `desktop` は `Slint::Window::set_rendering_notifier(...)` を使って WGPU 28 の direct rendering を差し込み、`RenderFrame` を GPU texture へアップロードできる

現時点では、「フェーズ3: 保存と再読込」は最小形で実装済みであり、
「フェーズ4: パネルホストと内部プラグイン1号」および「フェーズ5: 変更経路の統一」の最小縦切りが Slint ベースで進行中であると見てよい。

## 実装済みの主要文書

- `docs/SKETCH.md`
  - 要件、MVP、非目標、技術選定の基本方針
- `docs/ARCHITECTURE.md`
  - クレート分割、責務境界、内部プラグインの考え方
- `docs/ROADMAP.md`
  - フェーズ別の実装順序
- `docs/IMPLEMENTATION_STATUS.md`
  - この文書。実装進捗と性能改善の現状メモ

## 実装済みフェーズ

## フェーズ0: 最小契約

実装済み。

### 主な成果物

- ルート `Cargo.toml` による workspace 構成
- `crates/app-core`
- `crates/render`
- `crates/ui-shell`
- `crates/plugin-api`
- `apps/desktop`

### 導入した最小概念

- `Document`
- `Work`
- `Page`
- `Panel`
- `LayerNode`
- `Command`
- `PanelPlugin`
- `RenderContext`
- `UiShell`

### 意図

MVP 時点で必要な「コア」「描画」「UIホスト」「プラグイン境界」を、過剰に作り込みすぎない最小形で固定した。

## フェーズ1: 最小起動ループ

実装済み。

### 実装内容

- Slint ベースの単一ウィンドウ起動
- キャンバス表示面を含む最小レイアウト
- `Document::default()` を表示対象にする最小デスクトップアプリ

### 到達点

- アプリが起動する
- キャンバス表示基盤がある
- 将来のパネル追加を妨げない最小構造がある

## フェーズ2: 描画できる最小垂直スライス

実装済み。

### 実装内容

- `CanvasBitmap` による最小ラスタキャンバス
- キャンバス初期色を白で初期化
- キャンバス外背景を灰色で表示
- キャンバス外周枠を描画
- `draw_point()` による点描画
- `draw_line()` による 1px 線分描画
- `Document::draw_stroke()` によるストローク反映
- マウスドラッグ中の連続ストローク描画
- キャンバス中央配置
- ウィンドウ座標からキャンバス座標への変換
- リサイズ追従

### 実装メモ

- 線分描画は Bresenham ベースの最小実装
- まだ筆圧や太さはない
- レイヤーはまだ単一ラスタのみ
- 複数コマ、Undo/Redo、本格的な標準内部パネルUIは未着手

## フェーズ3: 保存と再読込

最小形を実装済み。

### 実装内容

- `crates/storage` を追加
- `AltpaintProjectFile` による最小プロジェクト保存形式を定義
- `format_version` をファイルに保持
- `save_document_to_path(...)` を実装
- `load_document_from_path(...)` を実装
- 未知のフォーマットバージョンを拒否
- `desktop` に新規作成・保存・読込の最小導線を追加
- 起動時に既定ファイルが存在すれば自動読込する処理を追加

### 到達点

- 描画結果を保存できる
- 次回起動または手動読込で状態を復元できる
- 将来のフォーマット更新に備えるための明示的バージョンを持てている

### 現時点の制約

- 保存形式は暫定的に JSON ベースである
- 保存先パスは `altpaint-project.altp.json` の固定値である
- ファイルダイアログや最近使ったファイル一覧はまだない

## フェーズ4: パネルホストと内部プラグイン1号

最小縦切りを実装済み。

### 実装内容

- `crates/builtin-plugins` を追加
- `LayersPanelPlugin` を内部標準プラグイン1号として実装
- `ToolPalettePlugin` を追加
- `AppActionsPlugin` を追加
- `ui-shell` がデフォルト内部プラグインを自動登録するよう変更
- `PanelPlugin` trait に最小観測用の `debug_summary()` を追加
- `PanelPlugin` trait に `PanelView` / `view()` を追加
- `PanelPlugin` trait に `PanelUi` / `ui()` を追加
- `desktop` に Slint ベースの左サイドパネルUIを追加
- `ui-shell` に `SlintPanelModel` 正規化を追加
- `desktop` は `PanelUi` 中間表現を Slint モデルへ変換して描画するよう変更
- キーボードショートカットとパネルクリックの両方が `Document::apply_command(...)` 経由で状態更新するよう変更
- アプリレベル副作用を持つ `SaveProject` / `LoadProject` / `NewDocument` は `DesktopApp::execute_command(...)` で処理する形に統一した
- キャンバス pointer 操作も `Command` 経由で `Document` を更新するよう変更

### 到達点

- キャンバス以外の標準UI要素を、内部プラグインとしてホストする経路が通った
- 内部プラグインが `Document` を読み取り、自身の状態を持てることを確認できた
- パネルホストと内部プラグイン境界の最初の検証ができた
- `desktop` 上でツールパレット、アプリアクション、レイヤーパネルを視認できる
- パネルUIとキャンバスUIの双方で宣言的 `Command` 発行経路を確認できた

### 現時点の制約

- `desktop` 既定バイナリは Slint ベースの単一 window 構成である
- キャンバス描画は `set_rendering_notifier(...)` を使う WGPU 28 経路へ移行済みである
- パネルの追加/削除/配置変更をユーザー操作で行うUIはまだない
- `plugin-host` クレートは未実装であり、現状は `ui-shell` から直接内部プラグインを登録している
- 起動確認では `desktop` バイナリのビルド、単一 window 起動、既定プロジェクト読込、組み込みパネル初期化、キャンバス画像表示を確認対象とする

## Slint 移行後に残る作業

`desktop` の既定バイナリは、Slint の単一 window による最小 UI 構成へ移行した。

一方で、MVPとして今後さらに必要な作業は残っている。

- `CanvasBridge` と `WorkspaceShell` の責務分離をより明確にする
- WGPU 側の render target / viewport 制御を拡張し、実キャンバス領域への描画制御を洗練する
- 動的なパネル数やパネル配置変更を Slint 側で扱えるよう拡張する
- 将来の外部プラグイン導入を見据え、`plugin-host` を追加する

## 主要クレートの現状

## `crates/app-core`

現在の役割:

- ドメインモデルの保持
- 最小描画対象データの保持
- 最小描画変更 API の提供

主要な型:

- `Document`
- `Work`
- `Page`
- `Panel`
- `LayerNode`
- `CanvasBitmap`
- `DirtyRect`

重要な現状:

- `Document::draw_point()` は dirty rect を返す
- `Document::draw_stroke()` は dirty rect を返す
- dirty rect を使って UI 側で差分更新できる

## `crates/render`

現在の役割:

- 最初のコマのビットマップを `RenderFrame` として取り出す

主要な型:

- `RenderContext`
- `RenderFrame`

現状の注意点:

- `RenderFrame` は `Vec<u8>` を持つ
- `render_frame()` は現状 `panel.bitmap.pixels.clone()` を行う
- ただし現計測では、これは支配的ボトルネックではなくなっている

## `crates/ui-shell`

現在の役割:

- パネルホストの最小境界
- `RenderContext` と `PanelPlugin` 群の束ね
- 組み込み内部プラグインの自動登録
- Slint 向けパネルモデルへの正規化

主要な責務:

- `update(document)`
- `render_frame(document)`
- `register_panel(...)`
- `panel_debug_summaries()`
- `panel_views()`
- `panel_uis()`
- `slint_panels()`

## `crates/plugin-api`

現在の役割:

- `PanelPlugin` trait の定義

現状:

- 最小のパネル境界として実用に入り始めた段階
- `debug_summary()` により、最小UIやデバッグ用観測を支えられる
- `PanelView` / `view()` により、簡易可視UIや Xilem 側の表示データ源として利用できる
- `PanelUi` / `PanelUiNode` により、`xilem` 固有型に閉じないパネル記述子を返せる

## `crates/storage`

現在の役割:

- 最小プロジェクトファイルの保存/読込
- フォーマットバージョンの検証

主要な型と関数:

- `AltpaintProjectFile`
- `CURRENT_FORMAT_VERSION`
- `save_document_to_path(...)`
- `load_document_from_path(...)`

現状の注意点:

- 形式は JSON ベースの暫定実装である
- 将来的な部分ロードや差分保存には未対応

## `crates/builtin-plugins`

現在の役割:

- 内部標準プラグイン群の格納場所
- `layers-panel` と `tool-palette` の実装

主要な型:

- `LayersPanelPlugin`
- `LayersPanelSnapshot`
- `ToolPalettePlugin`
- `ToolPaletteSnapshot`

## `apps/desktop`

現在の役割:

- 実行可能な最小デスクトップアプリ
- Slint ウィンドウの起動
- パネルUIとキャンバスUIのバインディング
- pointer 入力から `Command` への変換
- キャンバスフレームの表示
- 保存/読込/新規作成の実行
- 起動時自動読込
- 内部プラグイン状態の最小ログ出力
- 左右サイドパネルの可視UI
- 読みやすい大型ビットマップフォント描画
- ブラシ/消しゴム切替

重要な実装要素:

- `CanvasLayout`
- `window_to_canvas_position(...)`
- `draw_at_cursor()`
- `redraw()`
- `blit_canvas_to_window(...)`
- `blit_canvas_region_to_window(...)`
- `Profiler`
- `save_project()`
- `load_project()`
- `load_project_if_present()`
- `new_document()`
- `draw_visible_panels(...)`
- `draw_text_line(...)`
- `draw_glyph(...)`
- `glyph_pattern(...)`

## `apps/xilem-spike`

現在の役割:

- Xilem 0.4 を使った最小周辺UIスパイク
- `tool-palette` / `layers-panel` を Xilem で表示できるかの検証

現状:

- `Document` と `UiShell` を流用して左右パネル相当の表示ができる
- キャンバスそのものはまだ Xilem へ統合していない

## パフォーマンス改善の経緯

## 問題の発端

当初は、速く曲線を引こうとするとフレーム間隔が長く、見た目が折れ線になっていた。

この時点で疑われた候補は大きく3つあった。

- `render_frame()` 内の `pixels.clone()`
- キャンバスをウィンドウへ拡大転送する CPU 側の全面 blit
- 毎フレーム再描画要求を出していること

## まず行った改善

最初に、不要な継続再描画を止めるために `needs_redraw` を導入した。

### 追加した状態

- `needs_redraw`

### 効果

- 何も変化していないときに `request_redraw()` し続けない
- 無駄な再描画を止める

ただし、これだけではまだ描画は重かった。

## 次に行ったこと: 区間計測

どこが本当に重いかを断定するため、`apps/desktop/src/main.rs` に実行時プロファイラを追加した。

### 有効化方法

`ALTPAINT_PROFILE=1` を付けて起動する。

### 測定区間

- `draw_input`
- `render_frame`
- `blit_canvas`
- `present`
- `redraw_whole`

### 初回測定で分かったこと

初期実装では、支配的ボトルネックは `blit_canvas` だった。

おおむね以下の傾向だった。

- `render_frame`: ほぼ無視できる
- `draw_input`: ほぼ無視できる
- `present`: 数 ms
- `blit_canvas`: 約 140 ms 前後
- `redraw_whole`: 約 145 ms 前後

この結果から、重さの主因は UI 差分更新ではなく、**キャンバス全面の CPU 転送**であることが分かった。

## dirty rect 差分転送

その後、dirty rect を `app-core` から `desktop` まで通し、変更があった領域だけをウィンドウへ転送する形に変更した。

### 導入したもの

- `DirtyRect`
- `pending_dirty_rect`
- `needs_full_redraw`
- `blit_canvas_region_to_window(...)`

### 再描画方針

- 初回表示とリサイズ時は全面再描画
- 通常の描画中は dirty 領域のみ差分転送

### 実測結果

差分転送導入後、ユーザー測定では以下の結果が得られた。

```text
[profile] ---- last 2s ----
[profile] render_frame calls=  120 avg=   0.001ms max=   0.003ms total=   0.132ms
[profile]  blit_canvas calls=  120 avg=   0.148ms max=   0.647ms total=  17.726ms
[profile]      present calls=  120 avg=   5.707ms max=   6.549ms total= 684.868ms
[profile] redraw_whole calls=  120 avg=   5.890ms max=   6.638ms total= 706.854ms
[profile]   draw_input calls=  120 avg=   0.046ms max=   0.068ms total=   5.518ms
```

### 解釈

- `blit_canvas` は約 140 ms 級から約 0.15 ms 級へ下がった
- 律速は `present` に移った
- `redraw_whole` 平均は約 5.9 ms なので、60fps の予算 16.67 ms に十分収まっている

したがって、dirty rect による差分転送は有効であり、現時点では CPU 側キャンバス転送は大きな問題ではなくなった。

## `xilem` についての現時点の整理

ここまでの調査から、少なくとも今回の重さの主因は `xilem` のような UI 差分フレームワーク不在ではなかった。

判断ポイントは以下。

- 重かった主因は UI ツリー更新ではなくキャンバス全面転送だった
- dirty rect 導入で大きく改善した
- 現在の主要コストは `present` 側にある

このため、`xilem` を導入する価値がゼロではないが、**キャンバス性能対策としての優先順位は高くない**。

`xilem` は今後も、以下の文脈では検討価値がある。

- キャンバス以外の標準 UI を内部プラグイン前提で整理する
- `ui-shell` を宣言的に組み替えやすくする
- サイドバーやパネルの構成変更を扱いやすくする

一方で、キャンバス描画高速化の主打としては考えない。

## テスト状況

現時点で、少なくとも以下の観点のユニットテストがある。

### `app-core`

- 最小ドキュメント構造の確認
- `draw_point()` の反映確認
- `draw_stroke()` の反映確認
- キャンバス初期色の確認
- `DirtyRect::union()` の確認

### `storage`

- 保存→読込の往復確認
- 未知フォーマットバージョンの拒否確認

### `builtin-plugins`

- `LayersPanelPlugin` がドキュメント概要を追従することの確認
- `ToolPalettePlugin` がアクティブツールに追従することの確認

### `ui-shell`

- 組み込み `layers-panel` がデフォルト登録されることの確認
- 組み込み `tool-palette` がデフォルト登録されることの確認

### `desktop`

- デフォルト状態の確認
- 点描画の反映確認
- ストローク接続の確認
- 再描画フラグの確認
- レイアウト中央配置の確認
- ウィンドウ座標→キャンバス座標変換の確認
- 背景とキャンバス転送の確認
- ランタイムウィンドウサイズ時のレイアウト確認
- dirty rect のウィンドウ変換確認
- 差分転送の部分更新確認
- 新規ドキュメント作成時の状態リセット確認
- 消しゴムで白に戻せることの確認
- サイドパネル描画の確認
- サイドパネルを考慮したレイアウト確認
- `glyph_pattern()` の確認
- `draw_glyph()` の描画確認

## 現在の未実装事項

まだ未着手、あるいは本格実装していない主な項目は以下。

- 複数コマ対応
- レイヤー構造編集
- `Command` 経路の本格統一
- Undo/Redo
- `plugin-host` クレート
- `jobs` クレート
- `render_frame()` の clone 削減
- GPU ネイティブなキャンバス提示方式の検討
- 本体 `desktop` への Xilem 統合
- 保存形式の部分ロード/差分保存対応

## 次の有力候補

優先度順に考えると、次の候補は以下。

1. フェーズ5の `Command` ディスパッチ導入
2. パネルクリックやツール切替を `Command` 経路へ寄せる
3. 本体 `desktop` へ Xilem ベースUIをどう統合するか検証する
4. `render_frame()` の clone 削減を検討する

現状の性能が 60fps 予算に入っているため、直近では「性能で詰まって先に進めない」状態ではなくなっている。したがって、次はロードマップに戻ってフェーズ3以降へ進むのが自然である。

## 参照のしかた

素早く状況を把握したいときは、以下の順で読むとよい。

1. `docs/IMPLEMENTATION_STATUS.md`
2. `docs/ROADMAP.md`
3. `docs/ARCHITECTURE.md`
4. `docs/SKETCH.md`

`xilem` の検討状況だけを見たい場合は `docs/XILEM_SPIKE.md` を参照する。

