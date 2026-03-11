# Tool Palette

## 概要

Tool Palette は、現在の描画ツールを切り替える最小のビルトインパネルです。

- panel id: builtin.tool-palette
- title: Tools
- 実装: plugins/tool-palette/

## 目的

キャンバス操作と独立に、ホスト側パネルからツール選択を行えることを確認するためのパネルです。

現時点で扱うツールは 4 種類です。

- Pen
- Eraser
- Bucket
- Lasso Bucket

## 内部状態

このプラグインは ToolPaletteSnapshot を持ちます。

保持内容:

- active_tool

update(...) のたびに Document.active_tool を読み取り、現在選択中ツールを追従します。

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Tools
  - Button: Pen
  - Button: Eraser
  - Button: Bucket
  - Button: Lasso Bucket
  - Pen preset summary
    - Prev Pen
    - Next Pen
    - Reload Pens
  - Shortcut settings
    - Button: Keyboard...
    - Capture Pen
    - Capture Eraser
    - Bucket / Lasso Bucket shortcut labels

active_tool と一致するボタンは active=true になります。
ホスト側ではこの active 状態を使って強調表示します。

## 発行する HostAction

- Pen → Command::SetActiveTool { tool: ToolKind::Pen }
- Eraser → Command::SetActiveTool { tool: ToolKind::Eraser }
- Bucket → paint plugin による flood fill を使うツールへの切替 command
- Lasso Bucket → paint plugin による lasso fill を使うツールへの切替 command
- Prev/Next Pen → ペンプリセット切替 command
- Reload Pens → `pens/` 再読込 command
- keyboard handler → `config.*` の shortcut と一致したら対応ツール切替 command を発行

## view() での補助表示

簡易行表示では、現在選択中ツールに > マーカーを付けます。

例:

- > [P] Pen
-   [E] Eraser
-   [G] Bucket
-   [Shift+G] Lasso Bucket

## 既知の制約

- ペン一覧はまだ Prev/Next 切替のみです
- ショートカット表示は補助テキストであり、パネル自身がキーバインド解決を持つわけではありません
- 幅の詳細調整は `builtin.pen-settings` 側で行います

補足: 現時点では Pen / Eraser をキャプチャ対象にしつつ、Bucket / Lasso Bucket も設定値として保持します。

## 今後の拡張候補

- 選択ツール群
- 塗りつぶしや移動ツール
- ツールカテゴリ分割
