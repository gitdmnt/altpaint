# CLAUDE.md

Claude Code がこのリポジトリを扱うときの入口。必要な文書だけを順に読む。

**今の位置**: ADR 020 (骨格再建・選別移植方式) の試行期間 (Phase 0〜1) にある。
main のコードは凍結した旧コード (`legacy-final` タグ) のままで、新しい骨格は Phase 2 から作る。
旧コードに機能を足したり、不具合を直したりしない。

**文書とコードが食い違う場合は、動いているコードが正本。** 文書は同じ PR で直す。

## 最初に読む順序

1. このファイル
2. [docs/STATUS.md](docs/STATUS.md): 現況、次の 3 項目、再開手順
3. 作業に応じて読む文書
   - [docs/PRODUCT.md](docs/PRODUCT.md): 要件、制作シナリオ、後回しの項目、スコープを削る順番
   - [docs/ACCEPTANCE.md](docs/ACCEPTANCE.md): 受け入れ ID と確かめ方
   - [docs/adr/020-codebase-rebuild-strategy.md](docs/adr/020-codebase-rebuild-strategy.md): 方針の本体。5 章は 020-architecture.md、6 章と 11 章は 020-plan.md、8 章は 020-process-rules.md

## 規約

正本は ADR 020 の 8 章 ([docs/adr/020-process-rules.md](docs/adr/020-process-rules.md)) で、ここはその要約である。

- **PR の単位**: 1 課題 1 ブランチ 1 PR にする。手書きのソースの追加は 400 行以内 (移植 PR は 800 行以内)、変更ファイルは 15 以内。本文に `Refs: A-xx` (受け入れ ID) か `Refs: #番号` (Issue) を書く。CI の meta-check が検査する。
- **レビュー待ちは 3 本まで**: オーナーのレビュー待ちの PR が 3 本になったら、新しい実装を止め、調査と下書きだけを行う。記録だけの PR は数えない。
- **マージはオーナーが行う**: エージェントは次のことをしない。
  - PR のマージと、自動マージの設定
  - 免除ラベル (`size-exempt`、`files-exempt`) の付与
  - ブランチ保護・rulesets・リポジトリ設定の変更
  - 保護したタグ (`legacy-*`、`archive/*`、`spike-archive/*`、`rebuild-partial`) の削除と更新
  - 認証情報の切り替えと取り出し、hooks とそのログの変更
- **実装は 1 セッション 1 PR**: コードを変更する多エージェントの自動バッチは禁止する。調査・レビュー・計測にはサブエージェントを使ってよいが、コードは変更させない。
- **構造だけの変更には事前のチケット**: 改名・分割・移動・抽象の追加は、`structure` ラベルの Issue でオーナーの承認を得てから行う。Issue には、必要とする失敗テストか計測値、今の構造で何が壊れるかのメモ、触るファイルの一覧、タイムボックスを書く。機能 PR の中での整理は、同じファイル内で 100 行以内まで。
- **完了の定義**: 受け入れ ID のテストが green で、オーナーの実機デモ (docs/DEMO-LOG.md に記録) に合格したら完了とする。テスト件数・clippy 0・grep 0 件は完了条件にしない。完了条件は後から書き換えず、未達は未達として記録する。見積もりの 150% を超えたら止めて再計画する。
- **トランク運用**: main 1 本で運用する。ブランチは作成から 3 日以内に PR を出すか削除する。未コミットの作業は 1 日以内に片付ける。長く残す記録はタグで残す。
- **テスト**: TDD で進める。先に失敗するテストを書き、失敗したログを PR に残す。本番と同じ経路を通す。`catch_unwind` でアサートを握り潰さない。GPU が無ければ失敗させる。
- **移植**: `legacy-final` タグから、1 PR に 1 部品 (800 行以内) ずつ移植する。PR に `Ported-from: legacy-final:<パス>`、元のコミット、変えた前提、持ち込んだテストを書く。
- **文書**: 置いてよい文書は 8 章 11 の一覧だけである。コードのコメントに BL- / Phase / 「旧」を書かない。
- **ADR**: 大きな決定は docs/adr/ に書く。状態・作業日時・モデル名を必ず書く。決定を覆すときは、同じ PR で古い ADR の状態も更新する。
- **中断と再開**: 2 週以上空いた後は、docs/STATUS.md の「再開手順」から始める。
- **タスクの終わり**: ask_user で待機する。

## コーディング方針

- 関数型のスタイルで書く。
- OS 固有のコードを書かない。`#[cfg(target_os = "...")]` や `cfg!(target_os = ...)` をアプリケーション層に書かず、OS の差は winit・dirs・fontdb などのライブラリに任せる。
- 後方互換のコードを残さない。例外は、v0.2 以降の保存形式の移行の段だけである (ADR 020 の 5 章「保存」)。

## コマンド (旧コード)

手順の詳細と必要なもの (Python 3、GPU など) は [README-LEGACY.md](README-LEGACY.md) にある。

```bash
# 同梱パネルの Wasm をビルドする (テストの前に必要)
.\scripts\build-ui-wasm.ps1            # Windows (PowerShell)
bash scripts/build-ui-wasm.sh          # Linux / WSL2 (workspace-layout は README-LEGACY.md の追加手順で)

cargo build
cargo test --workspace
cargo run -p altpaint-desktop

# メタ検査の単体テスト
node --test .github/scripts/tests/meta_check.test.mjs
```

## 旧コードの構成

旧コードは 30 クレートの Cargo workspace (`apps/desktop`、`crates/*`、`crates/builtin-panels/*`) である。
構成の説明は docs/ARCHITECTURE.md と docs/MODULE_DEPENDENCIES.md にある。
これらの文書と旧コードは、Phase 2 の撤去 PR で main から外す (`legacy-final` タグからは参照できる)。
新しい骨格のコマンドと構成は、Phase 2 の骨格 PR でこの節に書く。
