# ADR 009: DSL パネルの HTML/CSS 翻訳器導入と HtmlPanelEngine 統合

- 作業日時: 2026-04-26
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted

## コンテキスト

Phase 9 (`crates/render/` クレート完全削除) の最大の山として、9 個の DSL
`.altp-panel` パネル群 (`color-palette`, `job-progress`, `layers-panel`,
`panel-list`, `pen-settings`, `snapshot-panel`, `text-flow`, `tool-palette`,
`view-controls`, `workspace-presets`) と、ステータスバーの CPU テキスト描画
(`render::status::compose_status_region` + `render::text::draw_text_rgba`) の
GPU 化が残っていた。

Phase 8F で既に **HTML パネル GPU 直描画基盤** (`HtmlPanelEngine` + Blitz
HTML/CSS パーサ + parley テキスト + `vello::Renderer::render_to_texture`) が
完成しており、`PresentScene::html_panel_quads` (L5) として外部所有テクスチャ
を quad 合成する経路ができていた。

DSL パネルのレンダリング経路を以下の三案から選定する必要があった:

- **案 A**: DSL → Vello/Blitz スタイル直接構築 (中間 IR を vello シーンに直訳)
- **案 B**: DSL → HTML/CSS 純関数翻訳器を追加 (既存 `HtmlPanelEngine` を再利用)
- **案 C**: DSL 専用の GPU パイプラインを新規作成 (parley + 自前レイアウトエンジン)

## 決定

**案 B (DSL → HTML 翻訳器) を採用** し、`crates/panel-runtime/src/dsl_to_html.rs`
として純関数翻訳器を実装。`DslPanelPlugin` に `HtmlPanelEngine` を内蔵させ、
既存の HTML パネル経路 (`PresentScene::html_panel_quads`) にすべて合流させた。
ステータスバーも同じ `HtmlPanelEngine` インスタンスを抱える `StatusPanel`
(`apps/desktop/src/frame/status_panel.rs`) に置換した。

### 採用根拠

| 観点 | 案 A | **案 B (採用)** | 案 C |
|------|------|-----------------|------|
| 工数 | 中 (vello シーン直訳器を新規) | **小 (純関数翻訳のみ)** | 大 (新パイプライン) |
| 既存資産再利用 | 部分 | **HtmlPanelEngine 全面再利用** | なし |
| HiDPI / 動的サイズ | 自前 | **8F 済 (ADR 008 で確立)** | 自前 |
| HTML 表現力 | parley/vello 直接 | CSS 全機能 | 自前定義必要 |
| 中間表現の保守性 | 中 | **HTML/CSS で人間可読** | 低 |
| カスタム widget の妥協 | フル制御 | プリミティブに限定 | フル制御 |

案 B の最大の利点は、Phase 8F で確立した `HtmlPanelEngine` の挙動 (動的サイズ
追従 / フォーカス管理 / `<details>` 開閉 / `:hover` / `<button>` クリック /
parley テキスト / vello GPU 描画 / dirty 管理) がそのまま DSL パネルにも適用
されること。9E の工数を「翻訳器 1 つ + engine 内蔵」に圧縮できた。

## カスタム widget の妥協

DSL ノードのうち、HTML プリミティブに直接マップしにくいものは alpha 期間限定で
妥協し、post-alpha でカスタム widget 化する:

- **ColorWheel**: `<input type="color">` で代用。OS ネイティブの色選択ダイアログ
  に委譲し、HSV/HSL 環状ホイール UI は失われる。`// TODO(post-alpha): replace
  with custom color-wheel canvas widget` コメントを翻訳関数の直前に挿入。
- **Slider のドラッグ中インクリメンタル値**: `<input type="range">` の change
  だけでは取れないため、`data-action="alt:slider:<id>"` + `data-args` に
  `{"min":N,"max":N,"step":N}` を載せ、`HtmlPanelEngine::on_input` の PointerMove
  フックで連続値を解決する設計とした。

## LayerList 多選択 / D&D の非対応

現行 CPU 実装が単一選択のみサポートしていたため、HTML 経路でも単一選択
(`<li data-index="N">` クリック → `HostAction::SelectLayer(N)`) のみを実装し、
多選択 / コンテキストメニュー / ドラッグ並べ替えは引き続き非サポート。将来
Phase で個別検討する。

## Slider/Dropdown/TextInput/ColorWheel の altp:* 配線

9E-1 / 9E-2 で翻訳器とディスパッチコントラクト (`alt:slider:<id>` 等) を確立し、
9E-3 でルーティングを切り替えたが、`HostAction` への変換まで含めたエンドツー
エンド配線は完了していない。Phase 9F 以降で `parse_data_action` 拡張と
`HostAction` 変換を実装する。

## 影響

- `crates/panel-runtime/src/dsl_to_html.rs` を新規追加 (純関数翻訳器)
- `crates/panel-runtime/src/dsl_panel.rs::DslPanelPlugin` に `HtmlPanelEngine`
  を内蔵し `render_gpu()` / `gpu_target()` / `forward_input()` /
  `collect_action_rects()` を追加
- `crates/render/src/{text,status,panel}.rs` を全削除 (font8x8 / ab_glyph /
  fontdb 依存もゼロ化)
- `crates/ui-shell/src/surface_render.rs::rebuild_panel_bitmaps` 系を撤去
- `apps/desktop/src/frame/status_panel.rs` を新規追加
- 9E-5 で `crates/render-types/src/test_support.rs` を追加し、ピクセル比較を
  弱検証 (色矩形 / 暗色ピクセル数) に置き換える共通ユーティリティを集約

### スコープ縮小事項 (Phase 9F に移送)

- `PresentScene::base_layer` / `ui_panel_layer` の **型自体** の物理削除
- `html_panel_quads` → `panel_quads` リネーム
- `crates/render/` クレート物理削除と workspace member 登録解除

これらは render クレート削除と同じレイヤー整理タスクであり、PR 粒度を揃えた
方が安全という判断で 9E のスコープ外とした。

## 受け入れ条件と結果

| 条件 | 結果 |
|------|------|
| 9 個の DSL パネルが GPU 経路でのみ描画 | OK (CPU bitmap 経路は撤去済) |
| ステータスバーが `HtmlPanelEngine` 経路で描画 | OK (`StatusPanel` 内蔵) |
| `crates/render/Cargo.toml` から font8x8 / ab_glyph / fontdb 依存ゼロ | OK |
| `cargo test --workspace` 通過 (pre-existing 失敗は許容) | OK (17 失敗、すべて baseline 同) |
| `cargo clippy --workspace --all-targets` 警告増加なし | OK (83 件、baseline と同数) |
| `cargo build --release -p desktop` 通過 | OK |
| Tab キーでパネル間フォーカス遷移 | OK (`HtmlPanelEngine` 標準動作) |
| `<details>` 開閉状態が再翻訳で保持 | OK (Blitz 標準動作) |

## 代替案を採用しなかった理由

- **案 A (DSL → Vello シーン直訳)**: parley テキスト + 自前レイアウトを書き直す
  必要があり、HtmlPanelEngine の動的サイズ計算 (ADR 008) や Tab フォーカス管理を
  再実装する負担が大きい。
- **案 C (DSL 専用 GPU パイプライン)**: 既存資産を一切流用できず、9F の render
  クレート削除目標に対して逆方向の実装拡張になる。
