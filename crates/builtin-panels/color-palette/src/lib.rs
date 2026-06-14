//! `builtin.color-palette` パネル (Phase 10 DOM mutation 版)。
//!
//! HSV / Lab / Oklch の 3 経路で色を編集できる。スライダー入力で色空間変換 → RGB を計算し、
//! `commands::tool::set_color_hex` を発行する。

use panel_sdk::{
    commands::{self, RgbColor},
    dom::{query_selector, set_attribute, set_button_active, set_slider, set_text, set_visible},
    host_state::{ColorState, section},
    runtime::{emit_request, host_section, set_state_i32, state_i32},
    serde::Deserialize,
    state,
};

/// スライダー入力 payload (`altp:slider:*` は `event_payload.value` を整数で運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SliderValue {
    #[serde(default)]
    value: i32,
}

const HUE: state::IntKey = state::int("hue");
const SATURATION: state::IntKey = state::int("saturation");
const VALUE: state::IntKey = state::int("value");
const COLOR_MODE: state::IntKey = state::int("color_mode");
const LAB_L: state::IntKey = state::int("lab_l");
const LAB_A: state::IntKey = state::int("lab_a");
const LAB_B: state::IntKey = state::int("lab_b");
const OKLCH_L: state::IntKey = state::int("oklch_l");
const OKLCH_C: state::IntKey = state::int("oklch_c");
const OKLCH_H: state::IntKey = state::int("oklch_h");

fn render_dom() {
    // BL-142: color セクションを型付き DTO として 1 回取得する。
    let color = host_section::<ColorState>(section::COLOR).unwrap_or(ColorState {
        active: String::new(),
        red: 0,
        green: 0,
        blue: 0,
    });
    let r = color.red as i32;
    let g = color.green as i32;
    let b = color.blue as i32;
    let hex = color.active;
    let mode = state_i32(COLOR_MODE);

    let (hue, sat, val) = rgb_to_hsv(r, g, b);
    let (ll, la, lb) = rgb_to_lab(r, g, b);
    let (ol, oc, oh) = rgb_to_oklch(r, g, b);

    set_state_i32(HUE, hue);
    set_state_i32(SATURATION, sat);
    set_state_i32(VALUE, val);
    set_state_i32(LAB_L, ll);
    set_state_i32(LAB_A, la);
    set_state_i32(LAB_B, lb);
    set_state_i32(OKLCH_L, ol);
    set_state_i32(OKLCH_C, oc);
    set_state_i32(OKLCH_H, oh);

    set_text("#active-hex", &hex);
    if let Some(node) = query_selector("#color-preview") {
        set_attribute(node, "style", &format!("background:{}", hex));
    }
    set_slider("#hue", hue, "#hue-display");
    set_slider("#saturation", sat, "#saturation-display");
    set_slider("#value", val, "#value-display");
    set_slider("#lab\\.l", ll, "#lab-l-display");
    set_slider("#lab\\.a", la, "#lab-a-display");
    set_slider("#lab\\.b", lb, "#lab-b-display");
    set_slider("#oklch\\.l", ol, "#oklch-l-display");
    set_slider("#oklch\\.c", oc, "#oklch-c-display");
    set_slider("#oklch\\.h", oh, "#oklch-h-display");

    set_visible("#hsv-section", mode == 0);
    set_visible("#lab-section", mode == 1);
    set_visible("#oklch-section", mode == 2);

    set_button_active("#mode\\.hsv", mode == 0);
    set_button_active("#mode\\.lab", mode == 1);
    set_button_active("#mode\\.oklch", mode == 2);
}

fn emit_color_from_rgb(r: i32, g: i32, b: i32) {
    let rgb = RgbColor::new(clamp_channel(r), clamp_channel(g), clamp_channel(b));
    emit_request(&commands::tool::set_color_hex(rgb.to_hex_string()));
    render_dom();
}

#[panel_sdk::panel_init]
fn init() {
    render_dom();
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_mode_hsv() {
    set_state_i32(COLOR_MODE, 0);
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_mode_lab() {
    set_state_i32(COLOR_MODE, 1);
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_mode_oklch() {
    set_state_i32(COLOR_MODE, 2);
    render_dom();
}

#[panel_sdk::panel_handler]
fn set_hue(payload: SliderValue) {
    let h = payload.value.rem_euclid(360);
    let rgb = hsv_to_rgb(h, state_i32(SATURATION), state_i32(VALUE));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_saturation(payload: SliderValue) {
    let s = payload.value.clamp(0, 100);
    let rgb = hsv_to_rgb(state_i32(HUE), s, state_i32(VALUE));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_value(payload: SliderValue) {
    let v = payload.value.clamp(0, 100);
    let rgb = hsv_to_rgb(state_i32(HUE), state_i32(SATURATION), v);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_lab_l(payload: SliderValue) {
    let l = payload.value.clamp(0, 100);
    let rgb = lab_to_rgb(l, state_i32(LAB_A), state_i32(LAB_B));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_lab_a(payload: SliderValue) {
    let a = payload.value.clamp(-128, 127);
    let rgb = lab_to_rgb(state_i32(LAB_L), a, state_i32(LAB_B));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_lab_b(payload: SliderValue) {
    let b = payload.value.clamp(-128, 127);
    let rgb = lab_to_rgb(state_i32(LAB_L), state_i32(LAB_A), b);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_oklch_l(payload: SliderValue) {
    let l = payload.value.clamp(0, 100);
    let rgb = oklch_to_rgb(l, state_i32(OKLCH_C), state_i32(OKLCH_H));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_oklch_c(payload: SliderValue) {
    let c = payload.value.clamp(0, 400);
    let rgb = oklch_to_rgb(state_i32(OKLCH_L), c, state_i32(OKLCH_H));
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[panel_sdk::panel_handler]
fn set_oklch_h(payload: SliderValue) {
    let h = payload.value.rem_euclid(360);
    let rgb = oklch_to_rgb(state_i32(OKLCH_L), state_i32(OKLCH_C), h);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

// === 色空間変換 ===

fn hsv_to_rgb(hue: i32, saturation: i32, value: i32) -> RgbColor {
    let h = hue.rem_euclid(360) as f32;
    let s = (saturation.clamp(0, 100) as f32) / 100.0;
    let v = (value.clamp(0, 100) as f32) / 100.0;
    if s <= f32::EPSILON {
        let gray = (v * 255.0).round() as u8;
        return RgbColor::new(gray, gray, gray);
    }
    let sector = (h / 60.0).floor();
    let fraction = h / 60.0 - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * fraction);
    let t = v * (1.0 - s * (1.0 - fraction));
    let (r, g, b) = match sector as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    RgbColor::new(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn rgb_to_hsv(red: i32, green: i32, blue: i32) -> (i32, i32, i32) {
    let r = clamp_channel(red) as f32 / 255.0;
    let g = clamp_channel(green) as f32 / 255.0;
    let b = clamp_channel(blue) as f32 / 255.0;
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if (max - r).abs() <= f32::EPSILON {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max - g).abs() <= f32::EPSILON {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let saturation = if max <= f32::EPSILON { 0.0 } else { delta / max };
    (
        hue.round() as i32,
        (saturation * 100.0).round() as i32,
        (max * 100.0).round() as i32,
    )
}

fn clamp_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn rgb_to_xyz(r: i32, g: i32, b: i32) -> (f32, f32, f32) {
    let r = srgb_to_linear(clamp_channel(r) as f32 / 255.0);
    let g = srgb_to_linear(clamp_channel(g) as f32 / 255.0);
    let b = srgb_to_linear(clamp_channel(b) as f32 / 255.0);
    let x = r * 0.4124564 + g * 0.3575761 + b * 0.1804375;
    let y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
    let z = r * 0.0193339 + g * 0.119_192 + b * 0.9503041;
    (x, y, z)
}

fn xyz_to_rgb(x: f32, y: f32, z: f32) -> RgbColor {
    let r = x * 3.2404542 + y * -1.5371385 + z * -0.4985314;
    let g = x * -0.969_266 + y * 1.8760108 + z * 0.0415560;
    let b = x * 0.0556434 + y * -0.2040259 + z * 1.0572252;
    RgbColor::new(
        clamp_channel((linear_to_srgb(r) * 255.0).round() as i32),
        clamp_channel((linear_to_srgb(g) * 255.0).round() as i32),
        clamp_channel((linear_to_srgb(b) * 255.0).round() as i32),
    )
}

fn lab_f(t: f32) -> f32 {
    let delta: f32 = 6.0 / 29.0;
    if t > delta.powi(3) { t.cbrt() } else { t / (3.0 * delta * delta) + 4.0 / 29.0 }
}

fn lab_f_inv(t: f32) -> f32 {
    let delta: f32 = 6.0 / 29.0;
    if t > delta { t.powi(3) } else { 3.0 * delta * delta * (t - 4.0 / 29.0) }
}

const XN: f32 = 0.95047;
const YN: f32 = 1.00000;
const ZN: f32 = 1.08883;

fn xyz_to_lab(x: f32, y: f32, z: f32) -> (i32, i32, i32) {
    let fx = lab_f(x / XN);
    let fy = lab_f(y / YN);
    let fz = lab_f(z / ZN);
    let l = (116.0 * fy - 16.0).round() as i32;
    let a = (500.0 * (fx - fy)).round() as i32;
    let b = (200.0 * (fy - fz)).round() as i32;
    (l, a, b)
}

fn lab_to_xyz(l: i32, a: i32, b: i32) -> (f32, f32, f32) {
    let fy = (l as f32 + 16.0) / 116.0;
    let fx = a as f32 / 500.0 + fy;
    let fz = fy - b as f32 / 200.0;
    (XN * lab_f_inv(fx), YN * lab_f_inv(fy), ZN * lab_f_inv(fz))
}

fn rgb_to_lab(r: i32, g: i32, b: i32) -> (i32, i32, i32) {
    let (x, y, z) = rgb_to_xyz(r, g, b);
    xyz_to_lab(x, y, z)
}

fn lab_to_rgb(l: i32, a: i32, b: i32) -> RgbColor {
    let (x, y, z) = lab_to_xyz(l, a, b);
    xyz_to_rgb(x, y, z)
}

fn xyz_to_oklab(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    let l = (0.818_933 * x + 0.361_866_74 * y - 0.128_859_71 * z).cbrt();
    let m = (0.032_984_544 * x + 0.929_311_9 * y + 0.036_145_64 * z).cbrt();
    let s = (0.048_200_3 * x + 0.264_366_27 * y + 0.633_851_7 * z).cbrt();
    (
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    )
}

fn oklab_to_xyz(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (lc, mc, sc) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let x = 1.227_013_8 * lc - 0.557_8 * mc + 0.281_256_14 * sc;
    let y = -0.040_580_18 * lc + 1.112_256_9 * mc - 0.071_676_68 * sc;
    let z = -0.076_381_28 * lc - 0.421_481_97 * mc + 1.586_163_2 * sc;
    (x, y, z)
}

fn oklab_to_oklch(l: f32, a: f32, b: f32) -> (i32, i32, i32) {
    let c = (a * a + b * b).sqrt();
    let h = b.atan2(a).to_degrees().rem_euclid(360.0);
    ((l * 100.0).round() as i32, (c * 1000.0).round() as i32, h.round() as i32)
}

fn oklch_to_oklab(l: i32, c: i32, h: i32) -> (f32, f32, f32) {
    let lf = l as f32 / 100.0;
    let cf = c as f32 / 1000.0;
    let hf = (h as f32).to_radians();
    (lf, cf * hf.cos(), cf * hf.sin())
}

fn rgb_to_oklch(r: i32, g: i32, b: i32) -> (i32, i32, i32) {
    let (x, y, z) = rgb_to_xyz(r, g, b);
    let (l, a, bv) = xyz_to_oklab(x, y, z);
    oklab_to_oklch(l, a, bv)
}

fn oklch_to_rgb(l: i32, c: i32, h: i32) -> RgbColor {
    let (lf, a, b) = oklch_to_oklab(l, c, h);
    let (x, y, z) = oklab_to_xyz(lf, a, b);
    xyz_to_rgb(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_channel_limits_values_into_byte_range() {
        assert_eq!(clamp_channel(-10), 0);
        assert_eq!(clamp_channel(127), 127);
        assert_eq!(clamp_channel(300), 255);
    }

    #[test]
    fn hsv_to_rgb_red_is_correct() {
        let rgb = hsv_to_rgb(0, 100, 100);
        assert_eq!(rgb.red, 255);
        assert_eq!(rgb.green, 0);
        assert_eq!(rgb.blue, 0);
    }

    #[test]
    fn lab_roundtrip() {
        for (r, g, b) in [(200, 100, 50), (100, 180, 100), (100, 100, 200)] {
            let (l, a, bv) = rgb_to_lab(r, g, b);
            let out = lab_to_rgb(l, a, bv);
            assert!((out.red as i32 - r).abs() <= 2);
            assert!((out.green as i32 - g).abs() <= 2);
            assert!((out.blue as i32 - b).abs() <= 2);
        }
    }

    #[test]
    fn oklch_roundtrip() {
        for (r, g, b) in [(200, 100, 50), (100, 180, 100), (100, 100, 200)] {
            let (l, c, h) = rgb_to_oklch(r, g, b);
            let out = oklch_to_rgb(l, c, h);
            assert!((out.red as i32 - r).abs() <= 2);
            assert!((out.green as i32 - g).abs() <= 2);
            assert!((out.blue as i32 - b).abs() <= 2);
        }
    }

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        set_mode_hsv(),
        set_mode_lab(),
        set_mode_oklch(),
        set_hue(SliderValue { value: 120 }),
        set_saturation(SliderValue { value: 50 }),
        set_value(SliderValue { value: 80 }),
        set_lab_l(SliderValue { value: 50 }),
        set_lab_a(SliderValue { value: 20 }),
        set_lab_b(SliderValue { value: -10 }),
        set_oklch_l(SliderValue { value: 70 }),
        set_oklch_c(SliderValue { value: 100 }),
        set_oklch_h(SliderValue { value: 180 }),
    });
}
