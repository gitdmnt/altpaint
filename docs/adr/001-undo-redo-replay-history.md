# ADR-001: Undo/Redo 基盤 — replay 方式・CommandHistory 設計

- 作業日時: 2026-03-13
- 作業 Agent: claude-sonnet-4-6

## 背景

フェーズ7 の最優先課題として、描画操作の Undo/Redo 基盤を構築する必要があった。

## 決定事項

### 1. replay 方式を採用する（before-bitmap 保持なし）

**採用した方式**: undo 時に対象レイヤーを透明にリセットし、past に残る同 panel/layer の
全 `BitmapEditRecord` を順番に replay して前状態を再構築する。

**却下した方式**: before-bitmap 方式（操作前のビットマップをスナップショット保存し、
undo 時にそのまま差し戻す）。

**理由**:
- before-bitmap 方式はメモリ消費が大きい（1操作ごとに全レイヤービットマップを保持）
- replay 方式は記録サイズが小さい（座標・筆圧・ペン設定のみ）
- replay コストが問題になる場合は 7-4 の snapshot 基盤（チェックポイント）で対処予定

### 2. `BitmapEditRecord` を `app-core/src/painting.rs` に定義する

**理由**: 当初 `canvas/src/edit_record.rs` に定義しようとしたが、
`CommandHistory` が `app-core/src/history.rs` に属する型であり、
`app-core → canvas` という依存逆流が発生するため。

依存方向制約: `canvas → app-core` のみ許可。

`canvas/src/edit_record.rs` は re-export のみとし、型本体は `app-core` に置く。

### 3. `BitmapEditRecord` に pen/color/tool スナップショットを保持する

replay 時に current document 状態（現在の選択ペン・色・ツール）ではなく、
操作時の状態を正確に再現するために、`BitmapEditRecord` に以下を保持する:
- `pen_snapshot: PenPreset` — ペン設定（サイズ・圧力感度等）のコピー
- `color_snapshot: ColorRgba8` — 描画色のコピー
- `tool_id: String` — 使用ツール ID

### 4. `execute_undo()`/`execute_redo()` は `execute_document_command` を経由しない

`Command::Undo`/`Command::Redo` は `execute_command` から直接 `execute_undo()`/`execute_redo()`
へ委譲する。`execute_document_command` を経由すると `document.apply_command` が呼ばれるが、
Undo/Redo は document state delta ではなく desktop orchestration 層の責務であるため。

### 5. `PanelPlugin::update` のシグネチャに `can_undo`/`can_redo` を追加する

host snapshot に履歴状態を反映するため、`update` のシグネチャを変更:
```
fn update(&mut self, document: &Document, can_undo: bool, can_redo: bool)
```

`CommandHistory` は `DesktopApp` に属するため、`PanelRuntime` の `sync_document` を通じて渡す。

## 結果

- `crates/app-core/src/history.rs` — `CommandHistory`・`HistoryEntry` 実装 + テスト
- `crates/app-core/src/painting.rs` — `BitmapEditOperation`・`BitmapEditRecord` 追加
- `crates/canvas/src/runtime.rs` — `PaintResult`・`replay_paint_record` 追加
- `apps/desktop/src/app/mod.rs` — `history: CommandHistory` フィールド追加
- `apps/desktop/src/app/services/mod.rs` — `execute_undo()`・`execute_redo()` 実装
- `crates/panel-runtime/src/host_sync.rs` — host snapshot に `"history"` キー追加
- `plugins/app-actions` — undo/redo ボタン追加

## トレードオフ

- replay コストは操作数に比例するため、多数の操作後の undo は遅くなる可能性がある
  → 7-4 の snapshot 基盤（チェックポイント bitmap）で対処する
- `PanelPlugin::update` のシグネチャ変更は trait の破壊的変更だが、
  外部クレートへの公開インターフェースでないため問題なし
