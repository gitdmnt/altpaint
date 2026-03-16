# altpaint 目標アーキテクチャ

## この文書の目的

この文書は、`altpaint` が今後採るべき**目標構造と責務境界**を定義するための文書である。

ここで書くのは現状の説明ではない。
現状の構造は [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md) を参照すること。

この文書では、次を固定する。

- どの責務をどの層へ置くか
- host に残す高性能経路は何か
- plugin へ委譲する非性能領域は何か
- plugin panel / tool / workspace / project の境界をどう考えるか

## 基本的理念

`altpaint` は次の理念を採る。

> GPU 処理などの性能が要求されるものは基本的にアプリに組み込む。
> それ以外のものはすべて plugin として実装する。

この原則により、`altpaint` は

- host が性能要求の高い runtime を持ち
- plugin が機能拡張と UI を担う

という構造を目指す。

さらに、キャンバス編集処理に関して次の原則を採る。

> **キャンバスへの描画は必ず GPU を使って行う。**
> **CPU と GPU 間の通信は最低限に抑える。**

具体的には次を意味する。

- スタンプ生成・ブラシ合成・塗りつぶし等のキャンバス編集操作は GPU compute shader で実行する
- キャンバスビットマップは GPU テクスチャとして常時保持する
- **キャンバス編集中のビットマップデータの CPU→GPU 転送は禁止**（差分であっても不可）
- CPU は「座標・サイズ・色・ペン設定」などのパラメータ（uniform buffer）のみを GPU へ渡す
- CPU が GPU テクスチャの内容を読み書きするのは、プロジェクトの保存・読込時のみ

この原則に反する実装（CPU での `Vec<u8>` ベースのブラシ合成等）は、性能改善フェーズで順次 GPU 実装へ移行する。

## 目標の層構造

### 1. `desktopApp`

`desktopApp` は起動時およびランタイムの I/O と event loop を担当する。

置くもの:

- アプリ起動
- window 作成
- OS 入力の受信
- event loop
- GPU device / surface / presenter の所有
- 各 subsystem 呼び出し順の制御

置かないもの:

- ドメイン編集ロジックの本体
- project / workspace の意味論
- ツール差分生成の本体
- panel 定義の parse
- plugin author 向け SDK

### 2. `app-core`

アプリの中核的な処理は `app-core` が担う。

置くもの:

- document model
- command model
- layer / page / panel / work などの中核状態
- tool や workspace に関する純粋状態
- host から見た不変条件

置かないもの:

- OS / GPU / event loop
- plugin file load
- panel runtime
- canvas 差分生成アルゴリズム

### 3. `render`

表示するための画面の生成はすべて `render` が担う。

置くもの:

- canvas の表示計画
- overlay の表示計画
- panel surface の表示計画
- dirty rect 写像
- 画面座標変換
- 画面生成のための scene / pass / quad / compose 計画

置かないもの:

- OS イベント処理
- file I/O
- plugin 実行判断

### 4. `desktop-support`

実行時間の計測や desktop 固有補助は `desktop-support` が担う。

置くもの:

- profiler
- native dialog 境界
- desktop 固有 config
- desktop 起動補助

置かないもの:

- project 形式そのもの
- canvas 演算
- panel runtime

### 5. `canvas`

キャンバスへの処理は `canvas` が担当する。

置くもの:

- キャンバス入力解釈
- ツール実行ランタイム
- bitmap 差分生成
- ブラシ / 消しゴム / 塗りつぶしなどの canvas オペレーション
- canvas 固有の演算補助

置かないもの:

- event loop
- 最終画面生成
- plugin panel runtime

補足:

- `canvas` は性能要求の高い領域として host 側に置く。

### 6. `ui-shell`

plugin への API 提供を含めた処理は `ui-shell` が担当する。

置くもの:

- plugin panel と host の仲介
- panel event 変換
- host action の受理
- workspace 上の panel 管理 API
- plugin に対する host service 提供

置かないもの:

- Wasm runtime 実装そのもの
- panel 定義ファイル parse 本体
- canvas 差分生成

### 7. `plugin-host`

plugin panel の runtime は `plugin-host` が担う。

置くもの:

- Wasm runtime
- ABI bridge
- sandbox / isolation
- plugin 呼び出しのエラー境界

### 8. `panel-dsl`

plugin panel のファイル parse は `panel-dsl` が担う。

置くもの:

- parser
- validator
- normalized IR
- panel manifest の解釈

### 9. `plugin-sdk`

plugin panel の SDK は `plugin-sdk` が担当し、macro はそのサブモジュールが担当する。

置くもの:

- plugin 作者向け安定 API
- typed command / state / host accessor
- panel authoring surface
- macro export surface

補足:

- macro は物理的に別 crate でもよいが、論理的には `plugin-sdk` 配下の authoring surface として扱う。

## 目標 crate 配置草案

現時点でまだ存在しない crate を含め、今後の責務移動先は次で固定して読む。

| 論理名          | 現在の主配置                                 | 目標配置               | 主責務                                                | 新規コードを置く判断基準                             |
| --------------- | -------------------------------------------- | ---------------------- | ----------------------------------------------------- | ---------------------------------------------------- |
| `desktopApp`    | `apps/desktop`                               | `apps/desktop`         | event loop、OS I/O、GPU 所有、subsystem orchestration | OS/window/GPU/event loop に触るならここ              |
| `app-core`      | `crates/app-core`                            | `crates/app-core`      | `Document`、`Command`、純粋状態、不変条件             | UI/GPU/Wasm を知らない純粋状態ならここ               |
| `render`        | `crates/render`                              | `crates/render`        | frame plan、dirty rect、座標変換、compose 計画        | 画面生成のための計算ならここ                         |
| `canvas`        | 未作成                                       | `crates/canvas`        | canvas 入力解釈、tool runtime、bitmap op              | canvas 差分生成や gesture state machine ならここ     |
| `ui-shell`      | `crates/ui-shell`                            | `crates/ui-shell`      | panel presentation、host facade、panel UI 管理 API    | panel の見た目・hit-test・focus・text input ならここ |
| `panel-runtime` | 未作成                                       | `crates/panel-runtime` | panel discovery、DSL/Wasm bridge、host snapshot sync  | panel runtime と presentation を分けたい処理ならここ |
| `plugin-host`   | `crates/plugin-host`                         | `crates/plugin-host`   | Wasm runtime、ABI、sandbox                            | Wasm 実行器そのものならここ                          |
| `panel-dsl`     | `crates/panel-dsl`                           | `crates/panel-dsl`     | `.altp-panel` parser / validator / normalized IR      | DSL parse / validate ならここ                        |
| `plugin-sdk`    | `crates/plugin-sdk` + `crates/plugin-macros` | `crates/plugin-sdk` 系 | plugin 作者向け安定 API と macro surface              | plugin 作者が直接触る API ならここ                   |

### 将来の物理配置イメージ

```text
apps/desktop
	-> app-core
	-> render
	-> canvas
	-> ui-shell
		 -> panel-runtime
				-> panel-dsl
				-> plugin-host

plugins/*
	-> plugin-sdk
```

補足:

- `panel-runtime` はフェーズ3で導入候補とする。
- `canvas` はフェーズ2で追加する前提とする。
- `plugin-sdk` は `plugin-macros` を再 export し、作者向け入口を 1 つに保つ。

### crate ごとの判断基準

#### `desktopApp`

- 入出力順序、再描画要求、OS イベント配線だけを持つ。
- project / workspace の意味論は置かない。
- canvas や panel の詳細アルゴリズムは持ち込まない。

#### `app-core`

- 保存可能な状態と不変条件だけを持つ。
- runtime 文脈の組み立ては置かない。
- file path、dialog、Wasm runtime、GPU 型を受け取らない。

#### `render`

- 描画結果を決める計算は置く。
- project 読込/保存や plugin discovery は置かない。
- UI の意味論ではなく表示計画だけを扱う。

#### `canvas`

- pointer/gesture から `BitmapEdit` 相当を作る責務を持つ。
- panel runtime や workspace layout は持たない。
- 最終提示や GPU upload は扱わない。

#### `ui-shell`

- panel presentation と host facade に寄せる。
- runtime bridge は `panel-runtime` 導入後に薄くする。
- panel 描画と focus/input 管理はここに寄せる。

#### `panel-runtime`

- DSL/Wasm/runtime sync を閉じ込める。
- panel surface の描画は持たない。
- `ui-shell` presentation 側へ Wasm 詳細を漏らさない。

#### `plugin-host`

- ABI と Wasm 実行だけを持つ。
- panel layout、workspace、描画キャッシュは持たない。

#### `panel-dsl`

- parser / validator / normalized IR に閉じる。
- host state や UI presentation を持たない。

#### `plugin-sdk`

- plugin 作者が依存する唯一の安定表面を目指す。
- host 内部型への依存を隠蔽する。
- runtime ABI の詳細を直接露出しない。

## plugin が担うべき領域

以下の機能は、それぞれ plugin が担う。

### 1. project file の読み込み / 保存

project の意味論と操作フローは plugin が持つ。

host は次だけを提供する。

- file I/O service
- serializer 実行 service
- 現在 document へのアクセス

### 2. workspace の読み込み / 保存

workspace の意味論、表示 panel の管理、配置管理は plugin が担う。

host は panel 配置 API と永続化 service を提供する。

### 3. ツール関連

plugin が次を担う。

- ツール一覧の読み込み / 一覧表示
- ツールパラメータおよび処理の親の読み込み / 一覧表示 / 設定
- キャンバスに書き込むための差分の生成

補足:

- 描画処理さえ plugin に記述され、アプリ本体はそれを実行する runtime である、という構造を目指す
- ツール処理 plugin は、処理を共有する子ツールを外部から追加できるべきである
- ツール処理 plugin は parameter file を読み取り、ツール処理を実行し、canvas 差分を生成する
- ペンプラグインはビルダーまたはマクロによって WGSL のパイプラインを生成し、`canvas` からそれを呼び出せるようにする

### 4. キャンバスビューの移動

view 操作の UI と意味論は plugin が担う。

host は view state 更新 API を提供する。

### 5. panel を持つ plugin 一覧の表示、表示非表示切り替え

workspace / panel 管理 plugin が担う。

### 6. color palette

color 選択 UI とその操作フローは plugin が担う。

## runtime flow の目標形

### 1. 起動

1. `desktopApp` が window / GPU / event loop を初期化する
2. `desktopApp` が `app-core`、`render`、`canvas`、`ui-shell` を起動する
3. `ui-shell` が `panel-dsl` と `plugin-host` を使って plugin panel を準備する
4. plugin が必要な project / workspace / tool catalog を読み込む
5. `render` が最初の画面生成を行う

### 2. canvas 入力

1. `desktopApp` が入力を受ける
2. `canvas` が入力を解釈する
3. `app-core` の document state を参照する
4. tool plugin が差分を生成する
5. host runtime が差分を適用する
6. `render` が画面を再生成する

### 3. panel イベント

1. `desktopApp` が panel 入力を受ける
2. `ui-shell` が panel event に変換する
3. `plugin-host` が panel plugin を呼び出す
4. plugin は host API / command request を返す
5. host が `app-core` / `canvas` / `render` へ反映する

### 4. project / workspace I/O

1. plugin が保存 / 読込操作を開始する
2. `ui-shell` が host service を介して I/O を仲介する
3. host が serializer や runtime state へアクセスする
4. 結果を plugin と host state に反映する

## 依存方向の原則

### 守る方向

- `desktopApp` -> `app-core`, `render`, `desktop-support`, `canvas`, `ui-shell`
- `ui-shell` -> `plugin-host`, `panel-dsl`, `plugin-sdk` が前提とする契約
- `canvas` -> `app-core`
- `render` -> `app-core`
- `plugin-host` -> panel/plugin schema
- plugin -> `plugin-sdk`

### 禁止したい方向

- `app-core` -> `apps/desktop` / OS / GPU / `wgpu`
- `app-core` -> `plugin-host` / Wasm runtime
- plugin -> host 内部型の直接参照
- plugin -> GPU / event loop 直接制御
- `desktop-support` -> canvas / panel runtime の本体
- `render` -> file I/O や plugin discovery
- `render` -> project / workspace I/O の意味論
- `ui-shell` の presentation 側 -> Wasm runtime の詳細
- `canvas` -> panel runtime

## 境界設計の原則

### 1. host は高性能 runtime を持つ

host は次を直接所有する。

- GPU
- event loop
- canvas 実行 runtime
- 画面生成

### 1-a. キャンバス編集は GPU で行う

- キャンバスへの全ての描画操作（ブラシ・消しゴム・塗りつぶし等）は GPU compute shader で実行する
- キャンバスビットマップは GPU テクスチャとして保持し、CPU バッファに戻す操作を行わない
- CPU は描画パラメータの計算とコマンド発行のみを担い、ピクセル演算は GPU に委ねる
- **キャンバス編集中のビットマップデータの CPU→GPU 転送は禁止**（差分であっても不可）
- CPU から GPU へ渡すのは座標・サイズ・色等の uniform buffer パラメータのみ

### 2. plugin は意味論と UI を持つ

plugin は次を持つ。

- UI
- 操作フロー
- project / workspace / tool / color / view の意味論
- host へ要求する command / service request

### 3. plugin は host を直接触らない

plugin は常に安定 API を通る。

直接参照を禁止するもの:

- `Document` の内部構造
- GPU resource
- window handle
- runtime 内部状態

## 追加判断基準

新しい機能を追加するときは、まず次を確認する。

### host に置くべきもの

- GPU / 高速描画 / 低遅延入力処理
- canvas 差分適用 runtime
- 厳しい性能要件がある処理

### plugin に置くべきもの

- UI
- I/O フロー
- 設定管理
- view / panel / tool / color / workspace の操作意味論
- 外部記述ファイルに基づく振る舞い

## 新規ファイル配置規約

今後 module を増やすときは、少なくとも次の意味で名前を使い分ける。

### `runtime/`

- 外部 runtime や stateful bridge を置く。
- Wasm / event / host snapshot などの仲介を含める。

### `presentation/`

- layout、hit-test、focus、text input、surface 生成など見た目寄りを置く。
- runtime の詳細を直接持ち込まない。

### `services/`

- project / workspace / export / catalog など I/O orchestration を置く。
- serializer や dialog を束ねる上位フローを置く。

### `ops/`

- canvas や render の高頻度オペレーションを置く。
- stateless か、少なくとも狭い演算責務へ切る。

### `tests/`

- crate 単位・module 単位で分離した境界テストを置く。
- integration でしか検証できないもの以外は `apps/desktop` へ残さない。

### `lib.rs`

- module 宣言、公開 API、薄い re-export に寄せる。
- 大きな実装や分岐を `lib.rs` に戻さない。

## この文書の結論

`altpaint` の目標構造は、

- host が性能要求の高い runtime を持ち
- plugin が機能と UI を持ち
- `desktopApp`、`app-core`、`render`、`canvas`、`ui-shell`、`plugin-host` が明確に分担する

という形である。

現状がこの形とずれていても、今後の変更は常にこの文書を基準に寄せていく。
