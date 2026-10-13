# ADR 017: 座標系の型レベル区別と依存グラフの階層化 (Phase 15)

- 作業日時: 2026-06-12
- 作業 Agent: claude-fable-5 (Claude Code)
- ステータス: 完了

## 背景

ADR 016 (Phase 14) でクレート依存の最小化を行ったが、次の課題が残っていた。

1. **座標系が型で区別されない**: `app-core::coordinates` に 11 の座標型が定義済みにもかかわらず、
   ui-shell の hit-test API (`html_panel_at(x: usize, y: usize)` 等)、`gpu-canvas` の
   `dispatch_stroke(positions: &[(f32, f32)], ...8 引数)`、render-types の UV 変換 `(f32, f32)`、
   `PanelResizeState::start_pointer: (i32, i32)` 等で生タプル / bare 引数が残存。
   screen 座標と page 座標が同じ `usize, usize` で表現され型検査が効かない。
   矩形包含判定が ui-shell 内に 3 回手書き重複し、desktop 側に
   `point.x.max(0) as usize` / `if point.x < 0` のガードが 4+ 箇所重複していた。
2. **依存の分枝集中**: `apps/desktop` が 11 クレートへ放射状に直接依存し、
   パネル系だけで panel-runtime / panel-api / panel-html / builtin-panels / ui-shell の 5 入口があった。
3. **不要な中間層**: `canvas/render_bridge.rs` (1 関数 26 行)、`canvas/registry.rs` (type alias +
   factory 20 行)、`app/drawing.rs` (1 行 re-export)、`app/commands.rs` (コメントのみ)、
   `builtin-panels` umbrella クレート (loader のみ 113 行)。
4. **命名の異義**: `registry.rs` が canvas (plugin factory) と panel-runtime (`PanelRuntime` 本体)
   で同名異義。`app/state.rs` は名前から責務が読めない。
5. **状態の平置き**: `DesktopApp` 31 フィールド。提示無効化フラグ 9 個 + GPU リソース Option 5 個
   + バックグラウンドジョブが構造化されず散在。

## 決定

### 1. 座標系の型レベル区別 (Phase 15B)

- **`app-core::coordinates::CanvasPointF` 新設**: キャンバス座標のサブピクセル版
  (`to_canvas_point()` / `lerp_toward()` 付き)。手ぶれ補正の
  `CanvasInputState::last_smoothed_position: Option<(f32, f32)>` を置換。
  表示座標の `CanvasDisplayPoint` とは別型として区別。
- **`render-types::PixelRect` の型付き API**: `contains(x: i32, y: i32)` →
  `contains(WindowPoint)`、`to_local_point(WindowPoint) -> Option<PanelSurfacePoint>`、
  `contains_local(PanelSurfacePoint)` を追加。手書き矩形判定を全廃。
- **ui-shell hit-test API の `WindowPoint` 統一**: `html_panel_at` / `html_panel_hit_at` /
  `html_panel_move_handle_at` / `panel_resize_hit_at` / `resize_hit_in_rect` が
  `WindowPoint` を受け、負値ガードは API 内部 (`PixelRect::contains`) へ移動。
  `html_panel_at` の戻り値は `(String, u32, u32)` → `(String, PanelSurfacePoint)`。
- **gpu-canvas のパラメータ構造体化と座標型化**:
  - `dispatch_stroke` の 8 引数 → `BrushStrokeParams { color_rgba, radius, opacity, antialias, tool_kind }`
    + `positions: &[PanelLocalPoint]` (clippy too_many_arguments 解消)。
    計画時は `PanelLocalPointF` 新設を想定したが、`compute_stamp_positions` の戻り値が
    既存 `PanelLocalPoint` (整数) でありサブピクセル情報が存在しないため、既存型を直接受けて
    f32 変換を `build_positions_bytes` 内に閉じる方が型と変換の重複を減らせると判断した。
  - `upload_region` / `snapshot_region` の x/y/w/h 4 引数 → `CanvasDirtyRect`、
    `restore_region` の x/y → `PanelLocalPoint` (いずれもコマローカル座標であることを型で明示)。
- **render-types の UV 変換**: `(f32, f32)` タプル → `SourceUv` / `RotatedUv` の 2 型に分離
  (回転前後の UV 空間を型で区別)。
- **desktop の生タプル排除**: `PanelResizeState::start_pointer: (i32, i32)` → `WindowPoint`、
  `PanelDragState::grab_offset_x/y: usize` → `grab_offset: PanelSurfacePoint`、
  `compute_resized_rect(pointer: (i32, i32))` → `WindowPoint`、
  `panel_is_hovered(x, y)` → `WindowPoint`。

### 2. 依存グラフの階層化 (Phase 15C)

- **`builtin-panels` umbrella クレート削除**: `loader.rs` を `panel-runtime::loader` へ統合
  (workspace 28 → 27 メンバー)。パネル資産と 12 パネルクレートは `crates/builtin-panels/` のまま。
- **panel-runtime を panel サブシステムの facade に**: `HostAction` / `PanelEvent` /
  `PanelMoveDirection` / `ResizeEdge` / `ServiceRequest` / `services` (panel-api) と
  `panel_html as html` を再公開。ui-shell も自 API の戻り値型 `ResizeEdge` を再公開。
- **desktop の直接依存 11 → 8**: builtin-panels / panel-api / panel-html を削除。
  パネル系の入口は panel-runtime (runtime + api + html) と ui-shell (presentation) の 2 系統に整理。

```text
desktop ──┬── app-core
          ├── canvas ──────────── app-core, render-types
          ├── gpu-canvas ──────── app-core
          ├── render-types ────── app-core
          ├── storage ─────────── app-core
          ├── desktop-support ─── app-core
          ├── ui-shell ────────── app-core, panel-api, render-types
          └── panel-runtime ───── app-core, panel-api, panel-schema, plugin-host, panel-html
```

### 3. 不要な中間層・dead code の削除 (Phase 15A)

- `canvas/render_bridge.rs` → `input_state.rs` へ統合 (公開名 `panel_creation_preview_bounds` 維持)
- `canvas/registry.rs` → `plugins/mod.rs` へ統合
- `apps/desktop/src/app/drawing.rs` (1 行 re-export) / `commands.rs` (プレースホルダ) 削除
- `frame/geometry.rs` の `map_window_to_panel_surface` / `_clamped` 削除
  (テストからのみ参照される production dead code、Phase 9E の残骸)
- `SnapshotStore` の `label` フィールド削除 (production で未読)、
  `snapshot_create` の label 引数・service パラメータも撤去
- clippy 警告 79 → 0 (autofix + 手動修正。excessive_precision / collapsible_if /
  question_mark / needless_range_loop / drop_non_drop / too_many_arguments 等)

### 4. 命名改善と状態の構造化 (Phase 15D)

- `panel-runtime/src/registry.rs` → `runtime.rs` (定義する `PanelRuntime` とファイル名を一致、
  canvas 側の旧 registry.rs 削除と合わせ同名異義を解消)
- `apps/desktop/src/app/state.rs` → `canvas_state.rs` (責務 = キャンバスフレーム状態と overlay 構築)
- **`PresentInvalidation` 構造体** (present_state.rs): 提示無効化フラグ 9 個
  (pending_* dirty rect 3 + canvas_transform + deferred_* 2 + needs_* 3) を集約。
  `at_startup()` / `clear_pending()` で初期状態とリセットを構造化。
- **`GpuPaintEngine` 構造体** (app/mod.rs): GPU リソース 5 Option → 1 つの
  `Option<GpuPaintEngine>`。install 時の all-or-nothing 性を型で表現し、
  「pool は有るが brush が無い」という不正状態を表現不能にした。
- `DesktopIoState::pending_jobs` → `DesktopApp::background_jobs`
  (I/O パス管理とジョブキューの責務分離。操作箇所は background_tasks.rs のみ)。
- `DesktopApp` フィールド 31 → 18。

## 検証

- `cargo test --workspace`: **415 passed / 0 failed / 8 ignored**
  (ベースライン 412 passed に対し、新規の座標型テスト +5、削除した dead code のテスト −2)
- `cargo clippy --workspace --all-targets`: 警告 **79 → 0**
- `cargo machete`: クリーン (未使用依存なし)
- アプリ起動スモーク確認: 25 秒生存、クラッシュなし
- 受け入れ条件 (計画書 `.context/plan-phase15-refactor.md`) すべて充足:
  desktop 依存 11 → 8、workspace 28 → 27、DesktopApp 31 → 18 フィールド、
  対象公開 API から生座標タプル排除、正味 約 −800 行 (63 ファイル、+744 / −1553)

## スコープ外 (将来候補)

1. **`app_core::Panel` (コマ) の改名**: "Panel" の三義性 (ドメインのコマ / workspace UI パネル /
   HTML パネル) は残存する。改名はストレージ層・12 パネルプラグイン・全ドキュメントへ波及する
   独立した大規模変更のため見送り。漫画のコマの英訳として "panel" は標準語であり、UI 側は
   `WorkspacePanelState` 等の接頭辞で部分的に区別済み。
2. **`wgpu_canvas.rs` (2000+ 行) の分割**: 描画パイプライン再設計と密結合のため別フェーズ。
3. **tool 実行の plugin 主導化**: ROADMAP 上の独立フェーズ。
4. **`panel-api` が `app-core::Command` を知る点の再評価**: HostAction 境界の再設計が必要で、
   新 DTO 層を増やし「経路単純化」と逆行するため見送り。
