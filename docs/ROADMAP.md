# altpaint ロードマップ

---

## 完了フェーズ

| フェーズ | 内容                                                                             | 完了時点   |
| -------- | -------------------------------------------------------------------------------- | ---------- |
| 0        | 境界の固定と作業前提の統一                                                       | 2026-03-11 |
| 1        | `desktopApp` の縮小                                                              | 2026-03-11 |
| 2        | `canvas` 層の新設                                                                | 2026-03-12 |
| 3        | panel runtime / presentation 分離                                                | 2026-03-12 |
| 4        | plugin-first 化の本格化（`ServiceRequest` 導入）                                 | 2026-03-12 |
| 5        | `render` 中心の画面生成整理                                                      | 2026-03-12 |
| 6        | API 名称と物理配置の整理                                                         | 2026-03-12 |
| 7        | 再編後の機能拡張（Undo/Redo・export・snapshot・text-flow・tool child・profiler） | 2026-03-14 |
| 8        | キャンバス描画のGPU化                                                            | 2026-04-23 |
| 9A       | `gpu` feature default-on 化、CPU フォールバック削除                              | 2026-04-26 |
| 9B       | `render-types` クレート抽出 (純データ DTO の分離)                                | 2026-04-26 |
| 9C-1     | Solid Quad パイプライン + 矩形 GPU 化 (背景/キャンバス枠/アクティブ枠)            | 2026-04-26 |

詳細は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) を参照。

---

## 維持項目

### パフォーマンス計測

- profiler 維持
- panel / canvas / input のボトルネック観測
- 責務移動後の回帰確認

### テストと回帰防止

- `cargo test` と `cargo clippy --workspace --all-targets` を継続
- panel runtime / canvas runtime / render plan の単体検証を厚くする

### 文書同期

- 現況は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- 目標構造は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 実コードの構造は [docs/CURRENT_ARCHITECTURE.md](docs/CURRENT_ARCHITECTURE.md)

---

## Phase 9 — render クレート完全削除と描画完全 GPU 化

### ゴール

altpaint の描画責務を完全に GPU へ寄せ、CPU 合成・CPU ラスタライズに依存している `crates/render/` をリポジトリから削除する。

詳細計画: [docs/tmp/phase9-render-crate-removal-plan.md](tmp/phase9-render-crate-removal-plan.md)

### 戦略の核

`render` の責務を 3 種に分解する。

1. **純データ型** (`PixelRect` / `FramePlan` / `CanvasPlan` / `OverlayPlan` / `PanelPlan` / `LayerGroupDirtyPlan` / `union_dirty_rect` / `brush_preview_dirty_rect` 他) — 移動先が必要、削除はしない
2. **CPU 合成** (`compose_*` / `blit_*` / `fill_rgba_block` / `scroll_canvas_region`) — Phase 8 でキャンバスは GPU 化済み。残る装飾レイヤーを GPU 化すれば消せる
3. **CPU ラスタライザ** (`rasterize_panel_layer` / `draw_text_rgba` / `measure_panel_size` / `wrap_text_lines`) — DSL パネルとテキストの GPU 化が必要。Phase 9 の最大の山

### サブフェーズ

| サブ | 内容                                                       | 想定工数      | 依存       |
| ---- | ---------------------------------------------------------- | ------------- | ---------- |
| 9A   | `gpu` feature default-on 化、CPU フォールバック削除 ✅     | 1 セッション  | なし       |
| 9B   | 純データ型を `render-types` クレートへ抽出 ✅              | 1 セッション  | 9A         |
| 9C-1 | Solid Quad パイプライン + 矩形 GPU 化 ✅                   | 1 セッション  | 9B         |
| 9C-2 | ステータステキスト GPU 化 + L1 背景フレーム廃止            | 1 セッション  | 9C-1       |
| 9D   | L3 ブラシプレビュー overlay の GPU 化                      | 1 セッション  | 9B         |
| 9E   | DSL パネルの GPU 直描画化（DSL → Vello シーン直翻訳）      | 3〜5 セッション | 9C, 9D     |
| 9F   | 残存 CPU 経路の駆除と `crates/render/` ディレクトリ削除    | 1 セッション  | 9E         |

### 依存グラフ

```
9A → 9B → ┬─ 9C ─┐
          └─ 9D ─┴─→ 9E → 9F
```

9C と 9D は 9B 後に並列着手可。9E は 9C/9D で整備した GPU テキスト基盤に乗せる。

### 完了条件

- `crates/render/` が削除されている
- workspace 内に `use render::` の参照が 0 件
- 全描画 (キャンバス・パネル・overlay・テキスト・ステータス・背景) が GPU 経路
- `cargo test --workspace` / `cargo build --release` / `cargo clippy --workspace --all-targets` が通過
- `docs/IMPLEMENTATION_STATUS.md` / `docs/MODULE_DEPENDENCIES.md` / `docs/ARCHITECTURE.md` / `docs/CURRENT_ARCHITECTURE.md` / `CLAUDE.md` から render への参照が消える
- ADR `docs/adr/009-render-crate-removal.md` が作成されている

### 着手前に決める判断ポイント

1. 9B で新クレートを切るか `app-core` 吸収か
2. 9C で Vello 経由か専用クワッドシェーダか
3. 9E のテキストエンジンに parley と cosmic-text のどちらを採用するか（Phase 8F の Blitz は parley を使うため統一しやすい）

### スコープ外

- HiDPI (scale != 1.0) 対応
- パネルテクスチャアトラス化
- マルチウィンドウ対応
- WebGPU バックエンド変更

### 想定リスク

- 9A で `srgb_view_supported = false` 環境（古い GPU）を切り捨てる — alpha 期間として許容前提
- 9C で単色矩形 1 個に Vello はオーバーヘッドが大きい可能性 — 専用シェーダとの比較判断が必要
- 9E で parley / cosmic-text と font8x8 の表示結果が異なり、スナップショット系テストの基準値再設定が必要
- 9E でパネル数 × 描画頻度のオーバーヘッド計測が必要（将来的なアトラス化検討）
