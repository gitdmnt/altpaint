/// ストロークセグメントあたりのスタンプ最大数。
///
/// GPU TDR（Windows のタイムアウト検出）を回避し、CPU 過負荷を防ぐための上限値。
/// `canvas::ops::stroke` と `gpu-canvas::brush` の両方がこの定数を参照する。
pub const MAX_STAMP_STEPS: usize = 64;
