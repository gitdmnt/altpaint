use plugin_sdk::{
    CommandDescriptor,
    commands::{self, RgbColor},
    host,
    runtime::{emit_command, event_string, set_state_i32, set_state_string},
    state,
};

const HUE: state::IntKey = state::int("hue");
const SATURATION: state::IntKey = state::int("saturation");
const VALUE: state::IntKey = state::int("value");
const ACTIVE_HEX: state::StringKey = state::string("active_hex");

/// 形式 色 を計算して返す。
fn format_color(hue: i32, saturation: i32, value: i32) -> String {
    hsv_to_rgb(hue, saturation, value).to_hex_string()
}

/// 色 コマンド を構築する。
fn build_color_command(hue: i32, saturation: i32, value: i32) -> CommandDescriptor {
    commands::tool::set_color_hex(format_color(hue, saturation, value))
}

/// 色 状態 を現在の状態へ同期する。
fn sync_color_state(hue: i32, saturation: i32, value: i32) {
    set_state_i32(HUE, hue);
    set_state_i32(SATURATION, saturation);
    set_state_i32(VALUE, value);
    set_state_string(ACTIVE_HEX, format_color(hue, saturation, value));
}

/// 入力や種別に応じて処理を振り分ける。
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
    let (hue, saturation, value) = rgb_to_hsv(color.red, color.green, color.blue);
    sync_color_state(hue, saturation, value);
}

/// HSV を設定する。
#[plugin_sdk::panel_handler]
fn set_hsv() {
    let payload = event_string("value");
    let Some((hue, saturation, value)) = parse_hsv_payload(&payload) else {
        return;
    };
    emit_color(hue, saturation, value);
}

/// 構築 色 コマンド に対応するコマンドを発行する。
///
/// 内部でコマンドを発行します。
fn emit_color(hue: i32, saturation: i32, value: i32) {
    sync_color_state(hue, saturation, value);
    emit_command(&build_color_command(hue, saturation, value));
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

    /// 形式 色 uses HSV model and uppercase 16進文字列 が期待どおりに動作することを検証する。
    #[test]
    fn format_color_uses_hsv_model_and_uppercase_hex() {
        assert_eq!(format_color(0, 100, 100), "#FF0000");
    }

    /// 構築 色 コマンド writes 色 payload が期待どおりに動作することを検証する。
    #[test]
    fn build_color_command_writes_color_payload() {
        let command = build_color_command(210, 67, 22);

        assert_eq!(command.name, "tool.set_color");
        assert_eq!(
            command
                .payload
                .get("color")
                .and_then(|value| value.as_str()),
            Some("#132538")
        );
    }

    /// パネル entrypoints are callable on native targets が期待どおりに動作することを検証する。
    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
        init();
        sync_host();
        set_hsv();
    }
}
