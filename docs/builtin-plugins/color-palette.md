# Color Palette

## 概要

Color Palette は、現在のブラシ色を RGB スライダーで調整し、結果をライブプレビューする最小のビルトインパネルです。

- panel id: builtin.color-palette
- title: Colors
- 実装: crates/builtin-plugins/src/color_palette.rs

## 目的

ホスト側パネルからブラシ色を変更し、その結果がキャンバス描画へ反映される導線を確認するためのパネルです。
プリセット色パレットは持たず、現在色のプレビューを見ながら直接調整します。

## 内部状態

このプラグインは ColorPaletteSnapshot を持ちます。

保持内容:

- active_color

update(...) のたびに Document.active_color を読み取り、現在選択中の色を追従します。

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Custom
  - ColorPreview: Live Preview #RRGGBB
  - Text: R:.. G:.. B:..
  - Slider: Red
  - Slider: Green
  - Slider: Blue

ColorPreview は現在の `Document.active_color` をそのまま矩形で表示するライブプレビューです。
RGB スライダーは 0〜255 の範囲で現在色の各チャンネルを直接変更できます。
スライダー操作中も `Command::SetActiveColor` が継続発行され、プレビュー表示も即時に追従します。

## 発行する HostAction

- Red slider / Green slider / Blue slider → Command::SetActiveColor { color: 現在色の該当チャンネルだけ更新した値 }

## view() での補助表示

簡易行表示では、現在色と RGB 値を 1 行で表示します。

例:

- Preview #000000 / R:0 G:0 B:0

## 既知の制約

- 現在のスライダーは RGB 3 本だけです
- HSV スライダー、不透明度変更はまだありません
- 現時点ではブラシ色のみが変わり、消しゴム色は常に白です

## 今後の拡張候補

- カスタム色履歴
- スポイト
- 前景色/背景色の二段構成
- 不透明度、ブラシサイズとの統合