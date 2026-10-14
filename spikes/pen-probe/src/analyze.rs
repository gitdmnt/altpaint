//! 記録したイベントの列から、A-SPK-b の判定と、表の値を出す。
use crate::event::{Button, Kind, Load, PenEvent, Source};

/// 頻度 (Hz)。個数を時間で割った値と、受信の間隔の中央値の逆数。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rate {
    pub by_count: f64,
    pub by_interval: f64,
}

/// 接触している間の、位置のイベントの頻度と、筆圧のサンプルの頻度。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rates {
    pub position: Rate,
    pub pressure: Rate,
}

/// 表の 1 行。
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: &'static str,
    pub item: &'static str,
    pub verdict: &'static str,
    pub observed: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// 入力元ごとの件数 (ペン、マウス、タッチ、その他)。
    pub sources: [usize; 4],
    /// イベントループを止めない状態と、16ms 止めた状態の頻度。
    pub idle: Rates,
    pub busy: Rates,
    pub rows: Vec<Row>,
}

/// 接触している間の移動を、ストロークごとに分ける。あわせて、浮かせている間の移動の件数を返す。
fn strokes(pen: &[PenEvent]) -> (Vec<Vec<PenEvent>>, usize) {
    let step = |(mut strokes, hover, down): (Vec<Vec<PenEvent>>, usize, bool), e: &PenEvent| match (
        e.kind, down,
    ) {
        (Kind::Down, _) => {
            strokes.push(Vec::new());
            (strokes, hover, true)
        }
        (Kind::Up, _) => (strokes, hover, false),
        (Kind::Move, false) => (strokes, hover + 1, false),
        (Kind::Move, true) => {
            strokes.last_mut().expect("接触の後である").push(*e);
            (strokes, hover, true)
        }
    };
    let (strokes, hover, _) = pen.iter().fold((Vec::new(), 0, false), step);
    (strokes, hover)
}

/// ストロークごとの時刻の列から、頻度を出す。
fn rate(times: &[Vec<u64>]) -> Rate {
    let spans = times.iter().filter(|t| t.len() >= 2);
    let (count, us) = spans.fold((0, 0), |(count, us), t| {
        (count + t.len() - 1, us + t[t.len() - 1] - t[0])
    });
    let mut gaps: Vec<u64> = times
        .iter()
        .flat_map(|t| t.windows(2).map(|w| w[1] - w[0]))
        .collect();
    gaps.sort_unstable();
    let by_count = if us > 0 {
        count as f64 * 1e6 / us as f64
    } else {
        0.0
    };
    Rate {
        by_count,
        by_interval: gaps
            .get(gaps.len() / 2)
            .map_or(0.0, |&gap| 1e6 / gap.max(1) as f64),
    }
}

/// 筆圧のサンプルの時刻。直前と筆圧が違うか、1ms 以上あとに届いたイベントだけを数える
/// (まとめて届いた、同じ筆圧の写しを数えない)。
fn pressure_times(stroke: &[&PenEvent]) -> Vec<u64> {
    let with: Vec<&PenEvent> = stroke
        .iter()
        .copied()
        .filter(|e| e.pressure.is_some())
        .collect();
    let fresh = |pair: &&[&PenEvent]| {
        pair[1].pressure != pair[0].pressure || pair[1].t_us - pair[0].t_us >= 1000
    };
    let rest = with.windows(2).filter(fresh).map(|pair| pair[1].t_us);
    with.first()
        .map(|e| e.t_us)
        .into_iter()
        .chain(rest)
        .collect()
}

/// 負荷の状態ごとの頻度と、接触していた時間 (µs)、1ms 以内にまとまって届いた最大の件数。
fn rates(strokes: &[Vec<PenEvent>], load: Load) -> (Rates, u64, usize) {
    let parts: Vec<Vec<&PenEvent>> = strokes
        .iter()
        .map(|s| s.iter().filter(|e| e.load == load).collect())
        .collect();
    let position: Vec<Vec<u64>> = parts
        .iter()
        .map(|s| s.iter().map(|e| e.t_us).collect())
        .collect();
    let pressure: Vec<Vec<u64>> = parts.iter().map(|s| pressure_times(s)).collect();
    let contact = position
        .iter()
        .filter(|t| t.len() >= 2)
        .map(|t| t[t.len() - 1] - t[0])
        .sum();
    let run = |(best, run): (usize, usize), w: &[u64]| {
        let run = if w[1] - w[0] < 1000 { run + 1 } else { 1 };
        (best.max(run), run)
    };
    let burst = position
        .iter()
        .map(|t| t.windows(2).fold((1, 1), run).0)
        .max()
        .unwrap_or(1);
    (
        Rates {
            position: rate(&position),
            pressure: rate(&pressure),
        },
        contact,
        burst,
    )
}

/// A-SPK-b (1): 筆圧が 0〜1 の連続値か。上から順に見て、最初に当たった判定を返す。
fn pressure_row(pen: usize, contact: &[PenEvent]) -> (&'static str, String) {
    let mut values: Vec<f32> = contact.iter().filter_map(|e| e.pressure).collect();
    values.sort_by(f32::total_cmp);
    let (Some(&min), Some(&max)) = (values.first(), values.last()) else {
        let why = if pen == 0 {
            "ペンとして届いていない"
        } else {
            "筆圧が届いていない"
        };
        return ("不合格", why.to_string());
    };
    let samples = values.len();
    let missing = contact.len() - samples;
    values.dedup();
    let verdict = match values.len() {
        _ if samples < 300 => "操作不足",
        _ if min < 0.0 || max > 1.0 => "不合格",
        0..=2 => "不合格",
        _ if min > 0.15 || max < 0.85 => "操作不足",
        3..=63 => "粗い",
        _ => "合格",
    };
    (
        verdict,
        format!(
            "最小 {min:.3}、最大 {max:.3}、異なる値 {} 個、筆圧なし {missing} 件",
            values.len()
        ),
    )
}

/// A-SPK-b (2): 無負荷で、筆圧のサンプルが 100Hz 以上で届くか。
fn rate_verdict(idle: Rate, contact_us: u64) -> &'static str {
    match (idle.by_count >= 100.0, idle.by_interval >= 100.0) {
        _ if contact_us < 2_000_000 => "操作不足",
        (true, true) => "合格",
        (false, false) => "不合格",
        _ => "要確認",
    }
}

/// 傾きが動いているか、止まっているか、届いていないか。x か y が 10 度以上動けば、取れるとする。
fn tilt_row(pen: &[PenEvent]) -> (&'static str, String) {
    let x = || pen.iter().filter_map(|e| e.tilt).map(|t| t.0);
    let y = || pen.iter().filter_map(|e| e.tilt).map(|t| t.1);
    let spread = |min: Option<i8>, max: Option<i8>| Some(i32::from(max?) - i32::from(min?));
    let seen = format!(
        "x {:?}〜{:?}、y {:?}〜{:?}",
        x().min(),
        x().max(),
        y().min(),
        y().max()
    );
    let spreads = spread(x().min(), x().max()).zip(spread(y().min(), y().max()));
    match spreads {
        None => ("取れない", seen),
        Some((x, y)) if x.max(y) >= 10 => ("取れる", seen),
        Some(_) => ("値が動かない", seen),
    }
}

pub fn analyze(events: &[PenEvent]) -> Report {
    let count = |source| events.iter().filter(|e| e.source == source).count();
    let sources = [Source::Pen, Source::Mouse, Source::Touch, Source::Other].map(count);
    let from_pen = events.iter().copied().filter(|e| e.source == Source::Pen);
    let pen: Vec<PenEvent> = from_pen.collect();
    let (strokes, hover) = strokes(&pen);
    let (idle, idle_us, _) = rates(&strokes, Load::Idle);
    let (busy, busy_us, burst) = rates(&strokes, Load::Busy);

    // 2 つの区間が終わった後の接触は、(1) の判定に入れない。
    let mut judged = strokes.concat();
    judged.retain(|e| e.load != Load::After);
    let (pressure, pressure_seen) = pressure_row(sources[0], &judged);
    let rate_seen = format!(
        "個数から {:.0}Hz、間隔から {:.0}Hz、接触 {:.1} 秒 (位置は {:.0}Hz)",
        idle.pressure.by_count,
        idle.pressure.by_interval,
        idle_us as f64 / 1e6,
        idle.position.by_count
    );
    let erased = |e: &PenEvent| e.kind == Kind::Down && e.button == Some(Button::Eraser);
    let eraser = if pen.iter().any(erased) {
        "取れる"
    } else {
        "観測なし"
    };
    let (tilt, tilt_seen) = tilt_row(&pen);
    let ratio = |busy: f64, idle: f64| if idle > 0.0 { busy / idle } else { 0.0 };
    let position_ratio = ratio(busy.position.by_count, idle.position.by_count);
    let pressure_ratio = ratio(busy.pressure.by_count, idle.pressure.by_count);
    let position = match (busy_us > 0, burst >= 2, position_ratio >= 0.9) {
        (false, ..) => "観測なし",
        (true, false, _) => "判定できず",
        (true, true, true) => "取れる",
        (true, true, false) => "落ちる",
    };
    let shared = match (busy_us > 0, pressure_ratio >= 0.9) {
        (false, _) => "観測なし",
        (true, true) => "サンプルごと",
        (true, false) => "まとめて 1 つ",
    };
    let position_hz = busy.position.by_count;
    let position_seen = format!(
        "負荷あり {position_hz:.0}Hz (無負荷の {position_ratio:.2} 倍)、最大 {burst} 件が同時"
    );
    let pressure_hz = busy.pressure.by_count;
    let shared_seen = format!("負荷あり {pressure_hz:.0}Hz (無負荷の {pressure_ratio:.2} 倍)");
    let hovering = if hover >= 20 {
        "取れる"
    } else {
        "取れない"
    };

    let row = |id, item, verdict, observed| Row {
        id,
        item,
        verdict,
        observed,
    };
    let other = |item, verdict, observed| row("A-SPK-b (3)", item, verdict, observed);
    let rows = vec![
        row(
            "A-SPK-b (1)",
            "筆圧が 0〜1 の連続値",
            pressure,
            pressure_seen,
        ),
        row(
            "A-SPK-b (2)",
            "筆圧が 100Hz 以上",
            rate_verdict(idle.pressure, idle_us),
            rate_seen,
        ),
        other(
            "ホバー",
            hovering,
            format!("浮かせている間の移動 {hover} 件"),
        ),
        other(
            "消しゴム側",
            eraser,
            "接触のときのボタンとして届くかを見た".to_string(),
        ),
        other("傾き", tilt, tilt_seen),
        other("結合サンプル (位置)", position, position_seen),
        other("結合サンプル (筆圧)", shared, shared_seen),
    ];
    Report {
        sources,
        idle,
        busy,
        rows,
    }
}
