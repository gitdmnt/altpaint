# Phase 9: render クレート完全削除までのロードマップ

作成日: 2026-04-25
作業モデル: claude-opus-4-7 (1M context)
ステータス: ドラフト (未着手)

## ゴール

altpaint の描画を完全に GPU 責務へ寄せ、CPU 合成・CPU ラスタライズに依存している `crates/render/` を最終的にリポジトリから削除する。

## 戦略の核

`render` クレートが現在抱えている責務を 3 種に分解する。

1. **純データ型** (`PixelRect` / `FramePlan` / `CanvasPlan` / `OverlayPlan` / `PanelPlan` / `LayerGroupDirtyPlan` / `union_dirty_rect` / `CanvasOverlayState` / `PanelNavigatorOverlay`)
   - 描画ではなく宣言的な計画/座標型
   - 移動先が必要、削除はしない
2. **CPU 合成** (`compose_*` / `blit_*` / `fill_rgba_block` / `scroll_canvas_region`)
   - Phase 8 でキャンバスは GPU 化済み
   - 残りの装飾レイヤー (背景・ステータス・パネル枠・overlay) を GPU 化すれば消せる
3. **CPU ラスタライザ** (`rasterize_panel_layer` / `draw_text_rgba` / `measure_panel_size` / `wrap_text_lines`)
   - DSL パネルとテキストの GPU 化が必要
   - Phase 9 の最大の山

## Phase 9A — `gpu` feature default-on 化、CPU フォールバック削除

想定: 約 1 セッション
依存: なし (即着手可)

### 作業

- `apps/desktop/Cargo.toml` で `default = ["gpu"]` を有効化
- `should_use_gpu_canvas_source` の CPU 分岐 (`srgb_view_supported = false` パス) を削除
- `present_state::apply_bitmap_edits` の `#[cfg(feature = "gpu")]` 排除し GPU 経路一本化
- `compose_canvas_host_region` / `refresh_canvas_frame_region` のキャンバス用呼び出しを削除
- `gpu` feature を Cargo.toml から削除 (常時有効)

### 完了条件

- `gpu` feature 不要となる
- キャンバス用 CPU compose が呼び出されない
- `cargo test --workspace` 通過

### リスク

- `srgb_view_supported = false` 環境 (古い GPU) を切り捨てる
- alpha 期間として許容前提

---

## Phase 9B — 純データ型を `render-types` クレートへ抽出

想定: 約 1 セッション
依存: 9A 完了後

### 作業

- 新規 `crates/render-types/` を作成 (またはシンプルに `app-core` へ吸収)
- 移植対象: `PixelRect` / `FramePlan` / `CanvasPlan` / `OverlayPlan` / `PanelPlan` / `LayerGroupDirtyPlan` / `union_dirty_rect` / `CanvasOverlayState` / `PanelNavigatorOverlay` / `brush_preview_dirty_rect`
- 既存依存元 (`canvas` / `ui-shell` / `apps/desktop`) を `render` → `render-types` に張り替え

### 完了条件

- render クレートに残るのは「実描画ロジック」のみになる
- workspace ビルド通過

### 判断ポイント

- 新クレートを切るか `app-core` に吸収するか — 9B 着手時に決定
- `FramePlan` は実質ドメイン型なので `app-core` 吸収もあり得る

---

## Phase 9C — ステータスバー / デスクトップ背景 / アクティブパネル枠の GPU 化

想定: 約 1〜2 セッション
依存: 9B 完了後

### 作業

- `wgpu_canvas.rs` に GPU 矩形 fill パス (単色クワッド) を追加
- 8F で導入済みの `vello::Renderer` を共有してテキスト描画に再利用
- `compose_desktop_frame` / `compose_status_region` / `compose_active_panel_border` を削除
- ステータステキスト計測は parley または cosmic-text 経由へ移行

### 完了条件

- render から `compose.rs` の装飾系関数群が消える (パネル系除く)
- ステータスバー/背景/アクティブ枠が GPU 経路で出る

### リスク

- 単色矩形 1 個に Vello はオーバーヘッドが大きい場合がある
- 専用シェーダ (既存 wgpu_canvas のクワッドパイプ拡張) との比較判断が必要

---

## Phase 9D — L3 ブラシプレビュー overlay の GPU 化

想定: 約 1 セッション
依存: 9B 完了後 (9C と並列可)

### 作業

- ブラシプレビュー矩形を Vello シーンまたは専用クワッドシェーダで描画
- `compose_temp_overlay_frame` / `compose_temp_overlay_region` 削除
- `brush_preview_dirty_rect` は 9B で `render-types` 側に残す (純粋計算)

### 完了条件

- L3 temp overlay が完全に GPU 経路
- `compose.rs` から overlay 関連関数が消える

---

## Phase 9E — DSL パネルの GPU 直描画化 (最大の山)

想定: 約 3〜5 セッション
依存: 9C/9D 完了後 (GPU テキスト基盤を共用)

### 案

**案 E1: DSL → Vello シーン直翻訳 (推奨)**

- `panel-runtime` または新 `panel-gpu` クレートが `PanelTree` から `vello::Scene` を組み立てる
- 既存 `text.rs` の glyph 計測ロジックを parley/cosmic-text へ置換
- DSL ラスタライズパイプライン (`rasterize_panel_layer`) を GPU 経路で置き換え
- 既存 10 個の DSL パネルは無改修

**案 E2: 全 DSL パネルを HTML 化 (却下推奨)**

- 8F 経路に統一できるが、10 プラグインの書き換えが必要で工数大
- 永続化やパネルローカル状態の作り直しコスト

### 作業 (案 E1 を採用する場合)

- `panel-runtime` に `build_panel_scene(panel_tree, layout) -> vello::Scene` を新設
- ヒットテストは既存 `PanelHitRegion` ロジックを `render-types` から呼び出し
- `rasterize_panel_layer` / `measure_panel_size` の置換
- `wgpu_canvas.rs::PresentScene` に DSL パネル用クワッド配列を追加 (8F の `html_panel_quads` と統合可能)
- `ui-shell::surface_render` の CPU ラスタライズ経路を撤去

### 完了条件

- `render::panel::*` / `render::text::*` が呼ばれない
- 10 個の DSL パネル全てが GPU 経路で表示される
- フォーカス/フォーカス枠/ヒットテストが従来どおり動く
- `cargo test --workspace` 通過

### リスク

- parley/cosmic-text と font8x8 の表示結果が異なる
- スナップショット系テストの基準値再設定が必要
- パネル数 × 描画頻度のオーバーヘッド計測が必要 (アトラス化検討)

---

## Phase 9F — 残存 CPU 経路の駆除と render クレート削除

想定: 約 1 セッション
依存: 9E 完了後

### 作業

- `crates/render/` ディレクトリ削除
- workspace `Cargo.toml` の members から `crates/render` 削除
- 残存依存 (`canvas` / `ui-shell` / `apps/desktop`) の `Cargo.toml` から `render = ...` 行を削除
- `use render::` の grep が 0 件になることを確認
- ドキュメント更新
  - `docs/IMPLEMENTATION_STATUS.md` に Phase 9 完了節を追加
  - `docs/MODULE_DEPENDENCIES.md` から render を削除
  - `docs/ARCHITECTURE.md` の render 言及を更新
  - `docs/CURRENT_ARCHITECTURE.md` の更新
  - `CLAUDE.md` の主要クレート表から render 行を削除
- ADR 作成: `docs/adr/009-render-crate-removal.md`

### 完了条件

- `cargo test --workspace` 通過
- `cargo build --release` 通過
- `cargo clippy --workspace --all-targets` 警告増加なし
- `grep -r "use render::" .` が 0 件

---

## 全体の依存グラフ

```
9A (CPU フォールバック削除)
  ↓
9B (純データ型抽出)
  ↓
9C (装飾 GPU 化)    9D (overlay GPU 化)   ← 並列可
  ↓                   ↓
       9E (DSL パネル GPU 化)  ← 最大の山
              ↓
       9F (render クレート削除)
```

## 想定総工数

8〜13 セッション程度 (9E が最大の不確実性)

## 判断が必要なポイント (着手前)

1. 9B で新クレートを切るか `app-core` に吸収するか
2. 9C で Vello 経由か専用クワッドシェーダか
3. 9E で案 E1 (Vello 直翻訳) と案 E2 (全 HTML 化) のどちらを取るか
4. テキストエンジンに parley と cosmic-text のどちらを採用するか (8F 採用の Blitz は parley を使うため統一しやすい)

## スコープ外 (Phase 9 では扱わない)

- HiDPI (scale != 1.0) 対応
- パネルテクスチャアトラス化
- マルチウィンドウ対応
- WebGPU バックエンド変更

## 関連文書

- [docs/IMPLEMENTATION_STATUS.md](../IMPLEMENTATION_STATUS.md) — Phase 8 までの完了履歴
- [docs/adr/003-gpu-canvas-migration.md](../adr/003-gpu-canvas-migration.md) — Phase 8A 経緯
- [docs/adr/007-html-panel-experiment.md](../adr/007-html-panel-experiment.md) — Phase 8F (HTML パネル GPU 直描画)
- [docs/adr/008-html-panel-dynamic-size-and-engine-consolidation.md](../adr/008-html-panel-dynamic-size-and-engine-consolidation.md) — Phase 8G
- [docs/tmp/gpu-migration-plan-phase8.md](gpu-migration-plan-phase8.md) — Phase 8 計画原本
