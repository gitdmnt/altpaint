# GPU 移行計画: Phase 8A–8E（CPU→GPU 完全移行）

> 作成: 2026-04-19  
> 作成モデル: claude-sonnet-4-6

---

## 背景と目的

altpaint は現在、キャンバス編集（ブラシ合成・塗りつぶし等）を CPU の `Vec<u8>` で行い、フレームごとに `queue.write_texture` で GPU へアップロードするアーキテクチャを持つ。`RENDERING-ENGINE.md` が定める目標原則は「**キャンバスへの描画は必ず GPU を使って行う。編集中のビットマップ CPU→GPU 転送は禁止**」であり、この原則を実現するための移行計画が Phase 8A–8E である。

---

## 現在地（2026-04-25 時点）

| フェーズ | 状態 | 内容 |
|---------|------|------|
| 8A | ✅ 完了 (2026-03-16) | `crates/gpu-canvas` 新設。`GpuCanvasContext` / `GpuLayerTexture` / `GpuCanvasPool` / `GpuPenTipCache` / `supports_rgba8unorm_storage` |
| 8B | ✅ 完了 (2026-04-25) | `install_gpu_resources` / `sync_all_layers_to_gpu` 実装。`GpuBrushDispatch` が実テクスチャに書き込む状態へ到達 |
| 8C | ✅ 完了 (2026-04-25) | `CanvasLayerSource::Gpu` 追加。単一レイヤー時に GPU テクスチャを表示の正本として使用（sRGB view 経由）。複数レイヤー時は CPU パス継続 |
| 8D | ✅ 完了 (2026-04-25) | CPU ペイントパスの廃止（GPU 経路で `apply_bitmap_edits` スキップ、Undo/Redo を `GpuBitmapPatch` 化） |
| 8E | ✅ 完了 (2026-04-25) | GPU 塗りつぶし + 多レイヤー合成 + `BlendMode::Custom` 削除 |

### Phase 8B の現状詳細

`apps/desktop/src/app/services/project_io.rs` の `execute_paint_input` では、stroke/stamp 操作時に `#[cfg(feature = "gpu")]` ブロックで `gpu_brush.dispatch_stroke` を呼び出した**後**、`self.apply_bitmap_edits(edits)` で CPU ビットマップ更新も実行している。

```
GPU dispatch → (CPU apply_bitmap_edits) → 表示は CPU bitmap を read_texture でアップロード
```

表示経路（`wgpu_canvas.rs`）はまだ CPU bitmap の `queue.write_texture` を使用しており、GPU テクスチャは表示に接続されていない。

---

## Phase 8C: GPU テクスチャを表示の正本へ切り替え

### 目的

`GpuCanvasPool` に保持されている `GpuLayerTexture` を、`wgpu_canvas.rs` のキャンバス quad 描画の入力テクスチャとして使う。CPU 側の canvas bitmap アップロード（`canvas_upload`）を除去する。

### 実装概要

1. **`crates/gpu-canvas`**: `GpuCanvasPool::get_view(panel_id, layer_index)` — `TextureView` を返す API を追加する
2. **`apps/desktop/src/wgpu_canvas.rs`**: `canvas_layer` の texture を `GpuCanvasPool` から取得するよう変更する
   - `Self::upload_layer(... canvas_layer.upload_region ...)` の呼び出しを除去する
   - `WgpuPresenter::present` のシグネチャまたは `PresentScene` に `canvas_pool: Option<&GpuCanvasPool>` を追加する
3. **`apps/desktop/src/app/`**: `present.rs` / `present_state.rs` から canvas bitmap の `upload_region` 計算を除去する
4. **レイヤー合成の暫定措置**: Phase 8C 時点では多レイヤー GPU 合成は実装しない。複数レイヤーが存在する場合は CPU compose で合成したテクスチャを `queue.write_texture` で GPU に送る（従来方式）を継続し、アクティブレイヤーへの GPU ブラシ描画のみ GPU テクスチャ経由にする。「アクティブレイヤーの内容は GPU テクスチャが正本、他レイヤーは CPU bitmap が正本」として表示時にそれぞれから読む構造にする。完全な CPU 廃止は Phase 8D/8E で行う。

### 完了条件

- `cargo test --workspace` と `cargo clippy --workspace --all-targets` が通る
- `gpu` feature 有効時に、アクティブレイヤーへの描画では CPU bitmap upload が発生しない
- 複数レイヤー構成でも表示が正しく合成される（非アクティブレイヤーは CPU 経由）
- `cargo build -p desktop` (feature なし) が通る
- 既存保存ファイルを開いた後、GPU テクスチャで正しく表示されること

---

## Phase 8D: CPU ペイントパスの廃止

### 目的

stroke/stamp 操作の `apply_bitmap_edits`（CPU bitmap 書き込み）を除去する。キャンバス CPU bitmap (`Document` 内 `CanvasBitmap` / `Vec<u8>`) は保存・読込時のみ使用するデータに格下げする。

### 実装概要

1. **`apps/desktop/src/app/services/project_io.rs`**:
   - `execute_paint_input` の stroke/stamp パスから `apply_bitmap_edits(edits)` を除去する（`#[cfg(feature = "gpu")]` で分岐）
   - `edits` から dirty rect のみを抽出し、`PendingStroke.dirty` に蓄積する
2. **Undo/Redo の再設計**:
   - 現状の `BitmapPatch` は CPU bitmap のリージョンを保存している
   - `gpu` feature 有効時は GPU テクスチャリージョンのコピー（`wgpu::CommandEncoder::copy_texture_to_texture`）で Undo/Redo 用スナップショットを取る
   - `HistoryEntry::GpuBitmapPatch { panel_id, layer_index, dirty, before_texture: wgpu::Texture, after_texture: wgpu::Texture }` を追加する
   - スナップショットのタイミング: 既存の `PendingStroke` と同様に「ストローク開始時（最初の Stamp/StrokeSegment）」に before テクスチャをコピーし、`commit_stroke_to_history` 時（PointerUp）に after テクスチャをコピーして `GpuBitmapPatch` を push する。FloodFill/LassoFill は即時コピーで確定する。
   - 旧フォーマット（CPU `BitmapPatch`）のレコードが履歴に残っている場合: `execute_undo/redo` で `BitmapPatch` variant を処理する既存コードはそのまま維持し、互換性を確保する（履歴をクリアする必要はない）
3. **`apps/desktop/src/app/services/mod.rs`**: `execute_undo` / `execute_redo` に `GpuBitmapPatch` の適用パス（`copy_texture_to_texture` → submit → dirty rect 更新）を追加する
4. **プロジェクト保存**: `gpu` feature 有効時は `queue.copy_texture_to_buffer` → CPU → SQLite の経路でビットマップを読み出す（`GpuCanvasPool::read_back_to_cpu(panel_id, layer_index)` を `gpu-canvas` に追加）

### 完了条件

- `cargo test --workspace` と `cargo clippy --workspace --all-targets` が通る
- `gpu` feature 有効時のペイント操作で CPU bitmap 書き込みが発生しない
- Undo/Redo がストロークを正しく取り消せる（GPU テクスチャスナップショット）
- 旧 `BitmapPatch` 方式の履歴エントリも Undo/Redo で正しく動作する（互換性維持）
- `cargo build -p desktop` (feature なし) が通る

---

## Phase 8E: GPU 塗りつぶし + レイヤー合成

### 目的

Flood fill・Lasso fill を GPU compute shader で実行し、CPU でのピクセル演算を完全に除去する。また多レイヤーの合成を GPU で行い、CPU compose (`render::compose.rs`) のキャンバス合成パスを除去する。

### 実装概要

1. **`crates/gpu-canvas/src/shaders/flood_fill.wgsl`**: 並列 BFS / jump-flood 塗りつぶし compute shader
2. **`crates/gpu-canvas/src/shaders/lasso_fill.wgsl`**: ポリゴン内判定 + 塗りつぶし compute shader
3. **`crates/gpu-canvas/src/fill.rs`**: `GpuFillDispatch` — flood fill / lasso fill のディスパッチャ
4. **`apps/desktop/src/app/services/project_io.rs`**: `execute_paint_input` の FloodFill / LassoFill パスを GPU dispatch に切り替える
5. **多レイヤー合成**:
   - `crates/gpu-canvas/src/shaders/layer_composite.wgsl`: ブレンドモード対応合成 compute shader
   - `crates/gpu-canvas/src/composite.rs`: `GpuLayerComposite` — 合成ディスパッチャ
   - `apps/desktop/src/wgpu_canvas.rs`: 合成済みテクスチャを canvas quad に使う
   - `crates/render/src/compose.rs`: キャンバス bitmap 合成パスを除去（`gpu` feature 有効時）

### 完了条件

- `cargo test --workspace` と `cargo clippy --workspace --all-targets` が通る
- `gpu` feature 有効時に CPU でのキャンバスピクセル演算が一切行われない
- Flood fill・Lasso fill が GPU で正しく動作する
- 多レイヤー合成が GPU で正しく動作する

---

## フェーズ間の依存関係

```
8A (完了) → 8B (部分実装) → 8C → 8D → 8E
                                 ↕
                        (Undo/Redo 再設計は 8D で同時対応)
```

- 8C は 8B のシェーダーが GPU テクスチャへ書き込んでいることを前提とする
- 8D は 8C で GPU テクスチャが表示の正本になってから CPU パスを廃止する（表示が壊れないことを確認した後）
- 8E は 8D 完了後（CPU パスが廃止済みの状態）に残る塗りつぶしと合成を対象とする

---

## 注意事項

- `gpu` feature なし (`cargo build -p desktop`) は各フェーズを通して動作継続すること
- 各フェーズ完了後に `docs/IMPLEMENTATION_STATUS.md` を更新すること
- Undo/Redo のスナップショット戦略は Phase 8D で確定するが、GPU テクスチャの読み書きはパフォーマンスに影響するため、ストローク確定時のみスナップショットを取る現行方針を維持すること
