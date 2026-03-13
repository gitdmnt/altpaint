# ADR-002: PNG Export と汎用バックグラウンドジョブ基盤（フェーズ7-3）

- 作業日時: 2026-03-13
- 作業 Agent: claude-sonnet-4-6

## 背景

フェーズ7-3 の課題として、アクティブパネルの内容を PNG 画像として書き出す機能と、
バックグラウンドジョブ状態を job-progress パネルへ反映する基盤を構築する必要があった。

## 決定事項

### 1. PNG エンコードには `png` クレートを採用する

**採用した実装**: `crates/storage/src/export.rs` に `export_active_panel_as_png(document, path)` を定義し、
`Panel.bitmap`（合成済み RGBA8 ビットマップ）を `png` クレートで書き出す。

**理由**:
- `Panel.bitmap` は描画のたびに `composite_panel_bitmap` で最新状態に保たれており、
  追加の合成処理なしに直接 PNG に書き出せる
- `png` クレートは純 Rust 実装で、外部 C ライブラリへの依存がない
- `image` クレートは多機能だが、PNG 1 形式のみを扱う今回の要件には過剰である

### 2. `PendingSaveTask` を `BackgroundJob` へ汎用化する

**採用した構造**:
```rust
pub(crate) enum JobKind { Save, Export { path_display: String } }
pub(crate) struct BackgroundJob { kind: JobKind, handle: JoinHandle<Result<(), String>> }
```

`io_state.pending_save_tasks: Vec<PendingSaveTask>` → `io_state.pending_jobs: Vec<BackgroundJob>` へ変更。

**理由**:
- 今後 export・snapshot・サムネイル生成など複数種の非同期ジョブが増える想定であり、
  型を早期に汎用化しておくと追加コストが小さい
- `label()` メソッドを持たせることでエラーメッセージが種別を含めて分かりやすくなる

### 3. `active_jobs` を host snapshot に反映する

**変更連鎖**:
```
io_state.pending_jobs.len()
  → DesktopApp::present (active_jobs 変数)
  → PanelRuntime::sync_document(document, can_undo, can_redo, active_jobs)
  → PanelPlugin::update(document, can_undo, can_redo, active_jobs)
  → build_host_snapshot(document, can_undo, can_redo, active_jobs)
  → json: "jobs": { "active": active_jobs, ... }
```

`job-progress` パネルは `host::jobs::active()` / `status()` でこの値を読み取る。

**理由**:
- 既存の `can_undo`/`can_redo` 伝達と同じパターンで追加できる
- `host_sync.rs` の jobs フィールドが常に `active: 0` のままだと
  job-progress パネルが常に idle 状態を表示してしまう

### 4. `export.image` service request のパスは payload 優先・なければダイアログ

**挙動**:
- payload に `"path"` キーがある場合はそのパスをそのまま使用する
- ない場合は `dialogs.pick_save_image_path()` を呼ぶ
- `DesktopDialogs` trait に `pick_save_image_path` を追加し、デフォルト実装は `None` を返す

**理由**:
- path を指定して呼ぶ自動テストが可能になる（ダイアログをモックしなくてよい）

## 結果

- `crates/storage/src/export.rs` — PNG export 関数（テスト 3 件付き）
- `crates/storage/Cargo.toml` — `png = "0.17"` 追加
- `apps/desktop/src/app/background_tasks.rs` — `BackgroundJob` / `JobKind`
- `apps/desktop/src/app/io_state.rs` — `pending_jobs: Vec<BackgroundJob>` に変更
- `apps/desktop/src/app/services/export.rs` — `EXPORT_IMAGE` service handler
- `crates/panel-api/src/lib.rs` — `PanelPlugin::update` に `active_jobs: usize` 追加
- `crates/panel-runtime/src/host_sync.rs` — `active_jobs` パラメータ追加・JSON 反映
- `crates/panel-runtime/src/registry.rs` — `sync_document` 系シグネチャ更新
- `apps/desktop/src/app/present.rs` — `active_jobs` 渡す
- `apps/desktop/src/app/bootstrap.rs` — `active_jobs = 0` で呼ぶ
- `crates/desktop-support/src/dialogs.rs` — `pick_save_image_path` メソッド追加

## トレードオフ

- 大きなドキュメントでは PNG 書き出しに時間がかかるが、バックグラウンドスレッドで実行するため UI はブロックしない
- `Panel.bitmap` は常に合成済みであり別途 flatten 処理は不要だが、将来レイヤー単体 export が必要になった場合は `export.rs` を拡張する必要がある
- `.altp` ドキュメント形式の export は既存の `save_project_to_path` で対応済みのため、7-3 では PNG のみを追加した
