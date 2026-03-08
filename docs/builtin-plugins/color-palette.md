# Color Palette

## 概要

Color Palette は、現在のブラシ色を切り替える最小のビルトインパネルです。

- panel id: builtin.color-palette
- title: Colors
- 実装: crates/builtin-plugins/src/color_palette.rs

## 目的

ホスト側パネルからブラシ色を変更し、その結果がキャンバス描画へ反映される導線を確認するためのパネルです。

## 内部状態

このプラグインは ColorPaletteSnapshot を持ちます。

保持内容:

- active_color

update(...) のたびに Document.active_color を読み取り、現在選択中の色を追従します。

## 現在のUI構造

PanelTree 上では 1 つの Section を持ちます。

- Section: Palette
  - Row: Black / Red / Blue
  - Row: Green / Gold / Violet

各ボタンは色付きの塗りで描画され、選択中の色は active=true になります。
ホスト側では active 状態を強調枠として描画します。

## 発行する HostAction

- Black → Command::SetActiveColor { color: #000000 }
- Red → Command::SetActiveColor { color: #E53935 }
- Blue → Command::SetActiveColor { color: #1E88E5 }
- Green → Command::SetActiveColor { color: #43A047 }
- Gold → Command::SetActiveColor { color: #FB8C00 }
- Violet → Command::SetActiveColor { color: #8E24AA }

## view() での補助表示

簡易行表示では、現在選択中の色に > マーカーを付けます。

例:

- > Black (#000000)
-   Red (#E53935)

## 既知の制約

- 色は 6 色の固定プリセットです
- カスタム色選択、HSV スライダー、不透明度変更はまだありません
- 現時点ではブラシ色のみが変わり、消しゴム色は常に白です

## 今後の拡張候補

- カスタム色履歴
- スポイト
- 前景色/背景色の二段構成
- 不透明度、ブラシサイズとの統合