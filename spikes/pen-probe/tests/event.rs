//! CSV の単体テスト。
use spike_pen_probe::event::{Button, CSV_HEADER, Kind, Load, PenEvent, Source, csv_line};

fn event(t_us: u64, kind: Kind) -> PenEvent {
    PenEvent {
        t_us,
        kind,
        source: Source::Pen,
        x: 10.5,
        y: 20.0,
        pressure: None,
        tilt: None,
        twist: None,
        button: None,
        load: Load::Idle,
    }
}

#[test]
fn a_csv_line_has_one_field_per_column_and_leaves_missing_values_empty() {
    let full = PenEvent {
        pressure: Some(0.5),
        tilt: Some((10, -5)),
        twist: Some(90),
        button: Some(Button::Eraser),
        load: Load::Busy,
        ..event(1234, Kind::Down)
    };
    assert_eq!(
        csv_line(&full),
        "1234,Down,Pen,10.500,20.000,0.50000,10,-5,90,Eraser,Busy"
    );
    assert_eq!(
        csv_line(&event(7, Kind::Move)),
        "7,Move,Pen,10.500,20.000,,,,,,Idle"
    );
    assert_eq!(
        csv_line(&full).split(',').count(),
        CSV_HEADER.split(',').count()
    );
}
