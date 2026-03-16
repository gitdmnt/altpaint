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
