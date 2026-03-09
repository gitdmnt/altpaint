# refactor context

## 目的

大規模リファクタ時の判断材料と進行状況を簡潔に残す。

## 2026-03-09 ベースライン

- `cargo test --workspace`: 成功
- `cargo llvm-cov --workspace --summary-only`: 成功
- ベースライン総カバレッジ
  - Regions: 77.16%
  - Functions: 76.40%
  - Lines: 75.83%

## 低カバレッジの主な対象

- `apps/desktop/src/wgpu_canvas.rs`: 8.48% Lines
- `crates/app-core/src/workspace.rs`: 0.00% Lines
- `plugins/app-actions/src/lib.rs`: 0.00% Lines
- `plugins/color-palette/src/lib.rs`: 0.00% Lines
- `plugins/job-progress/src/lib.rs`: 0.00% Lines
- `plugins/layers-panel/src/lib.rs`: 0.00% Lines
- `plugins/snapshot-panel/src/lib.rs`: 0.00% Lines
- `plugins/tool-palette/src/lib.rs`: 0.00% Lines

## 今回のリファクタ方針

1. 既存動作を固定する追加テストを先に入れる
2. `apps/desktop/src/main.rs` から純粋なフレーム合成処理と性能計測処理を分離する
3. `DesktopApp` の状態更新フラグ操作をヘルパーへ集約し、副作用を局所化する
4. プラグイン側はコマンド記述子生成や入力正規化を純粋関数へ寄せてテストしやすくする
5. 必要なコメントとモジュールドキュメントを追加する

## 進行メモ

- まず `workspace.rs` と主要プラグイン crate のテストを追加する
- 次に `desktop` の構造分離を行う
- 最後に `cargo test` / `cargo clippy --workspace --all-targets` / `cargo llvm-cov --workspace --summary-only` を再実行する

## 実施結果

- `apps/desktop/src/main.rs` から純粋なフレーム合成ロジックを `apps/desktop/src/frame.rs` へ分離
- `apps/desktop/src/main.rs` から性能計測ロジックを `apps/desktop/src/profiler.rs` へ分離
- `DesktopApp` の UI 再描画フラグ操作をヘルパーへ集約
- `DesktopRuntime` の入力計測処理をヘルパーへ集約
- `crates/app-core/src/workspace.rs` に回帰テストを追加
- `plugins/*` の command 構築を純粋関数化し、主要プラグイン crate のテストを追加

## 2026-03-09 事後結果

- `cargo test --workspace`: 成功
- `cargo clippy --workspace --all-targets`: 成功
- `cargo llvm-cov --workspace --summary-only`: 成功
- 事後総カバレッジ
  - Regions: 79.02%
  - Functions: 80.80%
  - Lines: 77.65%

## ベースラインとの差分

- Regions: 77.16% → 79.02% (+1.86pt)
- Functions: 76.40% → 80.80% (+4.40pt)
- Lines: 75.83% → 77.65% (+1.82pt)
