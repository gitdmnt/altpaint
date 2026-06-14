# altpaint 目標アーキテクチャ

## この文書の目的

この文書は、`altpaint` が採る**目標構造と責務境界**を定義する。

ここで書くのは「どこに何を置くべきか」の設計原則であり、現状コードの逐次説明ではない。
現状コードの依存事実は [docs/MODULE_DEPENDENCIES.md](MODULE_DEPENDENCIES.md) を、現況の到達点は
[docs/IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) を参照すること。

**文書とコードが食い違う場合、現に動いているコードが正本。** この文書は、新規コードを
どの層へ置くかを判断するための基準として使う。

この文書では次を固定する。

- どの責務をどの層へ置くか
- host に残す高性能経路は何か
- plugin（パネル）へ委譲する非性能領域は何か
- 水平土台と垂直 feature の設計原則
- 守るべき依存方向

> 用語注: ADR 018 で「plugin」は予約語化した。現在 host を拡張する実体は **HTML+CSS+Wasm
> パネル**である。本文書では「パネル」を実体名として使い、「plugin が担う領域」という
> 表現は将来の拡張点を示す概念語として用いる。「Koma（コマ）」は作品ドメインの分割単位、
> 「panel」は UI パネルを指す（ADR 018 B1 で命名衝突を解消済み）。

## 基本的理念

`altpaint` は次の理念を採る。

> 性能が要求されるものは host（アプリ本体）に組み込む。
> それ以外のものは plugin（パネル）として実装する。

この原則により、`altpaint` は

- host が性能要求の高い runtime を持ち
- パネルが機能拡張と UI を担う

という構造を目指す。

さらに、キャンバス編集処理に関して次の原則を採る。

> **キャンバスへの描画は GPU を使って行う。**
> **CPU と GPU 間の通信は最低限に抑える。**

具体的には次を意味する。

- スタンプ生成・ブラシ合成・塗りつぶし等のキャンバス編集操作は GPU compute shader で実行する
- キャンバスビットマップは GPU テクスチャとして常時保持する
- **キャンバス編集中のビットマップデータの CPU→GPU 転送は禁止**（差分であっても不可）
- CPU は「座標・サイズ・色・ペン設定」などのパラメータ（uniform buffer）のみを GPU へ渡す
- CPU が GPU テクスチャの内容を読み書きするのは、プロジェクトの保存・読込時のみ

補足: GPU が利用できない環境向けに CPU 参照経路（`paint-engine::ops` の bitmap op +
`CpuPaintBackend`）を持つが、これはフォールバックであり、性能要件のある通常経路は GPU である。

## 層構造の全体像

クレートは責務性質で 4 群に分かれる。

```text
host（apps/desktop）
  ├─ 垂直 feature  : 機能 1 つを「実装 + 状態 + 翻訳」で縦に閉じる
  ├─ パネル基盤    : HTML+CSS+Wasm パネルの実行・配置・契約
  └─ 水平土台      : feature 横断で共有する純粋な型・演算（最安定層）
```

- **水平土台**: 座標・ビットマップ・ドメインモデル・編集セッション・パネル契約など、
  複数 feature が共有する純粋な型と演算。GPU / OS / Wasm を知らない。最も安定させる層。
- **パネル基盤**: パネル（HTML+CSS+Wasm）を実行・配置・描画し、host との契約を仲介する層。
- **垂直 feature**: paint / project / export 等の機能を 1 つずつ縦に切った層。`apps/desktop`
  内の `features/` スライスとして実装する。
- **host**: `apps/desktop`。OS window と GPU を唯一所有し、event loop と提示を回す。

## 水平土台の設計原則

水平土台は **feature 横断で共有される純粋な型と演算**だけを置く層である。

- `winit` / `wgpu` / `wasmtime` / `blitz` に依存しない（GPU・OS・Wasm 非依存）
- I/O・dialog・file path を持たない
- 依存方向は厳密に一方向にし、循環を作らない
- ある型が「複数の feature から参照される純粋な状態か演算」であれば水平土台へ、
  「特定機能の I/O フローやランタイム配線」であれば feature 側へ置く

水平土台クレート（ドメイン側）:

| クレート          | 責務                                                                       | ローカル依存                                  |
| ----------------- | -------------------------------------------------------------------------- | --------------------------------------------- |
| `geometry`        | 座標型・矩形・dirty rect 演算                                               | なし（最下層）                                |
| `raster`          | `RgbaBitmap`・ブレンド・ラスタライズ・`BitmapEdit`                          | `geometry`                                    |
| `editor-state`    | `EditorSession`・ツール/ペン定義・`SessionCommand`・`ToolDescriptor`・`view_policy` | なし（`document-model` 非依存 = 循環回避） |
| `document-model`  | `Document`/`Work`/`Page`/`Koma`/`RasterLayer`・`DocumentCommand`            | `geometry` / `raster` / `editor-state`        |
| `canvas-geometry` | `CanvasViewGeometry`（view↔page 写像）・提示用テクスチャ矩形                | `geometry` / `editor-state`                   |
| `frame-profiler`  | フレーム実行時間計測（整形は host 側）                                      | なし                                          |

ドメインの二大入口は `document-model::Document`（作品コンテンツ）と
`editor-state::EditorSession`（編集セッション）である。状態変更はそれぞれ
`DocumentCommand` / `SessionCommand` を入口とする。`editor-state` は `document-model` に
依存させない（一方向にして循環を避ける）。

パネル契約側の水平土台:

| クレート         | 責務                                                       | ローカル依存 |
| ---------------- | ---------------------------------------------------------- | ------------ |
| `panel-protocol` | host↔Wasm 共有 DTO・wire 名定数・ABI 定数・`HostState` DTO  | なし         |

`panel-protocol` は契約クレートとして純粋であり、ドメインクレート（`document-model` /
`editor-state`）にも依存しない。これによりパネル基盤全体が feature 非依存の水平土台として
成立する（ADR 018 B6 で旧 `panel-api` のドメイン依存を切断して達成）。

## パネル基盤の設計原則

パネル基盤は **host とパネル（HTML+CSS+Wasm）の境界**を担う層である。

- パネルは host 内部型を直接参照しない。常に安定 API（`panel-sdk`）を通る
- host↔Wasm のやりとりは `panel-protocol` の DTO に閉じる
- runtime（Wasm 実行・host state 同期）と配置（layout・focus・hit-test）を分離する
- host からの入口は `panel-runtime`（runtime facade）と `panel-workspace`（配置）の 2 系統

| クレート          | 責務                                                                              | ローカル依存                                                            |
| ----------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `panel-protocol`  | host↔Wasm 契約 DTO・wire/ABI 定数・keyboard 規約（水平土台でもある）              | なし                                                                    |
| `panel-wasm-host` | wasmtime ベースの Wasm 実行器 + DOM mutation host functions                        | `panel-protocol`                                                        |
| `panel-html`      | Blitz（HTML/CSS）+ vello による GPU 直描画・hit 矩形収集                           | なし（Blitz / taffy / vello / wgpu に閉じる）                           |
| `panel-runtime`   | パネル registry・HTML/Wasm bridge・host state 同期・translator registry・loader    | `document-model` / `editor-state` / `raster` / `panel-protocol` / `panel-wasm-host` / `panel-html` |
| `panel-workspace` | ワークスペース配置・focus・hit-test                                               | `geometry`                                                              |
| `panel-sdk`       | パネル作者向け安定 API（`emit_request` / DOM ヘルパ / shortcut / state / test）   | `panel-macros` / `panel-protocol`                                       |
| `panel-macros`    | `#[panel_init]` / `#[panel_handler]` / `#[panel_on_host_change]` proc-macro       | なし                                                                    |

責務境界:

- `panel-runtime` が **runtime 側の正本**（Wasm 実行・host state 同期・hit 収集・契約型公開）
- `panel-workspace` が **配置側の正本**（layout・focus・hit-test 結果の利用）
- `panel-workspace` は `panel-runtime` へコンパイル依存しない。登録パネル一覧は
  `reconcile_panels(panel_ids)` の引数として host から受け取る
- `panel-runtime` が host 向け契約型（`HostRequest` / `PanelEvent` / `ServiceRequest`）と
  `panel-html` の最小面（`panel_runtime::html`）を host へ公開する。host は `panel-html` /
  `panel-wasm-host` へ直接依存しない
- パネル作者は `panel-sdk` のみに依存する。`panel-macros` は `panel-sdk` が再 export する

## 垂直 feature の設計原則

垂直 feature は **機能 1 つを「サービスハンドラ + 状態 + 翻訳器」で縦に閉じる**スライスである。
`apps/desktop/src/features/` 配下に置く。

- 1 スライス = 1 機能の I/O フロー・状態・request 翻訳をまとめて所有する
- 機能横断で共有する純粋な型は水平土台へ抜く（feature に純粋ドメイン型を抱え込まない）
- パネル/入力発の要求は `ServiceRequest` として届き、各スライスのサービスハンドラが受ける。
  ハンドラは名前空間 registry（`app/services/registry.rs::SERVICE_HANDLERS`）に登録する
- OS 固有 I/O（dialog / path）は feature ではなく `platform/` に寄せる

現在の 11 スライス:

| スライス            | 責務                                                                                |
| ------------------- | ----------------------------------------------------------------------------------- |
| `paint`             | ペイント実行配線・`PaintBackend`（Cpu/Gpu）・`EditHistory`・undo/redo・ブラシプレビュー |
| `project`           | project save/load・session save/load・canvas size preset                            |
| `export`            | PNG export とハンドラ                                                                |
| `workspace`         | workspace preset catalog・workspace I/O・layout service                             |
| `tools`             | ツールカタログ読込・既定カタログ・ツール設定/ペン import ハンドラ                    |
| `koma`              | コマ作成ジェスチャ・コマ移動ナビゲーションハンドラ                                   |
| `view`              | ビュー操作サービスハンドラ                                                           |
| `snapshots`         | `DocumentSnapshotStore` とハンドラ                                                   |
| `text`              | テキストラスタライズとハンドラ                                                       |
| `panel_interaction` | パネル drag/resize/press 幾何ステートマシン                                          |
| `status_bar`        | ステータスバー描画とスナップショット組み立て                                         |

## host（apps/desktop）の責務

`apps/desktop`（package `altpaint-desktop`、bin `altpaint`）は唯一の実行ホストである。

置くもの:

- アプリ起動・window 作成・OS 入力受信・event loop（`event_loop.rs`）
- GPU device / surface / presenter の所有（`presenter/`）
- GPU ペイントリソースの所有と dispatch 判断（`gpu-paint` の `LayerTextureStore` /
  `BrushPipeline` / `FillPipeline` / `CompositePipeline` を所有）
- canvas 入力 → `DocumentCommand` / `SessionCommand` / `PaintInput` 変換
- `DesktopApp` による状態遷移・副作用統合（composition root）と提示（`PresentFrame` 組み立て）
- 垂直 feature スライス群（`features/`）と OS 固有 I/O（`platform/`）

置かないもの:

- 純粋ドメイン状態の本体（→ 水平土台）
- Wasm 実行器・パネル配置アルゴリズムの本体（→ パネル基盤）
- パネル作者向け SDK（→ `panel-sdk`）

GPU ペイントの三層理解:

- **画素適用の実装**は `gpu-paint`（compute shader）
- **計画生成**は `paint-engine::plan_paint`（`PaintPlan` = 純データ計画。画素を作らない）
- **backend 選択・結合・dispatch 判断**は host の `features/paint`

## 垂直 feature 側の純粋演算クレート

feature の実装が依存する、GPU/ドメイン演算のクレート:

| クレート       | 責務                                                                       | ローカル依存                                  |
| -------------- | -------------------------------------------------------------------------- | --------------------------------------------- |
| `paint-engine` | gesture 解釈・ペイント文脈解決・`PaintPlan` 生成・CPU 参照 bitmap op        | `document-model` / `editor-state` / `geometry` / `raster` |
| `gpu-paint`    | ブラシ/塗り/合成の GPU compute 実装・レイヤーテクスチャ store               | `editor-state` / `geometry` / `raster` / `wgpu` |
| `project-store`| SQLite project save/load                                                   | `document-model` / `panel-workspace` / `raster` |
| `pen-io`       | ペンプリセット読込 / import / export                                       | `editor-state`                                |

- `gpu-paint` は `wgpu` に依存する唯一のペイント実装クレート。dispatch の判断（GPU/CPU 選択）
  は持たず、それは host 側にある
- `paint-engine` は panel runtime / project I/O を知らない
- `project-store` / `pen-io` は永続化を担うが、永続化パスの解決と session/preset 永続化は host
  の `platform/` / `features/` に置く

## plugin（パネル）が担うべき領域

次の機能は、それぞれパネル（HTML+CSS+Wasm）が UI と操作フローを担う。host は安定 API と
service だけを提供する。

| 領域              | パネルが持つもの                              | host が提供するもの                         |
| ----------------- | --------------------------------------------- | ------------------------------------------- |
| project I/O       | 保存/読込の操作フロー・UI                      | file I/O service・serializer・現在 document  |
| workspace I/O     | 表示パネル管理・配置 UI                        | 配置 API・永続化 service                     |
| ツール            | 一覧表示・パラメータ設定・選択 UI              | カタログ・ツール選択 command                |
| view 操作         | パン/ズーム/回転の UI                          | view state 更新 command（`SessionCommand`） |
| color palette     | 色選択 UI と操作フロー                         | 現在色 state・色変更 command                |
| snapshots         | スナップショット一覧・操作 UI                  | snapshot store service                      |

将来の拡張意図（現状スコープ外）: ツール処理そのものをパネル/プラグインに記述し、host が
それを実行する runtime に徹する構造（ペンが WGSL パイプラインを生成し host から呼ばれる等）。
本リファクタのスコープ外として明示する。

## runtime flow の目標形

### 1. 起動

1. host が window / GPU / event loop を初期化する
2. host が session / project / workspace preset を解決し、`PanelRuntime` / `PanelWorkspace` と
   `Document` の初期状態を組み立てる
3. `panel-runtime::register_builtin_panels` が同梱 12 パネルをロードし、各パネルが HTML と
   Wasm instance を初期化する
4. パネルサイズは `panel.meta.json::default_size` を初期値に、workspace 永続値で上書きする
5. host が GPU ペイントリソースを構築し、全レイヤーを GPU テクスチャへ同期する

### 2. キャンバス編集

1. host が OS pointer event を受け、canvas/panel を振り分ける
2. `canvas-geometry` が view 座標を page 座標へ変換する
3. `paint-engine::gesture` が down/drag/up を `PaintInput` やコマ矩形 preview へ変換する
4. host `features/paint` が `paint-engine::plan_paint` で `PaintPlan` を生成し、選択した
   `PaintBackend`（GPU 有効時 Gpu / 不在時 Cpu）へ適用を委譲する
5. `GpuPaintBackend` が `gpu-paint` の compute shader で GPU レイヤーテクスチャへ直接書込み・
   合成する（編集中に CPU 画素を作らない）
6. dirty rect / UI 再同期要求が host に蓄積され、提示フレームが組み立てられる

### 3. パネルイベント

1. host が panel 入力を受ける
2. host の `panel_interaction` 幾何ステートマシンと host_request_router が hit テーブル
   （`panel-runtime` が CPU 更新）で hit-test / drag / resize を中継する
3. 対象パネルへ `PanelEvent` が forward され、Wasm handler が実行される。handler は DOM
   mutation host functions で自パネルの DOM を更新できる
4. handler が返した `RequestDescriptor` を `panel-runtime` の translator registry が名前空間
   prefix で `HostRequest`（`DispatchDocumentCommand` / `DispatchSessionCommand` /
   `RequestService`）へ変換する。未登録 prefix/name は黙殺せず診断ログへ流す
5. host が `apply_document_command` / `apply_session_command` / `execute_service_request` へ
   振り分けて反映する
6. `panel-html` が vello で GPU テクスチャへ再描画し、host が panel quad 層で合成する

### 4. project / workspace I/O

1. パネル/入力層が `ServiceRequest`（`project_io.*` 等）を発行する
2. 保存前に host が GPU テクスチャを readback して `Document` の CPU bitmap を最新化する
3. project 保存は `project-store`、workspace layout は `panel-workspace`、panel config は
   `panel-runtime` から取り出して保存する
4. session 保存は host の `features/project/session.rs` へ委譲する（`dirs` ベースのパス）

project file と session file は役割が異なる:

- project file: 作品状態 `Document` + workspace layout + panel config（`EditorSession` は含まない）
- session file: 最後に開いた project と `EditorSession`（ツール/色/ペン/ビューの編集セッション）

## 依存方向の原則

### 守る方向

- `apps/desktop` → 水平土台（`geometry` / `raster` / `document-model` / `editor-state` /
  `canvas-geometry` / `frame-profiler`）、feature 演算（`paint-engine` / `gpu-paint` /
  `project-store` / `pen-io`）、パネル基盤入口（`panel-runtime` / `panel-workspace`）
- 水平土台ドメイン: `geometry` → `raster` → `editor-state` / `document-model`（一方向。
  `editor-state` は `document-model` に依存しない）
- `paint-engine` / `gpu-paint` / `canvas-geometry` → 水平土台ドメイン
- `panel-runtime` → `panel-wasm-host` / `panel-html` / `panel-protocol` /（host state 構築のため）
  `document-model` / `editor-state` / `raster`
- `panel-wasm-host` → `panel-protocol`
- `panel-workspace` → `geometry`
- `project-store` → `document-model` / `raster` / `panel-workspace`
- パネル crate → `panel-sdk` のみ

### 禁止する方向

- 水平土台 → `apps/desktop` / OS / GPU(`wgpu`) / Wasm(`wasmtime`) / `blitz`
- `editor-state` → `document-model`（循環回避）
- パネル基盤の契約（`panel-protocol`）→ ドメイン（`document-model` / `editor-state`）
- パネル crate → host 内部型の直接参照 / GPU / event loop 直接制御
- `canvas-geometry` → project / workspace I/O の意味論 / GPU 実装
- `panel-workspace` 配置側 → Wasm runtime の詳細
- `paint-engine` → panel runtime

## 境界設計の原則

### 1. host は高性能 runtime を持つ

host は次を直接所有する。

- GPU・event loop・GPU ペイント dispatch・画面生成

#### 1-a. キャンバス編集は GPU で行う

- キャンバスへの描画操作（ブラシ・消しゴム・塗りつぶし等）は GPU compute shader で実行する
- キャンバスビットマップは GPU テクスチャとして保持し、CPU バッファに戻す操作を行わない
- CPU は描画パラメータの計算とコマンド発行のみを担い、ピクセル演算は GPU に委ねる
- **キャンバス編集中のビットマップデータの CPU→GPU 転送は禁止**（差分であっても不可）
- CPU から GPU へ渡すのは座標・サイズ・色等の uniform buffer パラメータのみ

### 2. パネルは意味論と UI を持つ

パネルは UI・操作フロー・project / workspace / tool / color / view の操作意味論・host へ
要求する command / service request を持つ。

### 3. パネルは host を直接触らない

パネルは常に安定 API（`panel-sdk`）と DTO（`panel-protocol`）を通る。直接参照を禁止するもの:

- `Document` の内部構造・GPU resource・window handle・runtime 内部状態

## 新機能を追加するときの配置判断

### host に置くべきもの

- GPU / 高速描画 / 低遅延入力処理
- canvas 差分適用 runtime・厳しい性能要件のある処理

### パネルに置くべきもの

- UI・I/O フロー・設定管理
- view / panel / tool / color / workspace の操作意味論
- 外部記述ファイルに基づく振る舞い

### 水平土台に置くべきもの

- 複数 feature が共有する純粋な型・演算（GPU/OS/Wasm 非依存）

### 垂直 feature に置くべきもの

- 特定機能の I/O フロー・状態・request 翻訳

## 新規ファイル配置規約

module を増やすときは、少なくとも次の意味で名前を使い分ける。

### `runtime/`

- 外部 runtime や stateful bridge を置く（Wasm / event / host state snapshot などの仲介）

### `presentation/`

- layout / hit-test / focus / text input / surface 生成など見た目寄りを置く。runtime の詳細を
  直接持ち込まない

### `services/`

- project / workspace / export / catalog など I/O orchestration を置く

### `features/`

- 機能 1 つを縦に閉じる垂直スライス（サービスハンドラ + 状態 + 翻訳器）を置く

### `platform/`

- OS 固有 I/O（dialog / path）を置く。アプリ層ロジックに OS 分岐を書かない
  （`#[cfg(target_os)]` 禁止。差異はクロスプラットフォームライブラリに吸収させる）

### `ops/`

- canvas や render の高頻度オペレーションを置く。stateless か狭い演算責務へ切る

### `tests/`

- crate 単位・module 単位の境界テストを置く

### `lib.rs`

- module 宣言・公開 API・薄い re-export に寄せる。大きな実装を `lib.rs` に戻さない

## この文書の結論

`altpaint` の目標構造は、

- host（`apps/desktop`）が GPU・event loop・ペイント dispatch・提示という性能要求の高い
  runtime を所有し
- 水平土台が feature 横断の純粋な型・演算を最安定層として持ち
- パネル基盤が host とパネルの契約・実行・配置を仲介し
- 垂直 feature が機能 1 つを「実装 + 状態 + 翻訳」で縦に閉じ
- パネル（HTML+CSS+Wasm）が UI と操作意味論を担う

という形である。新規の変更は常にこの文書を基準に寄せていく。整合する依存事実は
[docs/MODULE_DEPENDENCIES.md](MODULE_DEPENDENCIES.md) を参照すること。
