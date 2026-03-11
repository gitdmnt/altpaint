# 入力フロー整理: 描画 / パン / パネル移動

更新日: 2026-03-11

このメモは、現在の desktop host で

- 描画入力をしたとき
- パン入力をしたとき
- パネルを移動したとき

に、どの関数がどの順で呼ばれ、各関数が何を担当するかを整理したもの。

---

## 1. 描画入力フロー

対象入力:

- マウス押下 / ドラッグ / 解放
- タッチ開始 / 移動 / 終了
- raw mouse motion による補間ドラッグ

### 入口

1. `DesktopRuntime::handle_mouse_button()`
2. `DesktopRuntime::handle_mouse_cursor_moved()`
3. `DesktopRuntime::handle_raw_mouse_motion()`
4. `DesktopRuntime::handle_touch_phase()`

役割:

- OS / winit の生イベントを受ける
- 現在カーソル位置や pressure を正規化する
- `DesktopApp` のポインタ API へ橋渡しする

### app 側のポインタ解釈

5. `DesktopApp::handle_pointer_pressed_with_pressure()`
6. `DesktopApp::handle_pointer_dragged_with_pressure()`
7. `DesktopApp::handle_pointer_released_with_pressure()`

役割:

- パネル操作を先に試す
- キャンバス入力なら `handle_canvas_pointer()` へ流す
- 現在の interaction state を見て down / drag / up を振り分ける

### キャンバス入力の解釈

8. `DesktopApp::handle_canvas_pointer()`

役割:

- window 座標を canvas 座標へ変換する
- active panel 外を弾く
- ペン補正 (`stabilized_canvas_position()`) を適用する
- 入力を `PaintInput` へ落とす
  - 点: `PaintInput::Stamp`
  - 線分: `PaintInput::StrokeSegment`
  - バケツ: `PaintInput::FloodFill`
  - 投げ縄塗り: `PaintInput::LassoFill`

### paint plugin 呼び出し

9. `DesktopApp::execute_paint_input()`

役割:

- `PaintPluginContext` を組み立てる
  - 現在ツール
  - 現在色
  - 現在ペン preset
  - 解決済みサイズ
  - active layer bitmap
  - composited bitmap
  - background layer かどうか
- `paint_plugins` から対象 plugin を解決する
- `PaintPlugin::process()` を呼ぶ
- plugin が返した `Vec<BitmapEdit>` を `apply_bitmap_edits()` へ渡す

### built-in bitmap plugin 内部

10. `BuiltinBitmapPaintPlugin::process()`
11. `stamp_edit()` / `stroke_segment_edit()` / `flood_fill_edit()` / `lasso_fill_edit()`

役割:

- 入力ごとに dirty rect と入力 bitmap A を生成する
- `BitmapEdit` として返す
- 同時に、元レイヤー bitmap B とどう合成するかの関数 `f: (bitmap_A, bitmap_B) -> bitmap` を
  `BitmapComposite` として返す

### レイヤー反映

12. `DesktopApp::apply_bitmap_edits()`
13. `Document::apply_bitmap_edits_to_active_layer()`
14. `apply_bitmap_edits()` in `crates/app-core/src/document/layer_ops.rs`

役割:

- dirty rect ごとに既存レイヤーから bitmap B を切り出す
- plugin から返った bitmap A と合成関数 `f` を使って
  `f(bitmap_A, bitmap_B)` を実行する
- 戻り値 bitmap を active layer の該当領域へ書き戻す
- panel bitmap の dirty 範囲だけ再合成する

### present / render 反映

15. `DesktopApp::refresh_canvas_frame_region()`
16. `DesktopApp::append_canvas_dirty_rect()`
17. `DesktopApp::prepare_present_frame()`

役割:

- CPU 側 `canvas_frame` の dirty 部分だけ更新する
- 次フレームで GPU に渡す dirty rect を集約する
- base / overlay / canvas update 情報をまとめて presenter 側へ渡す

### 描画入力の責務まとめ

- runtime: 生入力受理
- app input: 座標変換と gesture 解釈
- paint plugin: bitmap A と合成関数 `f` を生成
- app-core layer ops: 既存 bitmap B を切り出し、`f(A, B)` を適用
- present: dirty rect を提示系へ反映

---

## 2. パン入力フロー

現在の主経路はホイール入力。

### 入口

1. `DesktopRuntime::handle_mouse_wheel()`

役割:

- panel scroll / canvas pan / zoom を振り分ける
- canvas 上なら pan 量を `pending_wheel_pan` に蓄積する
- `advance_wheel_animation()` を呼ぶ

2. `DesktopRuntime::advance_wheel_animation()`

役割:

- 蓄積済み pan を 1 ステップぶん取り出す
- `Command::PanView` を `DesktopApp::execute_command()` へ渡す

### ドキュメント更新

3. `DesktopApp::execute_command()`
4. `DesktopApp::execute_document_command()`
5. `Document::apply_command(Command::PanView)`

役割:

- `view_transform.pan_x / pan_y` を更新する
- bitmap 自体は変更しない

### dirty と再提示

6. `DesktopApp::mark_canvas_transform_dirty()`

役割:

- 旧 transform と新 transform の差をもとに
  canvas host 側の再描画範囲を求める
- brush preview の旧位置 / 新位置の差分も dirty 化する

7. `DesktopApp::prepare_present_frame()`

役割:

- canvas bitmap はそのまま使う
- `render::prepare_canvas_scene()` で quad / UV / transform を再計算する
- 必要なら background / overlay の dirty 範囲だけ再描画する

### パン入力の責務まとめ

- runtime: wheel を pan コマンドへ落とす
- document: view state だけ更新する
- render/present: transform だけ差し替えて再提示する
- paint plugin は関与しない

---

## 3. パネル移動フロー

### 入口

1. `DesktopApp::handle_pointer_pressed_with_pressure()`
2. `DesktopApp::begin_panel_interaction()`

役割:

- `panel_move_hit_from_window()` でタイトルバー hit を検出する
- 移動対象なら `PanelDragState::Move` を開始する
- grab offset を保存する

### ドラッグ継続

3. `DesktopApp::handle_pointer_dragged_with_pressure()`
4. `DesktopApp::drag_panel_interaction()`

役割:

- `PanelDragState::Move` の場合、現在 window 座標から新しい panel 左上を計算する
- `UiShell::move_panel_to()` を呼ぶ
- panel surface dirty を立てる

### 解放

5. `DesktopApp::handle_pointer_released_with_pressure()`

役割:

- drag state を終了する
- `persist_session_state()` を呼んで panel 位置を保存する

### 再描画

6. `DesktopApp::mark_panel_surface_dirty()`
7. `DesktopApp::prepare_present_frame()`
8. `UiShell::render_panel_surface()`

役割:

- panel surface を再ラスタライズする
- 必要な dirty rect だけ overlay / base へ再合成する
- canvas bitmap 自体は変更しない

### パネル移動の責務まとめ

- app input: panel drag state の開始 / 更新 / 終了
- ui-shell: panel レイアウト変更
- present: panel surface の再ラスタライズと dirty 合成
- document の canvas 内容や paint plugin は関与しない

---

## 4. 3 フローの差分

### 描画入力

- 変更対象: active layer bitmap
- 中心関数: `execute_paint_input()`
- plugin 関与: あり
- dirty の主対象: canvas texture

### パン入力

- 変更対象: `view_transform`
- 中心関数: `execute_document_command(Command::PanView)`
- plugin 関与: なし
- dirty の主対象: canvas host / overlay / quad

### パネル移動

- 変更対象: workspace panel position
- 中心関数: `drag_panel_interaction()` -> `UiShell::move_panel_to()`
- plugin 関与: なし
- dirty の主対象: panel surface / overlay
