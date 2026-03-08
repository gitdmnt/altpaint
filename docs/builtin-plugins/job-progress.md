# Job Progress

## 概要

Job Progress は、将来の非同期ジョブ基盤に先立って置かれている読み取り専用のビルトインパネルです。

- panel id: builtin.job-progress
- title: Jobs
- 実装: crates/builtin-plugins/src/job_progress.rs

## 目的

フェーズ5時点で標準パネル5種を揃えることと、今後の jobs クレート接続先を先に確保することが主目的です。

## 内部状態

このプラグインは JobProgressSnapshot を持ちます。

保持内容:

- active_jobs
- queued_jobs
- status_line

現実装では jobs クレートが未導入のため、snapshot は暫定値です。

- active_jobs = 0
- queued_jobs = 0
- status_line = idle / work=<title>

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Queue
  - Text: active: ...
  - Text: queued: ...
  - Text: status: ...

## update() の振る舞い

Document.work.title を読み、status_line に反映します。
それ以外のジョブ数は固定値です。

## HostAction

このパネルは現在 HostAction を発行しません。
完全に読み取り専用です。

## 既知の制約

- 実ジョブ管理とは未接続です
- 進捗率、失敗状態、キャンセル操作はありません
- ジョブ履歴や詳細表示はありません

## 今後の拡張候補

- jobs クレートとの接続
- 進捗バー
- キャンセル
- エラー詳細
- 書き出しジョブやサムネイル生成ジョブの一覧
