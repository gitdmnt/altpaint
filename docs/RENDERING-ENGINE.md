# altpaint レンダリングエンジン設計

## この文書の目的

この文書は、`altpaint` のレンダリングエンジンをどの粒度で分割し、どの順番で実装し、各責務をどのクレート（`paint-engine` / `canvas-geometry` / `gpu-paint` / desktop の `present_quads` ・ `presenter`）へ割り当てるかを詳細に記述する。

対象は主に以下である。

- キャンバス描画パイプライン
- タイル管理
- GPUアップロード
- ビュー変換
- オーバーレイ描画
- 将来的な高機能化への拡張余地

UIパネルのレイアウトやプラグイン実行は主目的ではない。そちらは `docs/ARCHITECTURE.md` を正とする。

## 目標

レンダリングエンジンの第一目標は、イラスト・漫画制作アプリとして次を両立することにある。

- 低遅延での描画応答
- 大きなキャンバスや多ページ作品でも破綻しにくいメモリ利用
- ズーム/パン/オーバーレイを含む一貫した表示
- 将来的な GPU 最適化や高機能ブラシへの拡張余地

## 基本原則

1. **表示責務と編集意味論を分離する**
   - 作品状態の正しさは `document-model` / `editor-state`
   - どう見せるかは `paint-engine` の計画 + presenter の提示
2. **全展開を避ける**
   - 見えていない範囲まで毎回フル合成しない
   - タイル単位、dirty 範囲単位で扱う
3. **提示と内部表現を分ける**
   - 永続化形式とレンダリング向け展開形式は分けてよい
4. **GPU所有権はホストが握る**
   - `apps/desktop` がデバイス・キュー・サーフェスを所有する（presenter）
   - `paint-engine` の計画と `gpu-paint` の compute dispatch がそれに渡す描画入力を作る
5. **オーバーレイを第一級に扱う**
   - スナップガイド、選択枠、ブラシプレビュー、コマ境界は独立レイヤーとして扱う
6. **キャンバス編集は GPU で行う**
   - スタンプ生成・ブラシ合成・塗りつぶし等のキャンバス編集操作は GPU compute shader で実行する
   - キャンバスビットマップは GPU テクスチャとして常時保持する
   - **キャンバス編集中のビットマップデータの CPU→GPU 転送は禁止**（差分であっても不可）
   - CPU は「座標・サイズ・色・ペン設定」などのパラメータ（uniform buffer）のみを GPU へ渡す
   - CPU が GPU テクスチャの内容を読み書きするのは、プロジェクトの保存・読込時のみ
   - CPU 側の `Vec<u8>` へのピクセル演算は行わない

## レンダリングエンジン全体像

描画パイプラインは概ね次の層に分ける。

1. ドキュメント層（`document-model`）
   - `Document`
   - `Page`
   - `Koma`（コマ）
   - `RasterLayer`
2. 描画準備層
   - 可視範囲計算
   - 表示対象のレイヤー解決
   - dirty 範囲解決
3. 合成層
   - ラスタレイヤー合成
   - マスク適用
   - 合成モード適用
4. 表示変換層
   - ズーム
   - パン
   - 回転は後回し
5. オーバーレイ層
   - 選択
   - スナップ
   - ページ/コマ境界
   - ブラシカーソル
6. 提示層（現行: `present_quads` の純データ DTO 生成 → `presenter` の GPU 提示）
   - quad / overlay / canvas plan の生成（旧 `FramePlan` / `DirtyFramePlan` 相当）
   - UI ベーステクスチャ更新
   - キャンバステクスチャ更新
   - オーバーレイテクスチャ更新
   - GPU quad 合成

## 2026-03-09 時点で固定した提示アーキテクチャ

パン・ズーム時の支配コストが CPU 側の表示変換にあることが分かったため、現行ホストは提示構造を次の 3 層へ切り替えた。

1. UI ベース層
   - CPU で生成する
   - 背景、パネル、ステータス、キャンバス host 枠と背景のみを持つ
2. キャンバス本体層
   - 元キャンバス bitmap を GPU テクスチャとして保持する
   - パン・ズームは GPU quad の位置と UV で適用する
3. オーバーレイ層 (Phase 9D 以降は GPU 直描画)
   - L3a: AABB 単色矩形 (active panel mask / panel creation preview /
     panel navigator) を `SolidQuadPipeline` で描画
   - L3b: ブラシプレビュー円リングを `CircleQuadPipeline` (SDF) で描画
   - L3c: ラッソ線分を `LineQuadPipeline` (カプセル SDF) で描画
   - 各 quad は `apps/desktop/src/frame/overlay_quad.rs` の純関数ビルダで
     `CanvasOverlayState` から毎フレーム組み立てる
   - CPU フレーム (`compose_temp_overlay_*`) は廃止済

これにより、**パン・ズームでは CPU がキャンバス画素を再サンプリングしない** ことを現行アーキテクチャの正本とする。

## MVPでの描画契約

MVP時点では、レンダリングエンジンが満たすべき最低契約を以下とする。

- 入力
  - `Document`
  - 表示対象のページ/コマ
  - ビュー変換
  - dirty 範囲
- 出力
   - GPU へ渡すキャンバステクスチャ更新情報
   - UI ベースフレーム
   - オーバーレイフレーム
   - キャンバス quad / UV 情報
- エラー/制約
  - 不正なレイヤー参照は描画スキップ
  - GPUメモリ不足時は解像度やキャッシュ戦略を落とす余地を残す

成功条件:

- 描画パスが UI ランタイムとは独立に成立する
- 大半の更新で全画面再構築を避けられる

## 主要コンポーネント

> 注: 旧 `render` クレートは Phase 9F で物理削除済み。`RenderContext` /
> `RenderFrame` / `FramePlan` / `CanvasScene` / `RenderGraph` といった旧名は実在しない。
> 以下は現行（ADR 018 / B10 完了後）の対応コンポーネントである。MVP 期の設計スケッチ
> （タイルキャッシュ、合成器、`CanvasViewTransform` の独立型等）は未実装の将来余地として
> 残すが、現に動いている構造は「描画データフロー（現行 GPU 経路）」「責務分割の現在地」
> 「最小モジュール構成」の各節を正本とする。

### 1. 計画 / 表示幾何（`paint-engine` + `canvas-geometry`）

責務:

- `paint-engine::plan_paint` が `PaintInput` から `PaintPlan`（純データ計画）を組み立てる入口
- `canvas-geometry::CanvasViewGeometry` が view↔page 座標写像と表示幾何を提供する
  - `TextureQuad`
  - dirty rect の表示先写像
  - view 座標 ↔ page 座標変換
  - ブラシプレビュー矩形（`features/paint/preview.rs`）

旧 `CanvasScene` / `FramePlan` / `CanvasPlan` / `OverlayPlan` / `PanelPlan` /
`DirtyFramePlan` は次のように整理済み:

- `CanvasViewGeometry` 単一経路へ縮約（ADR 018 B5）
- `CanvasPlan` / overlay DTO は `apps/desktop` へ移管
- パネル面は `PanelRuntime::render_panels` の GPU 直描画で、計画に含まれない（ADR 016）
- base / overlay / panel / status の compose は presenter（`apps/desktop/src/presenter/`）が担う（overlay / panel は GPU 直描画）

### 2. `CanvasViewTransform`

責務:

- キャンバス座標と画面座標の相互変換
- ズーム倍率
- パンオフセット
- 可視範囲計算

保持したい最低情報:

- `zoom`
- `pan_x`
- `pan_y`
- `viewport_width`
- `viewport_height`

### 3. タイルキャッシュ

責務:

- キャンバスをタイル単位で保持する
- dirty 範囲に該当するタイルだけを再合成する
- キャッシュメモリ上限に応じて追い出す

MVPでは単純なフルフレームでもよいが、設計上は最初からタイル化前提の境界を持っておく。

推奨するタイル単位例:

- 128x128
- 256x256

実際の最適値はプロトタイプ後に再調整する。

### 4. 合成器

責務:

- レイヤーツリーを走査し、表示対象のラスタを合成する
- 合成モードを適用する
- マスクを適用する

MVPで対象にするレイヤー種別:

- ラスタ
- テキスト
- グループ
- マスク

MVPで対象にする合成モード:

- 通常
- 乗算
- スクリーン
- 加算

### 5. オーバーレイ描画器

責務:

- ドキュメント本体とは別に、UI支援描画を重ねる

対象:

- 選択枠
- スナップガイド
- コマ境界
- ページ枠
- ブラシプレビュー

重要なのは、これらを直接キャンバス内容へ焼き込まないことだ。

## 描画データフロー（現行 GPU 経路 / ADR 018 B8 以降）

1. `apps/desktop` が入力種別、筆圧、window 座標を受け取る
2. `paint-engine` の view↔page 写像（`canvas-geometry::CanvasViewGeometry`）で page 座標へ変換し、`advance_pointer_gesture` が `PaintInput`（Stamp / StrokeSegment / FloodFill / LassoFill）を生成する
3. `paint-engine::plan_paint(document, input)` が `build_paint_context` で active コマ / active layer / composited bitmap / active color / active tool を解決し、**画素を一切作らずに** `PaintPlan`（純データ計画: stamps 列 / seed / polygon + color + dirty rect）を返す
4. `apps/desktop/src/features/paint/execute.rs::apply_paint_input` が、GPU 有効時は `GpuPaintBackend`、不在時は `CpuPaintBackend` を選び、`PaintPlan` を適用させる（`PaintBackend` trait。BL-131）
5. `GpuPaintBackend` が `PaintPlan` を `BrushStrokeParams` / fill パラメータへ機械変換し、`BrushPipeline` / `FillPipeline` の compute shader を **GPU レイヤーテクスチャへ直接 dispatch** する（編集中に CPU 画素を作らない。flood fill は GPU 上のピンポンマスクで連結成分を解決し CPU visited 配列を持たない）。ストロークは brush pass と composite pass を 1 encoder へ積み 1 submit に集約する（BL-133）
6. `CompositePipeline` がコマの可視レイヤーを GPU 上で合成し composite テクスチャを更新する
7. presenter（`apps/desktop/src/presenter/`）が canvas composite テクスチャと、CPU 側で組み立てた overlay / panel quad を `quad / UV` で受け取り、GPU が base → canvas → temp overlay → ui panel の順に合成提示する
8. undo/redo は `PaintPatch`（`Cpu` / `Gpu` 型付きスナップショット）で記録する。GPU 経路は dirty 領域の before/after を GPU テクスチャスナップショットとして保持する

GPU 不在時のフォールバック（BL-136、GPU 必須化はしない）:

- `CpuPaintBackend` が `paint-engine` の CPU ops（`compute_paint_edits`）でアクティブレイヤービットマップへ書き込み、`CpuCanvasSnapshot`（旧 `CanvasFrame`）が表示経路へ画素を供給する

補足:

- dirty rect による差分更新は描画意味論ではなく表示更新の責務であり、`PaintPlan.dirty` を典拠に presenter 側で扱う
- `PaintBackend` は「どの計画をどう適用するか」に集中し、最終提示戦略は presenter が持つ

## 責務分割の現在地（ADR 018 完了後）

> 旧 `render` クレートは Phase 9F で物理削除、旧 `app-core` クレートは ADR 018 B5 で
> `geometry` / `raster` / `document-model` / `editor-state` の 4 クレートへ解体済み。

### ドメイン / セッション層（`document-model` / `editor-state`）

- `Document`（作品コンテンツ）と `EditorSession`（一過性編集状態）を保持する。`wgpu` / `winit` 非依存
- `CanvasViewTransform` のような**ユーザー操作で変化する view state** は `EditorSession` が保持する
- `SessionCommand`（`editor-state`）の意味論として zoom / pan / reset を受け持つ（`view_policy` に倍率・clamp・パン量を集約）
- `ToolDescriptor`（`editor-state`、BL-134）がツール種別から gesture 種別 / 合成モード / サイズ解決を導出し、`ToolKind` のクローズド match を 1 箇所へ集約する

### 計画層（`paint-engine`）

- `plan_paint` が `PaintInput` を `PaintPlan`（純データ）へ計画する。画素は作らない
- view↔page 写像 / 可視範囲 / quad / UV / overlay 幾何は `canvas-geometry::CanvasViewGeometry` が計算する

### 適用 / 提示層（`apps/desktop` + `gpu-paint`）

- `apps/desktop/src/features/paint`: `PaintBackend`（Cpu / Gpu）の選択・適用・履歴
- `gpu-paint`: brush / fill / composite の compute shader 実装（`wgpu` 依存）
- presenter: `winit` / `wgpu` 所有、desktop 固定レイアウト算出、最終提示

したがって、**state はドメイン / セッション層、計画は `paint-engine`、画素適用は backend（GPU 実装は `gpu-paint`）、提示は presenter** という分割を正とする。

## CPU と GPU の役割分担

### 設計原則（すべての段階に適用）

**キャンバスへの描画は必ず GPU を使って行う。CPU と GPU 間の通信は最低限に抑える。**

| 処理                                       | 担当         | 根拠                                           |
| ------------------------------------------ | ------------ | ---------------------------------------------- |
| スタンプ生成・ブラシ合成                   | GPU          | O(size²) の演算。CPU では size=200 以上で実用不可 |
| 塗りつぶし（Flood Fill / Lasso Fill）      | GPU          | O(area) の演算。大きなキャンバスでブロッキング発生 |
| パン・ズーム適用                           | GPU          | quad / UV 更新のみ。CPU 再サンプリング禁止      |
| キャンバスビットマップ保持                 | GPU          | テクスチャとして常時 GPU 上に保持               |
| 編集中のビットマップ CPU→GPU 転送          | **禁止**     | 差分であっても転送しない。GPU 上で完結させる     |
| 描画パラメータ（座標・サイズ・色等）の転送 | CPU→GPU OK   | uniform buffer のみ。ピクセルデータではない      |
| ビットマップ読み書き（保存・読込時）       | CPU↔GPU 許可 | プロジェクト I/O 時のみ例外的に許可              |
| ドキュメント走査・dirty 範囲計算           | CPU          | 状態管理・論理演算                              |
| UI ベースフレーム・オーバーレイピクセル    | CPU          | パネル等の非高頻度描画                          |

### 現状（ADR 018 B8 完了後 = GPU 経路が正）

GPU 利用可能時、キャンバス編集は GPU 経路で完結する。`PaintPlan`（計画、画素なし）→
`GpuPaintBackend` → compute shader dispatch という流れで、編集中に CPU 画素バッファを
生成しない（ストローク・flood fill ともに。回帰テスト
`features::paint::backend::tests::golden_equivalence::gpu_apply_does_not_touch_cpu_pixels`
で CPU ビットマップが apply 前後で不変であることを担保）。旧 CPU ベース編集
（`Vec<u8>` でのブラシ合成、太いブラシで O(size²) × 64 ステップの UI ブロッキング）は
GPU 経路へ移行済み。

CPU が担うのは:

- ドキュメント走査・dirty 範囲計算
- `paint-engine::plan_paint` による高レベルな描画計画（座標・サイズ・色・dirty）
- 描画パラメータ（uniform buffer）の組み立てと GPU へのバインド
- UI ベースフレーム生成・オーバーレイピクセル生成（パネル等の非高頻度描画）
- GPU 不在時のみ `CpuPaintBackend` による参照実装の CPU 合成（BL-136 フォールバック）

GPU が担うのは:

- キャンバスビットマップの常時保持（レイヤーテクスチャ。`gpu-paint::LayerTextureStore`）
- ブラシスタンプ生成・合成（compute shader。`BrushPipeline`）
- 塗りつぶし（compute shader。`FillPipeline`、ピンポンマスクで CPU visited 配列を持たない）
- レイヤー合成（`CompositePipeline`）
- パン・ズーム（quad UV 更新。テクスチャ再生成なし）
- base / canvas / overlay の最終合成表示

## Dirty 更新戦略

レンダリングエンジンは、編集のたびにフルフレーム再構築するのではなく、dirty 範囲中心で設計する。

想定する dirty 種別:

- `StrokeDirtyRect`
- `LayerVisibilityChanged`
- `ViewTransformChanged`
- `OverlayOnlyChanged`
- `DocumentStructureChanged`

更新方針:

- ストローク中はストローク周辺の dirty bitmap または dirty タイルだけ再合成し、その範囲だけ GPU キャンバステクスチャへ送る
- ズーム/パンはキャンバステクスチャ自体を再生成せず、quad / UV 更新で済ませる
- オーバーレイだけ変わる場合はオーバーレイフレームだけを更新する
- パネルやステータスだけ変わる場合は UI ベースフレームだけを更新する

## ビュー変換設計

ビュー変換では次を保証する必要がある。

- キャンバス上の点と表示上の点が一意に往復変換できる
- パン・ズーム・入力座標変換でズレない
- UIパネル領域とキャンバス領域が明確に分離される

MVPで扱う変換:

- 平行移動
- 一様スケール

後回し:

- 回転
- 透視変換
- 非一様スケール

## レンダリングエンジンの最小モジュール構成

Phase 9F (2026-04-29) で旧 `crates/render/` は物理削除され、ADR 018 / B10 完了時点では
純データ DTO・固定レイアウト計算・GPU 提示が次のように配置されている:

- `crates/canvas-geometry/src/` — `CanvasViewGeometry`（view↔page 写像 + `TextureQuad`）。表示幾何の単一経路
- `apps/desktop/src/present_quads/` — desktop 固定レイアウト計算と presenter 入力 DTO（`CanvasPlan` / `CanvasOverlayState` / solid・circle・line quad ビルダ / `LayerDirtyAccumulator`）。純関数で `wgpu` 非依存
- `apps/desktop/src/presenter/` — `wgpu` 提示パイプライン（旧 `wgpu_canvas.rs` を `shaders` / `pipelines` / `textures` / `frame` / `theme` へ分割）
- `apps/desktop/src/app/cpu_canvas_snapshot.rs` — CPU キャンバススナップショット (`CpuCanvasSnapshot`、旧 `render::RenderFrame`)。GPU 不在時フォールバックと viewport / 表示幾何計算で参照

純データ DTO（`present_quads`）と GPU 提示（`presenter`）を分離する責務境界は崩さない。パネル面は
この計画 DTO に含まれない（`PanelRuntime` による GPU 直描画。ADR 016）。

## パフォーマンス目標との接続

`docs/SKETCH.md` にある暫定性能目標に対して、レンダリングエンジン側では次を担保したい。

- 4K相当キャンバスのパン/ズームで体感上引っかからないこと
- バックグラウンド書き出し中でも主要UIを止めないこと
- 入力全体を無加工で丸ごと展開しないこと

このため、レンダリングエンジン側では特に以下を重視する。

- dirty 範囲中心の更新
- タイルキャッシュ
- 表示と保存の分離
- オーバーレイの独立更新

## 保存形式との関係

レンダリングエンジンは保存形式と密結合しない。

ただし、次の要件は保存形式に要求する。

- コマ単位・ページ単位の部分ロード
- タイル/チャンク単位の読み出し
- 差分保存またはフル保存の切替余地

つまり、レンダリング側の要求は `project-store`（SQLite 永続化）に伝えるが、`paint-engine` / presenter が直接 SQLite やファイル構造を知る必要はない。

## 将来拡張

後の段階で追加したいもの:

- ベクター線レンダリング
- GPU 支援ブラシ
- 高度なテキストレイアウト
- 調整レイヤー
- 色管理
- 高解像度サムネイルと proxy 表示

## 実装優先順位

1〜3 は ADR 018 / B10 時点で達成済み（現行構造）、4〜6 は将来余地である。

1. ✅ 安定提示（`present_quads` 純データ DTO → `presenter` GPU 提示。旧 `RenderFrame` 経路は撤去）
2. ✅ view 変換の集約（`canvas-geometry::CanvasViewGeometry` 単一経路 + `editor-state` の `view_policy`）
3. ✅ dirty 範囲の明示化（`PaintPlan.dirty` / `LayerDirtyAccumulator`）
4. タイルキャッシュ導入（未着手）
5. オーバーレイの独立描画（GPU 直描画化済み。さらなる独立更新は将来）
6. 合成モード拡張（将来）

## この文書の結論

`altpaint` のレンダリングエンジンは、単なる「画像を表示する層」ではない。

それは、

- 漫画制作向けの大きな作品を扱い
- 低遅延入力を維持し
- パネルUIとは独立に進化できる

ための中核基盤である。

したがって、実装は当面シンプルでよくても、責務境界だけは最初から

- ドキュメント意味論
- 描画計画
- 合成
- オーバーレイ
- GPU提示

に分けて育てる。
