# タスク: [architecture] 描画レイヤーの物理的分離

- **ROADMAP 項目**: `[architecture] 描画レイヤーの物理的分離`
- **優先度**: 高（後続タスクの前提）
- **作成日**: 2026-03-15
- **主な変更クレート**: `crates/render/`, `apps/desktop/src/`

---

## 目的

UIパネルの変更がキャンバス描画に波及してエラーを引き起こさない構造にする。

**背景 / キャンバス / temp-overlay / UIパネル** の 4 グループを
CPU フレームおよび GPU テクスチャとして物理的に分離し、
各グループを独立して更新した後、最後に GPU が順番にブレンドする設計へ移行する。

CPU でレイヤーを合成してから GPU にアップロードするパスは **完全に廃止**する。

---

## 現状の問題

### GPU テクスチャが 3 枚、L3/L4 が混在

| CPU フレーム | GPU テクスチャ | 内容 |
|---|---|---|
| `base_frame` | `base_texture` | ウィンドウ背景 + ステータスバー |
| `canvas_frame` | `canvas_texture` | ペイントビットマップ |
| `overlay_frame` | `overlay_texture` | L3（ブラシ・ラッソ等）**＋** L4（UIパネル）混在 |

`overlay_frame` の CPU 合成により、L4 が変化するたびに L3 の領域も再アップロードされる。

### dirty rect が混在

`pending_canvas_host_dirty_rect` が L3 と L4 の dirty を同一フィールドで管理しており、
パネル移動がブラシプレビューの再計算を強制する。

---

## 完了条件（ROADMAP 原文）

> `crates/render/` にレイヤーグループ別の dirty 判定ユニットテストが通る。
> UIパネル移動後にキャンバスレイヤーが再描画されないことをテストで確認できる。

---

## 設計方針

### 4 レイヤーグループ

```
L1: Background   — ウィンドウ背景・キャンバス背景・ステータスバー
L2: Canvas       — ペイントビットマップ（既存のまま）
L3: TempOverlay  — ブラシプレビュー・ラッソ・アクティブパネルマスク・パネルナビゲーター
L4: UiPanel      — 事前レンダリング済みパネルサーフェス
```

各グループは **独立した CPU `RenderFrame`** と **独立した GPU テクスチャ** を持つ。

### 更新と合成の原則

- 各グループは自グループの dirty rect が立ったときのみ CPU フレームを更新し GPU へアップロードする
- **CPU 側でレイヤーを合成するステップは存在しない**
- GPU が `draw_layer()` の呼び出し順序でアルファブレンドしながら合成する

```
[イベント処理]
  L1 dirty → pending_background_dirty_rect
  L2 dirty → pending_canvas_dirty_rect（既存）
  L3 dirty → pending_temp_overlay_dirty_rect
  L4 dirty → pending_ui_panel_dirty_rect

[prepare_present_frame()]
  L1 dirty → compose_background_region()  → background_frame 更新
  L2 dirty → (既存パス)                   → canvas_frame 更新
  L3 dirty → compose_temp_overlay_region() → temp_overlay_frame 更新
  L4 dirty → compose_ui_panel_region()     → ui_panel_frame 更新
  ※ CPU での合成ステップは一切ない

[GPU upload / draw]
  L1 dirty → background_texture へアップロード
  L2 dirty → canvas_texture へアップロード（既存）
  L3 dirty → temp_overlay_texture へアップロード
  L4 dirty → ui_panel_texture へアップロード

  draw_layer(L1) → draw_layer(L2, transform) → draw_layer(L3) → draw_layer(L4)
```

### GPU パイプラインの変更

現在の単一 `wgpu::RenderPipeline`（`ALPHA_BLENDING`）と `draw_layer()` の仕組みは変えない。
`draw_layer()` の呼び出しを 1 回増やし、`overlay_layer` を 2 枚に分割するだけ。

---

## 変更箇所一覧

### Step 1: `crates/render/src/layer_group.rs`（新規ファイル）

```rust
/// 描画レイヤーグループの識別子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerGroup {
    Background,
    Canvas,
    TempOverlay,
    UiPanel,
}

/// 各レイヤーグループの dirty rect を独立して管理する。
#[derive(Debug, Default, Clone, Copy)]
pub struct LayerGroupDirtyPlan {
    pub background: Option<PixelRect>,
    pub canvas: Option<CanvasDirtyRect>,
    pub canvas_transform_changed: bool,
    pub temp_overlay: Option<PixelRect>,
    pub ui_panel: Option<PixelRect>,
}

impl LayerGroupDirtyPlan {
    pub fn mark_background(&mut self, rect: PixelRect) { ... }
    pub fn mark_temp_overlay(&mut self, rect: PixelRect) { ... }
    pub fn mark_ui_panel(&mut self, rect: PixelRect) { ... }
}
```

`crates/render/src/lib.rs` に `pub mod layer_group;` を追加。

### Step 2: `crates/render/src/dirty.rs` — `DirtyFramePlan` を廃止

`DirtyFramePlan` を `LayerGroupDirtyPlan` へ統合・削除する。
`mark_base()` / `mark_overlay()` は削除し、呼び出し元を新 API へ移行する。

### Step 3: `crates/render/src/compose.rs` — レイヤーグループ別合成関数

```rust
// L1: 旧 compose_base_frame / compose_base_region を rename
pub fn compose_background_frame(frame_plan: &FramePlan<'_>) -> RenderFrame
pub fn compose_background_region(frame: &mut RenderFrame, frame_plan: &FramePlan<'_>, dirty: Option<PixelRect>)

// L3: キャンバス一時オーバーレイのみ（panel blit を含まない）
pub fn compose_temp_overlay_frame(frame_plan: &FramePlan<'_>, overlay_state: &CanvasOverlayState) -> RenderFrame
pub fn compose_temp_overlay_region(frame: &mut RenderFrame, frame_plan: &FramePlan<'_>, overlay_state: &CanvasOverlayState, dirty: Option<PixelRect>)

// L4: UIパネルサーフェスの blit のみ（canvas overlay 描画を含まない）
pub fn compose_ui_panel_frame(frame_plan: &FramePlan<'_>) -> RenderFrame
pub fn compose_ui_panel_region(frame: &mut RenderFrame, frame_plan: &FramePlan<'_>, dirty: Option<PixelRect>)
```

**削除するもの**:
- `compose_base_frame()` → `compose_background_frame()` に rename
- `compose_overlay_frame()` → `compose_temp_overlay_frame()` + `compose_ui_panel_frame()` に分割
- `compose_overlay_region()` → `compose_temp_overlay_region()` + `compose_ui_panel_region()` に分割

### Step 4: `apps/desktop/src/app/mod.rs` — フレームフィールドの再構成

```rust
// 旧フィールド → 新フィールド
// base_frame: Option<RenderFrame>    → background_frame: Option<RenderFrame>
// overlay_frame: Option<RenderFrame> → temp_overlay_frame: Option<RenderFrame>
//                                      ui_panel_frame: Option<RenderFrame>  ← 新設
// ※ canvas_frame はそのまま

// 旧 pending dirty rect → 新 pending dirty rect
// pending_canvas_background_dirty_rect: Option<Rect> → pending_background_dirty_rect: Option<Rect>
// pending_canvas_host_dirty_rect: Option<Rect>       → pending_temp_overlay_dirty_rect: Option<Rect>
//                                                       pending_ui_panel_dirty_rect: Option<Rect>  ← 新設
// ※ pending_canvas_dirty_rect / pending_canvas_transform_update はそのまま
```

公開メソッド: `base_frame()` → `background_frame()`、`overlay_frame()` → `temp_overlay_frame()` + `ui_panel_frame()`

### Step 5: `apps/desktop/src/app/present_state.rs` — メソッド再構成

```rust
// 旧 append_canvas_host_dirty_rect() を 2 つに分割
pub(super) fn append_temp_overlay_dirty_rect(&mut self, dirty: Rect) { ... }
pub(super) fn append_ui_panel_dirty_rect(&mut self, dirty: Rect) { ... }

// reset_pending_dirty() に新フィールドの初期化を追加
```

`mark_canvas_transform_dirty()` 内のブラシプレビュー dirty 更新は `append_temp_overlay_dirty_rect()` を呼ぶ。

### Step 6: `apps/desktop/src/app/panel_dispatch.rs` — L4 用メソッドに変更

L159, L230, L245 の `append_canvas_host_dirty_rect()` を `append_ui_panel_dirty_rect()` に変更。

### Step 7: `apps/desktop/src/app/input.rs` — L3 用メソッドに変更

L43, L55, L207, L213, L219, L244 の `append_canvas_host_dirty_rect()` を `append_temp_overlay_dirty_rect()` に変更。

### Step 8: `apps/desktop/src/app/present.rs` — 順序付き独立更新パイプライン

```rust
// フルリビルド時
self.background_frame   = Some(compose_background_frame(&frame_plan));
self.temp_overlay_frame = Some(compose_temp_overlay_frame(&frame_plan, &overlay_state));
self.ui_panel_frame     = Some(compose_ui_panel_frame(&frame_plan));
// ※ overlay_frame は廃止。CPU 合成ステップなし。

// インクリメンタル更新
// L1
if let Some(dirty) = self.pending_background_dirty_rect.take() {
    compose_background_region(background_frame, &frame_plan, Some(dirty));
    layer_dirty.mark_background(dirty);
}
// L2 (既存パス)
// L3
if let Some(dirty) = self.pending_temp_overlay_dirty_rect.take() {
    compose_temp_overlay_region(temp_overlay_frame, &frame_plan, &overlay_state, Some(dirty));
    layer_dirty.mark_temp_overlay(dirty);
}
// L4
if let Some(dirty) = self.pending_ui_panel_dirty_rect.take() {
    compose_ui_panel_region(ui_panel_frame, &frame_plan, Some(dirty));
    layer_dirty.mark_ui_panel(dirty);
}

return PresentFrameUpdate {
    background_dirty_rect:  layer_dirty.background,
    canvas_dirty_rect:      canvas_dirty,
    temp_overlay_dirty_rect: layer_dirty.temp_overlay,
    ui_panel_dirty_rect:    layer_dirty.ui_panel,
    canvas_transform_changed,
    canvas_updated: ...,
};
```

### Step 9: `apps/desktop/src/app/mod.rs` — `PresentFrameUpdate` の再定義

```rust
pub struct PresentFrameUpdate {
    pub background_dirty_rect: Option<PixelRect>,    // 旧 base_dirty_rect
    pub canvas_dirty_rect: Option<CanvasDirtyRect>,
    pub temp_overlay_dirty_rect: Option<PixelRect>,  // 旧 overlay_dirty_rect の L3 部分
    pub ui_panel_dirty_rect: Option<PixelRect>,      // 新設（L4 部分）
    pub canvas_transform_changed: bool,
    pub canvas_updated: bool,
}
```

### Step 10: `apps/desktop/src/wgpu_canvas.rs` — 4 テクスチャ対応

```rust
// PresentScene に ui_panel_layer を追加
pub struct PresentScene<'a> {
    pub base_layer: FrameLayer<'a>,         // L1
    pub canvas_layer: Option<CanvasLayer<'a>>, // L2
    pub temp_overlay_layer: FrameLayer<'a>, // L3（旧 overlay_layer）
    pub ui_panel_layer: FrameLayer<'a>,     // L4（新設）
}

// WgpuPresenter に ui_panel_layer テクスチャを追加
ui_panel_layer: Option<UploadedLayerTexture>,  // 新設

// render() の描画順序
pass.set_pipeline(&self.pipeline);
Self::draw_layer(&mut pass, self.base_layer.as_ref());         // L1
if scene.canvas_layer.is_some() {
    Self::draw_layer(&mut pass, self.canvas_layer.as_ref());   // L2
}
Self::draw_layer(&mut pass, self.temp_overlay_layer.as_ref()); // L3
Self::draw_layer(&mut pass, self.ui_panel_layer.as_ref());     // L4
```

既存の `pipeline` / shader は変更しない。`draw_layer()` の呼び出しを 1 回追加するだけ。

`PresentTimings` に `ui_panel_upload` / `ui_panel_upload_bytes` を追加する。

### Step 11: `apps/desktop/src/runtime.rs` — 呼び出し側の更新

```rust
// 旧
let Some(overlay_frame) = self.app.overlay_frame() else { return; };

// 新
let Some(temp_overlay_frame) = self.app.temp_overlay_frame() else { return; };
let Some(ui_panel_frame) = self.app.ui_panel_frame() else { return; };

// PresentScene 組み立て
presenter.render(PresentScene {
    base_layer:          FrameLayer { source: background_frame.into(), upload_region: base_upload },
    canvas_layer,
    temp_overlay_layer:  FrameLayer { source: temp_overlay_frame.into(), upload_region: temp_overlay_upload },
    ui_panel_layer:      FrameLayer { source: ui_panel_frame.into(), upload_region: ui_panel_upload },
})
```

---

## ユニットテスト（`crates/render/src/tests/layer_group_tests.rs`、新規）

```rust
#[test]
fn marking_ui_panel_dirty_does_not_affect_other_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_ui_panel(rect(0, 0, 100, 100));
    assert!(d.temp_overlay.is_none());
    assert!(d.canvas.is_none());
    assert!(d.background.is_none());
}

#[test]
fn marking_temp_overlay_dirty_does_not_affect_other_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_temp_overlay(rect(0, 0, 100, 100));
    assert!(d.ui_panel.is_none());
    assert!(d.canvas.is_none());
    assert!(d.background.is_none());
}

#[test]
fn marking_background_dirty_does_not_affect_overlay_groups() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_background(rect(0, 0, 200, 200));
    assert!(d.temp_overlay.is_none());
    assert!(d.ui_panel.is_none());
    assert!(d.canvas.is_none());
}

#[test]
fn dirty_rects_union_within_same_group() {
    let mut d = LayerGroupDirtyPlan::default();
    d.mark_temp_overlay(rect(0, 0, 50, 50));
    d.mark_temp_overlay(rect(60, 60, 50, 50));
    let r = d.temp_overlay.unwrap();
    assert!(r.width > 50);
}
```

---

## 作業手順

1. `crates/render/src/layer_group.rs` 新規作成 — `LayerGroup` / `LayerGroupDirtyPlan` 定義
2. `crates/render/src/dirty.rs` — `DirtyFramePlan` を `LayerGroupDirtyPlan` へ統合・削除
3. `crates/render/src/compose.rs` — L1/L3/L4 別合成関数を追加し旧関数を削除
4. `crates/render/src/tests/layer_group_tests.rs` — ユニットテスト追加、`cargo test -p render` 確認
5. `apps/desktop/src/app/mod.rs` — フレーム・dirty rect フィールドおよび `PresentFrameUpdate` を再構成
6. `apps/desktop/src/app/present_state.rs` — メソッドを分割・rename
7. `apps/desktop/src/app/panel_dispatch.rs` — 3 箇所を `append_ui_panel_dirty_rect()` に変更
8. `apps/desktop/src/app/input.rs` — 6 箇所を `append_temp_overlay_dirty_rect()` に変更
9. `apps/desktop/src/app/present.rs` — 順序付き独立更新パイプラインに書き直す
10. `apps/desktop/src/wgpu_canvas.rs` — `PresentScene` に `ui_panel_layer` 追加、`draw_layer()` 追加
11. `apps/desktop/src/runtime.rs` — `PresentScene` 組み立てを新構造に合わせる
12. `cargo test --workspace` → `cargo clippy --workspace --all-targets` を通す
13. `docs/IMPLEMENTATION_STATUS.md` を更新する

---

## 自己レビュー

### 性能

- CPU 合成ステップ（`final_compose_overlay`）を完全排除。L4 更新時に L3 の再処理なし。
- 各テクスチャアップロードは dirty rect のみ。L4 が変化しても L3 はアップロードしない。
- GPU の `draw_layer()` が 3→4 回になるが、呼び出しコストは無視できる。
- シェーダーは既存のまま。パイプライン再構築なし。

### リスク

- Step 2〜3 で旧関数を削除するとコンパイルエラーが連鎖する。Step 4〜11 を同一 PR でまとめて修正する。
- `WgpuPresenter` に `ui_panel_layer: Option<UploadedLayerTexture>` を追加する際、
  `ensure_layer_texture()` の呼び出しを `render()` 内に漏れなく追加すること。
- `PresentTimings` のフィールド追加により、タイミング読み取り箇所もすべて更新が必要。

### スコープ外

- GPU テクスチャのフォーマット変更（RGBA8 のまま）
- シェーダーの変更
- イベント駆動パネル再描画（次タスク）

---

## 完了後の確認事項

- [ ] `crates/render/src/tests/layer_group_tests.rs` の全テスト通過
- [ ] `cargo test --workspace` 全通過
- [ ] `cargo clippy --workspace --all-targets` 警告なし
- [ ] `wgpu_canvas.rs` に `overlay_layer` フィールド・`overlay_layer` 参照が残っていないこと
- [ ] `compose_overlay_region()` が削除されていること
- [ ] `final_compose_overlay()` が存在しないこと（CPU 合成ステップなし）
- [ ] `docs/IMPLEMENTATION_STATUS.md` に完了記録を追記
