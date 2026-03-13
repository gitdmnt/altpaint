# 実装状況差分メモ (2026-03-13)

## 比較対象

- 目標: `docs/ARCHITECTURE.md`
- 現状: `docs/CURRENT_ARCHITECTURE.md`
- 前回記録: `docs/IMPLEMENTATION_STATUS.md`（2026-03-12 時点）

## フェーズ完了状況

| フェーズ | 内容 | 状態 |
|----------|------|------|
| 0 | 境界の固定と作業前提の統一 | 完了 |
| 1 | `desktopApp` の縮小 | 完了 |
| 2 | `canvas` 層の新設 | 完了 |
| 3 | panel runtime / presentation 分離 | 完了 |
| 4 | plugin-first 化の本格化 | 完了 |
| 5 | `render` 中心の画面生成整理 | 完了 |
| 6 | API 名称と物理配置の整理 | 完了 |
| 7 | 再編後の機能拡張 (Undo/Redo 着手) | **進行中** |

## フェーズ7 の進行状況（2026-03-13 時点）

### 実装済み

- `crates/app-core/src/history.rs` 新設
  - `CommandHistory`（push / undo / redo / clear / past_entries）
  - `HistoryEntry::BitmapOp(BitmapEditRecord)`
  - `DEFAULT_HISTORY_CAPACITY = 50`
  - 全操作の単体テスト（push_and_undo_round_trip / undo_then_redo / push_clears_future / capacity_evicts_oldest / clear_empties_stacks）

- `crates/app-core/src/painting.rs` 追加型
  - `BitmapEditOperation`（Stamp / StrokeSegment / FloodFill / LassoFill）
  - `BitmapEditRecord`（panel_id / layer_index / operation / pen_snapshot / color_snapshot / tool_id）
  - `BitmapEditOperation::from_paint_input` / `to_paint_input` 変換

- `crates/app-core/src/lib.rs`
  - `pub mod history;` 追加
  - `CommandHistory`, `DEFAULT_HISTORY_CAPACITY`, `HistoryEntry` を re-export
  - `BitmapEditOperation`, `BitmapEditRecord` を re-export

- `crates/canvas/src/edit_record.rs` 新設
  - `app_core::BitmapEditOperation` / `BitmapEditRecord` の re-export

- `crates/canvas/src/lib.rs`
  - `pub mod edit_record;` 追加
  - `BitmapEditOperation`, `BitmapEditRecord` を re-export

- `crates/panel-api/src/services.rs`
  - `HISTORY_UNDO` / `HISTORY_REDO` service 名追加
  - `SNAPSHOT_CREATE` / `SNAPSHOT_RESTORE` service 名追加
  - `EXPORT_IMAGE` service 名追加

- `crates/plugin-sdk/src/services.rs`
  - `history::undo()` / `history::redo()` descriptor builder 追加
  - `snapshot::create()` / `snapshot::restore()` descriptor builder 追加
  - `export_image::export()` descriptor builder 追加

### 未実装（フェーズ7残項目）

- `canvas::CanvasRuntime` への undo 接続
  - `HISTORY_UNDO` service request を受けて `CommandHistory::undo()` → replay する経路
  - `CanvasRuntime` が `CommandHistory` を所有 or 参照する設計
- `apps/desktop` 側の service handler 追加
  - `HISTORY_UNDO` / `HISTORY_REDO` を `command_router` または `services/` で処理
- export job と snapshot handler（フェーズ7-3 / 7-4）
- tool child 構成 / text-flow / 高度な tool plugin 構成

## 目標アーキテクチャとの残差

### crate 配置

目標と現在の crate 配置は一致している。追加・移動が必要な crate は現時点でなし。

### 責務の残差

| 集中箇所 | 残課題 |
|----------|--------|
| `DesktopApp` | panel/runtime 橋渡しと orchestration がまだ大きい |
| `Document` | tool / pen runtime state をまだ広く持っている |
| `canvas::CanvasRuntime` | tool 実行が host 主導（plugin 主導への移行は未着手） |
| Undo/Redo | `CommandHistory` 基盤はできたが canvas への接続が未実装 |

### 目標 runtime flow との差分

目標の「canvas 入力」フローでは「tool plugin が差分を生成する」とあるが、現在は `canvas::CanvasRuntime` が built-in plugin を呼ぶ形で host 主導のままである。tool 実行 plugin 化はフェーズ7以降の候補タスク。
