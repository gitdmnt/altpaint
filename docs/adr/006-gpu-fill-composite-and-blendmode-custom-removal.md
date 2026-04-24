# ADR 006: GPU 塗りつぶし + レイヤー合成と `BlendMode::Custom` 削除

- 作成日: 2026-04-25
- 作業 Agent: claude-opus-4-7 (1M context)
- 関連: [ADR 003 GPU キャンバス移行](003-gpu-canvas-migration.md) / [ADR 005 Phase 8D CPU-GPU 通信最小化](005-gpu-canvas-phase-8d-communication-minimization.md)

## コンテキスト

Phase 8D までで単一レイヤーのブラシ/消しゴムは GPU compute で完結し、`GpuLayerTexture` が表示の正本になっていた。残る CPU ピクセル演算は次の 2 経路:

1. FloodFill / LassoFill を `crates/canvas/src/ops/` の CPU コードで計算し、`BitmapEdit` 経由で CPU bitmap に書き、さらに GPU テクスチャへ `upload_region` する二重処理
2. 多レイヤー合成が常に `composite_panel_bitmap_region` で走り、`should_use_gpu_canvas_source` が単一レイヤーのみ GPU パスを許可していた

`RENDERING-ENGINE.md` の「キャンバス描画は必ず GPU を使う」原則を完結させるため Phase 8E で上記 2 経路を GPU 化する。合わせて、式パーサーで評価する `BlendMode::Custom(String)` を GPU 合成で対応する工数が過大と判断し、削除する。

## 決定

### FloodFill / LassoFill の GPU 実装

Ping-pong マスクテクスチャによる iterative region growing + `fill_apply` の二段構成を採用する:

1. `flood_fill_step.wgsl`: 2 枚の `Rgba8Unorm` マスクテクスチャを交互に読み書きし、seed 座標からの 4-connect 同色連結成分を 1 ステップずつ拡張する。atomic カウンタで「今回新たにマークされたピクセル数」を計測
2. 32 iter ごとに counter を CPU へ readback し、`0` なら収束として early break。最大 iter は `FLOOD_FILL_ITERATION_CAP = 8192`
3. `lasso_fill_mark.wgsl`: ポリゴン頂点を storage buffer で受け取り、ピクセル中心 `(x + 0.5, y + 0.5)` に対する ray casting で内部判定
4. `fill_apply.wgsl`: mark 1.0 のピクセルに source-over で `fill_color` を書き込む

**Jump Flood Algorithm は却下**。boolean 連結成分の伝播は JFA のセマンティクスを保証せず、flood fill として誤った結果を生む危険がある。

**seed 色の取得**: 各 invocation が `textureLoad(source, seed)` で直接読む（GPU L2 キャッシュに乗るため追加コストは軽微）。`source` は呼び出し側で composite テクスチャがあればそれ、無ければ active layer 自身を渡す — CPU 実装の「`composited_bitmap` から target 色を取る」セマンティクスと一致する。

### 多レイヤー合成の GPU 実装

Panel ごとに 1 枚の **composite テクスチャ** を `GpuCanvasPool` に保持する。`CanvasLayerSource::GpuComposite { panel_id, width, height }` variant を新設し、presentation 層は panel_id から合成テクスチャを解決して描画する。

合成は iterative compute:

1. `composite_clear.wgsl` で dirty 領域を透明にクリア
2. レイヤーを bottom → top で走査し、`visible == false` はスキップ、残りは `layer_composite.wgsl` を dispatch
3. Shader は `blend_code: u32` (Normal=0 / Multiply=1 / Screen=2 / Add=3) と `has_mask: u32` で分岐。マスクは `Rgba8Unorm` 別テクスチャとして bind

**Quad render 方式は却下**。既存 `CanvasLayerSource::Gpu` が 1 テクスチャを指す前提で `wgpu_canvas.rs` が組まれており、multiple layer texture の fragment blend に切り替えると L3 temp overlay / L4 UI panel との整合コストが高い。Composite テクスチャ方式は presentation 層の変更が最小で済む。

### BlendMode::Custom 削除

`BlendMode::Custom(String)` variant と CPU 側の `CustomBlendFormula` 式パーサーを完全削除する。

- GPU 合成で式を評価する miniVM / WGSL 動的生成は Phase 8E のスコープに合わず、工数対効果が薄い
- alpha 開発中のため後方互換は不要（ユーザー決定）
- `BlendMode::parse_name` は未知文字列で `Some(Normal)` を返す（旧プロジェクトのロード互換として Normal に格下げ）

将来 Custom blend が必要になれば、Phase 9 以降で WGSL の動的生成 or shader permutation cache で対応する。

### CPU 経路の温存

`gpu` feature 無効ビルド (`cargo build -p desktop`) は従来通り動作する必要があるため、`composite_panel_bitmap_region` / `compose_canvas_host_region` / `refresh_canvas_frame_region` は**削除せず残す**。`gpu` feature 有効時は `should_use_gpu_canvas_source` が `true` を返す runtime 分岐でこれらをスキップする。

### Undo/Redo

FloodFill / LassoFill は即時操作なので、GPU dispatch 直前に `capture_panel_layer_region` で before region を CPU bitmap から取得し、`create_and_upload` で GPU テクスチャ化。dispatch 後に `snapshot_region` で after を GPU-to-GPU コピーし、`HistoryEntry::GpuBitmapPatch` に push する（既存 Phase 8D の仕組みを流用）。Undo / Redo 時に `restore_region` で復元後、`recomposite_panel(panel_id, dirty)` で composite テクスチャも更新する。

## 結果

- FloodFill / LassoFill の CPU apply 経路が `gpu` feature 時に走らなくなる
- 多レイヤー panel でも GPU テクスチャが表示の正本になり、`queue.write_texture` による毎フレームアップロードが消滅
- `BlendMode::Custom` variant および `CustomBlendFormula` パーサー相当 ~260 行を削除
- `gpu` feature 無効ビルド / 既存テストはすべてそのまま動作
- GPU smoke test 4 件（flood / lasso / single-layer passthrough / invisible skip）を追加し、実機で合格
- `BlendMode::parse_name` は「空文字列 → None、その他 → Some(Normal) または Some(Multiply/Screen/Add)」のシンプルな契約へ縮退

## リスクと緩和

- **R1 Rgba8Unorm storage 非対応アダプター**: `format_check::supports_rgba8unorm_storage` を `install_gpu_resources` 時にチェックし、未対応時は `srgb_view_supported = false` で CPU フォールバック経路へ落ちる
- **R2 FloodFill の 4K 以上で iter 数が増える**: 32 iter ごとの atomic readback + `FLOOD_FILL_ITERATION_CAP = 8192` で最悪ケースを抑える。将来 tile-based BFS or workgroup 共有メモリ拡張を検討
- **R3 Custom 削除で既存保存が見た目変化**: ロード時に Normal へ格下げ。alpha 期間のため後方互換は不要
