# ADR 011: `html-panel` feature gate の完全撤去

- 作業日時: 2026-05-02
- 作業 Agent: claude-opus-4-7 (1M context)
- ステータス: Accepted

## コンテキスト

Phase 9E (2026-04-26) で DSL パネルの CPU ラスタライザ (`render::panel::rasterize_panel_layer`) と
ステータスバーの CPU テキスト (`render::status::compose_status_region`) を完全撤去し、
`HtmlPanelEngine` (Blitz HTML/CSS + parley + vello) 経路に乗せ替えた。
これにより**パネルとステータスバーの描画パスは `HtmlPanelEngine` 一本**となった。

ところが `apps/desktop` の `html-panel` feature は **default OFF のまま放置**されており、
`cargo run -p desktop` (feature 指定なし) でビルドすると次のような実行時挙動になる:

```rust
#[cfg(not(feature = "html-panel"))]
let html_panel_quads_slice: &[GpuPanelQuad<'_>] = &[];

#[cfg(not(feature = "html-panel"))]
let status_quad: Option<GpuPanelQuad<'_>> = None;
```

すなわち**全パネル＋ステータスバーが画面から消える**。Phase 9E / 9F の検証は
`cargo build --release -p desktop` のコンパイル成功のみを確認しており、
実行時のパネル描画を確認していなかった。

`panel-runtime` 側は Phase 9E 時点で `default = ["html-panel"]` を入れていたが、
これは `apps/desktop` の feature 指定が無くてもクレート単体テストは通る、という限定的な
緩和でしかなく、`apps/desktop` 内部の `cfg(feature = "html-panel")` 分岐は
`apps/desktop/Cargo.toml` の `[features]` テーブルを参照するため、無効化されたままだった。

## 決定

`html-panel` feature gate を**コードベースから完全撤去**する。

- `apps/desktop/Cargo.toml`: `[features]` テーブル削除、`panel-html-experiment` /
  `keyboard-types` を `optional = true` から必須依存へ昇格。
- `crates/panel-runtime/Cargo.toml`: `[features]` テーブル削除。
- `apps/desktop/src/**`: `#[cfg(feature = "html-panel")]` / `#[cfg(not(feature = "html-panel"))]`
  分岐 14 箇所をすべて素のコードへ変換。`forward_html_pointer`, `HtmlPointerKind`,
  `StatusPanel`, `frame::status_panel` モジュール宣言, `status_panel` フィールドおよび
  `build_status_snapshot` をすべて常時有効化。
- `crates/panel-runtime/src/**`: `#[cfg(feature = "html-panel")]` 分岐 23 箇所を撤去。
  `PanelGpuFrame`, `PanelGpuContext`, `panel_engine_mut`, `PanelEngineKind`,
  `gpu_ctx` / `pending_panel_size_changes` フィールド、`gpu_context_parts` /
  `install_gpu_context` / `panel_ids_with_gpu` / `panel_measured_sizes` /
  `measured_size` / `forward_panel_input` / `restore_panel_size` /
  `take_panel_size_changes` / `render_panels` メソッド、HTML パネル登録ループ、
  `test_mark_panel_size_changed` テストフックを常時有効化。
- `crates/panel-runtime/src/lib.rs`: `mod html_panel;` を無条件化、
  `pub use html_panel::*` / `pub use registry::PanelGpuFrame` を無条件化。

`html-panel` という名称は ADR/IMPLEMENTATION_STATUS など履歴文書には残るが、
今後の新規コードでは使わない。

## 代替案

- **`apps/desktop/Cargo.toml` に `default = ["html-panel"]` を追加するだけ** —
  最小変更だが「死んだ抽象化」（無意味な feature gate）が残り、新規開発者を混乱させる。
  CLAUDE.md の「後方互換コードを残さない / コードベースの肥大化を常に軽減する」
  方針に反する。
- **CPU panel 描画を復活させる** — Phase 9E 全体の設計判断 (ADR 009) と矛盾するため非採用。

## 影響

### 完了条件 (達成済み)

- workspace 内 `cfg(feature = "html-panel")` / `cfg(not(feature = "html-panel"))`
  参照が 0 件
- `Cargo.toml` 内の `html-panel` feature 定義が 0 件 (`apps/desktop` /
  `panel-runtime`)
- `cargo run -p desktop` (feature 指定なし) でパネル＋ステータスバーが正常表示される
- `cargo build --workspace` 成功
- `cargo test --workspace` ベースライン同等（並列実行時の flaky 1 件は
  単独実行で PASS、9E-5 由来）
- `cargo clippy --workspace --all-targets` warning 70 件
  (ベースライン 83 → 13 件減、新規警告 0)

### 削減されたサーフェス

- `cfg(feature = "html-panel")` ブロック: 37 箇所
- `cfg(not(feature = "html-panel"))` ブロック: 4 箇所
- `[features]` セクション: 2 ファイル (apps/desktop, panel-runtime)
- `optional = true` 依存: 2 件 (panel-html-experiment, keyboard-types)

### 残課題

なし。

## 後続: Section ノードを `<details>` から `<div>` へ切り替え

### 経緯

`html-panel` feature 撤去後の起動検証で、`builtin.tool-palette` の DSL→HTML 翻訳結果を Blitz/stylo が処理する際に panic が発生した:

```
thread 'main' panicked at stylo-0.16.0/data.rs:186:31:
called `Option::unwrap()` on a `None` value
```

スタックは `panel_runtime::registry::PanelRuntime::render_panels` →
`panel_html_experiment::engine::HtmlPanelEngine::on_render` →
`HtmlPanelEngine::resolve_layout` → `blitz_dom::resolve` → `stylo::ElementStyles::primary().unwrap()`。

ICU4X の `No segmentation model for language: ja` 警告が先行するが、これは非致命的で
panic の直接原因ではない。`builtin.color-palette` / `builtin.layers-panel` /
`builtin.pen-settings` は通過し、tool-palette のみで落ちる。HTML をダンプして比較した
ところ、tool-palette のみ**ネストした `<section>` (= `<details>`)** を持っていた
（外側「ツール」内に「子ツール」「インポート結果」を含む）。Blitz/stylo の現行版
(0.3-alpha.2 / 0.16.0) はネストした `<details>` の primary style 解決に失敗する。

Phase 9G の feature 撤去自体には起因せず、Phase 9E の DSL→HTML 翻訳器 (ADR 009) 導入時から
潜在していたバグ。`html-panel` feature が default OFF だったため実行時に到達せず
顕在化していなかった。

### 対応

`crates/panel-runtime/src/dsl_to_html.rs` の `PanelNode::Section` 翻訳を
`<details><summary>` から `<div class="alt-section"><div class="alt-section-title">...`
へ切り替えた。CSS の `.alt-section > summary` セレクタも `.alt-section > .alt-section-title`
へ更新。テスト `translate_section_uses_details_summary` を `translate_section_uses_div_with_title`
へ書き換え。

### トレードオフ

- 失う: Section の open/close UX（常時開いた状態で表示）
- 得る: ネスト深度に依存しない描画安定性
- post-alpha で Blitz の `<details>` サポートが安定したら戻すか、独自 disclosure ウィジェットへ
  移行するかを ADR で再検討する。

## 関連 ADR / 文書

- ADR 007: HTML panel experiment (`html-panel` feature 導入)
- ADR 008: HTML panel dynamic size and engine consolidation
- ADR 009: DSL → HTML 翻訳器採用 (Phase 9E、CPU 経路撤去の決定)
- ADR 010: `crates/render` クレート物理削除 (Phase 9F、リネーム作業)
- `docs/IMPLEMENTATION_STATUS.md` Phase 9E / 9F 完了記録
