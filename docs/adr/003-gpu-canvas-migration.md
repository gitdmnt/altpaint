# ADR-003: GPU キャンバスクレートの新設 (Phase 8A)

- 作業日時: 2026-03-16
- 作業モデル: claude-sonnet-4-6

## ステータス

採択

## 背景

altpaint は現在 CPU 上でビットマップ操作を行い、フレームごとに `queue.write_texture` で GPU へアップロードするアーキテクチャを持つ。大きなキャンバスや複数レイヤーの合成をすべて CPU で行うため、描画パフォーマンスに上限がある。GPU コンピュートシェーダーを使ったペイント処理（Phase 8B 以降）へ移行するための基盤として、GPU キャンバスリソースを管理する専用クレートが必要になった。

## 決定内容

`crates/gpu-canvas` を新設し、以下を担う:

1. `GpuCanvasContext` — `Arc<wgpu::Device>` と `Arc<wgpu::Queue>` を保持する共有コンテキスト
2. `GpuLayerTexture` — 1 レイヤー = 1 `wgpu::Texture` のラッパー (Rgba8Unorm)
3. `GpuCanvasPool` — `(panel_id, layer_index)` をキーにレイヤーテクスチャを管理するプール
4. `GpuPenTipCache` — ビットマップペン先テクスチャのキャッシュ
5. `format_check::supports_rgba8unorm_storage` — アダプターの Rgba8Unorm ストレージサポート確認関数

`wgpu` は optional 依存とし、`gpu` feature で有効化する。`crates/canvas` は wgpu 非依存を維持する。

`apps/desktop` の `WgpuPresenter` の `device`/`queue` フィールドを `Arc<T>` に変更し、`GpuCanvasPool` / `GpuPenTipCache` への共有を可能にする。

## 代替案

### A: `crates/canvas` に wgpu 依存を追加する

CPU パスと GPU パスを同一クレートで管理できるが、`crates/canvas` が wgpu に依存すると他のクレート（`render` 等）の依存グラフが複雑化し、テスト時に GPU なし環境でのビルドが困難になる。棄却。

### B: `apps/desktop` に直接実装する

アプリ層にロジックを閉じ込めることができるが、テスタビリティが低く、将来的に `render` クレートから GPU リソースを参照しにくくなる。棄却。

## 影響範囲

- `Cargo.toml` (workspace) — `crates/gpu-canvas` メンバー追加
- `apps/desktop/Cargo.toml` — `gpu-canvas` optional 依存と `gpu` feature 追加
- `apps/desktop/src/wgpu_canvas.rs` — `device`/`queue` の Arc 化、getter 追加
- `apps/desktop/src/app/mod.rs` — `gpu_canvas_pool`/`gpu_pen_tip_cache` フィールド追加 (cfg feature)
- `apps/desktop/src/app/command_router.rs` — ペン切り替え時の `upload_from_preset` 呼び出し (cfg feature)

## テクスチャフォーマット選択

Rgba8Unorm を採用する。STORAGE_READ_WRITE のサポート有無は `supports_rgba8unorm_storage` で確認できる設計にするが、Phase 8A では実際のコンピュートパスへの接続は行わない。サポートなしの場合の Rgba32Float + blit パスは Phase 8B 以降で実装する。

## 完了条件

- `cargo test -p gpu-canvas --no-default-features` が通る
- `cargo build -p desktop` (gpu feature なし) が通る
- `cargo build -p desktop --features gpu` が通る
- `cargo test --workspace` が通る
- `cargo clippy --workspace --all-targets` が通る

---

# ADR-003 追補: GPU リソース初期化と表示切り替え (Phase 8B + 8C)

- 作業日時: 2026-04-25
- 作業モデル: claude-sonnet-4-6

## Phase 8B: GPU リソース初期化

### 決定: 全同期方式を採用する

`sync_all_layers_to_gpu()` は全 Panel × 全 RasterLayer を毎回フルアップロードする。

**理由**: Phase 8D で CPU ペイントパスを廃止するまでは、undo/redo や load の度に CPU bitmap が正本として書き換わる。差分追跡に必要なイベントフックをすべて追加するよりも、全同期で確実性を確保する方が Phase 8D 以前は正しい。差分同期は Phase 8D の CPU パス廃止時に必要になった時点で実装する。

### 決定: `sync_all_layers_to_gpu` の呼び出し箇所を command_router / services / project_io の 3 箇所に集約する

| 呼び出し元 | 対象操作 |
|-----------|---------|
| `command_router.rs` | `AddRasterLayer`, `RemoveActiveLayer`, `SelectLayer`, `MoveLayer`, `AddPanel`, `SelectPanel`, `NewDocumentSized` |
| `services/mod.rs` | `execute_undo`, `execute_redo` の全 true 返却 arm |
| `services/project_io.rs` | `load_project` 成功パス末尾 |

`Command::LoadProjectFromPath` の `command_router` arm には追加しない（`project_io.rs` の 1 箇所に集約して二重同期を防ぐ）。

## Phase 8C: GPU テクスチャを表示の正本へ

### 決定: `view_formats: &[Rgba8UnormSrgb]` で sRGB 変換を吸収する

GPU テクスチャは `Rgba8Unorm`（線形）で保持してコンピュートシェーダーの `STORAGE_READ_WRITE` 要件を満たし、Present 時に `Rgba8UnormSrgb` view を通じて GPU が自動的にリニア→sRGB 変換を行う。

**理由**: CPU ビットマップが sRGB ピクセル値を持つため、GPU テクスチャの View フォーマットを sRGB にすることで Present 時の色一致を確保できる。別途ピクセル変換シェーダーを挟む代替案はパス数が増えてコードが複雑化するため棄却。

### 決定: `srgb_view_supported` フラグで非対応ハードウェアに CPU フォールバックする

起動時に `supports_rgba8unorm_storage(&adapter)` を評価し `WgpuPresenter.srgb_canvas_view_supported` に保存する。`should_use_gpu_canvas_source()` がこのフラグを確認し、非対応の場合は CPU 経路を継続する。

**理由**: `view_formats` に `Rgba8UnormSrgb` を指定できないアダプターではテクスチャ生成自体が panic する。フォールバックにより機能制限付きで CPU 経路が動き続けることを優先する。

### 決定: 単一レイヤーのみ GPU テクスチャ直接表示、複数レイヤーは CPU 経路を継続する（暫定）

`should_use_gpu_canvas_source()` は `active_panel.layers.len() == 1` を条件に含む。

**理由**: 多レイヤー合成（Phase 8E）は未実装。複数レイヤー構成で GPU テクスチャを使うには合成シェーダーが必要であり、Phase 8C のスコープを超える。暫定措置として単一レイヤーのみ GPU パスを有効にし、多レイヤーは Phase 8E まで CPU 経路を維持する。

### 決定: `CanvasLayerSource<'a>` の `panel_id` を `&'a str` で保持して `Copy` を維持する

`CanvasLayerSource::Gpu` の `panel_id` フィールドを `String` ではなく `&'a str` にすることで、`CanvasLayerSource<'a>` / `CanvasLayer<'a>` / `PresentScene<'a>` が `Copy` を実装したまま維持できる。

**理由**: `render()` 内で `scene.canvas_layer` を複数回参照するパターンがあり、`Copy` でなければ `clone()` や再構築が必要になる。`runtime.rs` 側で `active_panel_gpu_id: Option<String>` に一時保存し、その `&str` を渡すことでライフタイムを延ばす。

### 決定: `GpuBindGroupCache` を `WgpuPresenter` に保持し `(panel_id, layer_index, width, height)` 変化時のみ再生成する

毎フレーム `pool.get_view()` で `TextureView` を新規生成するが、bind group の再生成は key の変化時のみに限定する。

**理由**: `TextureView` の生成は軽量だが、bind group の生成には `device.create_bind_group()` が必要でやや高コスト。アクティブパネルや解像度が変わらない通常描画フレームでは再利用することでフレームあたりのオーバーヘッドを最小化する。

## 影響範囲（Phase 8B + 8C）

- `crates/gpu-canvas/src/gpu.rs` — `view_formats` 追加、`create_srgb_view`、`get_view`
- `apps/desktop/src/wgpu_canvas.rs` — `CanvasLayerSource` enum、`GpuBindGroupCache`、`render` 分岐、`update_gpu_canvas_bind_group`
- `apps/desktop/src/app/mod.rs` — `install_gpu_resources`、`sync_all_layers_to_gpu`、`should_use_gpu_canvas_source`、`gpu_canvas_pool` getter、`srgb_view_supported` フィールド
- `apps/desktop/src/app/command_router.rs` — 同期呼び出し追加
- `apps/desktop/src/app/services/mod.rs` — Undo/Redo 後の同期
- `apps/desktop/src/app/services/project_io.rs` — ロード後の同期
- `apps/desktop/src/app/present_state.rs` — GPU 経路時の `refresh_canvas_frame_region` スキップ
- `apps/desktop/src/runtime.rs` — GPU リソース初期化・`CanvasLayerSource::Gpu` 分岐・`render` への pool 渡し
