# altpaint アーキテクチャ設計

## この文書の目的

この文書は、`altpaint` の**現在の実装と、そこから維持したい設計原則**を整理するための中核文書である。

依存関係の事実関係は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) を正本とし、この文書では次を定義する。

- どの責務をどの層へ置くか
- どこを安定境界として扱うか
- desktop host / render / panel runtime / storage の分離原則
- 新しいクレートや機能を追加する際の判断基準

## 読む順番

1. 現在の到達点を知りたいときは [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
2. 実際の依存関係を知りたいときは [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
3. 設計原則と責務境界を知りたいときはこの文書
4. 実装順序を知りたいときは [docs/ROADMAP.md](docs/ROADMAP.md)

## 基本方針

`altpaint` は次の方針を採る。

1. **ホストが window / GPU / event loop を所有する**
2. **ドメイン状態は `app-core` に閉じ込める**
3. **パネル UI は host 主導の中間表現で扱う**
4. **Wasm panel は host API を直接触らず、DTO と `HostAction` を経由する**
5. **project 永続化と desktop session 永続化を分離する**
6. **標準パネルも外部パネルに近い形へ寄せる**

## 現在のアーキテクチャを一言で言うと

2026-03-10 時点の `altpaint` は、次の構成になっている。

- `app-core` がドメイン中心
- `apps/desktop` が実行ホスト
- `ui-shell` が panel runtime 統合点
- `plugin-host` が Wasm 実行器
- `panel-dsl` / `panel-schema` / `panel-sdk` / `panel-macros` が panel 基盤
- `storage` が project I/O
- `desktop-support` が desktop 固有 I/O と profiler
- `render` は存在するが、まだ最小レンダ入口の段階

つまり、理想図としての「render 中心の描画エンジン」はまだ途中であり、**現在の実装では desktop 側が描画責務をかなり持っている**。

## 層構造

### 1. ドメイン層: `app-core`

置くもの:

- `Document`
- `Work -> Page -> Panel -> Layer`
- `Command`
- 色、ペン、レイヤー、ビュー変換
- workspace layout の保存対象モデル

置かないもの:

- `winit`
- `wgpu`
- `wasmtime`
- panel DSL / ABI / file dialog

### 2. データ永続化・desktop 補助層: `storage`, `desktop-support`

#### `storage`

置くもの:

- project save/load
- format version 管理
- workspace layout / plugin config 永続化
- pen preset 読込

#### `desktop-support`

置くもの:

- desktop config 定数
- session save/load
- native dialog 境界
- runtime profiler

ルール:

- project file と session file は分ける
- desktop 固有の path / dialog / profiler は `storage` に混ぜない

### 3. パネル契約層: `plugin-api`

置くもの:

- `PanelPlugin`
- `PanelTree`
- `PanelNode`
- `PanelEvent`
- `HostAction`

意味:

- host が panel を理解する最小中間表現
- panel が host へ伝える操作要求の型

### 4. パネル定義・ABI 層: `panel-dsl`, `panel-schema`, `panel-sdk`, `panel-macros`, `plugin-host`

#### `panel-dsl`

- `.altp-panel` parser / validator / normalized IR

#### `panel-schema`

- host-Wasm 間 DTO

#### `panel-sdk`

- plugin author 向け安定 API

#### `panel-macros`

- export 宣言を安全化する proc-macro

#### `plugin-host`

- `wasmtime` 上の panel runtime

ルール:

- ABI の生 details は `panel-schema` と `plugin-host` に閉じ込める
- plugin crate から host 内部型を直接参照させない

### 5. パネル実行・描画層: `ui-shell`

置くもの:

- panel registry
- `.altp-panel` 再帰ロード
- DSL panel の状態管理
- Wasm handler 呼び出し
- panel layout / hit test / focus / text input / scroll
- software panel rendering

現在の実態:

- 名前は `ui-shell` だが、実際には **panel runtime 統合層** である
- `render` への依存も持ち、最小 `RenderContext` を内部保持している

設計上の判断:

- 今はこの集中を許容する
- ただし将来的に DSL/Wasm runtime 部分と panel draw/hit-test 部分を分ける余地は残す

### 6. 描画・提示層: `render` と `apps/desktop`

#### `render`

現状の責務:

- `Document` から `RenderFrame` を作る最小入口

#### `apps/desktop`

現状の責務:

- `winit` event loop
- `wgpu` presenter
- desktop fixed layout
- CPU 側 base / overlay frame 合成
- canvas input routing
- `DesktopApp` による副作用統合

重要事項:

- 現時点では、レンダリングの実務の多くが `apps/desktop` にある
- この状態は「バグ」ではなく、現段階の実装到達点として扱う
- ただし将来は `render` へ寄せる判断余地がある

## compile-time 依存の原則

実装上の現在値は [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md) を参照する。
ここでは守るべき方向だけを書く。

### 守る方向

- `apps/desktop` -> `app-core`, `render`, `ui-shell`, `storage`, `desktop-support`, `plugin-api`
- `ui-shell` -> `app-core`, `plugin-api`, `panel-dsl`, `panel-schema`, `plugin-host`, `render`
- `storage` -> `app-core`
- `desktop-support` -> `app-core`
- `plugin-api` -> `app-core`
- `render` -> `app-core`
- `plugin-host` -> `panel-schema`
- `plugins/*` -> `panel-sdk`

### 禁止したい方向

- `app-core` -> `winit` / `wgpu` / `wasmtime`
- `plugins/*` -> `app-core` / `ui-shell` / `apps/desktop`
- `storage` -> `winit` / `wgpu`
- `desktop-support` -> `panel-dsl` / `plugin-host`

## runtime 境界

### 1. キャンバス編集境界

キャンバス編集は次の流れを通す。

1. OS input
2. `apps/desktop::runtime`
3. `apps/desktop::app::input`
4. `canvas_bridge`
5. `Command`
6. `Document::apply_command(...)`
7. dirty 集約
8. frame/presenter 更新

この経路の目的は、入力解釈とドメイン更新を分けることにある。

### 2. パネルイベント境界

パネルは host 内部状態を直接変更しない。

1. host が `PanelEvent` を作る
2. `UiShell` が panel へ配送する
3. DSL/Wasm panel は `StatePatch` と `CommandDescriptor` を返す
4. host はそれを `HostAction` / `Command` に変換して適用する

この構造により、panel は「UI を提案する側」であって「window や GPU を直接操作する側」ではない。

### 3. 永続化境界

保存系は2種類ある。

#### project 保存

- `storage` が扱う
- `Document` + `WorkspaceLayout` + `plugin_configs` を保存する

#### session 保存

- `desktop-support` が扱う
- 最後に開いたファイルや desktop session を扱う

この分離は今後も崩さない。

## 表示アーキテクチャ

現在の提示は 3 層で考える。

1. **UI ベース層**
   - CPU で生成
   - 背景、パネル、ステータス、キャンバス host 枠
2. **キャンバス層**
   - GPU texture として保持
   - パン/ズームは quad と UV で適用
3. **オーバーレイ層**
   - CPU で生成
   - ブラシプレビューなどの上物

この方針の目的は、パン/ズーム時に CPU 側でキャンバス全体を焼き直さないことにある。

## 追加クレート判断基準

新しい責務を増やすときは、まず次を確認する。

### `app-core` に置くべきもの

- ドメイン状態そのもの
- `Command` の意味論
- UI 非依存のデータ型

### `storage` に置くべきもの

- project file / asset file の読込保存
- format version

### `desktop-support` に置くべきもの

- desktop 固有 path
- dialog
- session
- profiler

### `ui-shell` に置くべきもの

- panel runtime / panel draw / panel input の責務

### `apps/desktop` に置くべきもの

- OS event loop
- GPU presenter
- desktop 固有の提示 orchestration

### `panel-*` に置くべきもの

- panel authoring 基盤
- panel DSL / ABI / SDK

## 現時点の設計メモ

### 1. `plugin-api` が `Command` を知っている

これは panel から host action を出す最短経路としては実用的である。
一方で、将来より疎結合にしたい場合は再検討余地がある。

### 2. `ui-shell` が `render` を知っている

理想的には panel runtime と render はさらに分けられる。
ただし現時点では `UiShell` が最小 render 入口も抱えており、この構成を前提に設計判断する。

### 3. `render` の責務は将来拡大余地が大きい

`RENDERING-ENGINE.md` に理想像はあるが、現実の正本はまだ desktop 側に広く分散している。

## 今後も維持したい原則

1. `app-core` を UI/GPU から切り離す
2. panel を host 主導の中間表現で扱う
3. `apps/desktop` だけが OS と GPU を所有する
4. project 保存と session 保存を混ぜない
5. built-in panel も external panel に近いモデルで保つ
6. 実装の事実は文書よりコードを優先する

## 関連文書

- [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/SKETCH.md](docs/SKETCH.md)
