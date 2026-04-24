# ADR-005: GPU キャンバス Phase 8D — CPU-GPU 通信最小化

- 作業日時: 2026-04-25
- 作業モデル: claude-opus-4-7

## ステータス

採択

## 背景

Phase 8B/8C で GPU compute shader ディスパッチと GPU テクスチャ直接表示が実装された。しかし以下の非効率が残っていた:

- Stamp/StrokeSegment のたびに GPU dispatch と CPU bitmap 書き込みを並行実施
- Undo/Redo のたびに `sync_all_layers_to_gpu()` が全レイヤー CPU→GPU 全転送
- FloodFill 後に GPU テクスチャが未同期（既存バグ）

CPU-GPU 往復を最小化し、GPU テクスチャを描画時の正本としたい。

## 決定内容

GPU feature 有効時の通信パスを以下に再構成する:

| タイミング | 通信 |
|---|---|
| ストローク中（Stamp/StrokeSegment） | GPU 通信ゼロ（compute shader のみ、CPU bitmap 書き込みスキップ） |
| ストローク commit | CPU→GPU 1 回（before スナップショット）+ GPU→GPU 1 回（after スナップショット） |
| Undo（GpuBitmapPatch） | GPU→GPU のみ（`restore_region` で小テクスチャ → レイヤー） |
| Redo（GpuBitmapPatch） | GPU→GPU のみ |
| Undo/Redo（BitmapPatch、レガシー） | `upload_region` で dirty 領域のみ GPU 同期（全量転送を廃止） |
| FloodFill/LassoFill | CPU apply 後に dirty 領域 CPU→GPU 1 回 |
| プロジェクト保存 | GPU→CPU 全レイヤー readback 1 回 |

実装の骨子:

1. `GpuCanvasPool` に 5 メソッド追加: `snapshot_region`（GPU→GPU）、`restore_region`（GPU→GPU）、`upload_region`（CPU→GPU 部分）、`read_back_full`（GPU→CPU）、`create_and_upload`（CPU→GPU 新規テクスチャ）
2. `app-core` に型消去ラッパー `OpaqueGpuData(Arc<dyn Any + Send + Sync>)` と `HistoryEntry::GpuBitmapPatch` variant を追加。`app-core` は wgpu 非依存を維持
3. desktop 層に `GpuPatchSnapshot { before: wgpu::Texture, after: wgpu::Texture }`（dirty 領域サイズ）を定義し、履歴へ格納
4. `PendingStroke.before_layer` を `Option<CanvasBitmap>` に変更。GPU パスでは `None`（CPU bitmap が書き換わらないためストローク前状態を保持している）
5. 保存前に `sync_gpu_bitmaps_to_cpu` を呼び、GPU テクスチャを CPU bitmap に読み戻してから `document.clone()` を行う

## 代替案

### A: ストローク開始時に GPU 全レイヤーバックアップ（CPU↔GPU ゼロ）

- stroke start に GPU→GPU で全レイヤーを backup_tex へコピー → commit で dirty 抽出
- 長所: CPU↔GPU 通信完全ゼロ
- 短所: ストローク開始毎に全レイヤーの GPU→GPU コピー（VRAM 帯域コスト）。dirty が小さい通常ストロークでは採択案（dirty 領域 CPU→GPU 1 回）の方が総転送量が少ない

### B: 従来方式（CPU と GPU を並行更新）を継続

- 短所: Phase 8B/8C の動機に反する。CPU bitmap 書き込みが GPU 描画と同じ頻度で走り続ける

## トレードオフ

- **VRAM 使用量の累積**: 履歴容量 (50) × 2 テクスチャ × dirty 領域サイズ分 GPU メモリを消費する。大規模キャンバスでのストローク多数時に要観察。将来的には TTL や LRU 圧縮を検討
- **保存時のレイテンシ**: `read_back_full` は `device.poll(Wait)` で GPU→CPU を同期的に待つ。大きなレイヤー × 複数で保存時間が伸びる可能性あり（Phase 8E 以降で非同期化検討）
- **feature gate 複雑化**: `#[cfg(feature = "gpu")]` の分岐が `execute_paint_input` / `commit_stroke_to_history` / `execute_undo` / `execute_redo` / `enqueue_save_project` に広がる。GPU 無効時も従来 CPU パスで動作することを CI でカバーする

## 影響範囲

- `crates/gpu-canvas` — `GpuCanvasPool` に 5 メソッド追加
- `crates/app-core/history.rs` — `OpaqueGpuData` 型と `GpuBitmapPatch` variant 追加
- `apps/desktop` — `execute_paint_input` / `commit_stroke_to_history` / `execute_undo` / `execute_redo` / `enqueue_save_project` の変更、`services/gpu_sync.rs` 新設
