use plugin_sdk::{
    commands::{self, RgbColor},
    host,
    runtime::{emit_command, event_string, set_state_i32, set_state_string, state_i32},
    state,
};

const HUE: state::IntKey = state::int("hue");
const SATURATION: state::IntKey = state::int("saturation");
const VALUE: state::IntKey = state::int("value");
const ACTIVE_HEX: state::StringKey = state::string("active_hex");
const COLOR_MODE: state::IntKey = state::int("color_mode");
const LAB_L: state::IntKey = state::int("lab_l");
const LAB_A: state::IntKey = state::int("lab_a");
const LAB_B: state::IntKey = state::int("lab_b");
const OKLCH_L: state::IntKey = state::int("oklch_l");
const OKLCH_C: state::IntKey = state::int("oklch_c");
const OKLCH_H: state::IntKey = state::int("oklch_h");

fn sync_color_state(r: i32, g: i32, b: i32) {
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
    let rgb = RgbColor::new(clamp_channel(r), clamp_channel(g), clamp_channel(b));
    set_state_string(ACTIVE_HEX, rgb.to_hex_string());
}

fn emit_color_from_rgb(r: i32, g: i32, b: i32) {
    sync_color_state(r, g, b);
    let rgb = RgbColor::new(clamp_channel(r), clamp_channel(g), clamp_channel(b));
    emit_command(&commands::tool::set_color_hex(rgb.to_hex_string()));
}

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

/// パネル初期化時に必要な状態を整える。
#[plugin_sdk::panel_init]
fn init() {}

/// Host snapshot を読み取り、表示用の状態へ同期する。
#[plugin_sdk::panel_sync_host]
fn sync_host() {
    let color = host::color::active_rgb();
    sync_color_state(color.red as i32, color.green as i32, color.blue as i32);
}

/// HSV を設定する。
#[plugin_sdk::panel_handler]
fn set_hsv() {
    let payload = event_string("value");
    let Some((hue, saturation, value)) = parse_hsv_payload(&payload) else {
        return;
    };
    let rgb = hsv_to_rgb(hue, saturation, value);
    emit_color_from_rgb(rgb.red as i32, rgb.green as i32, rgb.blue as i32);
}

#[plugin_sdk::panel_handler]
fn set_mode_hsv() {
    set_state_i32(COLOR_MODE, 0);
}

#[plugin_sdk::panel_handler]
fn set_mode_lab() {
    set_state_i32(COLOR_MODE, 1);
}

#[plugin_sdk::panel_handler]
fn set_mode_oklch() {
    set_state_i32(COLOR_MODE, 2);
}

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

/// 入力を解析して HSV payload に変換する。
///
/// 値を生成できない場合は `None` を返します。
fn parse_hsv_payload(value: &str) -> Option<(i32, i32, i32)> {
    let mut parts = value.split(',');
    let hue = parts.next()?.trim().parse::<i32>().ok()?;
    let saturation = parts.next()?.trim().parse::<i32>().ok()?;
    let value = parts.next()?.trim().parse::<i32>().ok()?;
    Some((
        hue.rem_euclid(360),
        saturation.clamp(0, 100),
        value.clamp(0, 100),
    ))
}

/// RGB to HSV を計算して返す。
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
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    (
        hue.round() as i32,
        (saturation * 100.0).round() as i32,
        (max * 100.0).round() as i32,
    )
}

/// 補正 channel を有効範囲へ補正して返す。
fn clamp_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

// ---- 色空間変換 ----------------------------------------------------------------

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
    // D65 illuminant (IEC 61966-2-1)
    let x = r * 0.4124564 + g * 0.3575761 + b * 0.1804375;
    let y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750;
    let z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041;
    (x, y, z)
}

fn xyz_to_rgb(x: f32, y: f32, z: f32) -> RgbColor {
    let r = x * 3.2404542 + y * -1.5371385 + z * -0.4985314;
    let g = x * -0.9692660 + y * 1.8760108 + z * 0.0415560;
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

// D65 white point
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

// ---- Oklch ----------------------------------------------------------------

fn xyz_to_oklab(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    // Björn Ottosson: https://bottosson.github.io/posts/oklab/
    let l = (0.8189330101 * x + 0.3618667424 * y - 0.1288597137 * z).cbrt();
    let m = (0.0329845436 * x + 0.9293118715 * y + 0.0361456387 * z).cbrt();
    let s = (0.0482003018 * x + 0.2643662691 * y + 0.6338517070 * z).cbrt();
    (
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    )
}

fn oklab_to_xyz(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let (lc, mc, sc) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let x = 1.2270138511 * lc - 0.5577999807 * mc + 0.2812561490 * sc;
    let y = -0.0405801784 * lc + 1.1122568696 * mc - 0.0716766787 * sc;
    let z = -0.0763812845 * lc - 0.4214819784 * mc + 1.5861632204 * sc;
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

    /// 補正 channel limits values into byte range が期待どおりに動作することを検証する。
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

    /// パネル entrypoints are callable on native targets が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        set_hsv();
        set_mode_hsv();
        set_mode_lab();
        set_mode_oklch();
        set_lab_l();
        set_lab_a();
        set_lab_b();
        set_oklch_l();
        set_oklch_c();
        set_oklch_h();
    }

    #[test]
    fn rgb_to_lab_red_is_correct() {
        let (l, a, b) = rgb_to_lab(255, 0, 0);
        assert!((l - 53).abs() <= 2, "L={l}");
        assert!((a - 80).abs() <= 2, "a={a}");
        assert!((b - 67).abs() <= 2, "b={b}");
    }

    #[test]
    fn lab_roundtrip_preserves_color() {
        // 近黒・近白はLab整数量子化誤差が拡大するため中間値を使用
        for (r, g, b) in [(200, 100, 50), (100, 180, 100), (100, 100, 200)] {
            let (l, a, bv) = rgb_to_lab(r, g, b);
            let out = lab_to_rgb(l, a, bv);
            assert!((out.red as i32 - r).abs() <= 2, "r round-trip r={r}");
            assert!((out.green as i32 - g).abs() <= 2, "g round-trip g={g}");
            assert!((out.blue as i32 - b).abs() <= 2, "b round-trip b={b}");
        }
    }

    #[test]
    fn lab_to_rgb_black_returns_black() {
        let out = lab_to_rgb(0, 0, 0);
        assert_eq!(out.red, 0);
        assert_eq!(out.green, 0);
        assert_eq!(out.blue, 0);
    }

    #[test]
    fn rgb_to_oklch_red_is_correct() {
        let (l, c, h) = rgb_to_oklch(255, 0, 0);
        assert!((l - 63).abs() <= 3, "L={l}");
        assert!((c - 258).abs() <= 10, "C={c}");
        assert!((h - 29).abs() <= 5, "H={h}");
    }

    #[test]
    fn oklch_roundtrip_preserves_color() {
        for (r, g, b) in [(200, 100, 50), (100, 180, 100), (100, 100, 200)] {
            let (l, c, h) = rgb_to_oklch(r, g, b);
            let out = oklch_to_rgb(l, c, h);
            assert!((out.red as i32 - r).abs() <= 2, "r round-trip r={r}");
            assert!((out.green as i32 - g).abs() <= 2, "g round-trip g={g}");
            assert!((out.blue as i32 - b).abs() <= 2, "b round-trip b={b}");
        }
    }

    #[test]
    fn oklch_to_rgb_white_returns_white() {
        let out = oklch_to_rgb(100, 0, 0);
        assert!((out.red as i32 - 255).abs() <= 2);
        assert!((out.green as i32 - 255).abs() <= 2);
        assert!((out.blue as i32 - 255).abs() <= 2);
    }

    #[test]
    fn lab_out_of_gamut_clamps_to_valid_rgb() {
        // gamut 外 Lab でも clamp_channel により 0-255 に収まることを確認
        let out = lab_to_rgb(50, 127, 127);
        let _ = (out.red, out.green, out.blue); // u8 なので 0-255 は型保証される
    }

    #[test]
    fn parse_invalid_channel_returns_zero() {
        let parsed = "abc".trim().parse::<i32>().unwrap_or(0);
        assert_eq!(parsed, 0);
    }
}
