# altpaint パネル UI DSL 設計

## この文書の役割

この文書は、フェーズ6で導入するパネル UI DSL の正本である。

対象は次である。

- `.altp-panel` のファイル構造
- JSX 風構文の範囲
- state schema
- handler binding
- host snapshot 参照
- validation
- EBNF

処理実装、Wasm ABI、SDK 方針は [docs/panel-ui-definition/wasm-runtime.md](docs/panel-ui-definition/wasm-runtime.md) を正本とする。

## 2026-03-09 時点の実装状況

フェーズ6は、この文書で定義した最小スコープについては完了している。

- `.altp-panel` の parser / validator / normalized IR は実装済み
- `runtime { wasm: ... }` と handler binding は実装済み
- `ui-shell` は DSL panel をロードし、`PanelTree` へ正規化して表示できる
- `plugins/phase6-sample/panel.altp-panel` で sample panel の表示と操作を確認できる

一方で、より広い式評価、追加 widget、外部 plugin 向け権限本格化はフェーズ7以降の作業である。

## 結論

`altpaint` の UI 側は、**JSX 風の制限付き DSL** として定義する。

固定したい点は次である。

- UI は declarative に記述する
- layout は flex 的 primitive に限定する
- UI DSL には処理記述を埋め込まない
- イベント時は handler 名だけを bind する
- state はパネルローカル state の schema 宣言に留める
- host が最終的に `PanelTree` を生成する

## UI DSL の責務

UI DSL は `.altp-panel` で表現する。

持つ責務:

- パネルの manifest
- レイアウト構造
- atomic node の構成
- state schema
- host snapshot 参照
- handler 名の binding

持たない責務:

- 任意コード実行
- 複雑な state 更新ロジック
- host API 直接呼び出し
- `wgpu` / OS / native widget 直接操作
- command descriptor の組み立て

## UI ファイル形式

UI 定義ファイルは `.altp-panel` とする。

最小構造は次の5ブロックでよい。

1. `panel`
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

## JSX 風 DSL の範囲

### 採るもの

- JSX 風のネスト構文
- 小文字タグ
- 属性による宣言的指定
- `{...}` による制限付き式
- `on:*` による handler binding

### 制限するもの

- 任意関数定義
- 汎用スクリプト構文
- 再帰コンポーネント
- host 内部型へのアクセス
- UI DSL 内での command 構築

## MVP の node

- `column`
- `row`
- `section`
- `text`
- `button`
- `toggle`
- `separator`
- `spacer`
- `when`

必要なら後で追加:

- `list`
- `for`
- `input`
- `select`

## 既存ビルトイン移植を前提にした必要機能

この DSL は、次の次の段階で既存ビルトインプラグインを移植する前提で詰める。

対象 panel:

- `app-actions`
- `tool-palette`
- `color-palette`
- `layers-panel`
- `job-progress`
- `snapshot-panel`

この前提から、MVP の次に必要になる UI 機能を先に固定しておく。

### 移植初期に必須の機能

- `section` / `row` / `column`
- `text`
- `button`
- `toggle`
- `active` 表示
- `disabled` 表示
- `on:click`
- `on:change`

### ビルトイン移植時に追加で必要になる機能

- ボタン塗り色
  - `color-palette` 用
- 反復表示
  - `layers-panel`、`snapshot-panel` 用
- 空状態表示
  - 読み取り専用 panel 用
- 読み取り専用リスト行
  - `job-progress`、`snapshot-panel` 用

このため、`list` と `for` は「後で追加」ではあるが、ビルトイン移植前には導入する前提で考える。

## ビルトイン移植向け node / 属性拡張方針

既存 panel を UI DSL へ移すため、次の拡張を優先候補とする。

### `button`

追加候補属性:

- `active={expr}`
- `disabled={expr}`
- `fill={expr}`
- `variant="default" | "primary" | "danger" | "color"`

### `text`

追加候補属性:

- `tone="normal" | "muted" | "accent" | "danger"`
- `weight="normal" | "bold"`

### `list`

`layers-panel` や `snapshot-panel` を見据え、最終的には次のどちらかを導入する。

- `<for each={host.layers.items} item="layer"> ... </for>`
- `<list items={host.layers.items}> ... </list>`

MVP の実装容易性を優先するなら、まずは `for` の方が単純である。

## state モデル

state はパネルローカル state に限定する。

MVP の型候補:

- `bool`
- `int`
- `float`
- `string`
- `enum`
- `color`

重要な原則:

- document 正本とは分離する
- document 変更は常に `Command` 経由にする
- UI は state schema を宣言するだけに留める
- 更新ルールは Wasm handler が返す

責務分担:

- UI DSL: state 名、型、初期値
- Wasm: state patch の生成
- host: state store の適用と再評価

## イベント処理モデル

UI DSL は handler 名だけを bind する。

例:

```text
<button on:click="activate_brush">Brush</button>
```

処理の流れ:

1. host が入力を `PanelEvent` に変換する
2. 対応する handler 名を UI 定義から引く
3. host が Wasm handler を呼ぶ
4. Wasm が state patch / command descriptors / diagnostics を返す
5. host が state を適用し、必要なら `Command` に変換する
6. host が panel を再評価し `PanelTree` を更新する

UI DSL 側には処理記述を入れない。

## host snapshot と runtime 参照

UI DSL は host 側が渡す読み取り専用 snapshot を参照する。

想定名前空間:

- `host.document.*`
- `host.tool.*`
- `host.layers.*`
- `host.jobs.*`

MVP では最小限に絞る。

- `host.tool.active`
- `host.document.title`
- `host.layers.items`

ビルトイン移植前には、少なくとも次が必要になる。

- `host.color.active`
- `host.layers.active_id`
- `host.layers.items[*].id`
- `host.layers.items[*].label`
- `host.jobs.items`
- `host.snapshots.items`

また、`runtime` ブロックでは対応する Wasm module を指す。

```text
runtime {
  wasm: "sample_tools.wasm"
}
```

## 正規化と内部段階

UI DSL を直接描画しない。

内部段階は次でよい。

1. Parse AST
2. Validated AST
3. Normalized View IR
4. Runtime panel instance
5. Wasm 実行結果反映
6. `PanelTree` 生成

## EBNF

MVP を説明するための簡略 EBNF は次とする。

```text
panel-file      = panel-block, permissions-block, runtime-block, state-block, view-block ;
panel-block     = "panel", "{", panel-field*, "}" ;
panel-field     = "id", ":", string
                | "title", ":", string
                | "version", ":", integer ;

permissions-block = "permissions", "{", permission*, "}" ;
permission        = identifier, ".", identifier ;

runtime-block   = "runtime", "{", "wasm", ":", string, "}" ;

state-block     = "state", "{", state-decl*, "}" ;
state-decl      = identifier, ":", state-type, "=", literal ;
state-type      = "bool"
                | "int"
                | "float"
                | "string"
                | "color"
                | enum-type ;
enum-type       = "enum", "(", string, { ",", string }, ")" ;

view-block      = "view", "{", node, "}" ;
node            = element | when-element ;
when-element    = "<when", "test=", expr, ">", node*, "</when>" ;

element         = start-tag, node*, end-tag
                | empty-tag ;
start-tag       = "<", tag-name, attribute*, ">" ;
end-tag         = "</", tag-name, ">" ;
empty-tag       = "<", tag-name, attribute*, "/>" ;

tag-name        = "column"
                | "row"
                | "section"
                | "text"
                | "button"
                | "toggle"
                | "separator"
                | "spacer" ;

attribute       = identifier, "=", attr-value ;
attr-value      = string | integer | boolean | expr ;
expr            = "{", expr-body, "}" ;
expr-body       = literal
                | state-ref
                | host-ref
                | comparison-expr
                | logical-expr ;

state-ref       = "state", ".", identifier ;
host-ref        = "host", ".", identifier, { ".", identifier } ;
comparison-expr = expr-atom, ("==" | "!="), expr-atom ;
logical-expr    = expr-atom, ("&&" | "||"), expr-atom ;
expr-atom       = literal | state-ref | host-ref ;

literal         = string | integer | boolean ;
boolean         = "true" | "false" ;
identifier      = letter, { letter | digit | "_" | "-" } ;
```

これは parser 実装用の最終仕様ではなく、MVP の構文境界を共有するための説明用定義である。

## validation で見ること

最低限必要な validation:

- `panel.id` がある
- `panel.version` が対応範囲内
- `runtime.wasm` がある
- node `id` が一意
- 未知属性がない
- `state.*` 参照先が存在する
- `host.*` 参照に必要権限がある
- `on:*` の handler 名が解決できる

ビルトイン移植前には次も確認対象にする。

- `for` / `list` の参照先が配列型である
- `fill` 属性が `color` または host 側色型へ解決できる
- `active` / `disabled` が `bool` へ解決できる

Wasm の戻り値 schema と SDK 方針は [docs/panel-ui-definition/wasm-runtime.md](docs/panel-ui-definition/wasm-runtime.md) で扱う。

## 次の段階で実装するもの

次の段階では、まず DSL 基盤そのものを成立させる。

- `crates/panel-dsl` の追加
- lexer / parser / AST / validator
- `.altp-panel` のロード / 再ロード
- static panel の描画
- handler binding の解決

この段階では、既存ビルトインの全面移植はまだ行わない。

## その次の段階で移植するもの

その次の段階で、既存ビルトインを新基盤へ移植する。

### 優先順

1. `app-actions`
2. `tool-palette`
3. `color-palette`
4. `layers-panel`
5. `job-progress`
6. `snapshot-panel`

### 移植完了の目安

- UI が `.altp-panel` から構築される
- handler が Wasm module から実行される
- host snapshot 参照で現状態を表示できる
- 既存 Rust 実装と同等の `Command` 発行結果を確認できる

## フェーズ別の実装順

### フェーズ6前半

- `.altp-panel` parser
- JSX 風 UI DSL
- static panel 表示
- `column` / `row` / `section` / `text` / `button`

この範囲は実装済みである。

### フェーズ6後半

- `runtime { wasm: ... }`
- handler binding
- Wasm handler 実行との接続
- state patch 反映

この範囲も sample panel まで含めて実装済みである。

## MVP スコープ

MVP では次で十分である。

- ファイル形式: `.altp-panel`
- UI: JSX 風制限 DSL
- handler: `on:click`, `on:change`
- state: `bool`, `enum`, `string`
- host snapshot: `host.tool.active`, `host.document.title` 程度
- reload: 手動または file watch

## 結論

UI 側は、**JSX 風 DSL に責務を限定する**のがよい。

- UI は JSX 風 DSL
- 処理は別文書の Wasm runtime に分離
- host が `PanelTree` へ正規化する
- state は schema 宣言に留める
- handler binding までを UI 文書の責務とする

この方針であれば、フェーズ6の DSL 導入を小さく保ちつつ、後続の Wasm runtime に自然に接続できる。
