//! 判定の単体テスト。ペンタブが無くても回せるように、イベントの列を合成する。
use spike_pen_probe::analyze::{Report, analyze};
use spike_pen_probe::event::{Button, Kind, Load, PenEvent, Source};

const PRESSURE: &str = "筆圧が 0〜1 の連続値";
const RATE: &str = "筆圧が 100Hz 以上";

/// 項目の名前から、判定を引く。
trait Verdict {
    fn verdict(&self, item: &str) -> &'static str;
}

impl Verdict for Report {
    fn verdict(&self, item: &str) -> &'static str {
        let row = self.rows.iter().find(|row| row.item == item);
        row.map_or("", |row| row.verdict)
    }
}

fn event(t_us: u64, kind: Kind) -> PenEvent {
    PenEvent {
        t_us,
        kind,
        source: Source::Pen,
        x: 100.0 + (t_us / 100) as f64,
        y: 50.0,
        pressure: None,
        tilt: None,
        twist: None,
        button: None,
        load: Load::Idle,
    }
}

/// start から、hz で secs 秒の 1 本のストローク (接触、移動の列、離す)。
/// each が、何番目かと進み具合 (0〜1) から、移動のイベントを仕上げる。
fn stroke(
    start: u64,
    hz: f64,
    secs: f64,
    each: impl Fn(u64, f64, PenEvent) -> PenEvent,
) -> Vec<PenEvent> {
    let (count, step) = ((hz * secs) as u64, (1e6 / hz) as u64);
    let contact = |t, kind| PenEvent {
        button: Some(Button::Contact),
        ..event(t, kind)
    };
    let moves = (1..=count).map(|i| {
        each(
            i,
            i as f64 / count as f64,
            event(start + step * i, Kind::Move),
        )
    });
    let down = std::iter::once(contact(start, Kind::Down));
    down.chain(moves)
        .chain(std::iter::once(contact(
            start + step * (count + 1),
            Kind::Up,
        )))
        .collect()
}

fn ramp(_: u64, progress: f64, e: PenEvent) -> PenEvent {
    PenEvent {
        pressure: Some((0.05 + 0.95 * progress) as f32),
        ..e
    }
}

/// 負荷ありの区間。20ms ごとに 4 件がまとまって届く (位置は 200Hz)。筆圧は、shared なら 4 件とも同じ値にする。
fn coalesced(start: u64, secs: f64, shared: bool) -> Vec<PenEvent> {
    stroke(start, 200.0, secs, |i, _, e| {
        let group = (i - 1) / 4;
        let level = if shared { group * 4 } else { i } as f64 / (200.0 * secs);
        let t_us = start + 20_000 * (group + 1) + (i - 1) % 4;
        PenEvent {
            t_us,
            pressure: Some((0.05 + 0.9 * level) as f32),
            load: Load::Busy,
            ..e
        }
    })
}

fn near(value: f64, want: f64) -> bool {
    (value - want).abs() <= want * 0.05
}

fn observed(report: &Report, item: &str) -> String {
    let row = report.rows.iter().find(|row| row.item == item);
    row.map(|row| row.observed.clone()).unwrap_or_default()
}

#[test]
fn ramping_pressure_at_200hz_passes_both_conditions() {
    let report = analyze(&stroke(0, 200.0, 5.0, ramp));
    assert_eq!(
        (report.verdict(PRESSURE), report.verdict(RATE)),
        ("合格", "合格")
    );
    assert!(
        near(report.idle.pressure.by_count, 200.0),
        "{:?}",
        report.idle
    );
    assert!(
        near(report.idle.pressure.by_interval, 200.0),
        "{:?}",
        report.idle
    );
    assert_eq!(report.sources, [1002, 0, 0, 0]);
}

#[test]
fn sixty_hz_fails_the_rate_but_not_the_pressure() {
    let report = analyze(&stroke(0, 60.0, 5.0, ramp));
    assert_eq!(
        (report.verdict(PRESSURE), report.verdict(RATE)),
        ("合格", "不合格")
    );
}

#[test]
fn two_valued_pressure_fails_and_a_few_levels_are_coarse() {
    let levels = |n: u64| {
        move |i: u64, _: f64, e: PenEvent| PenEvent {
            pressure: Some((1 + i % n) as f32 / n as f32),
            ..e
        }
    };
    assert_eq!(
        analyze(&stroke(0, 200.0, 5.0, levels(2))).verdict(PRESSURE),
        "不合格"
    );
    assert_eq!(
        analyze(&stroke(0, 200.0, 5.0, levels(10))).verdict(PRESSURE),
        "粗い"
    );
}

#[test]
fn narrow_pressure_and_short_contact_ask_for_another_try() {
    let narrow = |_: u64, progress: f64, e: PenEvent| PenEvent {
        pressure: Some((0.4 + 0.2 * progress) as f32),
        ..e
    };
    assert_eq!(
        analyze(&stroke(0, 200.0, 5.0, narrow)).verdict(PRESSURE),
        "操作不足"
    );
    assert_eq!(
        analyze(&stroke(0, 200.0, 0.5, ramp)).verdict(RATE),
        "操作不足"
    );
}

#[test]
fn a_long_pause_inside_a_stroke_needs_a_look() {
    let paused = |i: u64, progress: f64, e: PenEvent| {
        let t_us = e.t_us + if i > 200 { 3_000_000 } else { 0 };
        ramp(i, progress, PenEvent { t_us, ..e })
    };
    let report = analyze(&stroke(0, 200.0, 2.0, paused));
    assert_eq!(report.verdict(RATE), "要確認");
    assert!(report.idle.pressure.by_count < 100.0, "{:?}", report.idle);
    assert!(
        near(report.idle.pressure.by_interval, 200.0),
        "{:?}",
        report.idle
    );
}

#[test]
fn missing_pressure_and_mouse_only_input_fail_and_say_why() {
    let no_pressure = analyze(&stroke(0, 200.0, 5.0, |_, _, e| e));
    assert_eq!(no_pressure.verdict(PRESSURE), "不合格");
    assert!(observed(&no_pressure, PRESSURE).contains("筆圧が届いていない"));

    let as_mouse = |e: PenEvent| PenEvent {
        source: Source::Mouse,
        ..e
    };
    let mouse = analyze(
        &stroke(0, 200.0, 5.0, ramp)
            .into_iter()
            .map(as_mouse)
            .collect::<Vec<_>>(),
    );
    assert_eq!(mouse.verdict(PRESSURE), "不合格");
    assert!(observed(&mouse, PRESSURE).contains("ペンとして届いていない"));
    assert_eq!(mouse.sources, [0, 1002, 0, 0]);
}

#[test]
fn coalesced_samples_are_reported_apart_from_the_verdict() {
    let idle = stroke(0, 200.0, 5.0, ramp);
    let shared = analyze(&[idle.clone(), coalesced(6_000_000, 5.0, true)].concat());
    assert_eq!(
        (shared.verdict(PRESSURE), shared.verdict(RATE)),
        ("合格", "合格")
    );
    assert!(
        near(shared.busy.position.by_count, 200.0),
        "{:?}",
        shared.busy
    );
    assert!(
        near(shared.busy.pressure.by_count, 50.0),
        "{:?}",
        shared.busy
    );
    assert_eq!(shared.verdict("結合サンプル (位置)"), "取れる");
    assert_eq!(shared.verdict("結合サンプル (筆圧)"), "まとめて 1 つ");

    let each = analyze(&[idle.clone(), coalesced(6_000_000, 5.0, false)].concat());
    assert_eq!(each.verdict("結合サンプル (筆圧)"), "サンプルごと");
    assert_eq!(analyze(&idle).verdict("結合サンプル (位置)"), "観測なし");
}

#[test]
fn hover_needs_moves_without_contact() {
    let hover: Vec<PenEvent> = (0..30).map(|i| event(i * 5_000, Kind::Move)).collect();
    let with = analyze(&[hover, stroke(1_000_000, 200.0, 5.0, ramp)].concat());
    assert_eq!(with.verdict("ホバー"), "取れる");
    assert_eq!(
        analyze(&stroke(0, 200.0, 5.0, ramp)).verdict("ホバー"),
        "取れない"
    );
}

#[test]
fn the_eraser_end_is_seen_as_a_button_at_contact() {
    let press = PenEvent {
        button: Some(Button::Eraser),
        ..event(9_000_000, Kind::Down)
    };
    let base = stroke(0, 200.0, 5.0, ramp);
    assert_eq!(analyze(&base).verdict("消しゴム側"), "観測なし");
    assert_eq!(
        analyze(&[base, vec![press]].concat()).verdict("消しゴム側"),
        "取れる"
    );
}

#[test]
fn tilt_tells_moving_values_from_fixed_and_missing_ones() {
    let tilted = |tilt: fn(u64) -> Option<(i8, i8)>| {
        analyze(&stroke(0, 200.0, 5.0, move |i, p, e| {
            ramp(i, p, PenEvent { tilt: tilt(i), ..e })
        }))
    };
    assert_eq!(
        tilted(|i| Some(((i % 50) as i8 - 20, 5))).verdict("傾き"),
        "取れる"
    );
    assert_eq!(tilted(|_| Some((0, 0))).verdict("傾き"), "値が動かない");
    assert_eq!(tilted(|_| None).verdict("傾き"), "取れない");
}

#[test]
fn a_short_record_asks_for_another_try_instead_of_failing() {
    let two_levels = |i: u64, _: f64, e: PenEvent| PenEvent {
        pressure: Some((1 + i % 2) as f32 / 2.0),
        ..e
    };
    assert_eq!(
        analyze(&stroke(0, 200.0, 0.5, two_levels)).verdict(PRESSURE),
        "操作不足"
    );
    assert_eq!(
        analyze(&stroke(0, 200.0, 5.0, two_levels)).verdict(PRESSURE),
        "不合格"
    );
}

#[test]
fn contact_after_both_phases_does_not_change_the_verdicts() {
    // 区間の後の接触に、範囲の外の筆圧を入れる。判定に入っていれば、不合格になる。
    let late = |_: u64, _: f64, e: PenEvent| PenEvent {
        pressure: Some(1.5),
        load: Load::After,
        ..e
    };
    let out_of_range = stroke(20_000_000, 30.0, 5.0, late);
    let report = analyze(&[stroke(0, 200.0, 5.0, ramp), out_of_range].concat());
    assert_eq!(
        (report.verdict(PRESSURE), report.verdict(RATE)),
        ("合格", "合格")
    );
    assert!(
        near(report.idle.pressure.by_count, 200.0),
        "{:?}",
        report.idle
    );
}

#[test]
fn the_rate_row_shows_how_many_samples_arrived_together_without_load() {
    let alone = analyze(&stroke(0, 200.0, 5.0, ramp));
    assert!(
        observed(&alone, RATE).contains("最大 1 件が同時"),
        "{}",
        observed(&alone, RATE)
    );

    // 無負荷でも、2 件ずつ同じ筆圧でまとまって届く 160Hz。筆圧のサンプルは 80Hz になる。
    let pairs = stroke(0, 160.0, 5.0, |i, _, e| {
        let pair = (i - 1) / 2;
        let t_us = 12_500 * (pair + 1) + (i - 1) % 2;
        PenEvent {
            t_us,
            pressure: Some(0.05 + pair as f32 / 420.0),
            ..e
        }
    });
    let together = analyze(&pairs);
    assert_eq!(together.verdict(RATE), "不合格");
    assert!(
        near(together.idle.position.by_count, 160.0),
        "{:?}",
        together.idle
    );
    assert!(
        observed(&together, RATE).contains("最大 2 件が同時"),
        "{}",
        observed(&together, RATE)
    );
}
