# ADR 018: 用語体系の確立と垂直スライス再編 (大規模リファクタリング)

- 作業日時: 2026-06-12 (起票。各バッチ完了時に追記し、B10 で確定)
- 作業 Agent: claude-fable-5 (Claude Code)
- ステータス: 進行中
- 設計書: [docs/refactor/2026-06-naming-and-boundaries.md](../refactor/2026-06-naming-and-boundaries.md)

## 背景

19 エージェントによる全クレート調査 (2026-06-12) で、次の構造問題が列挙された。

1. **命名と責務の乖離**: `app_core::Panel` (漫画のコマ) と UI パネルの衝突、`plugin-*` クレート群が実態はパネル専用、`CommandHistory` が Command を保持しない、`canvas` / `gpu-canvas` / `render-types` の用語混線、ほか約 100 件。
2. **責務集中**: `Document` の God オブジェクト化 (作品ツリー + セッション状態 + レイアウトポリシー + ラスタ演算)、`DesktopApp` の orchestration 肥大、`build_host_state` / `command_from_descriptor` の水平チョークポイント化。
3. **境界リーク**: `Command` enum への I/O variant 混入 (apply_command が no-op で握り潰し desktop が再変換する二重ディスパッチ)、panel-api の app-core 依存、GPU dispatch 判断の desktop 漏出。
4. **実バグ**: 筆圧カーブ二重適用 (CPU/GPU 線幅乖離)、HostSnapshotCache の stale 配信、`panel_rect` の usize::MAX フォールバックによる右/下アンカーの dirty rect 無効化。

## 決定

詳細は設計書を正本とする。骨子:

### 1. 用語体系

| 用語 | 意味 |
| --- | --- |
| **koma** / `Koma` | 漫画のコマ (旧 `app_core::Panel`) |
| **panel** | ワークスペース上の UI パネル (HTML+Wasm) 専用 |
| **page** | 作品のピクセル空間 (描画対象座標系) |
| **canvas** | キャンバスの表示面 (ビューポート上の見え方) |
| **paint** | 描画入力の解釈・差分生成・GPU 実行 |
| **frame** | 提示の 1 描画フレーム専用 |
| **plugin** | 予約語 (現行コードで使用禁止。将来の拡張機構名) |
| **host state** | ホスト→パネルへ配る状態 JSON (旧 host snapshot) |
| **snapshot** | `DocumentSnapshot` (ユーザー向け快照機能) 専用 |
| **runtime** | `panel-runtime` 専用 |
| **session** | エディタの一過性編集状態 |

### 2. コマ = `Koma` の採用理由

`Frame` は提示系語彙 (描画フレーム、profiler) と再衝突し Panel 二義問題を Frame 二義問題に移し替えるだけになる。`ComicPanel` は "Panel" 部分文字列が残り grep 分離不能。`Koma` は衝突ゼロ・grep 一意・UI 表示「コマ N」(プロジェクト文書は日本語が正本) と一致する。

### 3. クレート再編 (水平土台 + 垂直 feature)

- 水平土台: `geometry` / `raster` / `document-model` / `editor-state` / `canvas-geometry` / `frame-profiler` (旧 app-core / render-types を解体)
- パネル基盤 (水平): `panel-protocol` / `panel-wasm-host` / `panel-html` / `panel-runtime` / `panel-workspace` / `panel-sdk` / `panel-macros` (旧 panel-schema / plugin-host / ui-shell / plugin-sdk / plugin-macros を改名、panel-api は解体)
- 垂直 feature: `paint-engine` (旧 canvas) / `gpu-paint` (旧 gpu-canvas) / `project-store` + `pen-io` (旧 storage を分割)。desktop-support は解体
- desktop 内: `features/` 12 スライス (paint / project / export / workspace / tools / koma / view / snapshots / text / panel_interaction / status_bar 等)

### 4. 原則の例外: wire 名定数

「feature 固有の知識を水平土台に置かない」原則の唯一の例外として、Wasm 境界の wire 名定数は `panel-protocol` の feature 別 `names::<feature>` モジュールに置く。共有契約点が必要であり、新 feature は自分のモジュールを additive に追加するだけで既存モジュールの横断編集が発生しないため、垂直分割の目的 (横断編集の排除) とは矛盾しない。

### 5. その他の確定判断

- **GPU 非必須を維持** (BL-136): `CpuPaintBackend` を GPU 初期化失敗時の表示フォールバックとして残す。GPU 必須化はしない。
- **コマ合成は即時合成を維持** (BL-080): レイヤー変異の単一ミューテーション入口に集約。B8 の profiler 計測で合成コストが問題化した場合のみ dirty フラグ遅延評価へ切替える。
- **perf 項目の in/out 基準**: 境界修正・API 再形成に随伴して解消される性能問題は in-scope (GPU 同期粒度、encoder 集約、flood fill の CPU 走査廃止)。新規実装を要する性能機能 (タイルキャッシュ、hit 収集 dirty スキップ) は ROADMAP へ。
- **互換性は捨てる** (alpha 方針): Koma 改名 + SQLite スキーマ変更 + パネル ID 改名により、既存のプロジェクトファイル・セッション・ワークスペースプリセットは読めなくなる。マイグレーションは書かない。
- **Blitz/stylo グローバル Mutex** (`STYLE_RESOLVE_LOCK`): 外部ライブラリ制約のため維持。マルチウィンドウ/並行 resolve が要件化した時点で再評価。
- **レイヤー操作の安定 id 指定** (BL-148): layers パネルの index 反転問題は表示順 index ではなく `RasterLayer.id` ベースの request に統一して解消する。

## 実装バッチ

B0 死コード一掃 → B1 Koma 用語統一 → B2 クレート・型機械改名 → B3 重複一本化 + 既知バグ修正 → B4 コマンド経路一本化 → B5 土台再編 → B6 パネル境界整理 → B7 desktop 垂直分割 → B8 描画パイプライン再設計 → B9 パネル API 再設計 → B10 文書改稿。

各バッチの完了条件: `cargo test --workspace` 0 failed / `cargo clippy --workspace --all-targets` 警告 0 / wasm ビルド成功 / バッチ別スモーク (設計書 §4)。

## 経過記録

- 2026-06-12: 起票。調査 (19 agents) → 設計 → judge panel 3 レンズ × 3 ラウンドのレビューを完了し実装開始。ベースライン: テスト 415 passed / 0 failed / 8 ignored、clippy 警告 0。
- 2026-06-12: **B0 完了** (死コード一掃と文書浄化、claude-fable-5)。コミット 4bdeb13..7a59cc6 の 28 コミット (145 ファイル / +412 −5,143 行)。テスト 415 → 405 passed / 0 failed / 7 ignored (減少は死 API テストの削除によるもの。ラウンドトリップ/旧形式拒否テストを新規追加した上での正味値)、clippy 警告 0。skipped: BL-023 (`paint_params` モジュール削除) は `MAX_STAMP_STEPS` が gpu-canvas からも参照されるため B0 では実施せず、B8 の PaintPlan 化 (BL-130 で stamps が計画側に移り定数が paint-engine のみで完結) と同時に解決。BL-009 の一部公開 API (manifest 系ほか) は live 設計要素のため残置し B7 で再判断。
- 2026-06-12: **B1 完了** (用語統一第 1 波 — Koma、claude-fable-5)。コミット 17b20cc..6139d36 の 23 コミット (118 ファイル / +1,785 −1,477 行)。BL-036 (wire 名定数の `panel-schema::names` 一元化) を冒頭で先行実施した後、K1〜K13 (K11 は `panel_id` 引数の文字列改名のみ。型付き `KomaId` キー化は B8 BL-135)・C14〜C17 (パネル ID 改名: `builtin.koma-list` / `builtin.layers` / `builtin.snapshots` / `builtin.tool-settings`)・D16 (`DocumentSnapshotStore`) を実施。SQLite スキーマは `komas` / `koma_composites` / `layers.koma_id` へ書換えのみ (マイグレーションなし。alpha 方針により旧プロジェクトファイル・セッションは非互換 — 本 ADR「互換性は捨てる」を適用)。コマ矩形ツールのカタログ id も `builtin.panel-rect` → `builtin.koma-rect` へ追随。バッチ検証 (grep ゲート: `panel_nav` / `PanelId|PanelBounds|PanelLocalPoint` / `page_panel_count|active_panel_index` / `ToolKind::PanelRect` / 旧パネル id = すべて 0 件、`rg -i panel` の app-core ドメインモジュール検査) で検出した残存 (document.rs / layer_ops.rs / history.rs のコマ義メソッド・フィールドと desktop 呼出側) は検証修正コミット 6139d36 で一掃。テスト 408 passed / 0 failed / 7 ignored (B0 比 +3 は names モジュールのテスト追加)、clippy 警告 0、wasm ビルド成功、起動スモーク (パニックなし)。K14 (`NewDocument` → `NewProject`) は計画どおり B4 の Command 分割と同時に実施する。
- 2026-06-12: **B2 完了** (用語統一第 2 波 — クレート・型・モジュール機械改名、claude-fable-5)。コミット 7f71efe..59a4272 の 50 コミット。クレート改名 9 件 (C1〜C8、C13): `canvas`→`paint-engine` / `gpu-canvas`→`gpu-paint` / `render-types`→`canvas-geometry` / `plugin-host`→`panel-wasm-host` / `plugin-sdk`→`panel-sdk` / `plugin-macros`→`panel-macros` / `panel-schema`→`panel-protocol` / `ui-shell`→`panel-workspace` / desktop package→`altpaint-desktop` (bin `altpaint`)。型・関数・モジュール改名: R2 (`EditHistory`)、R4 (`normalize_after_load`)、R6〜R10 (`PaintEngine` / `compute_paint_edits` / `apply_paint_input` / `PagePoint` 系)、R13 (`CanvasViewGeometry`)、R16〜R20 (`LayerDirtyAccumulator` / `accumulate_dirty_rect` / `LayerTextureStore` / `GpuRgbaTexture` / `BrushPipeline` 系)、R23/R24/R26〜R30 (`create_snapshot_texture` / `ProjectStoreError` / `CURRENT_PROJECT_FORMAT_VERSION` / `pen_catalog` / `file_has_sqlite_header` / `StoredPenEngine` / `ProjectManifest`)、R32〜R39 (`CanvasSizePreset` / `FrameProfiler` / `DEFAULT_PROJECT_FILE_NAME` / `builtin_panels_dir` / `DEFAULT_PAGE_WIDTH/HEIGHT` / `PanelConfigs` / `tool_state` / `brush_size_for_pressure`)、R41 (`Koma.composite_cache`)、P2/P4/P5/P7〜P9 (`HtmlWasmPanel` / `RequestDescriptor` / `HandlerEffects` / `PanelWasmInstance` / `HostCallContext` / `PanelWasmHostError`)、P12〜P18/P20〜P23/P29/P30 (`host_state` / `request_translation` / `persistent_config` / `PanelWorkspace` / html_ 接頭辞除去 / `ResizeHandle` / 二重名 1 本化 / `HtmlPanelView` / `ActionRect` / `panel_size` / `PanelDispatchResult` / `RenderedPanelTexture`)、D1/D3〜D8/D11 (`event_loop.rs`+`DesktopEventLoop` / `PresentFrame` / `present_quads/` / `frame::Rect` 削除 / `StatusBar` / `CpuCanvasSnapshot` / `CanvasSurface` 系 / `invalidation.rs`)。計画どおりの保留: R11/R12 は B5、R21/R22 は B8、D2/D12/D13/D15 は B7、P6 は B6。検証修正 59a4272 (旧名を含む stale 削除テストメモの除去) のみ・コード移動/分割なし、全コミット「改名のみ」分離を維持。検証: テスト 408 passed / 0 failed / 7 ignored (B1 と同数 = 改名のみ)、clippy 警告 0、wasm ビルド成功、起動スモーク 22 秒パニックなし、旧名残存 grep 0 件 (wgpu_canvas.rs 由来の `gpu_canvas` 識別子は D2 = B7 スコープのため対象外)。
- 2026-06-12: **B3 / BL-030 完了** (筆圧カーブ二重適用の修正、claude-fable-5)。筆圧→実効サイズの解決を context 解決時 (`Document::brush_size_for_pressure` → `resolved_size`) の 1 回に統一し、`paint-engine/ops/stamp.rs` の `effective_size` (再適用) を削除。**意図的な挙動変更であり修正後が正**: 筆圧 < 1.0 の CPU 経路の線幅は従来の `round(round(base*f)*f)` から `round(base*f)` へ太くなり、GPU 経路 (`BrushStrokeParams.radius`) と一致する。TDD: カーブ 1 回適用の期待値テスト (paint-engine) + CPU/GPU 半径一致回帰テスト (desktop、GPU 実行不要のパラメータ比較) を red 確認後に修正。GPU パラメータ組立は desktop `project_io.rs` の `brush_stroke_params` 純関数へ抽出。テスト 410 passed / 0 failed / 7 ignored (+2)、clippy 警告 0。
- 2026-06-13: **B3 / BL-032・BL-033 完了** (ピクセルブレンド統合 + extract_region 重複統合、claude-fable-5)。CPU 合成のゴールデンテスト (代表 BlendMode × アルファ組合せ、ブラシ AA 被覆、BitmapComposite、storage 再計算経路) で現挙動を固定した後、`app_core::blend` を新設し単一実装へ差し替え: `composite_pixel` (BlendMode つき straight-alpha source-over = レイヤー/BitmapEdit 合成、GPU `layer_composite.wgsl` と同一式) / `source_over_coverage_pixel` (被覆率つきブラシ式) / `composite_layers` / `composite_layer_region_into`。旧実装 4 箇所 (app-core layer_ops/painting/bitmap + storage/project_sqlite) と paint-engine の同式複製を削除。3 実装間の丸め・クランプ差異はなし (挙動不変)。ブラシ式と合成式は意図的に別関数のまま維持 (前者は dst alpha 重み付け + out alpha 正規化で透明地の色を保持)。BlendMode→WGSL コード対応表は `blend.rs` のモジュール doc + `BlendMode::gpu_code` に単一文書化し、WGSL 側はコメント参照で同期 (自動生成なし)。BL-033: layer_ops の `extract_bitmap_region` (同一実装) を削除し `CanvasBitmap::extract_region` に統合。テスト 423 passed / 0 failed / 7 ignored (+6: ゴールデン 5 + storage 再計算 1)、clippy 警告 0。
- 2026-06-13: **B3 完了** (重複一本化と既知バグ修正、claude-fable-5)。コミット f3ffd8e..c428ede の 21 コミット (BL-030〜051、BL-036 は B1 冒頭で実施済みのため除く)。ユーザー可視バグ 3 件を TDD で修正 (BL-030 / BL-031 / BL-051。**いずれも意図的な挙動変更であり「修正後が正」**): (1) **BL-030 筆圧カーブ二重適用** — 上記参照。筆圧 < 1.0 の CPU 線幅が従来 `round(round(base*f)*f)` から `round(base*f)` へ太くなり GPU 経路と一致 (修正後が正)。(2) **BL-031 HostStateCache stale 配信** — キャッシュ無効化を件数+index から内容ベースへ変更 (layers キーに name/visible/blend_mode/masked、komas キーに bounds、pen_presets キーにプリセット内容)。従来はレイヤー名変更やプリセット内容編集がパネルへ反映されなかったのが正しく再 render される (修正後が正)。layers_json 反映の回帰テスト追加。完全な revision 化は B6 (BL-093)。(3) **BL-051 panel_rect の usize::MAX フォールバック** — viewport なし版を廃止し viewport 必須 API へ統合。右/下アンカーパネルが従来は画面外座標 (usize::MAX 由来) を返し dirty rect が無効化されていたのを修正 (修正後が正)。右下アンカー解決の回帰テスト追加。重複一本化 (挙動不変): BL-034 (`parse_document_size` + 上限定数を app-core 1 箇所へ)、BL-035 (`StatePatch` 適用を `panel-protocol::apply_patches` へ一本化)、BL-037/038 (panel-wasm-host の文字列コピー host fn 4 重複 + read_utf8/current_memory 二重定義を `memory.rs` へ、load を `state_api`/`host_state_api`/`request_api`/`dom_api` の register モジュールへ分割)、BL-039/040 (gpu-paint の build_pipeline 3 重複 + alpha 展開 2 重複を `pipeline.rs` へ共通化、矩形 3 流儀を半開矩形型 1 つへ統一)、BL-041/042 (`PixelRect` を `app_core::WindowRect` へ統合し desktop の毎フレーム手書き変換と `view_mapping.rs` ラッパーを廃止、view 座標変換は desktop が `canvas-geometry::map_view_to_canvas_with_transform` を直接呼ぶ)、BL-043 (panel-html の viewport クランプ規則を `local_render_size` 抽出)、BL-044 (`ToolKind` の wire/表示マッピングを app-core へ集約)、BL-045〜047 (desktop の dirty rect 畳み込み `merged_dirty` / ピクセル→NDC 変換 `pixel_rect_to_ndc` / コマンド後副作用 `invalidate_document_structure` を集約)、BL-048 (storage のディレクトリ走査を `collect_files` へ)、BL-049 (JSON 設定ロードを Loaded/Missing/Corrupt 区別の共通ローダへ。破損時のユーザー編集消失を防止)、BL-050 (gesture の Down 経路の未使用 stabilization 引数除去)。検証: workspace テスト 438 passed / 0 failed / 7 ignored (B2 の 408 から +30: バグ修正 3 件の回帰テスト + CPU 合成ゴールデン + storage 再計算 + 各重複統合のテスト)、clippy 警告 0、wasm ビルド成功、gpu-paint GPU スモーク (ローカル NVIDIA RTX 2070、Vulkan) 22 passed、起動スモーク 20 秒パニックなし、重複解消 grep 確認 (ブレンド実装 1 箇所 = `app_core::blend`、`extract_region` 1 箇所、`apply_patches` panel-protocol のみ、`parse_document_size` 1 箇所、`panel_rect` の usize::MAX フォールバック不在)。
