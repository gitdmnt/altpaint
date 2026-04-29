# ADR 010: `crates/render` クレート物理削除と `PresentScene` レイヤー再編成

- 作業日時: 2026-04-29
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted

## コンテキスト

Phase 9E 完了 (2026-04-26) で DSL パネル + ステータスバーの GPU 化が達成され、
`crates/render/` の役割は実質ゼロになっていた。残存していた表面は次のみ:

- `RenderFrame` / `RenderContext` — `apps/desktop` 内の CPU canvas snapshot のみで使用
- `compose_panel_host_region` / `compose_ui_panel_frame` / `compose_ui_panel_region` —
  呼び出し元 0 件 (L4 dummy 経路の合成路だが、Phase 9E-3 で GPU 化済み)
- `blit_scaled_rgba_region` / `build_source_axis_runs` / `fill_rgba_block` /
  `scroll_canvas_region` / `SourceAxisRun` — 呼び出し元 0 件 (旧 CPU 合成ユーティリティ)
- `PanelHitKind` / `PanelHitRegion` — `crates/ui-shell` の hit-test 互換型
  （実際の構築元は Phase 9E-3 で削除されており dead code）

`PresentScene` には Phase 9E-4 で 1×1 透明 dummy 化された `base_layer` (L1) と
`ui_panel_layer` (L4) が型として残置されており、`runtime.rs` で毎フレーム dummy
を構築・GPU アップロードしていた。実描画には寄与しないが API/CPU/GPU 帯域を消費。

## 決定

`crates/render/` ディレクトリ・クレート登録・依存を完全削除し、`PresentScene` を
GPU 化済みのレイヤー構成に整理する。残存型は最小コストで他クレートへ吸収する。

### 1. 型移管

| 残存 API | 移管先 | 方法 |
| --- | --- | --- |
| `RenderFrame` (struct) | `apps/desktop/src/app/canvas_frame.rs` | `CanvasFrame { width, height, pixels }` として再定義 |
| `RenderContext::render_frame` | 同上ファイル | `build_canvas_frame(document)` 純関数として inline |
| `PanelHitKind` / `PanelHitRegion` | `crates/ui-shell/src/presentation.rs` | `pub(crate)` のまま直接定義 |

### 2. CPU 合成ユーティリティ群の削除

`compose_*` / `blit_scaled_rgba_region` / `build_source_axis_runs` / `fill_rgba_block` /
`scroll_canvas_region` / `SourceAxisRun` は呼び出し元 0 件のため削除のみ。

### 3. PresentScene レイヤー再編成

```rust
// 旧 (Phase 9E-4)
pub struct PresentScene<'a> {
    pub background_quads: &'a [SolidQuad],   // L0
    pub base_layer: FrameLayer<'a>,          // L1 (1×1 dummy)
    pub canvas_layer: Option<CanvasLayer<'a>>, // L2
    pub overlay_solid_quads: &'a [SolidQuad],  // L3a
    pub overlay_circle_quads: &'a [CircleQuad], // L3b
    pub overlay_line_quads: &'a [LineQuad],    // L3c
    pub ui_panel_layer: FrameLayer<'a>,        // L4 (1×1 dummy)
    pub html_panel_quads: &'a [GpuPanelQuad<'a>], // L5
    pub foreground_quads: &'a [SolidQuad],     // L6
    pub status_quad: Option<GpuPanelQuad<'a>>, // L7
}

// 新 (Phase 9F)
pub struct PresentScene<'a> {
    pub background_quads: &'a [SolidQuad],     // L0
    pub canvas_layer: Option<CanvasLayer<'a>>, // L1
    pub overlay_solid_quads: &'a [SolidQuad],  // L2a
    pub overlay_circle_quads: &'a [CircleQuad], // L2b
    pub overlay_line_quads: &'a [LineQuad],    // L2c
    pub panel_quads: &'a [GpuPanelQuad<'a>],   // L3 (旧 html_panel_quads)
    pub foreground_quads: &'a [SolidQuad],     // L4
    pub status_quad: Option<GpuPanelQuad<'a>>, // L5
}
```

`FrameLayer` 構造体および `WgpuPresenter::base_layer` / `ui_panel_layer` フィールド・
`ensure_layer_texture` / `upload_layer` / `update_quad_uniform` 呼び出し・
`draw_layer` 呼び出しはまとめて削除。

`PresentTimings` から `base_upload` / `ui_panel_upload` / `base_upload_bytes` /
`ui_panel_upload_bytes` フィールドを削除し、`record_present` の集計対象も
`canvas_upload` のみに縮約した。

### 4. `canvas/view_mapping` シグネチャ整理

`map_view_to_canvas_with_transform` は `&RenderFrame` 引数のうち `width` / `height`
しか参照していなかったため、`(canvas_width: usize, canvas_height: usize)` 引数に
簡素化した。テスト 4 箇所と `app/input.rs` の呼び出し元も更新済み。

### 5. クレート削除手順

1. `Cargo.toml` workspace members から `"crates/render"` 削除
2. `crates/render/` ディレクトリ削除
3. `apps/desktop/Cargo.toml` から `render = { path = "../../crates/render" }` 削除
4. `crates/canvas/Cargo.toml` から同削除
5. `crates/ui-shell/Cargo.toml` から同削除

## 代替案

- **`RenderFrame` を `render-types` へ移動**: 純データ DTO として一貫性は保てるが、
  `render-types` は viewport/scene 計算用の純関数バンドルであり、CPU bitmap
  ストレージは概念的に別物。`apps/desktop` 内で完結する型なので局所化を選択。
- **`PanelHitKind/Region` を `panel-api` へ移動**: ホスト/Wasm 共有 DTO 化する
  選択肢もあったが、現在の構築元が dead code であり、将来的に Wasm 側に hit-test
  を委譲する設計検討が必要。Phase 9F の機械的移管としては `ui-shell` 内部型に
  留めるのが適切と判断。

## 影響

### 完了条件 (達成済み)

- workspace 内 `use render::` 参照が 0 件
- `Cargo.toml` workspace members に `crates/render` が無い
- `cargo build --release` 成功
- `cargo test --workspace` 失敗件数がベースラインと同等 (Phase 9E-5 の既存
  パネル hit-test 関連テスト 5〜7 件は Phase 9F のスコープ外)
- `cargo clippy --workspace --all-targets` 警告ベースライン維持

### 残課題

なし。Phase 9F 中に以下も整理した:

- `PanelHitKind` / `PanelHitRegion` / `PanelSurface::{hit_regions, hit_region_count,
  hit_test_at, move_panel_hit_test_at, drag_event_at}` を完全削除
- `panel_event_for_region` / `slider_value_for_position` / `color_wheel_value_for_position`
  ヘルパーを削除
- `panel_dispatch.rs` の DSL fallback (`surface.hit_test_at` 等) を削除し、
  HTML hit-table 経路 (`html_panel_hit_at` / `html_panel_move_handle_at`) に統一
- `PanelDragState::Control` ヴァリアントと `advance_panel_drag_source` を削除。
  `PanelDragState` は単一構造体 (`Move` 相当のフィールド) に簡素化
- `apps/desktop/src/app/state.rs::refresh_canvas_frame_region` を削除
- `panel_surface_hit_regions` profiler value を削除
- 関連テストを `update_html_panel_hits` / `update_html_panel_move_handle` で
  synthetic hit-table を構築する方式に書き直し、または dead path を検証していた
  4 件 (`panel_color_wheel_pointer_press_is_handled`,
  `layer_list_drag_keeps_dragged_layer_selected_while_reordering`,
  `panel_drag_source_advances_for_layer_list_drag`) を削除

最終ベースライン: `cargo test --workspace` 127 passed / 0 failed / 6 ignored
(Phase 9E-5 由来)。`cargo clippy --workspace --all-targets` 警告 83 件で 9F
着手前と同数を維持。

## 関連 ADR / 文書

- ADR 003: GPU canvas migration (Phase 8 起点)
- ADR 005: GPU canvas Phase 8D communication minimization
- ADR 008: HTML panel dynamic size and engine consolidation
- ADR 009: DSL → HTML 翻訳器採用 (Phase 9E)
- `docs/ROADMAP.md` Phase 9 全体計画
- `docs/IMPLEMENTATION_STATUS.md` Phase 9F 完了記録
