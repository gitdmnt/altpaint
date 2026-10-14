//! 記録するイベント。ウィンドウのライブラリの型に依らない。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Move,
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Pen,
    Mouse,
    Touch,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Contact,
    Barrel,
    Eraser,
    Other,
}

/// イベントを受けたときの区間。無負荷、負荷あり (イベントループを 16ms 止める)、2 つが終わった後。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    Idle,
    Busy,
    After,
}

/// 1 件のイベント。時刻は、受け取った時点の経過時間 (マイクロ秒)。
/// 筆圧・傾き・ひねりは、届かなければ None のままにする (0 や 1 で埋めない)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenEvent {
    pub t_us: u64,
    pub kind: Kind,
    pub source: Source,
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f32>,
    pub tilt: Option<(i8, i8)>,
    pub twist: Option<u16>,
    pub button: Option<Button>,
    pub load: Load,
}

pub const CSV_HEADER: &str = "t_us,kind,source,x,y,pressure,tilt_x,tilt_y,twist,button,load";

/// CSV の 1 行にする。None は空の欄にする。
pub fn csv_line(e: &PenEvent) -> String {
    let opt = |v: Option<String>| v.unwrap_or_default();
    [
        e.t_us.to_string(),
        format!("{:?}", e.kind),
        format!("{:?}", e.source),
        format!("{:.3}", e.x),
        format!("{:.3}", e.y),
        opt(e.pressure.map(|p| format!("{p:.5}"))),
        opt(e.tilt.map(|t| t.0.to_string())),
        opt(e.tilt.map(|t| t.1.to_string())),
        opt(e.twist.map(|t| t.to_string())),
        opt(e.button.map(|b| format!("{b:?}"))),
        format!("{:?}", e.load),
    ]
    .join(",")
}
