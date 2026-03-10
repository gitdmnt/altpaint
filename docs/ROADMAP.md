# altpaint ロードマップ

## この文書の役割

この文書は、`docs/SKETCH.md` と `docs/ARCHITECTURE.md` を前提に、内製描画/UIランタイム路線で `altpaint` をどう段階的に組み上げるかを定める。

実装ベースの現在の依存関係と module 構成は、まず [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) を参照すること。

今回のロードマップでは、単に「機能を足す順番」ではなく、次の二本柱を崩さずに進めることを重視する。

- キャンバスを `wgpu` ネイティブで直接描画すること
- Wasm/DSL のランタイムロードでパネルを構成し、そこからホストAPIを叩けるようにすること

## 開発方針の更新

過去の文書では、既存GUIフレームワークをUIホストに据えた上でキャンバスだけを直描画へ寄せる案を強く扱っていた。

しかし現時点では、その方針は最終アーキテクチャではない。

今後のロードマップは、**ホスト主導のデスクトップランタイムを育てる順序**として読む。

特に 2026-03-10 時点では、`render` よりも `apps/desktop` と `ui-shell` に実装責務が厚く乗っている。したがって、この文書は「今そうなっている」説明ではなく、「今後どう整理しながら伸ばすか」の文書として扱う。

## まず固定すること

最初に固定するべきものは多くないが、以下はもうぶらさない。

### 固定する

- ウィンドウとイベントループはホストが直接持つ
- キャンバス描画は `wgpu` ネイティブで行う
- パネルは Wasm/DSL をランタイムロードできる構造にする
- パネルはホスト定義の中間表現を返し、ホストが描画する
- アプリ状態変更はホストAPIまたは `Command` を経由する
- 標準パネルも将来的には同じモデルへ寄せる
- ドメインモデル骨格は `Work -> Page -> Panel -> LayerNode`

### まだ固定しすぎない

- DSL の最終文法
- Wasm ABI の最終形
- 保存形式の最終最適化
- テキスト描画基盤の最終選定
- 高度なUIウィジェット群

## 全体の進め方

実装の順番は、次の三つの縦切りを意識して積む。

1. **ホスト基盤を握る**
   - `winit` + `wgpu` のフレームループを自前で成立させる
2. **描画エンジンを育てる**
   - キャンバス描画、合成、ズーム、パン、オーバーレイの品質を上げる
3. **パネルランタイムを開く**
   - 組み込み→DSL→Wasm の順に拡張を開く

## フェーズ0: 最小契約の再定義

### 目的

方針転換後の最小境界を決め直す。

### 成果物

- `app-core` と `render` の責務再確認
- パネル中間表現の最小定義
- `HostAction` / `PanelEvent` / `PanelStateSnapshot` の最小案
- `apps/desktop` が持つべき責務一覧
- レンダリングエンジン文書の初版

### 完了条件

- GUIフレームワーク抜きでも、何を作れば一周するか説明できる
- パネルとキャンバスの責務境界が文書上で明確である

## フェーズ1: ホストアプリの自立

### 目的

`winit` + `wgpu` を直接使う空のデスクトップランタイムを成立させる。

### 実装するもの

- ネイティブウィンドウ起動
- `wgpu` デバイス/サーフェス初期化
- フレームループ
- clear だけ行う最小 render pass
- 基本入力受信

### 完了条件

- 外部GUIフレームワークなしでウィンドウが安定表示される
- リサイズ、再描画、終了処理が破綻しない

## フェーズ2: 最小キャンバス提示

### 目的

ホスト主導の `wgpu` 描画で「キャンバスが見える」を先に成立させる。

### 実装するもの

- 固定画像または最小 `RenderFrame` の表示
- ビューポート制御
- キャンバスと背景の分離表示
- CPU生成画像の GPU アップロード

### 完了条件

- キャンバス領域が正しく表示される
- 表示サイズ変更に追従する
- 今後のパン/ズームを入れられる土台になる

## フェーズ3: 最小描画ループ

### 目的

「描ける」を、完全にホスト主導の描画経路で成立させる。

### 実装するもの

- 単一ラスタレイヤー
- 基本ブラシ
- pointer 入力からのストローク生成
- dirty 範囲更新
- 再描画スケジューリング

### 完了条件

- 低遅延でストロークを描ける
- CPU側の更新と GPU 側の提示が破綻しない

### 注記

既存実装にはこの段階に相当する着手があるが、今後は GUI フレームワーク依存を外した経路で成立させ直す。

## フェーズ4: パネル中間表現の確立

### 目的

パネルを「描画済みUI」ではなく「ホストが描ける構造化データ」として扱う基盤を作る。

### 実装するもの

- `PanelTree`
- `PanelNode`
- `PanelEvent`
- `HostAction`
- 最小レイアウト
- 最小ヒットテスト
- ボタン、テキスト、セクション表示

### 完了条件

- 組み込みパネルをホストの自前描画で表示できる
- ボタン押下から `Command` を発行できる

## フェーズ5: 標準パネルの移植

### 目的

既存の標準UIを、新しいパネルランタイムへ移す。

### 優先順

1. `tool-palette`
2. `layers-panel`
3. `app-actions`
4. `job-progress`
5. `snapshot-panel`

### 完了条件

- 少なくとも 3 種類の標準パネルがホスト自前描画で動く
- フォーカス、クリック、スクロールの基本が揃う

## フェーズ6: パネル基盤 crate と UI DSL parser

### 目的

UI DSL + Wasm パネル基盤の最小土台を成立させる。

### 実装するもの

- `crates/panel-dsl`
   - lexer
   - parser
   - AST
   - validator
   - normalized IR
- `crates/panel-schema`
   - host / Wasm 間の共有 DTO
- `crates/panel-sdk`
   - Rust から Wasm handler を書くための最小 SDK
- `.altp-panel` ローダ
- validation
- ロード/再ロード
- パネル manifest
- static panel 描画への接続
- handler binding の解決

### MVPで許可する内容

- レイアウト構造
- テキスト
- ボタン
- トグル
- 最小 state schema
- handler 名バインド

### 完了条件

- `*.altp-panel` などのファイルをロードしてパネル表示できる
- 再読み込みでUI変更が反映される
- parser / validator / normalized IR の流れが成立している
- 次段階で既存ビルトイン panel を載せ替えられる土台がある

## フェーズ7: 既存ビルトイン panel の UI DSL + Wasm 移植

### 目的

既存のビルトイン panel を、新しい UI DSL + Wasm 基盤へ再構成する。

### 実装するもの

- `app-actions` の移植
- `tool-palette` の移植
- `color-palette` の移植
- `layers-panel` の移植
- `job-progress` の移植
- `snapshot-panel` の移植
- host snapshot 不足分の補完
- command descriptor から `Command` への変換検証
- built-in panel を同一 ABI / SDK へ寄せる整理

### 完了条件

- 少なくとも 3 種類の既存ビルトイン panel が UI DSL + Wasm で動く
- UI 表示と `Command` 発行結果が従来実装と一致する
- built-in 専用の別 ABI を増やさず移植できる

## フェーズ8: 外部 Wasm パネルランタイム

### 目的

ビルトイン移植で固めた基盤を、外部ロード可能な Wasm panel runtime へ一般化する。

### 実装するもの

- `crates/plugin-host`
   - Wasm モジュールロード
   - `wasmtime` 実行
- 権限 manifest
- `panel_init` / `panel_handle_event` / `panel_dispose` 相当
- ホスト関数の最小セット
- エラー隔離

### 完了条件

- 外部 Wasm panel を1つロードして表示できる
- そこからホストAPIまたは `Command` 経路を叩ける
- クラッシュや権限不足をホスト側で制御できる

## フェーズ8.5: `ui-shell` / ワークスペース抽象の再整理

### 目的

パネルランタイムとパネル表示系の責務をさらに切り分け、
workspace 配置・表示状態・差分更新・パネル性能改善を同じ抽象で扱えるようにする。

このフェーズは、次の tmp 文書にある未解決事項を回収するための差し込みフェーズでもある。

- [docs/tmp/architecture-gap-2026-03-10.md](docs/tmp/architecture-gap-2026-03-10.md)
- [docs/tmp/ui-shell-runtime-presentation-split-2026-03-10.md](docs/tmp/ui-shell-runtime-presentation-split-2026-03-10.md)
- [docs/tmp/panel-performance.md](docs/tmp/panel-performance.md)
- [docs/tmp/refactor_context.md](docs/tmp/refactor_context.md)

### 実装するもの

- `ui-shell` の runtime / presentation 分離の継続
- panel layout / hit-test / focus / software rendering の内部境界整理
- workspace panel の reorder / visibility / dirty rect を一貫した抽象へ寄せる
- パネル dirty rect と panel bitmap cache の再整理
- テキスト計測キャッシュ、ノードレイアウトキャッシュ、差分 blit の導入
- `apps/desktop/src/frame.rs` / `runtime.rs` の継続分割
- 低カバレッジ領域 (`wgpu_canvas`, `workspace`, 各 built-in plugin crate) の回帰テスト補強

### 完了条件

- workspace の並び替え・表示/非表示・差分更新が同じ責務境界で説明できる
- panel performance 改善を runtime 改修から独立して進められる
- `ui-shell` の presentation 変更が Wasm runtime 側へ不要に波及しない

## フェーズ8.6: ワークスペース配布とレスポンシブ配置

### 目的

ワークスペース UI を「ローカル状態」から「配布できるレイアウト資産」へ拡張し、
解像度やウィンドウサイズが変わっても破綻しにくい配置モデルへ移行する。

### 実装するもの

- パネル配置および ON/OFF 状態の配布形式
- 既定ワークスペース preset の読込/保存/適用
- パネル座標を 4 隅アンカー基準で保持する配置モデル
- 画面拡縮・解像度変更時の再配置ルール
- project / session / 配布 preset の ownership 整理

### 完了条件

- 他環境へ持ち運べる workspace preset を保存・配布できる
- ウィンドウサイズが変わってもパネル配置が極端に崩れない
- ローカル session 復元と配布 preset の責務が衝突しない

## フェーズ9: キャンバス機能の実用化

### 目的

描画エンジンを「触れる試作」から「作業に耐えるMVP」へ押し上げる。

### 実装するもの

- ズーム/パン
- ブラシプレビュー
- 外部ペンプリセット読込の最小導線
- 可変幅ペン
- ペン幅調整パネル
- 複数レイヤー
- 合成モード最小対応
- マスク最小対応
- オーバーレイ描画

### このフェーズで追加する差し込み項目

- ペン機能の強化と `altpaint` 標準ペンフォーマットの策定
- ペンパラメータの schema / versioning / import/export 仕様
- 大きなキャンバスで高速描画したときの線切れ対策

### 完了条件

- 実作業で破綻しにくい描画体験がある
- オーバーレイとキャンバスの責務が分離されている
- 最低限のペン追加・再読込・幅変更がホスト主導で行える

## フェーズ9.5: ペン互換インポート

### 目的

外部ツールのペン資産を `altpaint` へ取り込み、標準ペンフォーマットへ正規化できるようにする。

### 実装するもの

- CSP ペンデータの parser
- Photoshop ペンデータの parser
- 外部形式 → `altpaint` ペン形式への正規化
- 互換不能項目の degrade policy
- 変換結果の preview / import report

### 完了条件

- CSP / Photoshop 由来の代表的なペン設定を `altpaint` で再利用できる
- 互換差分がユーザーへ説明可能である
- 標準ペンフォーマットが import 拡張の受け皿として機能する

## フェーズ9.6: 無段階回転 renderer 完了

### 目的

plugin / command / host snapshot 側で先行した回転角表現を、renderer 側でも真に任意角対応へ揃える。

このフェーズは主に [docs/tmp/rotation-renderer-followup-2026-03-10.md](docs/tmp/rotation-renderer-followup-2026-03-10.md) の未解決事項を回収する。

### 実装するもの

- キャンバス無段階回転
- `render::CanvasScene` の quarter turn 依存除去
- dirty rect / UV / hit test / brush preview / lasso overlay の任意角対応
- WGPU 経路と software 合成経路の回転モデル統一

### 完了条件

- 非 90 度系回転でも画像が歪まない
- 入力・表示・dirty rect が同じ回転モデルで整合する
- view plugin からの回転操作が renderer まで破綻なく伝播する

## フェーズ10: 複数コマとコマ中心UI

### 目的

`altpaint` 独自のコマ中心ワークフローへ踏み込む。

### 実装するもの

- 複数コマ保持
- コマ選択・切替
- コマ境界表示
- コマ一覧パネル
- コマ中心ビュー導線

### 完了条件

- コマ単位で自然に編集対象を切り替えられる
- ページとコマの両方を見失わない

## フェーズ11: 保存形式の本格化

### 目的

内製描画エンジンの現実的なデータ管理基盤を固める。

### 実装するもの

- プロジェクトフォーマット再設計
- 部分ロード
- タイル/チャンク保存
- スナップショット永続化
- 差分保存とフル保存の切替余地

### 完了条件

- コマ/ページ単位のロード戦略が成立する
- 大きな作品でメモリ全展開を避けられる

## フェーズ12: テキスト流し込み最小版

### 目的

差別化要素である「テキストから絵作りへ」を実用最小で通す。

### 実装するもの

- Markdown ベース脚本入力
- コマ割り当て
- 吹き出し内流し込み
- 縦書き/横書き切替

### 完了条件

- ネーム用途で試せる導線がある

## フェーズ13: 非同期ジョブと書き出し

### 目的

重い処理でUIを止めない設計を実運用で確認する。

### 実装するもの

- ジョブキュー
- 進捗通知
- PNG/PDF 書き出し
- サムネイル生成
- ジョブパネル

### 完了条件

- 書き出し中でもキャンバス操作やパネル操作が継続できる

## フェーズ13.5: Mod API / filter layer 拡張

### 目的

SDK が UI/command の範囲を超えて、renderer の本質的な処理系へ安全に介入できる拡張点を設ける。

このフェーズは [docs/tmp/rotation-renderer-followup-2026-03-10.md](docs/tmp/rotation-renderer-followup-2026-03-10.md) に追記した render pass 割り込み / filter layer 課題の回収を含む。

### 実装するもの

- render pass への割り込みポイント設計
- filter layer / post effect の document model
- `plugin-host` / SDK / ABI の render hook 拡張
- timeout / fault isolation / fallback policy
- effect aware な dirty rect / cache / pass graph 再設計

### 完了条件

- SDK から filter layer や post effect を安全に追加できる
- 拡張が renderer 全体の安定性を壊さない
- 永続化と再現性を含めて effect chain を扱える

## フェーズ14: スナップショットと分岐

### 目的

コマ単位スナップショットという独自性を実用化する。

### 実装するもの

- コマ単位スナップショット
- 履歴ブラウズ
- 過去時点からの枝分かれ
- コマ単位復元

### 完了条件

- 作品全体を壊さず、特定コマだけ戻せる
- 「作業分岐」として使える

## 現在の実装との接続メモ

2026-03-08 時点のコードベースには、次の資産がある。

- `app-core` の最小ドメインモデル
- `render` の最小 `RenderFrame` 生成
- `storage` の最小保存/読込
- `wgpu` テクスチャアップロード経路
- 組み込みパネルの最小中間表現

これらは捨てるのではなく、以下のように再利用する。

- `RenderFrame` 経路は、初期の提示パスとして活かす
- `PanelUi` の考え方は、将来の `PanelTree` に発展させる
- 組み込みパネル群は、新ランタイムへの移植対象とする
- 旧 `Slint` 固有モデルはすでに撤廃済みであり、今後は再導入しない

## 各フェーズでのレビュー観点

毎フェーズで最低限、以下を確認する。

- GPU所有権はホスト側に残っているか
- パネルは中間表現を返すだけに留まっているか
- UI状態変更が `Command` / ホストAPI経由に揃っているか
- 描画エンジンとUIランタイムが密結合していないか
- 外部プラグイン導入時に ABI を壊しにくいか
- パフォーマンス要件に近づいているか

## 当面の優先アクション

直近で優先するのは以下である。

1. `RENDERING-ENGINE.md` に沿ってレンダリング責務を明文化する
2. `apps/desktop` のホスト主導フレームループへの移行計画を切る
3. `plugin-api` のパネル中間表現を DSL/Wasm ローダ接続に向けて拡張する
4. 組み込みパネルを新しい `PanelTree` モデルへ寄せる
5. DSL パネルの最小文法を試作する

## この文書の結論

ロードマップ上の最重要事項は、機能数を増やすことではない。

最初にやるべきは、

- `wgpu` ネイティブ描画キャンバス
- ランタイムロード可能なパネルUI
- パネルからホストAPIを叩ける仕組み

の三点を、同じホストアーキテクチャの中で矛盾なく成立させることである。

