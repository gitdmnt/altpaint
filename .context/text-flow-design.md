# テキスト流し込み機能 設計ドキュメント

作成日: 2026-03-14
フェーズ: 7-5b

---

## 目的

コミック制作ワークフローにおいて、セリフや説明文をキャンバスのレイヤーへ直接配置できるようにする。
まず「単一行 ASCII/UTF-8 の基本描画」を最小実装として完成させ、段階的に拡張する。

---

## 抽象化レイヤー

### `TextRenderer` trait（`crates/canvas/src/ops/text.rs`）

```rust
pub trait TextRenderer: Send + Sync {
    /// テキストを RGBA ビットマップへラスタライズする。
    fn render(&self, text: &str, font_size: u32, color: [u8; 4]) -> TextRenderOutput;
}

pub struct TextRenderOutput {
    pub pixels: Vec<u8>,  // RGBA row-major、アルファ事前乗算なし
    pub width: usize,
    pub height: usize,
}
```

インターフェースは「テキスト」「フォントサイズ」「色」→「RGBA ビットマップ」を返す形とし、
将来的に別ライブラリへ差し替えられる境界を固定する。

---

## 第一実装: `Font8x8Renderer`

### 選択理由

- `font8x8` crate はすでにワークスペース依存として存在する
- 外部ファイル（TTF等）不要、純 Rust
- 8×8 ビットマップフォントで ASCII/基本 Unicode をカバーする
- スケーリングにより任意の `font_size` に対応可能

### 実装方針

```
font_size → scale = font_size / 8 (整数、最小1)
各文字 → 8×8 ビットマスク → scale×scale ブロックで拡大 → 横に連結
```

### 将来の差し替え候補

| 実装 | 特徴 |
|------|------|
| `fontdue` | 純 Rust、TTF/OTF、スムーズなラスタライズ |
| `ab_glyph` | TTF/OTF、すでにワークスペースに存在 |

差し替えは `TextRenderer` impl を新しいクラスで追加し、ホスト側の選択ロジックで切り替えるだけで対応可能。

---

## Canvas Op

```rust
// crates/canvas/src/ops/text.rs
pub fn render_text_to_bitmap_edit(
    text: &str,
    font_size: u32,
    color: [u8; 4],
    x: usize,
    y: usize,
) -> Option<BitmapEdit>
```

`TextRenderer` 実装を呼び出し、`BitmapEdit` として返す。
呼び出し元（`text_render` service handler）がアクティブレイヤーへ適用する。

---

## Service API

| 名前 | ペイロード |
|------|-----------|
| `text_render.render_to_layer` | `text: String`, `font_size: u32`, `color_hex: String`, `x: usize`, `y: usize` |

---

## Plugin: `plugins/text-flow`

- テキスト入力フィールド（`<input bind=...>`）
- フォントサイズスライダー
- X / Y 座標スライダー
- 「テキストを描画」ボタン → `text_render.render_to_layer` service 発行

---

## 対象外（将来タスク）

- 複数行・折り返し
- IME / 日本語入力
- カーニング・字間調整
- テキストレイヤー（非破壊編集）
- フォントファイル選択

---

## 完了条件

- `Font8x8Renderer` が単一行 ASCII テキストを RGBA ビットマップへ変換できる
- `render_text_to_bitmap_edit` が `BitmapEdit` を返し、レイヤーへ適用できる
- `text-flow` plugin がビルドでき、`cargo test` が通る
- `TextRenderer` trait の差し替えが可能な構造になっている
