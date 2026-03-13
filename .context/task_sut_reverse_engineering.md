# タスク: CSP .sut ファイルの内部構造解析

作成日: 2026-03-13

## 目的

Clip Studio Paint (CSP) のペン設定ファイル `.sut` の内部構造を差分比較で特定し、
altpaint の `crates/storage` でペンカタログとして読み込む実装の基盤を作る。

## 前提作業（ユーザーが手動で行う）

以下の手順で `.sut` ファイルを用意する：

1. CSP で「基本のペン（デフォルト値のまま）」を1つエクスポート → `pen_baseline.sut`
2. 同じペンからパラメータを **1つだけ変えて** それぞれエクスポート：
   - 不透明度を変更 → `pen_opacity_XX.sut`
   - ブラシサイズを変更 → `pen_size_XX.sut`
   - 硬さ（ソフトネス）を変更 → `pen_hardness_XX.sut`
   - 筆圧感度（サイズ）を変更 → `pen_pressure_size_XX.sut`
   - 筆圧感度（不透明度）を変更 → `pen_pressure_opacity_XX.sut`
   - テクスチャ / ブラシ素材を変更 → `pen_texture_XX.sut`
   - その他、変えられるパラメータがあれば追加

ファイルは `.context/sut_samples/` に配置する。

## Agent が行うべき作業

### Step 1: バイナリ / テキスト判定

```bash
file .context/sut_samples/*.sut
xxd .context/sut_samples/pen_baseline.sut | head -40
```

- テキスト（XML / JSON / TOML 等）なら構造をそのまま読む
- バイナリなら `xxd` でヘッダを確認し、既知フォーマット（ZIP, SQLite, etc.）を判断する

### Step 2: ベースラインの全フィールド洗い出し

- テキストの場合：フィールド名と値を一覧化する
- バイナリの場合：ヘッダ・セクション構造をメモする

### Step 3: 差分比較でフィールドとパラメータを対応付ける

```bash
diff <(xxd pen_baseline.sut) <(xxd pen_opacity_XX.sut)
# または
diff pen_baseline.sut pen_opacity_XX.sut  # テキストの場合
```

各 `.sut` と baseline の差分を取り、変化したフィールドとパラメータ名の対応表を作成する。

### Step 4: 対応表の文書化

調査結果を `.context/sut_format.md` にまとめる。
最低限含めるべき内容：

| CSP パラメータ名 | .sut 内フィールド名/オフセット | 型 | 値域 |
|---|---|---|---|
| ブラシサイズ | ... | ... | ... |
| 不透明度 | ... | ... | ... |
| ... | ... | ... | ... |

### Step 5: パーサーの TDD 実装

- `crates/storage` に `.sut` パーサーを追加する
- TDD: 先にテストを書いてから実装する
- テストデータは `.context/sut_samples/` のファイルを使う（または最小のフィクスチャを `tests/fixtures/` にコピー）

```
crates/storage/
  src/
    sut_import.rs   ← 新規
  tests/
    sut_import_test.rs  ← TDD テスト
```

## 完了条件

- [ ] 全サンプル `.sut` の差分から主要パラメータとフィールドの対応が特定できている
- [ ] `.context/sut_format.md` に対応表がある
- [ ] `crates/storage` に `.sut` → `PenEntry`（または相当する型）への変換関数がある
- [ ] `cargo test -p storage` がパスする
- [ ] `cargo clippy --workspace --all-targets` がパスする

## 関連ファイル

- `crates/storage/src/` — ペンカタログの保存/読込
- `crates/app-core/src/` — `PenEntry` など domain 型
- `docs/ROADMAP.md` — ペンカタログ実装フェーズ
