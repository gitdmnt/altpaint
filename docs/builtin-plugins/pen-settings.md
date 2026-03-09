# Pen Settings

## 概要

Pen Settings は、現在アクティブなペンプリセットの幅を変更する最小パネルです。

- panel id: builtin.pen-settings
- title: Pen
- 実装: plugins/pen-settings/

## 目的

フェーズ9の可変幅ペンを、ホスト側パネルから調整できることを確認するためのパネルです。

## 現在のUI構造

- Section: Pen Width
  - 現在のプリセット名
  - 現在の幅表示
  - 幅スライダー

## 参照する host snapshot

- `host.tool.pen_name`
- `host.tool.pen_size`
- `host.tool.pen_min_size`
- `host.tool.pen_max_size`

## 発行する HostAction

- Slider change → `Command::SetActivePenSize`

## 既知の制約

- まだ筆圧やカーブはない
- 現時点では丸ペン系の最小 dab 表現のみ
- プリセット詳細編集UIはまだない
