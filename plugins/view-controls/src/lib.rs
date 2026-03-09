use panel_sdk::{
    commands,
    host,
    runtime::{emit_command, set_state_bool, set_state_string},
    state,
};

const ZOOM_LABEL: state::StringKey = state::string("zoom_label");
const PAN_LABEL: state::StringKey = state::string("pan_label");
const ROTATION_LABEL: state::StringKey = state::string("rotation_label");
const FLIP_X: state::BoolKey = state::bool("flip_x");
const FLIP_Y: state::BoolKey = state::bool("flip_y");

const PAN_STEP: f32 = 32.0;
const ZOOM_IN_FACTOR: f32 = 1.25;
const ZOOM_OUT_FACTOR: f32 = 0.8;

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_sync_host]
fn sync_host() {
    let zoom_milli = host::view::zoom_milli().max(1);
    let zoom_percent = zoom_milli as f32 / 10.0;
    set_state_string(ZOOM_LABEL, format!("{zoom_percent:.1}%"));
    set_state_string(
        PAN_LABEL,
        format!("{}, {}", host::view::pan_x(), host::view::pan_y()),
    );
    set_state_string(
        ROTATION_LABEL,
        format!("{}°", normalized_rotation_degrees(host::view::quarter_turns())),
    );
    set_state_bool(FLIP_X, host::view::flipped_x());
    set_state_bool(FLIP_Y, host::view::flipped_y());
}

fn zoom_with_factor(factor: f32) {
    let current = host::view::zoom_milli().max(1) as f32 / 1000.0;
    emit_command(&commands::view::zoom((current * factor).clamp(0.25, 16.0)));
}

fn pan(delta_x: f32, delta_y: f32) {
    emit_command(&commands::view::pan(delta_x, delta_y));
}

fn normalized_rotation_degrees(quarter_turns: i32) -> i32 {
    quarter_turns.rem_euclid(4) * 90
}

#[panel_sdk::panel_handler]
fn zoom_in() {
    zoom_with_factor(ZOOM_IN_FACTOR);
}

#[panel_sdk::panel_handler]
fn zoom_out() {
    zoom_with_factor(ZOOM_OUT_FACTOR);
}

#[panel_sdk::panel_handler]
fn reset_view() {
    emit_command(&commands::view::reset());
}

#[panel_sdk::panel_handler]
fn pan_left() {
    pan(-PAN_STEP, 0.0);
}

#[panel_sdk::panel_handler]
fn pan_right() {
    pan(PAN_STEP, 0.0);
}

#[panel_sdk::panel_handler]
fn pan_up() {
    pan(0.0, -PAN_STEP);
}

#[panel_sdk::panel_handler]
fn pan_down() {
    pan(0.0, PAN_STEP);
}

#[panel_sdk::panel_handler]
fn rotate_left() {
    emit_command(&commands::view::rotate(-1));
}

#[panel_sdk::panel_handler]
fn rotate_right() {
    emit_command(&commands::view::rotate(1));
}

#[panel_sdk::panel_handler]
fn flip_horizontal() {
    emit_command(&commands::view::flip_horizontal());
}

#[panel_sdk::panel_handler]
fn flip_vertical() {
    emit_command(&commands::view::flip_vertical());
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
        zoom_in();
        zoom_out();
        reset_view();
        pan_left();
        pan_right();
        pan_up();
        pan_down();
        rotate_left();
        rotate_right();
        flip_horizontal();
        flip_vertical();
    }
}
