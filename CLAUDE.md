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

1. 計画を立て、目的・範囲・完了条件を明確にする
2. 計画の自己レビューを行う
3. **TDD first**: まず失敗するテストを書き、最小実装で通す
4. 後方互換のコードは削除する
5. 変更後: `cargo test -p <crate-name>` → `cargo test` → `cargo clippy --workspace --all-targets`
6. コード変更直後にドキュメントを更新する（「コードが正本」の順序を崩さない）
7. **タスク終了時は ask_user で待機する**

---

## コーディング方針

- **OS 固有コード禁止**: `#[cfg(target_os = "...")]` や `cfg!(target_os = ...)` はアプリケーション層のロジックに書かない。OS 差異はクロスプラットフォームライブラリ（`fontdb`、`winit`、`dirs` 等）に吸収させる。どうしても必要な場合は PR レビューで明示的に承認を得ること。
- **後方互換コードを残さない**: 現在alpha版として開発中なので後方互換コードは全て消し、コードベースの肥大化を常に軽減する。

---

## アーキテクチャ概要

altpaint はデスクトップ向けデジタルペイントアプリ。Rust 2024-edition Cargo workspace（30 メンバー: ライブラリ 17、ビルトインパネル 12、デスクトップアプリ 1）。

### Runtime Flow

**起動**: `apps/desktop` が winit + wgpu 初期化 → `DesktopApp::new` がセッション/プロジェクト/ワークスペース復元 → `PanelRuntime` が `crates/builtin-panels/` の HTML+CSS+Wasm パネル 12 個を読み込む → `storage` がツール・ペンを読み込む → 初期レンダリング

**入力 → 描画**: OS入力 → `event_loop/pointer.rs` 正規化 → `app/input.rs` がキャンバスかパネルへ振り分け → `canvas-geometry::map_view_to_canvas_with_transform` が座標変換 → `paint_engine::gesture` が `PaintInput` を生成 → `paint_engine::context_builder` が `Document` からペイントコンテキストを解決 → `gpu-paint` の compute shader が GPU レイヤーテクスチャへ直接描画（ブラシ/塗りつぶし/合成）→ `wgpu_canvas.rs` が GPU へ提示

**パネル**: `HtmlWasmPanel` が `panel.html` + `panel.css` をロード → `panel-wasm-host`（wasmtime）が Wasm を実行し DOM mutation host function で直接 DOM を書換え → `PanelRuntime` が host state を同期 → `PanelEvent`（Activate/Keyboard 等）/`HostRequest`（`RequestDescriptor` ベース） → `DesktopApp` が translator registry 経由で `DocumentCommand`/`SessionCommand`/`ServiceRequest` またはサイドエフェクトとして適用 → `panel-html::HtmlPanelView`（Blitz + vello）が GPU テクスチャに直描画 → `wgpu_canvas` が `panel_quads` レイヤーで合成。hit / move handle テーブルは `prepare_present_frame` が GPU 非依存で毎フレーム更新

### 主要クレート

| クレート                              | 責務                                                                                      |
| ------------------------------------- | ----------------------------------------------------------------------------------------- |
| `apps/desktop`                        | winit + wgpu ホスト、`DesktopApp` 統括、入力ルーティング、提示、`EditHistory`（`PaintPatch` Cpu/Gpu）、`CanvasPlan`/overlay DTO |
| `crates/geometry`                     | 座標型・矩形・dirty rect 演算（ローカル依存ゼロ）                                          |
| `crates/raster`                       | `RgbaBitmap`、ピクセルブレンド、ラスタライズ、`BitmapEdit`（`geometry` のみ依存）          |
| `crates/document-model`               | `Document`、作品ドメインモデル（Work→Page→Koma→RasterLayer）、`DocumentCommand`、`normalize_after_load` |
| `crates/editor-state`                 | `EditorSession`、`ToolDefinition`/`PenPreset`、`SessionCommand`、`view_policy`、`tool_state` |
| `crates/paint-engine`                 | `PaintEngine`、ジェスチャーステートマシン、ビットマップ操作、`PaintInput`/`PaintPlugin`    |
| `crates/gpu-paint`                    | `LayerTextureStore`、ブラシ/塗りつぶし/レイヤー合成の compute shader dispatch（`BrushPipeline`/`FillPipeline`/`CompositePipeline`） |
| `crates/canvas-geometry`              | `CanvasViewGeometry` 単一経路（view↔page 座標写像 + `TextureQuad`）のキャンバス表示幾何     |
| `crates/panel-runtime`                | パネルサブシステム facade。`PanelRuntime`/`HtmlWasmPanel`（具象保持）、Wasm ブリッジ、`HostStateRegistry`（revision キャッシュ）、translator registry、`HostRequest`/`PanelEvent`/`ServiceRequest` 契約型（旧 panel-api を C9 で吸収）、永続設定、同梱パネル loader、panel-html の最小面再公開 |
| `crates/panel-html`                   | `HtmlPanelView`（Blitz HTML/CSS + parley + vello GPU 直描画、hit 矩形収集。責務別 4 モジュール分割） |
| `crates/panel-workspace`              | パネルワークスペースレイアウト、フォーカス、ヒットテスト、`PanelGeometry` 1 map、`ResizeHandle`/`PanelMoveDirection`（旧 panel-api から C9 で移設） |
| `crates/panel-wasm-host`              | wasmtime ベースの Wasm パネルランタイム + DOM mutation host functions                     |
| `crates/panel-protocol`               | ホスト↔Wasm 共有 DTO・ABI 定数・wire 名定数・`HostState`/`HostCallInput`/`HandlerEffects`（ローカル依存ゼロ、serde/serde_json のみ） |
| `crates/panel-sdk` + `panel-macros`   | パネル作者向け SDK と proc-macro                                                          |
| `crates/storage`                      | SQLite プロジェクト永続化、ペン/ツールカタログ                                            |
| `crates/desktop-support`              | セッション、ダイアログ、パス、プロファイラー、キャンバスサイズプリセット                  |
| `crates/builtin-panels/*`             | 12 個のビルトインパネル（各々 `panel.html` + `panel.css` + `panel.meta.json` + Rust/Wasm ソース） |

### ファイル配置規則

- `runtime/` — 外部ランタイム・ステートフルブリッジ
- `presentation/` — レイアウト、ヒットテスト、フォーカス、テキスト入力、サーフェス生成
- `services/` — I/O 統括（プロジェクト、ワークスペース、エクスポート、カタログ）
- `ops/` — 高頻度なキャンバス/レンダリング操作
- `tests/` — クレート/モジュール境界テスト
- `lib.rs` — モジュール宣言、再エクスポート、薄い公開 API のみ（大きな実装は置かない）

---

## タスク別の最小読書セット

| タスク種別                   | 追加で読む文書                                                     |
| ---------------------------- | ------------------------------------------------------------------ |
| バグ修正                     | 関連コード、必要なら `ARCHITECTURE.md`                             |
| 新機能追加                   | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`ROADMAP.md`          |
| 描画系の変更                 | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`RENDERING-ENGINE.md` |
| UI / パネル / プラグイン境界 | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`                        |
| 保存・永続化の変更           | `MODULE_DEPENDENCIES.md`、`ARCHITECTURE.md`、`SKETCH.md`           |

## 主要ドキュメント

| 文書                                                                                     | いつ読むか                         |
| ---------------------------------------------------------------------------------------- | ---------------------------------- |
| [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md)                           | 常に最初期（現在の実装状況）       |
| [docs/MODULE_DEPENDENCIES.md](docs/MODULE_DEPENDENCIES.md)                               | 多クレート修正、境界確認時         |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)                                             | 設計変更、責務追加、境界横断修正時 |
| [docs/ROADMAP.md](docs/ROADMAP.md)                                                       | 次に何を実装すべきか判断するとき   |
| [docs/RENDERING-ENGINE.md](docs/RENDERING-ENGINE.md)                                     | キャンバス・描画・dirty rect 関連  |
| [docs/SKETCH.md](docs/SKETCH.md)                                                         | 要件確認、仕様意図の確認           |
| [docs/builtin-plugins/PLUGIN_DEVELOPMENT.md](docs/builtin-plugins/PLUGIN_DEVELOPMENT.md) | プラグイン開発・Wasm ビルド        |

---

## コンテキスト節約ルール

- 最初はこのファイルと `IMPLEMENTATION_STATUS.md` だけで現在地を掴む
- 詳細が必要なときだけ該当文書へ進む
- 実装変更前に対象ファイルだけを追加で読む
- `target/` やビルド成果物は読まない
- 作業はなるべくサブエージェントを活用する
