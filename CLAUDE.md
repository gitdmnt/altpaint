# CLAUDE.md

Claude Code がこのリポジトリを扱う際の唯一の入口ファイル。
コンテキストを節約し、必要な文書だけを順に読むための案内として使う。

**文書とコードが食い違う場合、現に動いているコードが正本。**

---

## 最初に読む順序

1. このファイル（CLAUDE.md）
2. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) — 現在の到達点と直近の制約

多クレート境界の作業では追加で読む:
- [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

---

## コマンド

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test --workspace
cargo test -p <crate-name>
cargo test <test_name>

# Lint（コミット前に必須）
cargo clippy --workspace --all-targets

# Panel Wasm ビルド
bash scripts/build-ui-wasm.sh          # Linux / WSL2
.\scripts\build-ui-wasm.ps1            # Windows (PowerShell)
.\scripts\build-ui-wasm.ps1 -Release
```

---

## 開発ワークフロー

1. Issue を立て、目的・範囲・完了条件を明確にする
2. `main` に直接コミットしない。Issue に対応したブランチを切る
3. **TDD first**: まず失敗するテストを書き、最小実装で通す
4. 変更後: `cargo test -p <crate-name>` → `cargo test` → `cargo clippy --workspace --all-targets`
5. コード変更直後にドキュメントを更新する（「コードが正本」の順序を崩さない）
6. **タスク終了時は ask_user で待機する**

---

## コーディング方針

- **OS 固有コード禁止**: `#[cfg(target_os = "...")]` や `cfg!(target_os = ...)` はアプリケーション層のロジックに書かない。OS 差異はクロスプラットフォームライブラリ（`fontdb`、`winit`、`dirs` 等）に吸収させる。どうしても必要な場合は PR レビューで明示的に承認を得ること。

---

## アーキテクチャ概要

altpaint はデスクトップ向けデジタルペイントアプリ。Rust 2024-edition Cargo workspace（28 メンバー: ライブラリ 15、プラグイン 10、デスクトップアプリ 1）。

### Runtime Flow

**起動**: `apps/desktop` が winit + wgpu 初期化 → `DesktopApp::new` がセッション/プロジェクト/ワークスペース復元 → `PanelRuntime` が `plugins/**/*.altp-panel` を読み込む → `storage` がツール・ペンを読み込む → 初期レンダリング

**入力 → 描画**: OS入力 → `runtime/pointer.rs` 正規化 → `app/input.rs` がキャンバスかパネルへ振り分け → `canvas::view_mapping` が座標変換 → `canvas::gesture` が `PaintInput` を生成 → `canvas::context_builder` が `Document` からペイントコンテキストを解決 → ビルトインビットマッププラグインがビットマップ差分を書く → 差分を `Document` に適用 → `render::FramePlan` 組み立て → dirty rect 合成 → `wgpu_canvas.rs` が GPU へ提示

**パネル**: `panel-dsl` が `.altp-panel` をパース → `plugin-host`（wasmtime）が Wasm を実行 → `PanelRuntime` がホストスナップショットを同期 → `PanelEvent`/`HostAction` → `DesktopApp` が `Command` またはサイドエフェクトとして適用 → `render` がパネルサーフェスとヒット領域をラスタライズ

### 主要クレート

| クレート                              | 責務                                                                                |
| ------------------------------------- | ----------------------------------------------------------------------------------- |
| `apps/desktop`                        | winit + wgpu ホスト、`DesktopApp` 統括、入力ルーティング、提示                      |
| `crates/app-core`                     | `Document`、ドメインモデル（Work→Page→Panel→LayerNode）、`Command`、ペイント基本型   |
| `crates/canvas`                       | `CanvasRuntime`、ジェスチャーステートマシン、ビットマップ操作                        |
| `crates/render`                       | `FramePlan`/`CanvasPlan`/`OverlayPlan`/`PanelPlan`、dirty rect、CPU フレーム合成    |
| `crates/panel-runtime`                | パネルレジストリ、DSL/Wasm ブリッジ、ホストスナップショット同期、永続設定            |
| `crates/ui-shell`                     | パネルワークスペースレイアウト、フォーカス、ヒットテスト、サーフェスレンダリング     |
| `crates/panel-api`                    | パネル/ホスト間コントラクト（`PanelPlugin`、`PanelEvent`、`HostAction`）             |
| `crates/plugin-host`                  | wasmtime ベースの Wasm パネルランタイム                                              |
| `crates/panel-dsl`                    | `.altp-panel` パーサー/バリデーター/IR                                               |
| `crates/panel-schema`                 | ホスト↔Wasm 共有 DTO                                                                 |
| `crates/plugin-sdk` + `plugin-macros` | プラグイン作者向け SDK と proc-macro                                                 |
| `crates/storage`                      | SQLite プロジェクト永続化、ペン/ツールカタログ                                       |
| `crates/desktop-support`              | セッション、ダイアログ、パス、プロファイラー、キャンバステンプレート                 |
| `crates/workspace-persistence`        | `WorkspaceUiState`、`PluginConfigs` 共有 DTO                                        |
| `plugins/*`                           | 10 個のビルトインパネル（各々 `.altp-panel` + Rust/Wasm ソース + コンパイル済み `.wasm`） |

### 責務集中箇所（リファクタリング中）

- **`DesktopApp`** (`apps/desktop/src/app/`) — ブートストラップ、コマンドルーティング、パネルディスパッチ、I/O サービス、dirty rect 収集、提示ロジック
- **`Document`** (`crates/app-core/src/document.rs`) — ドメイン状態 + ツール/ペンランタイム状態
- **`CanvasRuntime`** (`crates/canvas/src/runtime.rs`) — ペイントプラグインレジストリ、コンテキスト構築、ビットマップ操作

### ファイル配置規則

- `runtime/` — 外部ランタイム・ステートフルブリッジ
- `presentation/` — レイアウト、ヒットテスト、フォーカス、テキスト入力、サーフェス生成
- `services/` — I/O 統括（プロジェクト、ワークスペース、エクスポート、カタログ）
- `ops/` — 高頻度なキャンバス/レンダリング操作
- `tests/` — クレート/モジュール境界テスト
- `lib.rs` — モジュール宣言、再エクスポート、薄い公開 API のみ（大きな実装は置かない）

---

## タスク別の最小読書セット

| タスク種別                          | 追加で読む文書                                              |
| ----------------------------------- | ----------------------------------------------------------- |
| バグ修正                            | 関連コード、必要なら `ARCHITECTURE.md`                     |
| 新機能追加                          | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`ROADMAP.md`  |
| 描画系の変更                        | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`RENDERING-ENGINE.md` |
| UI / パネル / プラグイン境界        | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`                |
| 保存・永続化の変更                  | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`SKETCH.md`   |

## 文書の正本優先順位

1. **現に動いているコード**
2. [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)
3. [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)
4. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
5. [docs/ROADMAP.md](docs/ROADMAP.md)
6. [docs/SKETCH.md](docs/SKETCH.md)

補足: コードと文書がズレていても、**未依頼なら大規模な設計修正を勝手に始めない**。

## 主要ドキュメント

| 文書                                                                   | いつ読むか                             |
| ---------------------------------------------------------------------- | -------------------------------------- |
| [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)         | 常に最初期（現在の実装状況）           |
| [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)             | 多クレート修正、境界確認時             |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)                           | 設計変更、責務追加、境界横断修正時     |
| [docs/ROADMAP.md](docs/ROADMAP.md)                                     | 次に何を実装すべきか判断するとき       |
| [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)                   | キャンバス・描画・dirty rect 関連      |
| [docs/SKETCH.md](docs/SKETCH.md)                                       | 要件確認、仕様意図の確認               |
| [docs/builtin-plugins/PLUGIN_DEVELOPMENT.md](docs/builtin-plugins/PLUGIN_DEVELOPMENT.md) | プラグイン開発・Wasm ビルド |

---

## パネルプラグイン開発

詳細は [docs/builtin-plugins/PLUGIN_DEVELOPMENT.md](docs/builtin-plugins/PLUGIN_DEVELOPMENT.md) を参照。

ビルトインプラグインは `plugins/<name>/` に配置（`panel.altp-panel`、`src/lib.rs`、コンパイル済み `.wasm`）。
`.wasm` は git-ignored のため、`build-ui-wasm.sh`（または `.ps1`）で再ビルドすること。

命名規則: `builtin-panel-<plugin-name>` → アーティファクト `builtin_panel_<plugin_name>.wasm`
新しいプラグインを追加した際は、両方のビルドスクリプト（`.ps1` と `.sh`）を更新すること。

---

## ブラックボックステスト結果 JSON への対応

`tests/blackbox-testsheet.html` から出力されるテスト結果 JSON（`schema: "altpaint-blackbox-testsheet/1"`）が渡された場合、次の手順を自動で実行する。

### 1. 結果を解釈する

- `status: "fail"` → 直接バグとして調査・修正
- `status: "skip"` + `notes` あり → notes が根本原因の手がかり（例: `"プラグインが表示されていないので切り替え不可"` → プラグインロード失敗を調査）
- `fps_rating: "<60"` → パフォーマンス問題として調査
- `seconds: null` の起動時間 → 計測できていないだけなので修正不要

### 2. プラグインが表示されない場合（最頻出パターン）

`.wasm` が未ビルドまたはパッケージ名不一致の可能性が高い。

1. `plugins/*/panel.altp-panel` の `runtime { wasm: "..." }` で期待ファイル名を確認
2. `ls plugins/*/*.wasm` でファイル存在を確認
3. なければビルドスクリプトを実行
4. `plugins/<name>/Cargo.toml` の `name =` とビルドスクリプトのパッケージ名が一致しているか確認

### 3. 調査後の流れ

1. `cargo test --workspace` と `cargo clippy --workspace --all-targets` を通す
2. `docs/IMPLEMENTATION_STATUS.md` を更新する
3. ask_user で結果を報告して待機する

---

## コンテキスト節約ルール

- 最初はこのファイルと `IMPLEMENTATION_STATUS.md` だけで現在地を掴む
- 詳細が必要なときだけ該当文書へ進む
- 実装変更前に対象ファイルだけを追加で読む
- `target/` やビルド成果物は読まない
- 作業はなるべくサブエージェントを活用する
