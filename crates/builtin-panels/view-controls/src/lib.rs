//! `builtin.view-controls` パネル (Phase 10 DOM mutation 版)。
//!
//! `panel_sync_host` で host snapshot から各 DOM 要素を直接 update する。
//! `panel_handle_*` はサービスを発行する (UI 表示は次フレームの sync_host で更新)。

use plugin_sdk::{
    dom::{query_selector, set_attribute, set_inner_html},
    host,
    runtime::emit_service,
    services,
};

const MIN_ZOOM_PERCENT: i32 = 25;
const MAX_ZOOM_PERCENT: i32 = 1600;
const PAN_SLIDER_CENTER: i32 = 2000;
const PAN_SLIDER_MIN: i32 = 0;
const PAN_SLIDER_MAX: i32 = 4000;

#[plugin_sdk::panel_init]
fn init() {}

#[plugin_sdk::panel_sync_host]
fn sync_host() {
    let zoom_milli = host::view::zoom_milli().max(1);
    let zoom_percent_f = zoom_milli as f32 / 10.0;
    let zoom_clamped =
        ((zoom_milli + 5) / 10).clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);

    if let Some(label) = query_selector("#zoom-label") {
        set_inner_html(label, &format!("{zoom_percent_f:.1}%"));
    }
    if let Some(slider) = query_selector("#zoom-slider") {
        set_attribute(slider, "value", &zoom_clamped.to_string());
    }

    let pan_x = host::view::pan_x();
    let pan_y = host::view::pan_y();
    if let Some(label) = query_selector("#pan-label") {
        set_inner_html(label, &format!("{pan_x}, {pan_y}"));
    }
    if let Some(slider) = query_selector("#pan-x-slider") {
        let v = (pan_x + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX);
        set_attribute(slider, "value", &v.to_string());
    }
    if let Some(slider) = query_selector("#pan-y-slider") {
        let v = (pan_y + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX);
        set_attribute(slider, "value", &v.to_string());
    }

    let rotation = host::view::rotation_degrees().rem_euclid(360);
    if let Some(label) = query_selector("#rotation-label") {
        set_inner_html(label, &format!("{rotation}°"));
    }
    if let Some(slider) = query_selector("#rotation-slider") {
        set_attribute(slider, "value", &rotation.to_string());
    }

    set_button_active("#view\\.flip\\.x", host::view::flipped_x());
    set_button_active("#view\\.flip\\.y", host::view::flipped_y());
}

fn set_button_active(selector: &str, active: bool) {
    if let Some(btn) = query_selector(selector) {
        let class = if active { "btn active" } else { "btn" };
        set_attribute(btn, "class", class);
    }
}

#[plugin_sdk::panel_handler]
fn set_zoom(value: i32) {
    let zoom_percent = value.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
    emit_service(&services::view::set_zoom(zoom_percent as f32 / 100.0));
}

#[plugin_sdk::panel_handler]
fn set_pan_x(value: i32) {
    let pan_x = value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    emit_service(&services::view::set_pan(
        pan_x as f32,
        host::view::pan_y() as f32,
    ));
}

#[plugin_sdk::panel_handler]
fn set_pan_y(value: i32) {
    let pan_y = value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    emit_service(&services::view::set_pan(
        host::view::pan_x() as f32,
        pan_y as f32,
    ));
}

#[plugin_sdk::panel_handler]
fn set_rotation(value: i32) {
    emit_service(&services::view::set_rotation(value.rem_euclid(360) as f32));
}

#[plugin_sdk::panel_handler]
fn reset_view() {
    emit_service(&services::view::reset());
}

#[plugin_sdk::panel_handler]
fn focus_active_panel() {
    emit_service(&services::panel_nav::focus_active());
}

#[plugin_sdk::panel_handler]
fn previous_panel() {
    emit_service(&services::panel_nav::select_previous());
}

#[plugin_sdk::panel_handler]
fn next_panel() {
    emit_service(&services::panel_nav::select_next());
}

#[plugin_sdk::panel_handler]
fn flip_horizontal() {
    emit_service(&services::view::flip_horizontal());
}

#[plugin_sdk::panel_handler]
fn flip_vertical() {
    emit_service(&services::view::flip_vertical());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrypoints_callable_on_native() {
        init();
        sync_host();
        set_zoom(125);
        set_pan_x(2100);
        set_pan_y(1950);
        set_rotation(270);
        reset_view();
        focus_active_panel();
        previous_panel();
        next_panel();
        flip_horizontal();
        flip_vertical();
    }
}
