use panel_sdk::{
    host,
    runtime::{emit_service, set_state_bool, set_state_i32, set_state_string},
    services, state,
};

const ZOOM_LABEL: state::StringKey = state::string("zoom_label");
const ZOOM_SLIDER: state::IntKey = state::int("zoom_slider");
const PAN_LABEL: state::StringKey = state::string("pan_label");
const PAN_X_SLIDER: state::IntKey = state::int("pan_x_slider");
const PAN_Y_SLIDER: state::IntKey = state::int("pan_y_slider");
const ROTATION_LABEL: state::StringKey = state::string("rotation_label");
const ROTATION_SLIDER: state::IntKey = state::int("rotation_slider");
const FLIP_X: state::BoolKey = state::bool("flip_x");
const FLIP_Y: state::BoolKey = state::bool("flip_y");

const MIN_ZOOM_PERCENT: i32 = 25;
const MAX_ZOOM_PERCENT: i32 = 1600;
const PAN_SLIDER_CENTER: i32 = 2000;
const PAN_SLIDER_MIN: i32 = 0;
const PAN_SLIDER_MAX: i32 = 4000;

fn update_zoom_state(zoom_percent: i32) {
    let clamped = zoom_percent.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
    set_state_i32(ZOOM_SLIDER, clamped);
    set_state_string(ZOOM_LABEL, format!("{:.1}%", clamped as f32));
}

fn update_pan_state(pan_x: i32, pan_y: i32) {
    set_state_i32(
        PAN_X_SLIDER,
        (pan_x + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX),
    );
    set_state_i32(
        PAN_Y_SLIDER,
        (pan_y + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX),
    );
    set_state_string(PAN_LABEL, format!("{pan_x}, {pan_y}"));
}

fn update_rotation_state(rotation_degrees: i32) {
    let clamped = rotation_degrees.rem_euclid(360);
    set_state_i32(ROTATION_SLIDER, clamped);
    set_state_string(ROTATION_LABEL, format!("{clamped}°"));
}

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    let zoom_milli = host::view::zoom_milli().max(1);
    let zoom_percent = zoom_milli as f32 / 10.0;
    set_state_string(ZOOM_LABEL, format!("{zoom_percent:.1}%"));
    set_state_i32(
        ZOOM_SLIDER,
        ((zoom_milli + 5) / 10).clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT),
    );
    set_state_string(
        PAN_LABEL,
        format!("{}, {}", host::view::pan_x(), host::view::pan_y()),
    );
    set_state_i32(
        PAN_X_SLIDER,
        (host::view::pan_x() + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX),
    );
    set_state_i32(
        PAN_Y_SLIDER,
        (host::view::pan_y() + PAN_SLIDER_CENTER).clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX),
    );
    set_state_string(
        ROTATION_LABEL,
        format!("{}°", host::view::rotation_degrees().rem_euclid(360)),
    );
    set_state_i32(
        ROTATION_SLIDER,
        host::view::rotation_degrees().rem_euclid(360),
    );
    set_state_bool(FLIP_X, host::view::flipped_x());
    set_state_bool(FLIP_Y, host::view::flipped_y());
}

#[cfg(test)]
fn normalized_rotation_degrees(quarter_turns: i32) -> i32 {
    quarter_turns.rem_euclid(4) * 90
}

#[panel_sdk::panel_handler]
fn set_zoom(value: i32) {
    let zoom_percent = value.clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
    update_zoom_state(zoom_percent);
    emit_service(&services::view::set_zoom(zoom_percent as f32 / 100.0));
}

#[panel_sdk::panel_handler]
fn set_pan_x(value: i32) {
    let pan_x = value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    update_pan_state(pan_x, host::view::pan_y());
    emit_service(&services::view::set_pan(
        pan_x as f32,
        host::view::pan_y() as f32,
    ));
}

#[panel_sdk::panel_handler]
fn set_pan_y(value: i32) {
    let pan_y = value.clamp(PAN_SLIDER_MIN, PAN_SLIDER_MAX) - PAN_SLIDER_CENTER;
    update_pan_state(host::view::pan_x(), pan_y);
    emit_service(&services::view::set_pan(
        host::view::pan_x() as f32,
        pan_y as f32,
    ));
}

#[panel_sdk::panel_handler]
fn set_rotation(value: i32) {
    update_rotation_state(value);
    emit_service(&services::view::set_rotation(value.rem_euclid(360) as f32));
}

#[panel_sdk::panel_handler]
fn reset_view() {
    emit_service(&services::view::reset());
}

#[panel_sdk::panel_handler]
fn focus_active_panel() {
    emit_service(&services::panel_nav::focus_active());
}

#[panel_sdk::panel_handler]
fn previous_panel() {
    emit_service(&services::panel_nav::select_previous());
}

#[panel_sdk::panel_handler]
fn next_panel() {
    emit_service(&services::panel_nav::select_next());
}

#[panel_sdk::panel_handler]
fn flip_horizontal() {
    emit_service(&services::view::flip_horizontal());
}

#[panel_sdk::panel_handler]
fn flip_vertical() {
    emit_service(&services::view::flip_vertical());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_rotation_degrees_wraps_turns() {
        assert_eq!(normalized_rotation_degrees(1), 90);
        assert_eq!(normalized_rotation_degrees(-1), 270);
    }

    #[test]
    fn panel_entrypoints_are_callable_on_native_targets() {
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
