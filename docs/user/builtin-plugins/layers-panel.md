# Layers Panel

## 概要

Layers Panel は、現在のドキュメント概要とアクティブレイヤー状態を表示し、最小のレイヤー操作も提供するビルトインパネルです。

- panel id: builtin.layers-panel
- title: Layers
- 実装: plugins/layers-panel/

## 目的

レイヤー系パネルをホスト描画できること、そしてフェーズ9の最小レイヤー操作を `Command` 経由で通せることを確認するための実装です。

## 内部状態

このプラグインは LayersPanelSnapshot を持ちます。

保持内容:

- work_title
- page_count
- panel_count
- active_panel_layer_name

update(...) のたびに Document からこれらを再計算します。

## 現在のUI構造

PanelTree 上では 2 つの Section を持ちます。

- Section: Document
  - Text: work: ...
  - Text: pages: ...
  - Text: panels: ...
  - Text: layer count
- Section: Active Layer
  - Text: layer: ...
  - Text: active index
  - Text: blend mode
  - Text: visible
  - Text: mask
  - Button: Add Layer
  - Button: Next Layer
  - Button: Cycle Blend
  - Button: Toggle Visibility
  - Button: Toggle Mask

## 表示の意味

- work_title は作品タイトルです
- page_count は Work 配下のページ数です
- panel_count は全ページ合計のコマ数です
- active_panel_layer_name は、現実装では先頭ページ・先頭コマの root_layer.name を指します

## 既知の制約

- 並べ替え、削除、名前変更はまだありません
- 高度なレイヤーツリーではなく線形な最小レイヤー列です
- マスクはデモ用の最小マスク切替です
- 合成モードは normal / multiply / screen / add の循環だけです

## 今後の拡張候補

- レイヤー一覧表示
- 可視/非可視トグル
- ロック状態
- レイヤー追加、削除、並べ替え
- 複数選択
