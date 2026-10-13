# ADR の索引

- 最終更新: 2026-10-13 (ADR 020 の Phase 0 で作成。作業 Agent: Claude Opus 5.5)
- ADR の決まりは ADR 020 の 8 章 10 にある。どの ADR も、冒頭に「作業日時」「作業 Agent」「ステータス」の 3 つの欄を持つ。
- 決定を覆すときは、同じ PR で、覆される ADR の状態とこの索引も更新する。

## 状態の読み方

| 状態 | 意味 |
|---|---|
| Proposed | 提案中。まだ承認されていない |
| Accepted | 承認され、今も有効 |
| On-hold | 保留。再開するかどうかを後で決める |
| Superseded-by NNN | ADR NNN が置き換えた |
| Abandoned | 取りやめた |

一部だけが覆された ADR は、題名に出ている中心の決定が今も有効かどうかで状態を決めた。
中心の決定が有効なら Accepted とし、覆された部分は各 ADR のステータスの欄の括弧に書いてある。
Superseded-by の番号は、中心の決定を覆したか、その実体を削除した ADR のうち、最初のものである。

ADR 020 はまだ Proposed である。
ADR 020 が「止血案」の ADR に置き換わった場合は (ADR 020 の 6 章「撤退の手順」)、同じ PR で、下の表の「Superseded-by 020」の ADR の状態を見直す。

## 一覧

| # | 題名 | 状態 | 作業日 | 今も使う部分 |
|---|---|---|---|---|
| [001](001-undo-redo-replay-history.md) | Undo/Redo 基盤 (replay 方式) | Superseded-by 016 | 2026-03-13 | なし (undo は ADR 020 の 5 章「Undo」で作り直す) |
| [002](002-export-png-background-job.md) | PNG Export と汎用バックグラウンドジョブ基盤 | Superseded-by 020 | 2026-03-13 | ファイルダイアログの抽象 (移植する) |
| [003](003-gpu-canvas-migration.md) | GPU キャンバスクレートの新設 | Superseded-by 020 | 2026-03-16 | なし (GPU は表示専用にする) |
| [004](004-color-palette-multimode-colorspace.md) | カラーパレットのマルチ色空間対応 | Superseded-by 010 | 2026-04-22 | HSV / Lab / Oklch の切替と色空間変換 (移植する) |
| [005](005-gpu-canvas-phase-8d-communication-minimization.md) | GPU キャンバス: CPU-GPU 通信の最小化 | Superseded-by 020 | 2026-04-25 | なし |
| [006](006-gpu-fill-composite-and-blendmode-custom-removal.md) | GPU 塗りつぶしとレイヤー合成、BlendMode::Custom の削除 | Superseded-by 020 | 2026-04-25 | BlendMode::Custom を持たないこと |
| [007](007-html-panel-experiment.md) | HTML/CSS パネルの実験層 (Blitz) | Accepted | 2026-04-25 | Blitz と vello による GPU 直描画 |
| [008](008-html-panel-dynamic-size-and-engine-consolidation.md) | HTML パネルの枠サイズの動的化 | Superseded-by 013 | 2026-04-25 | なし |
| [009](009-dsl-panel-html-translation.md) | DSL パネルの HTML/CSS 翻訳器 | Superseded-by 012 | 2026-04-26 | なし |
| [010](010-render-crate-removal.md) | render クレートの削除と PresentScene の再編成 | Accepted | 2026-04-29 | presenter の構成 (移植する) |
| [011](011-html-panel-feature-removal.md) | html-panel feature gate の撤去 | Accepted | 2026-05-02 | feature gate を持たないこと |
| [012](012-altp-panel-dsl-removal.md) | DSL の撤去と HTML/CSS/Wasm の直接記述 | Accepted | 2026-05-03 | HTML + CSS + Wasm で DOM を書き換える方式 |
| [013](013-html-panel-manual-resize.md) | HTML パネルの手動リサイズ | Accepted | 2026-05-12 | 8 ハンドルの手動リサイズ (移植する) |
| [014](014-paneltree-removal-and-workspace-layout-html.md) | PanelTree の撤廃と workspace-layout の HTML 化 | Accepted | 2026-05-15 | PanelTree を持たないこと |
| [015](015-html-panel-keyboard-abi-and-headless-hit-tables.md) | キーボード ABI の配線と、ヒットテーブルの GPU 非依存化 | Accepted | 2026-06-11 | ヘッドレスのヒット収集 (移植する) |
| [016](016-dependency-minimization.md) | クレート依存構造の最小化 | Accepted | 2026-06-12 | 死にコードと未使用依存の削除 |
| [017](017-coordinate-types-and-dependency-layering.md) | 座標系の型の区別と、依存グラフの階層化 | Accepted | 2026-06-12 | 座標の型の区別 (移植する) |
| [018](018-naming-and-vertical-slice-rearchitecture.md) | 用語体系の確立と垂直スライス再編 | Superseded-by 020 | 2026-06-12〜14 | 用語体系 (Koma ほか) |
| [019](019-ui-theme-and-clipping.md) | UI 整理: 共有テーマ CSS、アイコンフォント、クリッピング、DPI | Superseded-by 020 | 2026-07-07 | 決定の全部 (ADR 020 が引き継ぎ、作業途中のコードは `legacy-adr019-wip` タグから移植する) |
| [020](020-codebase-rebuild-strategy.md) | コードベース再建方針: 骨格再建・選別移植方式 | Proposed | 2026-09-23 | — |

ADR 020 は 6 つのファイルからなる: [本体](020-codebase-rebuild-strategy.md)、[5 章](020-architecture.md)、[6 章と 11 章](020-plan.md)、[8 章](020-process-rules.md)、[付録 A〜F と H](020-appendix-audit.md)、[付録 G](020-appendix-enforcement.md)。
ADR 021 (骨格の凍結) は、ADR 020 の Phase 4 で作る予定である。

## ADR 020 の指摘との対応

ADR 020 の 2 章の 28 番は「ADR 7 本が後から覆されたのに、状態欄は未更新」と指摘している。
監査が数えた 7 本は、001、003、004、008、009、016 の一部、017 の一部である。
このうち 016 と 017 は、覆されたのが一部だけなので、上の表では Accepted とした。
