# Pen Settings

## 概要

Pen Settings は、現在アクティブなツールが公開する設定項目を表示する最小パネルです。

- panel id: builtin.pen-settings
- title: Pen
- 実装: plugins/pen-settings/

## 目的

tool catalog から選ばれた active tool の設定項目を、host 側パネルから調整できることを確認するためのパネルです。

## 現在のUI構造

- Section: 現在のツール
  - active tool の表示名
  - active tool id
  - provider / drawing plugin id
- Section: 太さ
  - 現在のプリセット名
  - 現在の幅表示
  - 幅スライダー
- Section: 描画特性
  - active tool が公開した setting key に応じて筆圧 / antialias / stabilization を出し分け

## 参照する host API

- `panel_sdk::host::tool::pen_name()`
- `panel_sdk::host::tool::pen_size()`
- `panel_sdk::host::tool::active_id()`
- `panel_sdk::host::tool::active_label()`
- `panel_sdk::host::tool::active_provider_plugin_id()`
- `panel_sdk::host::tool::active_drawing_plugin_id()`
- `panel_sdk::host::tool::supports_*()`

取得した値は Wasm handler が local state へ反映し、`.altp-panel` 側は active tool が公開する設定キーだけを表示します。

## 発行する HostAction

- Slider change → `Command::SetActivePenSize`

## 既知の制約

- 任意の setting schema を動的生成する段階ではまだなく、現在は `size` / `pressure_enabled` / `antialias` / `stabilization` を出し分ける最小実装です
