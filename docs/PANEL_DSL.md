# altpaint Panel DSL 設計メモ

## 目的

この文書は、フェーズ6で導入する DSL パネルローダの最小設計案をまとめる。

対象要件は次の通りである。

- JSX 風の宣言的で atomic な構文
- Flex 的な要素で UI を組み立てられること
- ユーザー操作によって `Command` を発行できること
- ローカル state を持てること
- UI記述と言語処理を分離できること
- 処理側は Wasm としてコンパイルされ、`wasmtime` 上で動かせること
- ホストが描画・レイアウト・イベント配送の主導権を握り続けること

この DSL は、`docs/ARCHITECTURE.md` にある「パネルはホスト定義の中間表現を返す」という原則に従う。

## 方針更新: UI記述と処理記述を分離する

現時点では、1つの DSL に UI と処理を両方詰め込むより、
**UI記述言語と処理記述言語を分離する案を第一候補**とするのがよい。

採るべき構成は次である。

- UI は JSX 風 DSL で宣言的に記述する
- 処理は Rust などから Wasm へコンパイルする
- ホストは `wasmtime` 上で処理モジュールを実行する
- イベント時には UI DSL が handler 名を指し、実処理は Wasm 側が担当する

この分離の利点は明確である。

- UI 構文を読みやすく保てる
- UI DSL がミニ言語として肥大化しにくい
- 複雑な state 遷移や分岐を Wasm 側へ逃がせる
- フェーズ6の DSL とフェーズ7の Wasm を対立させず、自然に接続できる
- `wasmtime` を前提にした権限管理と隔離に乗せやすい

## この DSL の立ち位置

フェーズ6以降の panel は、概ね次の2要素の組で考えるのがよい。

- UI定義: JSX 風の `.altp-panel`
- 処理定義: Wasm モジュール

役割分担は次の通り。

- UI DSL は view 構造、レイアウト、binding、handler 参照を記述する
- Wasm はイベント処理、state 更新、command descriptor 生成を担当する
- ホストは両者を束ね、最終的な `PanelTree` 相当を生成・描画する

したがって UI DSL 自身は UI 実装でも処理実行器でもない。

- UI DSL は構造を記述する
- Wasm は処理を記述する
- ホストが `PanelTree` を構築する
- ホストがレイアウト、ヒットテスト、描画、フォーカス、スクロールを行う
- UI DSL と Wasm の両方が `wgpu`、OS ウィンドウ、ネイティブ widget を直接触らない

## 推奨する設計方針

### 1. JSX 風だが「制限付き」にする

記法は JSX 風にするが、任意コード実行は許可しない。

狙いは以下。

- 可読性を上げる
- 階層構造を見やすくする
- atomic な部品を組み合わせる感覚を明確にする
- parser と validator を単純に保つ

したがって、式は最小限に制限する。

- 定数
- `state.*` / `props.*` / `host.*` 参照
- 比較
- 論理演算
- 単純な三項式または `when`

MVP では次を入れない。

- 任意関数定義
- ループの一般構文
- 再帰コンポーネント
- ユーザー定義コンポーネント
- 任意 JavaScript 風式

### 2. Flex 的なレイアウトを最小ノードへ落とす

既存の `PanelNode` は `Column` / `Row` / `Section` / `Text` / `Button` を持つ。
フェーズ6の DSL もまずはこの方向に揃える。

MVP の atomic node 候補は以下。

- `Column`
- `Row`
- `Section`
- `Text`
- `Button`
- `Toggle`
- `Separator`
- `List`
- `Spacer`

ただし DSL のノードと内部 `PanelNode` は 1 対 1 でなくてよい。

例:

- `Toggle` は内部では `Button` + state update に落としてもよい
- `Spacer` はレイアウト専用の補助ノードとして扱ってよい
- `List` は MVP では host snapshot の配列を読むだけに制限してよい

### 3. コマンド発行とイベント処理は UI DSL に埋め込まない

UI DSL に `set ...` や `dispatch(...)` を処理言語として増やしすぎると、
小さな見た目用 DSL が処理系言語へ肥大化する。

そのため、UI DSL には handler 名だけを書き、イベント処理は Wasm 側へ寄せるのがよい。

推奨形:

- UI DSL: `on:click="activate_brush"`
- Wasm: `activate_brush` が state patch と command descriptor を返す
- ホスト: command descriptor を `Command::SetActiveTool { tool: ToolKind::Brush }` へ変換する

この方式の利点:

- UI DSL が Rust enum 名へ密結合しない
- UI DSL が処理系 DSL に化けにくい
- Wasm 側に複雑な分岐を寄せられる
- validation しやすい
- 権限モデルと接続しやすい

### 4. state は「UIで宣言し、処理で更新する」

state を許可する場合でも、まずはパネルローカル state に限定する。

MVP で持てる型:

- `bool`
- `int`
- `float`
- `string`
- `color`
- `enum`

MVP での用途:

- 展開/折りたたみ
- ローカル選択状態
- フィルタ文字列
- 一時的なタブ切替
- 入力中フラグ

重要なのは、document の正本 state とローカル state を分けること。

- document の変更は常に `Command` を経由する
- UI DSL 上の `state` は schema 宣言に留める
- 実際の state 更新ロジックは Wasm 側が担当する
- document を直接 mutation しない

推奨する責務分担:

- UI DSL: state 名、型、初期値を宣言する
- Wasm: state の更新ルールを実装する
- ホスト: state store を保持し、Wasm が返した patch を適用する

## 推奨 DSL 構造

`.altp-panel` は UI 定義ファイルとし、manifest と view を同じファイル内に持たせる。
処理は別の Wasm モジュールへ分離する。

大枠は次の 5 ブロックで十分である。

1. `panel` ヘッダ
2. `permissions`
3. `runtime`
4. `state`
5. `view`

例:

```text
panel {
  id: "builtin.sample-tools"
  title: "Sample Tools"
  version: 1
}

permissions {
  read.document
  write.command
}

runtime {
  wasm: "sample_tools.wasm"
}

state {
  selectedTool: enum("brush", "eraser") = "brush"
  showAdvanced: bool = false
}

view {
  <column gap=8 padding=8>
    <section title="Tools">
      <row gap=6>
        <button
          id="tool.brush"
          active={state.selectedTool == "brush"}
          on:click="activate_brush"
        >
          Brush
        </button>

        <button
          id="tool.eraser"
          active={state.selectedTool == "eraser"}
          on:click="activate_eraser"
        >
          Eraser
        </button>
      </row>

      <toggle
        id="advanced.toggle"
        checked={state.showAdvanced}
        on:change="toggle_advanced"
      >
        Advanced
      </toggle>

      <when test={state.showAdvanced}>
        <text tone="muted">Extra options will live here.</text>
      </when>
    </section>
  </column>
}
```

## 文法の考え方

### ヘッダ

`panel` ヘッダは manifest を兼ねる。

最低限必要な項目:

- `id`
- `title`
- `version`

将来追加候補:

- `author`
- `description`
- `capabilities`
- `min_host_version`

### runtime

`runtime` は UI 定義に対応する処理モジュールを指す。

例:

```text
runtime {
  wasm: "sample_tools.wasm"
}
```

MVP では 1 panel に対して 1 Wasm module とするのが単純である。

### Wasm 処理モジュール契約

Wasm 側は host が `wasmtime` 上で呼び出せる最小 export 群を持つ。

初期案:

- `panel_init()`
- `panel_handle_event(handler_name, event_payload, state_snapshot, host_snapshot)`
- `panel_dispose()`

`panel_handle_event(...)` の返り値は、少なくとも次を含める。

- state patch
- command descriptor の列
- 必要なら diagnostics

この方式なら、UI DSL は purely declarative に保てる。

### Wasm を直接書かず、Rust 等からコンパイルする前提

実運用では Wasm バイナリを直接手書きするのではなく、Rust などからコンパイルする前提で考えるべきである。

このとき重要なのは、プラグイン作者に何を `import` させるかを慎重に固定することだ。

結論としては、**ホスト内部 crate を直接使わせず、安定した SDK crate を1枚噛ませる**のがよい。

推奨構成:

- `altpaint-panel-sdk`
  - プラグイン作者が直接依存する高水準 SDK
  - handler 定義補助
  - state patch、command descriptor、event payload の型
  - host 呼び出しラッパ
- `altpaint-panel-schema`
  - host と Wasm の間で共有するシリアライズ用 DTO
  - ABI 安定を意識した最小型だけを置く
- `altpaint-panel-macros` ※必要なら
  - `#[panel_handler]` のような宣言マクロ

逆に、次は直接依存させない方がよい。

- `app-core`
- `ui-shell`
- `desktop`
- `plugin-host`

理由は単純である。

- ホスト内部の型変更をプラグイン ABI へ漏らさないため
- `Command` や `Document` の内部表現に密結合させないため
- 将来 Rust 以外の言語バインディングを作りやすくするため

### プラグイン作者が使うべき API の粒度

Rust 側では、次のような高水準 API を使わせるのがよい。

- `EventContext`
- `HostSnapshot`
- `StateValue`
- `StatePatch`
- `CommandDescriptor`
- `HandlerResult`

イメージ:

```rust
use altpaint_panel_sdk::{
    command, HandlerResult, HostSnapshot, PanelEvent, StatePatch,
};

#[no_mangle]
pub fn panel_handle_event(input: &[u8]) -> Vec<u8> {
    let event = altpaint_panel_sdk::decode_event(input).unwrap();

    match event.handler_name.as_str() {
        "activate_brush" => altpaint_panel_sdk::encode_result(&HandlerResult {
            state_patch: StatePatch::set("selectedTool", "brush"),
            commands: vec![command("tool.set_active").string("tool", "brush")],
            diagnostics: vec![],
        }),
        _ => altpaint_panel_sdk::encode_error("unknown handler"),
    }
}
```

重要なのは、ここでも `Command::SetActiveTool` のような host 内部 enum を触らせないことだ。

### command descriptor を SDK で包む

Wasm 側に生の JSON を手組みさせると壊れやすい。

そのため SDK 側で次を提供した方がよい。

- `command("tool.set_active")`
- `.string("tool", "brush")`
- `.color("color", "#1E88E5")`
- `.build()`

これにより、plugin 作者は schema に沿って payload を組み立てやすくなる。

### state patch も SDK で包む

state 更新も文字列ベースの生 patch ではなく、SDK helper を用意する方が安全である。

例:

- `StatePatch::set("selectedTool", "brush")`
- `StatePatch::toggle("showAdvanced")`
- `StatePatch::set_bool("expanded", true)`

## 推奨する ABI と import 方針

MVP では、複雑な host function 群を大量に `import` させるより、**少数の安定 export/import に絞る**のがよい。

### 第一候補

- target: `wasm32-wasip1`
- runtime: `wasmtime`
- 権限: デフォルトでは filesystem / network / env を与えない

これを第一候補にする理由:

- Rust の標準ライブラリを使いやすい
- `wasmtime` で扱いやすい
- 将来 WASI ベースの整理へ寄せやすい

ただし、plugin に OS 権限を素通ししてはいけない。
`wasmtime` 上では capability を明示的に絞る。

### ABI で最初に固定しすぎないこと

MVP では plugin 側の export は少なくてよい。

例:

- `panel_init`
- `panel_handle_event`
- `panel_dispose`

そして入出力は、まずはシリアライズ済み bytes でやり取りするのが単純である。

- input: event, state snapshot, host snapshot
- output: state patch, command descriptors, diagnostics

内部表現は最初は JSON でもよいが、将来的には `postcard` や `rmp-serde` のようなより軽い形式へ移れるよう DTO を分離しておくとよい。

## SDK とホスト API の分離原則

plugin 作者が import するのは、原則として SDK crate だけにする。

つまり次の構図にする。

- plugin author code
  - depends on `altpaint-panel-sdk`
- `altpaint-panel-sdk`
  - depends on `altpaint-panel-schema`
- host side
  - depends on `altpaint-panel-schema`

こうすると、host と plugin が共有するのは schema だけで済む。

これは非常に重要である。

- `app-core` の変更がそのまま plugin 破壊になりにくい
- `plugin-api` は host 内の概念整理に専念できる
- 将来 Rust 以外の language SDK を生やしやすい

## 依存ライブラリの許容方針

Rust から Wasm を作る以上、plugin 作者が crates.io のライブラリを使うこと自体は許容してよい。

ただし、MVP では次の制約を明文化した方がよい。

- `wasm32-wasip1` でビルド可能であること
- ネイティブ動的ライブラリ依存を持たないこと
- host 権限なしでは filesystem / network を前提にしないこと
- 重い計算を UI イベント同期処理へ持ち込まないこと

推奨する plugin 向け依存の例:

- `serde`
- `thiserror`
- `smallvec`
- `indexmap`
- `once_cell` または `std` の同等機能

慎重に扱うべき依存の例:

- スレッド前提 crate
- ネイティブ FFI 前提 crate
- 大きなランタイム依存
- OS ファイルシステムを当然視する crate

## 現実的な進め方

この問題に対しては、いきなり汎用 plugin SDK を広く作るより、まずは最小の公式 Rust SDK を1つ用意するのがよい。

最初に作るもの:

1. `altpaint-panel-schema`
2. `altpaint-panel-sdk`
3. 単一 handler を持つ最小サンプル plugin
4. `wasmtime` 上での読み込みテスト

その後に、必要なら次を追加する。

- macro support
- host call wrappers
- component model / WIT への移行検討

## 結論の補足

Rust などから Wasm を作る前提なら、重要なのは「何を import させるか」であって、「Wasm を書かせるかどうか」ではない。

設計上は次を守るのがよい。

- plugin 作者には host 内部 crate を見せない
- 公式 SDK crate を経由させる
- `Command` や `Document` の内部型を直接触らせない
- DTO と command descriptor を共有境界にする
- `wasmtime` 上で動かす前提で capability を絞る

この方針なら、Rust plugin の開発体験を確保しつつ、ホスト側の ABI と権限モデルも守りやすい。

### permissions

権限は `docs/ARCHITECTURE.md` の方針どおり宣言必須にする。

MVP で使う候補:

- `read.document`
- `read.selection`
- `read.jobs`
- `write.command`
- `write.snapshot`
- `write.export`

DSL 側で `dispatch(...)` を使うなら、少なくとも `write.command` を要求する。

### state

`state` ブロックはローカル UI state 宣言である。

例:

```text
state {
  showAdvanced: bool = false
  selectedTab: enum("layers", "snapshots") = "layers"
  query: string = ""
}
```

ここで重要なのは、state 参照は UI DSL に残し、state 更新は Wasm handler に寄せることである。

- 参照: `state.showAdvanced`
- 更新: Wasm が state patch を返す
- 反転: Wasm が新しい state 値を返す

### view

`view` ブロックは JSX 風構文を使う。

推奨ルール:

- 要素名は小文字固定
- 属性は宣言的に書く
- 子はネストで表す
- イベント属性は `on:*`
- 動的値は `{...}`

MVP ノード属性案:

- `<column gap=8 padding=8 align="stretch">`
- `<row gap=6 wrap=false>`
- `<section title="Colors">`
- `<text tone="muted">Hello</text>`
- `<button active={expr} on:click={actions}>Brush</button>`
- `<toggle checked={expr} on:change={actions}>Visible</toggle>`
- `<separator />`
- `<spacer size=8 />`
- `<when test={expr}>...</when>`
- `<for each={host.layers} item="layer">...</for>` ※導入するなら MVP 後半

## イベントとアクション

イベントは host 側の `PanelEvent` へ最終的に変換されるが、UI DSL では node ごとの handler binding として記述する。

MVP で許可するイベント:

- `on:click`
- `on:change`
- `on:focus`
- `on:blur`

ただし実装順は以下がよい。

1. `on:click`
2. `on:change`
3. フォーカス系

UI DSL 側ではアクション列を持たず、handler 名を bind する。

例:

```text
on:click="activate_brush"
```

Wasm handler が返せる作用候補:

- state patch
- `dispatch(command_name, payload)`
- diagnostics
- `emit(event_name, payload)` ※将来

## host snapshot の扱い

DSL が読む外部データは、document 全体ではなく host 側が渡す読み取り専用 snapshot に制限する。

推奨参照名前空間:

- `host.document.*`
- `host.tool.*`
- `host.layers.*`
- `host.jobs.*`

例:

```text
<text>{host.tool.active_label}</text>
```

この snapshot はフェーズ6で最小限に絞る。

例:

- `host.tool.active`
- `host.document.title`
- `host.layers.items`
- `host.color.active`

重要なのは、DSL が `Document` の Rust 構造そのものを知らないことである。

## PanelTree への落とし込み

フェーズ6以降も UI DSL を直接描画しない。
パーサーと validator の後で、内部の正規化 IR を作り、それを `PanelTree` へ変換する。

推奨する内部段階:

1. Parse AST
2. Validated AST
3. Normalized View IR
4. Runtime panel instance
5. Wasm handler 実行結果と state store を反映
6. `PanelTree` 生成

### なぜ中間 IR が必要か

理由は以下。

- JSX 風構文の糖衣を消せる
- validation エラー位置を扱いやすい
- UI DSL と Wasm を別実装のまま同じ runtime model に寄せやすい
- `Toggle` や `when` を primitive へ展開しやすい

## validation で必ず見ること

MVP で最低限必要な validation は次の通り。

- `panel.id` が存在する
- `panel.version` が対応範囲内
- node `id` が panel 内で一意
- 未知の属性がない
- `state` 参照先が存在する
- `runtime.wasm` が存在する
- `on:*` で参照される handler が Wasm 側に存在する
- `host.*` 参照に対して必要権限がある
- Wasm が返しうる command descriptor の schema が既知
- 禁止ノードが使われていない

validation エラーは reload 時にも出せる必要がある。

## コマンド schema 方針

Wasm から `Command` を安全に発行するには、host 側に command schema registry が必要である。

例:

- `tool.set_active`
  - payload: `{ tool: "brush" | "eraser" }`
- `document.new`
  - payload: `{}`
- `project.save`
  - payload: `{}`
- `project.load`
  - payload: `{}`
- `tool.set_color`
  - payload: `{ color: "#RRGGBB" }`

これにより、UI DSL parser と Wasm runtime のどちらも `app-core::Command` を直接知らずに済む。

## ステート更新モデル

ローカル state を許可するなら、Wasm handler の返却値として更新タイミングを単純に固定した方がよい。

推奨:

- host がイベントを `PanelEvent` に変換する
- 対応する Wasm handler を同期的に実行する
- Wasm が返した state patch を適用する
- 再評価結果から新しい `PanelTree` を作る
- host は差分比較せず毎回再構築でもよい

フェーズ6時点ではパネル規模が小さいため、まずは単純再構築で十分である。

## ホットリロード方針

フェーズ6の価値は再ロードにあるため、開発中はファイル監視を入れたい。

最小方針:

- `*.altp-panel` を読み込む
- 更新時に再 parse / validate する
- 成功時のみ panel 差し替え
- 失敗時は直前の有効版を維持し、エラー panel を表示する

エラー panel に出す情報:

- panel id
- エラー箇所
- 行番号
- メッセージ

## 既存構造への接続案

フェーズ6では次の追加が筋がよい。

- `crates/panel-dsl`
  - parser
  - AST
  - validator
  - normalized IR
- `crates/plugin-host`
  - `wasmtime` 実行基盤
  - Wasm handler 呼び出し
  - state patch / command descriptor 受け渡し
- `plugin-api`
  - command descriptor schema
  - host snapshot schema
  - panel manifest 型
  - local state value 型
- `ui-shell`
  - DSL runtime panel adapter
  - reload 対応
- `apps/desktop`
  - panel file 探索
  - file watch
  - エラー表示

## MVP の実装順

### ステップ1: 読み取り専用 UI DSL

最初は state も command も無しでよい。

- `panel` ヘッダ
- `view`
- `column` / `row` / `section` / `text` / `button`
- static label

これで parser と host 描画接続を固める。

### ステップ2: static handler binding

次に `button` から handler 名を bind できるようにする。

- `on:click`
- handler 名 validation
- まずは host 組み込み handler でもよい

これで既存の `app-actions` と `tool-palette` の DSL 化が見えてくる。

### ステップ3: Wasm runtime 接続

次に handler 実装を `wasmtime` 上の Wasm へ逃がす。

- `runtime { wasm: ... }`
- event -> Wasm handler 呼び出し
- state patch / command descriptor 返却

### ステップ4: local state

その後に `state` と `set` / `toggle` を入れる。

- `bool` / `enum` / `string`
- `active={...}`
- `when`

これで `color-palette` や簡単な設定パネルに足る。

### ステップ5: host snapshot

最後に読み取り専用 host data binding を開く。

- `host.tool.active`
- `host.document.title`
- `host.layers.items`

これで builtin plugin の一部を DSL へ移せる。

## 最小スコープの提案

フェーズ6の最小スコープは次で十分である。

- ファイル形式: `.altp-panel`
- 構文: JSX 風 + 制限付き式
- layout: `column` / `row` / `section`
- atoms: `text` / `button`
- event: `on:click`
- runtime: `wasmtime` 上の Wasm handler
- state: `bool` と `enum` のみ
- host data: `host.tool.active`, `host.document.title` 程度
- reload: 手動またはファイル更新監視

## この方針で得られること

- JSX 風で読みやすい
- UI と処理の責務を分離できる
- host 主導の UI runtime 原則を壊さない
- atomic node だけで始められる
- `Command` 発行経路を壊さず拡張できる
- DSL と Wasm を同じ中間表現へ寄せやすい
- `wasmtime` ベースの plugin-host に自然につながる
- state を許可しつつ document 正本を汚さない

## 結論

フェーズ6の DSL は、次の形に寄せるのがよい。

- 見た目は JSX 風
- UI DSL と処理 Wasm を分離する
- UI 側は制限付き宣言 DSL に留める
- layout は flex 的 primitive に限定
- 操作結果は Wasm が command descriptor を返し、host が `Command` 化する
- state はパネルローカルに限定し、schema は UI、更新は Wasm が担当する
- host snapshot は読み取り専用
- 最終出力は `PanelTree` 相当

この方針なら、現行の `plugin-api` / `ui-shell` の延長で実装でき、フェーズ6の UI DSL とフェーズ7の Wasm ランタイムを一続きの設計として扱える。
