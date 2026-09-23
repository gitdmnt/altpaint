# ADR 019: UI 整理 — 共有テーマ CSS・アイコンフォント・presenter クリッピング・DPI 対応

- 作業日時: 2026-07-07 (起票)
- 作業 Agent: claude-opus-4-8 (Claude Code)
- ステータス: 進行中 (Phase 0 起票 / Phase 1〜7 実装予定)
- 計画書: `.context/ui-cleanup-plan.md` (worktree 一時ファイル)

## 背景

UI の「無駄な図形・サイズ不適合・テーマ不統一・はみ出し」を HTML/CSS パネル層と wgpu 直描画層の両面で監査した結果、次が確定した。

1. **テーマ不統一 (CSS 層)**: 12 個のビルトインパネルが `.panel/.panel-title/.alt-section/.btn/hr/label/select/.muted/.row` を各 `panel.css` にコピペ重複し、色をハードコード。パネル背景 `#181c24` が presenter `theme.rs` の `APP_BACKGROUND #181818` と不一致。同一 `label` 要素の色がパネル間で食い違う。
2. **サイズ不適合**: `default_size` (パネルロード時の fallback) と `preset.size` (起動時に既定ワークスペースプリセットとして適用される実表示サイズ) がほぼ全パネルで食い違う (app-actions は幅高逆転)。`min-width` 不揃いで `default_size.width 220 < min-width 240` の横クリップ。縦積みボタン過多でスクロール常態化。
3. **無駄な図形**: 装飾 `<hr>`、`.panel` の常時グレー枠 + `border-radius` (presenter のアクティブ枠と二重)、冗長な外側 `.alt-section` ラッパ入れ子、color-preview 箱の装飾枠、豆腐化しやすい絵文字アイコン。
4. **はみ出し (GPU 層)**: presenter (wgpu) 層に `RenderPass::set_scissor_rect`/`set_viewport` が 0 件 (`apps/desktop` 配下)。GPU クリップの最終防壁が無く、パネル/オーバーレイのはみ出し防止を上流 CPU クランプに全依存。`present.rs:273-278` で位置クランプ基準 (workspace `entry.size`) と描画幅高 (view measured) が別ソースで右端はみ出し。overlay が `canvas_host_rect` でクリップされずUI領域へはみ出す。
5. **DPI 非対応**: 全寸法が物理ピクセル固定 (`scale_factor` 参照 0 件)。hi-DPI で UI が小さく見える (座標バグではない)。

## 決定

### 1. テーマ統一は「共有クラス集約」で行う (CSS 変数を使わない)

Blitz 0.3.0-alpha.2 は **CSS `var()` を解決しない** (依存クレート実装で確認)。したがってテーマは CSS カスタムプロパティでなく、共通クラスを単一の共有ベース CSS へ集約して統一する。

- 共有ベース CSS `PANEL_BASE_CSS` を `crates/panel-runtime` 内の新モジュールに 1 箇所定義する。
- 注入は `HtmlWasmPanel::load` で `HtmlPanelView::new(&html, &format!("{PANEL_BASE_CSS}\n{user_css}"))` の結合 (`ua_stylesheets` は Vec だが、結合文字列で単一化する)。`replace_document` 経路も同一結合を渡す。
- 各 `panel.css` からは重複ルールを削除し、パネル固有ルールのみ残す。

### 2. カラーパレット確定表 (単一の真実)

パネル層は次の値へ正規化する。presenter `theme.rs` と背景色を一致させる。

| 役割 | 値 | 備考 |
| --- | --- | --- |
| アプリ/パネル背景 | `#181818` | `theme.rs APP_BACKGROUND` と一致 |
| パネル前景テキスト | `#f3f7ff` | |
| 副次テキスト (muted / label / section-title) | `#8aa2c0` | 従来 label がパネル間で `#f3f7ff`/`#8aa2c0` に割れていたのを muted 系へ統一 |
| 枠線 | `#2a3140` | 従来の3値 (`#2a3140`/`#434c5d`/`#4d5a70`) を単一化 |
| ボタン面 | `#273540` | |
| ボタン枠 | `#4d5a70` | |
| アクティブ強調 (面) | `#3b4f63` | CSS 側のアクティブ表現。presenter の `ACTIVE_PANEL_BORDER #42a5f5` はパネル選択枠として別レイヤで維持 |
| アクティブ強調 (枠) | `#6f80a3` | |

`.panel` の常時グレー枠 + `border-radius` は撤廃し、アクティブ強調は presenter 描画の選択枠に委ねる (二重枠の解消)。

### 3. アイコンはアイコンフォント方式 (Flaticon UIcons, 方式B)

blitz 0.3.0-alpha.2 で確認した 3 方式のうち、**方式B (プログラムによるフォント登録)** を採用する。

- Flaticon UIcons の **ttf** を repo に vendoring (woff2 でなく非圧縮 ttf。方式B の `register_fonts` は生バイト前提)。
- パネル document 構築時に共有 `FontContext` を作り `Collection::register_fonts(Blob, None)` でアイコンフォントを登録し `DocumentConfig.font_ctx` へ設定 (`HtmlPanelView::new`/`replace_document` 双方)。net provider 不要・同期ロード。
- 共有ベース CSS にアイコンクラスを `::before { font-family:"uicons"; content:"\fXXX" }` で定義 (`content:"string"` は実装済み。`content:url()/attr()/counter()` は未実装)。
- 各 `panel.html` の絵文字をアイコンクラスへ置換。**フォント未ロード時もテキストラベルのみで機能する** (アイコンは装飾レイヤ)。

不採用: 方式A (インライン SVG, 追加配線ゼロだが各所に SVG 直書きで冗長) / 方式C (`@font-face` url + NetProvider, 非同期ロードで初回 render 遅延)。将来必要になれば再評価する。

**ライセンス**: Flaticon 無料枠は帰属表示が要る。vendoring 前に条件を確認し、不可なら MIT ライセンスのアイコンフォントへ差し替える (この場合も方式Bは不変)。

### 4. サイズ統一規則 (優先順位付き)

- **`default_size == preset.size` を不変条件**とする。
- preset を持つパネルは **preset を基準**とし `default_size` を preset に一致させる。
- preset のサイズで内容が収まらない場合は **preset 側を実測値へ引き上げ**、default をそれに合わせる。
- preset を持たない (koma-list / text-flow) は `default_size` を実測で調整。
- `min-width` は共有ベース CSS で単一基準に一元化し、全 `default_size.width ≥ min-width` を保証する。

### 5. presenter に GPU クリップ最終防壁 (scissor) を導入

- panel_quads (L3): 各パネル quad 直前に `set_scissor_rect(rect ∩ surface)`、描画後にフルサーフェスへ戻す。
- overlay (L2 円/線/navigator/mask): 描画前に `set_scissor_rect(canvas_host_rect ∩ surface)`。
- `present.rs:273-278` の rect 幅高を x/y クランプと同一サイズソースへ統一 (または push 後に window 境界で最終クランプ)。
- overlay bbox 計算 (pipelines/mod.rs) に surface 上限クランプを追加。

上流 CPU クランプ (canvas-geometry / panel-workspace / panel-html layout) は維持し、scissor はその正しさに依存しない最終防壁として二重化する。

### 6. DPI 対応 (scale_factor 反映) — 別 PR

- winit の `scale_factor` / `ScaleFactorChanged` を取り込み、論理寸法 → 物理寸法の変換を一元化する (`theme.rs` の `HEADER_HEIGHT`/`FOOTER_HEIGHT`/`WINDOW_PADDING`、`PANEL_CHROME_HEIGHT`、パネル既定サイズ、overlay 線幅/リング半径)。
- 入力座標系 (`event_loop/pointer.rs` → `app/input.rs` → `canvas-geometry`) との整合を保ち、scale の二重適用を避ける。
- OS 固有コード禁止方針に従い winit の値のみ使用する。
- 入力座標系全体へ波及し結合度が高いため、**Phase 0〜5,7 (UI 整理本体) とは別 PR** として着地させる。

## スコープ外

- 新規パネル・機能追加。
- presenter の描画アルゴリズム刷新 (painter's algorithm は維持)。
- アイコン方式A/C への移行 (方式B採用)。

## リスク

- **DPI (決定6) が最高リスク**: 入力座標系との整合。scale の二重適用・整数丸めズレを TDD で固定。問題時は本体から切り離して独立進行。
- Flaticon ライセンス未確定。代替 (MIT フォント) フォールバックを用意。
- Blitz `var()` 非対応のためテーマはクラス集約で実装 (変数化しない)。
