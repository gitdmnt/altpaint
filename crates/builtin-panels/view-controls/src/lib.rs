//! `builtin.view-controls` パネル (Phase 10 DOM mutation 版)。
//!
//! `panel_on_host_change` で host state から各 DOM 要素を直接 update する。
//! `panel_handle_*` はサービスを発行する (UI 表示は次フレームの sync_host で更新)。

use panel_sdk::{
    dom::{set_button_active, set_slider, set_text},
    host_state::{ViewState, section},
    runtime::{emit_request, host_section},
    serde::Deserialize,
    services,
};

const MIN_ZOOM_PERCENT: i32 = 25;
const MAX_ZOOM_PERCENT: i32 = 1600;
const PAN_SLIDER_CENTER: i32 = 2000;
const PAN_SLIDER_MIN: i32 = 0;
const PAN_SLIDER_MAX: i32 = 4000;

/// スライダー入力の payload (`altp:slider:*` は `event_payload.value` を運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SliderValue {
    #[serde(default)]
    value: i32,
}

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    // BL-142: view セクションを型付き DTO として 1 回取得する。
    let Some(view) = host_section::<ViewState>(section::VIEW) else {
        return;
    };

    let zoom_milli = view.zoom_milli.max(1) as i32;
    let zoom_percent_f = zoom_milli as f32 / 10.0;
    let zoom_clamped = ((zoom_milli + 5) / 10).clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);

    set_text("#zoom-label", &format!("{zoom_percent_f:.1}%"));
    set_slider("#zoom-slider", zoom_clamped, "");

    let pan_x = view.pan_x as i32;
    let pan_y = view.pan_y as i32;
    set_text("#pan-label", &format!("{pan_x}, {pan_y}"));
    let pan_x_slider = (pan_x + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX);
    let pan_y_slider = (pan_y + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX);
    set_slider("#pan-x-slider", pan_x_slider, "");
    set_slider("#pan-y-slider", pan_y_slider, "");

    let rotation = (view.rotation_degrees as i32).rem_euclid(360);
    set_text("#rotation-label", &format!("{rotation}°"));
    set_slider("#rotation-slider", rotation, "");

    set_button_active("#view\\.flip\\.x", view.flip_x);
    set_button_active("#view\\.flip\\.y", view.flip_y);
}

fn host_pan_y() -> i32 {
    host_section::<ViewState>(section::VIEW)
        .map(|view| view.pan_y as i32)
        .unwrap_or(0)
}

fn host_pan_x() -> i32 {
    host_section::<ViewState>(section::VIEW)
        .map(|view| view.pan_x as i32)
        .unwrap_or(0)
}

#[panel_sdk::panel_handler]
fn set_zoom(payload: SliderValue) {
    let zoom_percent = payload.value.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
    emit_request(&services::view::set_zoom(zoom_percent as f32 / 100.0));
}

#[panel_sdk::panel_handler]
fn set_pan_x(payload: SliderValue) {
    let pan_x = payload.value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    emit_request(&services::view::set_pan(pan_x as f32, host_pan_y() as f32));
}

#[panel_sdk::panel_handler]
fn set_pan_y(payload: SliderValue) {
    let pan_y = payload.value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    emit_request(&services::view::set_pan(host_pan_x() as f32, pan_y as f32));
}

#[panel_sdk::panel_handler]
fn set_rotation(payload: SliderValue) {
    emit_request(&services::view::set_rotation(
        payload.value.rem_euclid(360) as f32,
    ));
}

#[panel_sdk::panel_handler]
fn reset_view() {
    emit_request(&services::view::reset());
}

#[panel_sdk::panel_handler]
fn focus_active_koma() {
    emit_request(&services::koma_nav::focus_active());
}

#[panel_sdk::panel_handler]
fn select_previous_koma() {
    emit_request(&services::koma_nav::select_previous());
}

#[panel_sdk::panel_handler]
fn select_next_koma() {
    emit_request(&services::koma_nav::select_next());
}

#[panel_sdk::panel_handler]
fn flip_horizontal() {
    emit_request(&services::view::flip_horizontal());
}

#[panel_sdk::panel_handler]
fn flip_vertical() {
    emit_request(&services::view::flip_vertical());
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        set_zoom(SliderValue { value: 125 }),
        set_pan_x(SliderValue { value: 2100 }),
        set_pan_y(SliderValue { value: 1950 }),
        set_rotation(SliderValue { value: 270 }),
        reset_view(),
        focus_active_koma(),
        select_previous_koma(),
        select_next_koma(),
        flip_horizontal(),
        flip_vertical(),
    });
}
