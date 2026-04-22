# ADR 004: カラーパレット マルチ色空間対応

- **日時**: 2026-04-22
- **作業 Agent**: claude-sonnet-4-6

---

## 概要

`plugins/color-palette` に Lab と Oklch の入力モードを追加する。
現在の実装は HSV 固定（`color-wheel` ウィジェット）。完了後はモードボタンで HSV / Lab / Oklch を切り替えられる。

## 決定事項

### スライダーの負値対応: 選択肢A（i32 拡張）

現状 `PanelHitKind::Slider { min: usize, max: usize }` は非負のみだが、Lab の a/b チャンネルが -128〜+127 を要するため、`i32` に変更する。

- **変更対象**: `crates/panel-api`（`PanelHitKind`）、`crates/ui-shell/src/presentation.rs`
- **DSL パーサー**: 既に `i64` でパースしているため変更不要（キャスト調整のみ）
- **理由**: オフセット変数を増やすより DSL と state の設計を自然に保つほうが長期コスト低

---

## 完了条件

1. HSV / Lab / Oklch のモードボタンで切替できる
2. 各モードのスライダーで色変更 → キャンバスの active color に反映
3. `sync_host` で RGB から全モードの表示値が正しく更新される
4. 各スライダーが仕様の最小値（Lab a/b は -128）を受け付けること
5. モード切替時に active color がリセットされない
6. `cargo test -p color-palette` が通る（ラウンドトリップ精度 ±2/255 以内）
7. 不正入力（"abc" 等）は 0 に丸まる仕様
8. `cargo clippy --workspace --all-targets` が通る

---

## 設計メモ

| 色空間 | チャンネル | 状態格納 |
|--------|-----------|---------|
| HSV (既存) | H / S / V | そのまま整数 (0-360 / 0-100 / 0-100) |
| Lab | L / a / b | そのまま整数 (0-100 / -128..127 / -128..127) |
| Oklch | L / C / H | L×100 / C×1000 / そのまま整数 (0-100 / 0-400 / 0-360) |

- Oklch C max=400: sRGB gamut 内の最大 Chroma は約 0.37（×1000=370）、余裕を持たせて 400
- Oklch L 初期値=100: L×100=1.0（白）。HSV V=100 と整合。`sync_host` 起動後に上書きされる
- out-of-gamut: `clamp_channel` で 0-255 に収める（精度損失を許容）
- 外部クレート不使用（標準 Rust のみ）
- 参照実装: Lab → [CSS Color Module Level 4 §10.9](https://www.w3.org/TR/css-color-4/#css-lab)、Oklch → [Björn Ottosson の定義](https://bottosson.github.io/posts/oklab/)
- `sync_color_state` はプライベート関数。シグネチャ変更の影響は `plugins/color-palette/src/lib.rs` 内のみ
- 既存 HSV state キー（HUE, SATURATION, VALUE）は保持

---

## 実装ステップ

### Step 0: スライダー `i32` 対応

`crates/panel-api` と `crates/ui-shell/src/presentation.rs` の `PanelHitKind::Slider { min, max }` と `slider_value_for_position` を `usize` → `i32` に変更。

```rust
// Before
PanelHitKind::Slider { min: usize, max: usize }
fn slider_value_for_position(region, min: usize, max: usize, point) -> usize

// After
PanelHitKind::Slider { min: i32, max: i32 }
fn slider_value_for_position(region, min: i32, max: i32, point) -> i32
```

確認: `cargo test -p ui-shell`

### Step 1: 変換関数を TDD で実装

`plugins/color-palette/src/lib.rs` に追加する関数:

```rust
fn srgb_to_linear(c: f32) -> f32
fn linear_to_srgb(c: f32) -> f32
fn rgb_to_xyz(r: i32, g: i32, b: i32) -> (f32, f32, f32)   // → XYZ D65
fn xyz_to_lab(x: f32, y: f32, z: f32) -> (i32, i32, i32)   // → (L:0-100, a:-128..127, b:-128..127)
fn lab_to_xyz(l: i32, a: i32, b: i32) -> (f32, f32, f32)
fn xyz_to_rgb(x: f32, y: f32, z: f32) -> RgbColor
fn rgb_to_lab(r: i32, g: i32, b: i32) -> (i32, i32, i32)
fn lab_to_rgb(l: i32, a: i32, b: i32) -> RgbColor
fn xyz_to_oklab(x: f32, y: f32, z: f32) -> (f32, f32, f32)
fn oklab_to_oklch(l: f32, a: f32, b: f32) -> (i32, i32, i32) // → (L×100, C×1000, H:0-360)
fn oklch_to_oklab(l: i32, c: i32, h: i32) -> (f32, f32, f32)
fn oklab_to_xyz(l: f32, a: f32, b: f32) -> (f32, f32, f32)
fn rgb_to_oklch(r: i32, g: i32, b: i32) -> (i32, i32, i32)
fn oklch_to_rgb(l: i32, c: i32, h: i32) -> RgbColor
```

テストケース:
- `rgb_to_lab_red_is_correct` — (255,0,0) → Lab 既知値（L≈53, a≈80, b≈67）
- `lab_roundtrip_preserves_color` — ラウンドトリップ ±2/255
- `lab_to_rgb_black_returns_black`
- `rgb_to_oklch_red_is_correct`
- `oklch_roundtrip_preserves_color`
- `oklch_to_rgb_white_returns_white`
- `lab_out_of_gamut_clamps_to_valid_rgb` — gamut 外 Lab → RGB が 0-255 に収まること
- `parse_invalid_channel_returns_zero` — "abc".parse → 0

### Step 2: State 拡張

`panel.altp-panel` の `state` セクションに追加:

```
color_mode: int = 0
lab_l: int = 0
lab_a: int = 0
lab_b: int = 0
oklch_l: int = 100
oklch_c: int = 0
oklch_h: int = 0
```

`lib.rs` に state キー定数追加:

```rust
const COLOR_MODE: state::IntKey = state::int("color_mode");
const LAB_L: state::IntKey = state::int("lab_l");
const LAB_A: state::IntKey = state::int("lab_a");
const LAB_B: state::IntKey = state::int("lab_b");
const OKLCH_L: state::IntKey = state::int("oklch_l");
const OKLCH_C: state::IntKey = state::int("oklch_c");
const OKLCH_H: state::IntKey = state::int("oklch_h");
```

### Step 3: `sync_color_state` と `emit_color_from_rgb` を変更

```rust
// シグネチャ変更（影響: 同ファイル内のみ）
fn sync_color_state(r: i32, g: i32, b: i32) {
    let (hue, sat, val) = rgb_to_hsv(r, g, b);
    let (ll, la, lb)    = rgb_to_lab(r, g, b);
    let (ol, oc, oh)    = rgb_to_oklch(r, g, b);
    set_state_i32(HUE, hue); set_state_i32(SATURATION, sat); set_state_i32(VALUE, val);
    set_state_i32(LAB_L, ll); set_state_i32(LAB_A, la); set_state_i32(LAB_B, lb);
    set_state_i32(OKLCH_L, ol); set_state_i32(OKLCH_C, oc); set_state_i32(OKLCH_H, oh);
    let rgb = RgbColor::new(clamp_channel(r), clamp_channel(g), clamp_channel(b));
    set_state_string(ACTIVE_HEX, rgb.to_hex_string());
}

fn emit_color_from_rgb(r: i32, g: i32, b: i32) {
    sync_color_state(r, g, b);
    let rgb = RgbColor::new(clamp_channel(r), clamp_channel(g), clamp_channel(b));
    emit_command(&commands::tool::set_color_hex(rgb.to_hex_string()));
}

// sync_host: active_rgb → sync_color_state
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    let color = host::color::active_rgb();
    sync_color_state(color.red as i32, color.green as i32, color.blue as i32);
}

// emit_color: hsv_to_rgb → emit_color_from_rgb（set_hsv ハンドラ本体は変更不要）
fn emit_color(hue: i32, saturation: i32, value: i32) {
    let rgb = hsv_to_rgb(hue, saturation, value);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
```

### Step 4: イベントハンドラ追加

```rust
#[plugin_sdk::panel_handler] fn set_mode_hsv()   { set_state_i32(COLOR_MODE, 0); }
#[plugin_sdk::panel_handler] fn set_mode_lab()   { set_state_i32(COLOR_MODE, 1); }
#[plugin_sdk::panel_handler] fn set_mode_oklch() { set_state_i32(COLOR_MODE, 2); }

#[plugin_sdk::panel_handler]
fn set_lab_l() {
    let l = event_string("value").trim().parse::<i32>().unwrap_or(0).clamp(0, 100);
    let rgb = lab_to_rgb(l, state_i32(LAB_A), state_i32(LAB_B));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
#[plugin_sdk::panel_handler]
fn set_lab_a() {
    let a = event_string("value").trim().parse::<i32>().unwrap_or(0).clamp(-128, 127);
    let rgb = lab_to_rgb(state_i32(LAB_L), a, state_i32(LAB_B));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
#[plugin_sdk::panel_handler]
fn set_lab_b() {
    let b = event_string("value").trim().parse::<i32>().unwrap_or(0).clamp(-128, 127);
    let rgb = lab_to_rgb(state_i32(LAB_L), state_i32(LAB_A), b);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[plugin_sdk::panel_handler]
fn set_oklch_l() {
    let l = event_string("value").trim().parse::<i32>().unwrap_or(0).clamp(0, 100);
    let rgb = oklch_to_rgb(l, state_i32(OKLCH_C), state_i32(OKLCH_H));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
#[plugin_sdk::panel_handler]
fn set_oklch_c() {
    let c = event_string("value").trim().parse::<i32>().unwrap_or(0).clamp(0, 400);
    let rgb = oklch_to_rgb(state_i32(OKLCH_L), c, state_i32(OKLCH_H));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
#[plugin_sdk::panel_handler]
fn set_oklch_h() {
    let h = event_string("value").trim().parse::<i32>().unwrap_or(0).rem_euclid(360);
    let rgb = oklch_to_rgb(state_i32(OKLCH_L), state_i32(OKLCH_C), h);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}
```

### Step 5: View 更新

```
view {
  <column gap=8 padding=8>
    <section title="カラー">
      <color-preview id="color.preview" label="プレビュー" color={state.active_hex}></color-preview>
      <text>{state.active_hex}</text>
      <row>
        <button id="mode.hsv"   on:click="set_mode_hsv"   active={state.color_mode == 0}>HSV</button>
        <button id="mode.lab"   on:click="set_mode_lab"   active={state.color_mode == 1}>Lab</button>
        <button id="mode.oklch" on:click="set_mode_oklch" active={state.color_mode == 2}>Oklch</button>
      </row>
      <when test={state.color_mode == 0}>
        <color-wheel id="color.wheel" label="色相 / 彩度 / 明度"
          hue={state.hue} saturation={state.saturation} value={state.value}
          on:change="set_hsv"></color-wheel>
        <text>H {state.hue}° / S {state.saturation}% / V {state.value}%</text>
      </when>
      <when test={state.color_mode == 1}>
        <slider id="lab.l" label="L" min={0}    max={100} value={state.lab_l} on:change="set_lab_l"></slider>
        <slider id="lab.a" label="a" min={-128} max={127} value={state.lab_a} on:change="set_lab_a"></slider>
        <slider id="lab.b" label="b" min={-128} max={127} value={state.lab_b} on:change="set_lab_b"></slider>
        <text>L {state.lab_l} / a {state.lab_a} / b {state.lab_b}</text>
      </when>
      <when test={state.color_mode == 2}>
        <slider id="oklch.l" label="L" min={0} max={100} value={state.oklch_l} on:change="set_oklch_l"></slider>
        <slider id="oklch.c" label="C" min={0} max={400} value={state.oklch_c} on:change="set_oklch_c"></slider>
        <slider id="oklch.h" label="H" min={0} max={360} value={state.oklch_h} on:change="set_oklch_h"></slider>
        <text>L {state.oklch_l}% / C {state.oklch_c} / H {state.oklch_h}°</text>
      </when>
    </section>
  </column>
}
```

### Step 6: テストと確認

1. `cargo test -p ui-shell` — スライダー i32 対応テスト
2. `cargo test -p color-palette` — 変換関数テスト + 既存テスト
3. `.\scripts\build-ui-wasm.ps1` — Wasm 再ビルド
4. `cargo run` — UI 動作確認:
   - HSV / Lab / Oklch モード切替で active_hex がリセットされない
   - Lab a/b を -128 まで動かせること
   - RGB(255,0,0) → Lab → RGB が元の赤に近い値で戻ること
5. `cargo clippy --workspace --all-targets`
