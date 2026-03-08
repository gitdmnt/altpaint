# Snapshot Panel

## 概要

Snapshot Panel は、将来のスナップショット機能に向けた読み取り専用のビルトインパネルです。

- panel id: builtin.snapshot-panel
- title: Snapshots
- 実装: plugins/snapshot-panel/

## 目的

将来のスナップショット永続化や分岐UIの受け皿となるパネル枠を、先にホストランタイム上へ移植しておくことが目的です。

## 内部状態

このプラグインは SnapshotPanelSnapshot を持ちます。

保持内容:

- work_title
- page_count
- panel_count
- active_tool

update(...) のたびに Document からこれらを再計算します。

## 現在のUI構造

PanelTree 上では 2 つの Section を持ちます。

- Section: Current
  - Text: work: ...
  - Text: pages: ... / panels: ...
  - Text: current tool: ...
- Section: Status
  - Text: snapshot storage: pending

## 表示の意味

このパネルは、まだ実スナップショット一覧を持たず、現ドキュメントの状態要約だけを表示します。
Status セクションの pending は、保存基盤未接続であることを示すプレースホルダです。

## HostAction

このパネルは現在 HostAction を発行しません。
完全に読み取り専用です。

## 既知の制約

- スナップショット作成操作はありません
- スナップショット一覧はありません
- 分岐、比較、復元はありません
- 永続化形式とも未接続です

## 今後の拡張候補

- スナップショット作成
- 一覧表示
- 比較プレビュー
- 復元
- コマ単位またはページ単位の履歴導線
