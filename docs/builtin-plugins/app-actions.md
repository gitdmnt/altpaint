# App Actions

## 概要

App Actions は、アプリ全体に対する基本操作を並べる最小のビルトインパネルです。

- panel id: builtin.app-actions
- title: App
- 実装: plugins/app-actions/

## 目的

このパネルの役割は、キャンバス外からアプリ操作を発行することです。

現在は次の3操作だけを提供します。

- New
- Save
- Load

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Project
  - Button: New
  - Button: Save
  - Button: Load

各ボタンはホスト描画され、クリックまたはキーボードフォーカスから活性化できます。

## 発行する HostAction

各ボタンは次の Command を発行します。

- New → Command::NewDocument
- Save → Command::SaveProject
- Load → Command::LoadProject

実際の副作用処理は desktop 側の execute_command(...) が担当します。

## update() の振る舞い

このプラグインは Document の読取状態を保持しません。
update(...) は現在 no-op です。

## 既知の制約

- 保存先は現状固定パスです
- ファイルダイアログはありません
- 実行中ジョブ確認や確認ダイアログはありません
- ボタン活性状態の切替はまだありません

## 今後の拡張候補

- 名前を付けて保存
- 最近使ったファイル
- 終了時の未保存確認
- プロジェクトメタデータ表示
