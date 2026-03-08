# Layers Panel

## 概要

Layers Panel は、現在のドキュメント概要とアクティブレイヤー名を表示する読み取り専用のビルトインパネルです。

- panel id: builtin.layers-panel
- title: Layers
- 実装: plugins/layers-panel/

## 目的

レイヤー系パネルをホスト描画できること、そしてパネルが Document を読んで自分の表示状態を構築できることを確認するための最小実装です。

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
- Section: Active Layer
  - Text: layer: ...

## 表示の意味

- work_title は作品タイトルです
- page_count は Work 配下のページ数です
- panel_count は全ページ合計のコマ数です
- active_panel_layer_name は、現実装では先頭ページ・先頭コマの root_layer.name を指します

## 既知の制約

- 読み取り専用です
- レイヤーの追加、削除、並べ替えはできません
- 複数レイヤーツリーや選択中レイヤー切替はまだありません
- active layer は実質的に最初の root layer 固定です

## 今後の拡張候補

- レイヤー一覧表示
- 可視/非可視トグル
- ロック状態
- レイヤー追加、削除、並べ替え
- 複数選択
