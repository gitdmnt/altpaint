# Tool Palette

## 概要

Tool Palette は、現在の描画ツールを切り替える最小のビルトインパネルです。

- panel id: builtin.tool-palette
- title: Tools
- 実装: plugins/tool-palette/

## 目的

キャンバス操作と独立に、ホスト側パネルからツール選択を行えることを確認するためのパネルです。

現時点で扱うツールは 2 種類です。

- Brush
- Eraser

## 内部状態

このプラグインは ToolPaletteSnapshot を持ちます。

保持内容:

- active_tool

update(...) のたびに Document.active_tool を読み取り、現在選択中ツールを追従します。

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Tools
  - Button: Brush
  - Button: Eraser

active_tool と一致するボタンは active=true になります。
ホスト側ではこの active 状態を使って強調表示します。

## 発行する HostAction

- Brush → Command::SetActiveTool { tool: ToolKind::Brush }
- Eraser → Command::SetActiveTool { tool: ToolKind::Eraser }

## view() での補助表示

簡易行表示では、現在選択中ツールに > マーカーを付けます。

例:

- > [B] Brush
-   [E] Eraser

## 既知の制約

- ツール数は 2 つだけです
- ショートカット表示は補助テキストであり、パネル自身がキーバインド解決を持つわけではありません
- ブラシサイズや不透明度などの詳細パラメータはまだありません

## 今後の拡張候補

- ブラシ設定
- 選択ツール群
- 塗りつぶしや移動ツール
- ツールカテゴリ分割
