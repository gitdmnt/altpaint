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

2026-03-09 時点で、`altpaint` は以下まで到達している。

- Cargo workspace 構成がある
- 最小クレートとして `app-core`、`render`、`ui-shell`、`plugin-api`、`storage`、`builtin-plugins`、`apps/desktop` がある
- 単一ウィンドウのデスクトップアプリが起動する
- 単一ページ、単一コマ、単一ラスタレイヤーの最小 `Document` がある
- 白いキャンバスを灰色背景の上に表示できる
- マウス入力で黒い点・線ストロークを描ける
- キャンバス表示位置と入力座標変換が一致している
- 実行時プロファイラで描画コストを区間別に計測できる
- ウィンドウタイトルに FPS と主要フレーム時間を常時表示できる
- dirty rect による差分転送で、描画コストを大幅に削減済みである
- JSONベースの最小保存形式で `Document` を保存/読込できる
- フォーマットバージョン付きで保存し、未知バージョンを拒否できる
- `desktop` から新規作成・保存・読込・起動時自動読込ができる
- `ui-shell` が内部標準プラグインを自動登録できる
- 読み取り専用の最小 `layers-panel` 内部プラグインがある
- `tool-palette` と `layers-panel` の最小可視UIが `desktop` にある
- `plugin-api` に `PanelUi` / `PanelUiNode` を追加し、パネルUI記述を中間表現として扱える
- `plugin-api` に `PanelUi` / `PanelUiNode` を追加し、パネルUI記述をホスト非依存の中間表現として扱える
- `desktop` は `winit` + `wgpu` の単一ウィンドウホストとして起動する
- `desktop` はホスト描画したパネル面とキャンバス面を `wgpu` で合成提示できる
- ブラシと消しゴムの最小ツール切替がある
- `builtin.app-actions` パネルを追加し、`new` / `save` / `load` をパネル側から `Command` として発行できる
- `save` / `load` / `new` / `tool switch` が `DesktopApp::execute_command(...)` 経由に統一された
- `desktop` のキャンバス操作面から pointer 操作を `Command` へ変換し、`Document` 更新と再描画へ接続できる
- `desktop` はホスト側レイアウト情報を使ってサイドパネルとキャンバスへの入力ルーティングを行える
- `desktop` は最終合成済みフレームを WGPU texture へアップロードして提示できる
- `plugin-api` に `PanelTree` / `PanelNode` / `PanelEvent` / `HostAction` を追加し、`PanelUi` ベースの暫定表現からフェーズ4契約へ進めた
- `ui-shell` が最小レイアウト、ヒットテスト、ソフトウェア描画を持つホスト側パネルランタイムを持ち、組み込みパネルをホスト描画できる
- `desktop` のサイドバーはホスト描画されたパネルサーフェスを表示し、ボタン押下から `Command` を発行できる
- `ui-shell` にパネルフォーカス移動と縦スクロールの最小制御を追加した
- `desktop` は `Tab` / `Shift+Tab` / `Enter` / `Space` とマウスホイールを使ってパネルUIを操作できる
- `builtin.job-progress` と `builtin.snapshot-panel` を追加し、標準パネル6種をホスト自前描画へ揃えた

現時点では、「フェーズ3: 保存と再読込」は最小形で実装済みであり、
「フェーズ4: パネル中間表現の確立」は完了、
「フェーズ5: 標準パネルの移植」は最小フォーカス/クリック/スクロールを含む形で到達したと見てよい。

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

- `winit` ベースの単一ウィンドウ起動
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

実装済み。

### 実装内容

- `crates/builtin-plugins` を追加
- `LayersPanelPlugin` を内部標準プラグイン1号として実装
- `ToolPalettePlugin` を追加
- `AppActionsPlugin` を追加
- `ui-shell` がデフォルト内部プラグインを自動登録するよう変更
- `PanelPlugin` trait に最小観測用の `debug_summary()` を追加
- `PanelPlugin` trait に `PanelView` / `view()` を追加
- `PanelPlugin` trait に `PanelTree` / `handle_event()` を追加
- `desktop` にホスト描画ベースの左サイドパネルUIを追加
- `ui-shell` に最小レイアウト、ヒットテスト、パネル描画を追加
- `desktop` は `PanelTree` を直接ホスト描画して入力配送するよう変更
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

- `desktop` 既定バイナリは `winit` + `wgpu` ベースの単一 window 構成である
- キャンバス描画は最終合成済みフレームを WGPU texture として提示する経路である
- パネルの追加/削除/配置変更をユーザー操作で行うUIはまだない
- `plugin-host` クレートは未実装であり、現状は `ui-shell` から直接内部プラグインを登録している
- 起動確認では `desktop` バイナリのビルド、単一 window 起動、既定プロジェクト読込、組み込みパネル初期化、キャンバス画像表示を確認対象とする

## フェーズ5: 標準パネルの移植

最小形を実装済み。

### 実装内容

- `tool-palette` / `layers-panel` / `app-actions` をホスト自前描画の `PanelTree` 基盤へ揃えた
- `color-palette` を追加し、ライブプレビュー付き RGB スライダーからブラシ色を調整できるようにした
- `job-progress` を読み取り専用の標準パネルとして追加した
- `snapshot-panel` を読み取り専用の標準パネルとして追加した
- `ui-shell` にフォーカス対象追跡、フォーカス順移動、フォーカス中ボタンの可視強調を追加した
- `ui-shell` にサイドバー縦スクロール状態を追加した
- `desktop` に `Tab` / `Shift+Tab` によるフォーカス移動と `Enter` / `Space` によるアクティブ化を追加した
- `desktop` にパネルサーフェス上のマウスホイールスクロールを追加した

### 到達点

- 標準パネル6種がホスト自前描画で表示される
- 少なくとも `app-actions` / `tool-palette` / `layers-panel` の3種類は `Command` 発行または状態同期まで確認できる
- `color-palette` はライブプレビューと RGB スライダーから `Command::SetActiveColor` を発行し、選択色付きの描画へ接続される
- フォーカス、クリック、スクロールの基本導線が `desktop` ホストと `ui-shell` の間で成立した

### 現時点の制約

- フォーカスはボタン系ノードのみを対象にした最小実装である
- スクロールはサイドバー全体に対する単純な縦スクロールのみで、慣性や個別パネル内スクロールはまだない
- `job-progress` と `snapshot-panel` は将来の `jobs` クレートやスナップショット永続化に先立つ読み取り専用プレースホルダである

## Slint 撤廃後に残る作業

`desktop` の既定バイナリは、`winit` + `wgpu` の単一 window による最小 UI 構成へ移行した。

一方で、MVPとして今後さらに必要な作業は残っている。

- `CanvasBridge` と `WorkspaceShell` の責務分離をより明確にする
- WGPU 側の render target / viewport 制御を拡張し、実キャンバス領域への描画制御を洗練する
- 動的なパネル数やパネル配置変更をホストレイアウト側で扱えるよう拡張する
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
- パネルサーフェスのレイアウト、ヒットテスト、ソフトウェア描画

主要な責務:

- `update(document)`
- `render_frame(document)`
- `render_panel_surface(width, height)`
- `register_panel(...)`
- `handle_panel_event(...)`
- `panel_debug_summaries()`
- `panel_views()`
- `panel_trees()`

## `crates/plugin-api`

現在の役割:

- `PanelPlugin` trait の定義

現状:

- 最小のパネル境界として実用に入り始めた段階
- `debug_summary()` により、最小UIやデバッグ用観測を支えられる
- `PanelView` / `view()` により、簡易可視UIやデバッグ表示データ源として利用できる
- `PanelTree` / `PanelNode` / `PanelEvent` / `HostAction` により、ホストが直接描画と入力配送を担える

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
- `winit` ウィンドウの起動
- パネル面とキャンバス面のホスト合成
- pointer 入力から `Command` への変換
- キャンバスフレームの表示
- 保存/読込/新規作成の実行
- 起動時自動読込
- 内部プラグイン状態の最小ログ出力
- 左右サイドパネルの可視UI
- `fontdb` + `ab_glyph` によるシステムフォント描画
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
- `ui-shell::text::draw_text_rgba(...)`
- `ui-shell::text::wrap_text_lines(...)`
- `ui-shell::text::measure_text_width(...)`

### ウィンドウ文字描画の現状

現行のホストUI文字描画は、固定 8x8 ビットマップではなく、共有テキストレンダラでシステムフォントを解決して CPU ラスタライズする構成へ移行した。

- フォント解決: `fontdb::Database::load_system_fonts()`
- グリフラスタライズ: `ab_glyph`
- 適用対象: `ui-shell` のパネル文字列と `apps/desktop` のヘッダ/フッタ文字列
- フォールバック: システムフォントが取得できない環境では従来の `font8x8`

判断として、**現行アーキテクチャでもシステムフォントによる文字列描画は可能** である。理由は、パネル面とホスト面がどちらも最終的に RGBA バッファへ描画され、その後 `wgpu` へアップロードされるためである。GPU 側の提示経路を変えずに、CPU 側の文字ラスタライズだけ差し替えられる。

制約として、現時点の実装は複雑な文字 shaping までは行っていない。現在のホストUI文字列には十分だが、多言語組版を本格化する場合は `cosmic-text` などの shaping 対応エンジンを別途検討する余地がある。

## パフォーマンス改善の経緯

## 問題の発端

当初は、速く曲線を引こうとするとフレーム間隔が長く、見た目が折れ線になっていた。

初期の `winit` + `wgpu` ホストでは、キャンバス更新のたびに最終提示用フレーム全体を CPU 側で再合成し、その全体を GPU texture へ毎回アップロードしていた。

この時点で疑われた候補は大きく4つあった。

- `render_frame()` 内の `pixels.clone()`
- ホスト側の全面フレーム再合成
- `queue.write_texture(...)` による全面 GPU アップロード
- pointer 移動のたびに再描画要求を出していること

## まず行った改善

最初に、入力が変化していないときの無駄な再描画要求を止めた。

### 効果

- 描画していない単なるカーソル移動で `request_redraw()` し続けない
- 入力イベント起因の無駄なフレーム生成を減らせる

ただし、これだけではまだストローク中の重さは解消しなかった。

## 次に行ったこと: 実行時プロファイラの導入

どこが本当に重いかを断定するため、`apps/desktop/src/main.rs` と `apps/desktop/src/wgpu_canvas.rs` に簡易プロファイラを追加した。

### 有効化方法

- 常時: ウィンドウタイトルに `fps` / `frame` / `prep` / `ui` / `panel` / `present` を表示する
- 詳細ログ: `ALTPAINT_PROFILE=1` を付けて起動すると、2秒ごとの集計を標準エラーへ出す

### 測定区間

- `prepare_frame`
- `layout`
- `ui_update`
- `panel_surface`
- `compose_full_frame`
- `compose_dirty_canvas`
- `present_total`
- `present_upload`
- `present_encode`
- `present_swap`

### 現行プロファイラで分かったこと

現行実装では、**通常のストローク更新は十分軽い** 一方で、**初回または UI 再構成時の全面フレーム再合成とパネル面生成が高コスト** であることが分かった。

実測では、おおむね以下の傾向だった。

- `compose_full_frame`: 約 146〜150 ms
- `panel_surface`: 初回約 492 ms、再生成時でも十数 ms 級になることがある
- `compose_dirty_canvas`: 約 0.03 ms 前後
- `present_upload`: 約 0.10 ms 前後
- `present_encode`: 約 0.45〜0.55 ms
- `present_swap`: 約 0.06〜0.07 ms

この結果から、現時点の主因は steady state のキャンバス更新ではなく、**全面再合成時の CPU 側フレーム構築とパネルサーフェス生成** であると判断できる。

## UI 更新経路の見直し

UI 操作時の低下を切り分けやすくするため、`desktop` 側で次の2種類を分離した。

- `needs_ui_sync`
  - `Document` 変更を各パネルへ再配送する必要がある状態
- `needs_panel_surface_refresh`
  - フォーカス移動、スクロール、サイズ変更などでパネル面の再描画だけが必要な状態

これにより、少なくとも次のケースでは不要な `ui_update` を避けられるようになった。

- パネルフォーカス移動
- パネルスクロール
- レイアウト変更に伴う単純な再描画

加えて `ui-shell` 側では、パネル内容のオフスクリーン結果をキャッシュし、スクロール時は内容再構築ではなくビューポート切り出しを優先する形へ寄せた。

さらに `desktop` 側では、パネル更新時に毎回 `compose_full_frame` へ戻らず、次の差分再合成を使うようにした。

- `compose_dirty_panel`
  - パネルホスト矩形だけを再描画する
- `compose_dirty_status`
  - ツール名や色表示が変わるステータス領域だけを再描画する

これにより、少なくとも次のケースではフレーム全体の CPU 再合成を避けられる。

- パネルスクロール
- パネルフォーカス移動
- ツール切替
- 色変更

## dirty rect ベースの差分更新

その後、dirty rect を `app-core` から `desktop` まで通し、変更があったキャンバス領域だけをホスト合成済みフレームへ反映し、その領域だけ GPU texture へアップロードする形に変更した。

### 導入したもの

- `DirtyRect`
- `pending_canvas_dirty_rect`
- `present_frame` キャッシュ
- `compose_dirty_canvas`
- `UploadRegion`
- `upload_frame_region(...)`

### 再描画方針

- 初回表示、リサイズ、パネル更新時は全面再合成
- 通常の描画中は dirty 領域のみを既存 `present_frame` へ差分反映
- GPU 側も dirty 領域のみを `write_texture(...)` で更新

### 実測結果

描画中の steady state では、以下のような結果が得られた。

```text
[profile] ---- last 2s ----
[profile] compose_dirty_canvas calls=  124 avg=   0.026ms max=   0.094ms total=   3.185ms
[profile]             layout calls=  123 avg=   0.001ms max=   0.001ms total=   0.090ms
[profile]     present_encode calls=  123 avg=   0.492ms max=   1.114ms total=  60.477ms
[profile]       present_swap calls=  123 avg=   0.062ms max=   0.174ms total=   7.595ms
[profile]     present_upload calls=  124 avg=   0.096ms max=   0.238ms total=  11.877ms
```

### 解釈

- 通常ストローク中の dirty キャンバス反映はほぼ無視できる
- GPU への差分アップロードも 0.1 ms 前後で収まっている
- steady state のフレーム時間は 60fps 予算 16.67 ms に十分収まる
- 現在の改善対象は、主に初回全面合成とパネル面再生成である

したがって、dirty rect ベースの差分更新は有効であり、現時点では通常のキャンバス更新経路は大きな問題ではなくなった。

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
- `JobProgressPanelPlugin` が最小アイドル状態を追従することの確認
- `SnapshotPanelPlugin` がドキュメント概要を追従することの確認

### `ui-shell`

- 組み込み `layers-panel` がデフォルト登録されることの確認
- 組み込み `tool-palette` がデフォルト登録されることの確認
- フォーカス移動からボタン活性化まで通ることの確認
- 縦スクロール状態が更新されることの確認

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
- キーボードによるパネルフォーカス移動とアクティブ化の確認
- マウスホイールによるパネルスクロールの確認
- 共有テキストレンダラが可視ピクセルを出力する確認
- 長い単語の折り返しで文字欠落しない確認

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
- 保存形式の部分ロード/差分保存対応

## 次の有力候補

優先度順に考えると、次の候補は以下。

1. フェーズ6の DSL パネルローダ導入
2. `plugin-host` クレートの導入準備
3. `jobs` クレートと `job-progress` の実データ接続
4. `render_frame()` の clone 削減とパネルサーフェス再生成コストの削減

現状の性能が 60fps 予算に入っているため、直近では「性能で詰まって先に進めない」状態ではなくなっている。したがって、次はロードマップに戻ってフェーズ6以降へ進むのが自然である。

## 参照のしかた

素早く状況を把握したいときは、以下の順で読むとよい。

1. `docs/IMPLEMENTATION_STATUS.md`
2. `docs/ROADMAP.md`
3. `docs/ARCHITECTURE.md`
4. `docs/SKETCH.md`

