# ADR 007: HTML/CSS パネル実験層の追加（Blitz バックエンド）

- 作業日時: 2026-04-25
- 作業モデル: claude-opus-4-7 (1M context)
- 関連プラン: `C:\Users\tofuh\.claude\plans\servo-html-css-buzzing-moler.md`
- 改訂履歴:
  - 2026-04-25: 初稿（自作 HTML/CSS サブセットでの PoC）
  - 2026-04-25: **第 2 稿**。ユーザーから「自作にした根拠が弱い」「Blitz を実際に試すべき」との指摘を受け、自作層を破棄して Blitz をバックエンドに採用
  - 2026-04-25: **第 3 稿（Phase 8F）**。ユーザーから「CPU-GPU 通信は Phase 8 系の方針に反する」との指摘を受け、`anyrender_vello_cpu` を完全削除し、`vello::Renderer::render_to_texture` で altpaint 所有の wgpu テクスチャに直接描画する **GPU 直描画方式**へ移行。CPU pixels 経路はゼロ。

---

## Phase 8F 追記: GPU 直描画への移行 (2026-04-25)

### 背景

第 2 稿の Vello-CPU 経路は `anyrender_vello_cpu` で CPU 上に RGBA8 を作り、`queue.write_texture` で GPU にアップロードしていた。これは Phase 8D「CPU-GPU 通信最小化」と Phase 8E「GPU 塗りつぶし + レイヤー合成」の方針に逆行。HTML パネル数が増えるごとに CPU→GPU 転送量が線形増大する構造だった。

### 決定事項

#### 依存アップグレード（vello 0.8 / wgpu 28.0 整合）

```
blitz-dom    0.2.4         → 0.3.0-alpha.2
blitz-html   0.2.0         → 0.3.0-alpha.2
blitz-paint  0.2.1         → 0.3.0-alpha.2
blitz-traits 0.2.0         → 0.3.0-alpha.2
anyrender    0.6.x         → 0.8
anyrender_vello_cpu 0.8.1  → **削除**
anyrender_vello            → **新規** 0.8（GPU バックエンド）
vello                      → **新規** 0.8（wgpu 28.0 整合）
```

#### アーキテクチャ

- `PanelRuntime::install_gpu_context(device, queue)` で altpaint 所有の `Arc<wgpu::Device>` `Arc<wgpu::Queue>` を共有
- `vello::Renderer` は `PanelRuntime` が 1 つだけ集約所有（コスト数百 ms、device 毎で十分）
- `HtmlPanelPlugin` がパネル毎の `PanelGpuTarget` テクスチャ（Rgba8Unorm + STORAGE_BINDING + view_formats=[Rgba8UnormSrgb]）を所有
- `HtmlPanelEngine::build_scene(&mut vello::Scene, w, h, scale)` が `blitz_paint::paint_scene` で Vello シーンを構築
- `vello::Renderer::render_to_texture(device, queue, &scene, &target_view, params)` で GPU に直接書く
- 合成: `WgpuPresenter::PresentScene::html_panel_quads: &[GpuPanelQuad]` フィールドに各 HTML パネルの `(texture, screen_rect)` を渡す。L4 ui_panel_layer 直後にテクスチャ付きクワッドとして追加描画

#### dirty 判定

- `HtmlPanelPlugin::render_dirty: bool`
- `update()` で `apply_bindings` 後に DOM mutation があれば true（自前トラック、`BaseDocument::has_changes` は信頼できないため `apply_binding_target` の戻り値で集計）
- `render_gpu()` が target サイズ変化を検知したら true
- `RenderOutcome::{Rendered, Skipped}` enum を返してテスタブルに

#### 影響ゼロ保証

- `panel-html-experiment` および `panel-runtime::html_panel` の GPU 経路は `html-panel` feature gate 配下（default OFF）
- `PanelPlugin` トレイトに `as_any_mut(&mut self) -> Option<&mut dyn Any>` をデフォルト None で 1 件追加（後方互換、既存 DSL/Wasm 実装は変更不要）
- DSL パネルの ui_panel_layer CPU 経路は無変更
- `PresentScene::html_panel_quads` の既定値 `&[]` で feature 無効ビルド時は影響なし

### ベースライン比測定（Windows / clean release build）

| 項目 | ベースライン | 第 2 稿 (CPU 経路) | 第 3 稿 (GPU 直) |
|------|-------------|-------------------|------------------|
| ビルド時間 | 117s | 335s | 215s |
| バイナリサイズ | 21.97 MiB | 29.43 MiB | 36.46 MiB |

GPU 直描画では CPU→GPU 通信ゼロを実現する代わりに vello GPU シェーダ・blitz-paint 0.3-alpha 等で +6.7 MiB を追加。ビルド時間は実測 +84% に短縮（CPU 経路の +186% から改善 — anyrender_vello_cpu のビルド負荷が大きかった）。

### テスト

- `panel-html-experiment`: 21 件（うち GPU 必須 1 件: `gpu_panel_gpu_target_create_uses_storage_and_srgb_view`）
- `panel-runtime` (html-panel): 17 件（うち GPU 必須 3 件: red pixel readback / Skipped 判定 / resize 再生成）
- `cargo test --workspace`: 378 passed / 0 failed
- `cargo test --workspace --features desktop/html-panel`: 386 passed / 0 failed
- `cargo clippy -p panel-html-experiment -p panel-runtime --features panel-runtime/html-panel --all-targets`: 新規コード起因 0 warning

### 設計上の重要 API

- `panel-html-experiment::engine::HtmlPanelEngine`
  - `build_scene(&mut self, scene: &mut vello::Scene, w, h, scale)`
  - `collect_action_rects() -> Vec<RenderedPanelHit>`
  - `document_dirty() -> bool`（自前トラック）
- `panel-html-experiment::gpu::PanelGpuTarget`
  - `create(device, w, h)` / `create_render_view()` / `create_present_view()`
- `panel-runtime::html_panel::HtmlPanelPlugin`
  - `render_gpu(device, queue, &mut renderer, &mut scene, w, h, scale) -> RenderOutcome<'_>`
  - `collect_action_rects() -> Vec<RenderedPanelHit>`
  - `panel_tree()` は空ツリーを返し ui-shell の DSL レンダ経路から自動除外
  - `handle_event` を Override し `Activate` を `<button>` `data-action` で解決
- `panel-runtime::registry::PanelRuntime`
  - `install_gpu_context(Arc<Device>, Arc<Queue>)` で vello::Renderer を集約構築
  - `html_panel_ids() -> Vec<String>`
  - `render_html_panels(&[(panel_id, w, h)], scale) -> Vec<HtmlPanelGpuFrame>`

### 残課題（次々フェーズ）

- HiDPI（scale != 1.0）対応
- HTML パネル数が増えた場合のテクスチャアトラス化
- HTML パネルのドラッグ移動（ワークスペース管理経路に組み込む）
- 他ビルトインパネルの HTML 化
- `<script>` / JS 実行
- vello::Renderer 構築コストの遅延化

## 背景と経緯

`.altp-panel` DSL + Wasm による既存パネル基盤に対し、
パネル UI を **HTML/CSS で書ける**ようにすることで Web 知識の流用を可能にしたい、という要望があった。

初稿では「Blitz は Windows ビルドが難しい」という未検証の一般論に基づき、Taffy も Stylo も使わない自作の
HTML/CSS サブセットで実装した。しかし `panel.css` の実効反映が未達であり、PoC として動機を満たせなかった。

ユーザーから「自作するのも Blitz を使うのもコスト的には変わらないか、Blitz を使わない方がむしろ重いはず」
との指摘を受け、本稿で **自作層を完全に破棄して Blitz バックエンドに置き換えた**。

## 決定事項

### 採用バックエンド

- **Blitz**（DioxusLabs）。Stylo + Taffy + Parley + Vello-CPU の統合エンジンを CPU レンダリング経由で使用
- crates: `blitz-dom 0.2.4`, `blitz-html 0.2.0`, `blitz-paint 0.2.1`, `blitz-traits 0.2.0`,
  `anyrender 0.6.x`, `anyrender_vello_cpu 0.8.1`

### `crates/panel-html-experiment`

破棄したモジュール: `parser` / `style` / `layout` / `render` / `hit_test` / `dom`（自作 HTML/CSS パイプライン）

新規モジュール:
- `engine` — `HtmlPanelEngine`: HTML→DOM パース、user CSS 注入、layout 解決、Vello-CPU ラスタライズ、
  Blitz の `BaseDocument::hit` を使ったヒットテスト、`DocumentMutator` を使った binding 適用
- `binding` — `data-bind-text` / `data-bind-disabled` / `data-bind-class-*` の式評価ロジック（DOM 非依存）
- `action` — `data-action` / `data-args` → `ActionDescriptor` 変換（DOM 非依存）

### `panel-runtime::html_panel`

- `HtmlPanelPlugin` が `HtmlPanelEngine` を内包
- `update()` 時にホストスナップショットを `apply_bindings` 経由で DOM に反映
- `panel_tree()` は HTML DOM を既存 `PanelTree (PanelNode)` に翻訳して既存レンダラと並存
  - 翻訳ルール: `<button>` → `Button`, `<section>` → `Section`, `<div class="row">` → `Row`, etc.
- `render_rgba(width, height, scale)` で **実 Blitz 描画**の RGBA8 出力を取得可能
  - 現フェーズでは既存 `render::panel.rs` パイプラインへの統合は未実施
  - したがって実画面に表示されるのは PanelTree 翻訳経由（DSL レンダラ）であり、`panel.css` の実効反映は
    `render_rgba` テストでのみ検証されている。次フェーズで本パスを wgpu surface に流す

### `plugins/app-actions/panel.html` + `panel.css` + `panel.meta.json`

- HTML 版の ID は `builtin.app-actions.html`（既存 DSL 版と並存）
- 既存 `.altp-panel` は変更なし

## ベースライン比測定結果（Windows / clean release build）

| 項目 | プラン閾値 | ベースライン | Blitz 統合後 | 増分 |
|------|-----------|-------------|-------------|------|
| リリースビルド時間 | +25% | 117s | **335s** | **+186%** |
| リリースバイナリサイズ | +15% | 21.97 MiB | **29.43 MiB** | **+34%** |

両指標ともプラン閾値を**大幅に超過**。プラン上は「いずれか超過時は候補 B にフォールバック」となっているが、
ユーザーから「Blitz が自作より重いのは想定通り」との明示的同意を得ており、stop-the-line にはしない判断とした。
代わりに、次フェーズで以下のコスト最適化を検討する材料として記録する:

- `blitz-dom` の `default-features=false` から必要 feature だけ ON
- ビルトイン `.altp-panel` は default ビルドに残し、`html-panel` feature を opt-in に保つ（現状そう）
- Vello-CPU からネイティブ Vello（wgpu 共有）に移行し、`vello_cpu` ツリーを切る
- パネル毎の HTML パース結果をキャッシュ

## Windows でのビルド検証結果

- `blitz-dom 0.2.4` + `blitz-html 0.2.0` のみ: 1m36s でクリーンビルド成功
- `blitz-paint 0.2.1` + `anyrender_vello_cpu 0.8.1` 追加後: 1m00s（incremental）でビルド成功
- フル workspace `cargo build --release --features desktop/html-panel`: 5m35s 成功
- Stylo / Servo 系 crate も問題なくコンパイル。**「Stylo は Windows ビルド困難」は誤情報**

## テスト

- `panel-html-experiment`: 17 件（action 8 / binding 5 / engine 4: hit_test, render, binding 反映 3 種）
- `panel-runtime::html_panel`: 7 件（action 翻訳 3 / section / row / binding update / render_rgba）
- `cargo test --workspace` (default): 全通過（failed 0）
- `cargo test --workspace --features desktop/html-panel`: 全通過（381 passed, 0 failed）
- `cargo clippy -p panel-html-experiment -p panel-runtime --features panel-runtime/html-panel --all-targets`:
  新規コード起因の警告ゼロ（`plugin-host` 既存警告のみ）

## 動機達成の検証

プランの受け入れ条件「Rust コードを一切触らず `panel.html` / `panel.css` の編集だけで見た目変更が可能」について、
本フェーズで確認できた範囲:

- ✅ HTML 編集で要素構造が変わることを `panel_tree` 経由で確認（テスト 7 件）
- ✅ CSS が**実際に**ピクセルへ反映されることを Blitz の `render_rgba` 経由で確認（`engine_renders_html_to_rgba_pixels_with_user_css_applied` / `render_rgba_produces_pixels_for_simple_panel`）
- ⏳ ただし**実画面表示**は PanelTree 翻訳経由のため、現状の最終フレームでは `panel.css` の角丸 / 影 /
  グラデーションは反映されない。`render::panel.rs` への接続は次フェーズ

## 残課題（次フェーズ）

1. `HtmlPanelEngine::render` の RGBA8 出力を `crates/render/panel.rs` の `RasterizedPanelLayer` へ接続
   し、実画面で `panel.css` を反映させる
2. Vello CPU → ネイティブ Vello（wgpu 共有）への移行でレイテンシとビルドサイズを削減
3. HTML パースのキャッシュ・差分更新（毎回パースし直しを避ける）
4. WSL2 / Linux ネイティブでのビルド検証
5. デバッグビルド起動時間の実測

## 参考

- [Blitz リポジトリ](https://github.com/DioxusLabs/blitz)
- 自作 → Blitz の差し替え判断は本セッションのユーザーフィードバックに基づく
