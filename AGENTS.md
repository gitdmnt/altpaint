# AGENTS.md

このファイルは、`altpaint` を扱う LLM / coding agent が**最初に読む前提**の案内である。  
コンテキスト節約を優先し、必要な文書だけを順に読むための入口として使う。

## 目的

- 最小トークンで現在地を把握する
- どの文書が何の正本かを明確にする
- タスクごとに読むべき文書を絞る
- 実装時にコードと文書のどちらを優先すべきかを明確にする

## 最初に守ること

1. まずこのファイルを読む
2. 次に **`docs/IMPLEMENTATION_STATUS.md`** を読んで現在の到達点を確認する
3. その後はタスクに応じて必要な文書だけ読む
4. 実装の事実確認は必ず対象コードでも行う
5. `target/` は生成物なので読まない

## 速読順序

### ほぼ常に読む

1. [AGENTS.md](AGENTS.md)
2. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)

### 必要になったら読む

- 責務境界・依存方向・設計原則が必要  
  → [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 次に何を実装すべきか、フェーズ順を確認したい  
  → [docs/ROADMAP.md](docs/ROADMAP.md)
- 描画、dirty rect、ビュー変換、レンダリング分割を触る  
  → [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)
- プロダクト意図、MVP、非目標、要求背景を確認したい  
  → [docs/SKETCH.md](docs/SKETCH.md)

## 文書ごとの役割と優先順位

| 優先   | 文書                                                           | 役割                                       | いつ読むか                       |
| ------ | -------------------------------------------------------------- | ------------------------------------------ | -------------------------------- |
| 最優先 | [AGENTS.md](AGENTS.md)                                         | LLM向け入口。読む順番と判断基準を示す      | 常に最初                         |
| 最優先 | [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) | 現在の実装状況、到達済み機能、直近の制約   | 常に最初期                       |
| 高     | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)                   | クレート責務、依存方向、設計原則、境界条件 | 設計変更、責務追加、境界横断修正 |
| 高     | [docs/ROADMAP.md](docs/ROADMAP.md)                             | 実装順序、次フェーズ、優先実装候補         | 何を先に作るべきか判断するとき   |
| 中     | [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)           | render 系の詳細設計                        | キャンバス・描画・dirty 更新関連 |
| 中     | [docs/SKETCH.md](docs/SKETCH.md)                               | 要件、MVP、思想、背景                      | 要件確認、仕様意図の確認         |

## 正本の優先順位

文書同士または文書とコードが食い違う場合は、次の順で扱う。

1. **現に動いているコード**
2. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) の「現在の到達点」
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) の責務境界と原則
4. [docs/ROADMAP.md](docs/ROADMAP.md) のフェーズ順序
5. [docs/SKETCH.md](docs/SKETCH.md) の要求背景

補足:

- 「現在どうなっているか」はコードが正本
- 「どうあるべきか」の設計原則は `ARCHITECTURE.md` を優先
- コードと文書がズレている場合、**未依頼なら大規模な設計修正を勝手に始めない**
- ズレを見つけたら、必要に応じて修正対象を明示する

## タスク別の最小読書セット

### 1. バグ修正

- [AGENTS.md](AGENTS.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- 関連コード
- 必要なら [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

### 2. 新機能追加

- [AGENTS.md](AGENTS.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 必要なら [docs/ROADMAP.md](docs/ROADMAP.md)
- 関連コード

### 3. 描画系の変更

- [AGENTS.md](AGENTS.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)
- `crates/render/` と `apps/desktop/` の関連コード

### 4. UI / パネル / プラグイン境界の変更

- [AGENTS.md](AGENTS.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- `crates/plugin-api/`, `crates/ui-shell/`, `crates/builtin-plugins/`, `apps/desktop/` の関連コード

### 5. 保存形式や永続化の変更

- [AGENTS.md](AGENTS.md)
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/SKETCH.md](docs/SKETCH.md) のデータ要件
- `crates/storage/` と `crates/app-core/` の関連コード

## コードベースの最小地図

- `apps/desktop/`
  - デスクトップホスト
  - `winit` + `wgpu` による起動、入力、最終提示
- `crates/app-core/`
  - ドメインモデルと `Command`
  - UI や GPU に依存しない中核
- `crates/render/`
  - キャンバス描画入力、レンダリング関連処理
- `crates/ui-shell/`
  - ホスト側パネルランタイム
  - レイアウト、ヒットテスト、簡易描画
- `crates/plugin-api/`
  - パネル/ホスト間の契約
- `crates/builtin-plugins/`
  - 組み込み標準パネル
- `crates/storage/`
  - 保存/読込
- `docs/`
  - 設計・要件・進捗の文書
- `target/`
  - ビルド生成物。通常は無視

## 2026-03-09 時点の短い現在地

最小実装としては次が既にある。

- Cargo workspace 構成
- `app-core` / `render` / `ui-shell` / `plugin-api` / `storage` / `builtin-plugins` / `apps/desktop`
- `winit` + `wgpu` の単一ウィンドウデスクトップ起動
- 白キャンバス表示
- マウス入力による最小ストローク描画
- JSON ベースの最小保存/読込
- 組み込みパネルのホスト描画
- `tool-palette` / `layers-panel` / `app-actions` の最小UI
- `Command` 経由の操作統一

詳細は [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) を参照する。

## コンテキスト節約ルール

- まず文書を全部読まない
- 最初はこのファイルと `IMPLEMENTATION_STATUS.md` だけで現在地を掴む
- 詳細が必要なときだけ該当文書へ進む
- 実装変更前に、対象ファイルだけを追加で読む
- 大きな設計変更でなければ、不要な設計文書は読まない
- `target/` やビルド成果物は読まない

## 開発ワークフロー

このリポジトリでは、修正や機能追加を次の流れで進める。

1. まず Issue を立て、目的・範囲・完了条件を明確にする
2. `main` に直接変更せず、Issue に対応する作業ブランチを切る
3. **TDD を基本**とし、まず失敗するテストを書く
4. テストを通すための最小実装を行う
5. 必要ならリファクタリングを行い、テストが通り続けることを確認する
6. 変更後はテストと `clippy` を通す
7. ドキュメントを更新する
8. レビュー可能な状態にしてマージする

補足:

- 小さな修正でも、原則として Issue とブランチを経由する
- 直接 `main` へコミットしない
- 変更は常に「テストがある状態」を維持する

## テストとコードチェック

- 開発スタイルは **TDD first** とする
- バグ修正では、まず再現テストを追加してから実装を直す
- 新機能では、期待される振る舞いをテストで先に固定する
- コードチェックは `clippy` を使う
- 少なくとも `cargo test` と `cargo clippy --workspace --all-targets` が通る状態を保つ

## エージェント向け実務ルール

- 実装前に、変更対象クレートの責務が [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) と矛盾しないか確認する
- 進捗や現状確認はまず [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) を見る
- 何を次にやるべきか迷ったら [docs/ROADMAP.md](docs/ROADMAP.md) を見る
- 描画最適化や dirty rect を触る前に [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md) を見る
- プロダクト思想や MVP 範囲に迷ったら [docs/SKETCH.md](docs/SKETCH.md) を見る
- 修正作業では、Issue と作業ブランチを前提に進める
- 実装前に、先に追加すべきテストを考える
- 実装後は `cargo test` と `cargo clippy --workspace --all-targets` を確認する

## 人間向け補足

LLM に最初に読ませる文書としては、一般的な `README.md` よりも、エージェント向け意図が明確な **`AGENTS.md`** の方が適している。  
このリポジトリでは、今後 LLM 向けの入口文書は `AGENTS.md` に集約する。
