//! ツール記述子 (`ToolDescriptor`): ツール種別から導出する挙動メタデータ (BL-134)。
//!
//! ジェスチャ種別・合成モード・サイズ解決方針を `ToolKind` から導出し、
//! paint-engine / desktop に散在していた `ToolKind` のクローズド match を
//! 単一の問い合わせ点へ集約する。これは将来の「ツール処理プラグイン」
//! (ARCHITECTURE.md 最終目標) の安定境界になる。Wasm ツールプラグイン自体は
//! 本リファクタのスコープ外で、ここでは境界の確立のみを行う。
//!
//! 記述子は `ToolKind` (= `ToolDefinition::kind`) からのみ導出される純メタデータで、
//! セッション状態を持たない。サイズ解決のように状態が必要な問い合わせは、
//! 必要な入力 (ペンプリセット・基準サイズ・筆圧) を引数で受け取る。

use crate::{PenPreset, StrokeMode, ToolKind};

/// ツールが起動するポインタジェスチャの種別。
///
/// desktop / paint-engine の入力ルーティングが「どの経路で処理するか」を
/// `ToolKind` を直接 match せずに判定するための分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    /// ペン/消しゴム: down/drag/up でスタンプ列を生成するストローク。
    Stroke,
    /// バケツ: down 一発の連結領域塗りつぶし。
    FloodFill,
    /// 投げ縄バケツ: drag で多角形を構築し up で塗りつぶし。
    LassoFill,
    /// コマ作成: paint-engine の責務外であり desktop feature が別経路で処理する (BL-081)。
    KomaRect,
}

/// 実効ブラシサイズの解決方針。
///
/// `ToolDescriptor::resolve_size` が状態 (基準サイズ・プリセット・筆圧) から
/// 実効サイズを求める際に使う分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizePolicy {
    /// ペン: 筆圧カーブを基準サイズに適用する (プリセットが筆圧有効のとき)。
    PressureBrush,
    /// 消しゴム: 基準サイズをそのまま使う (筆圧非依存)。
    FixedBrush,
    /// 塗りつぶし/コマ: スタンプを持たないため実効サイズは 1。
    SinglePixel,
}

/// `ToolKind` から導出するツール挙動の記述子。
///
/// ジェスチャ種別・合成モード・サイズ解決方針を集約し、`ToolKind` の
/// クローズド match を呼び出し側から取り除く問い合わせ点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDescriptor {
    kind: ToolKind,
}

impl ToolDescriptor {
    /// `ToolKind` から記述子を導出する。
    pub const fn for_kind(kind: ToolKind) -> Self {
        Self { kind }
    }

    /// 元になった `ToolKind`。
    pub const fn kind(self) -> ToolKind {
        self.kind
    }

    /// このツールが起動するポインタジェスチャの種別。
    pub const fn gesture_kind(self) -> GestureKind {
        match self.kind {
            ToolKind::Pen | ToolKind::Eraser => GestureKind::Stroke,
            ToolKind::Bucket => GestureKind::FloodFill,
            ToolKind::LassoBucket => GestureKind::LassoFill,
            ToolKind::KomaRect => GestureKind::KomaRect,
        }
    }

    /// ストローク/塗りの合成モード (描画 source-over か消去か)。
    ///
    /// 消しゴムだけが `Erase`、その他はすべて `Paint`。
    pub const fn blend_mode(self) -> StrokeMode {
        self.kind.stroke_mode()
    }

    /// 実効ブラシサイズの解決方針。
    pub const fn size_policy(self) -> SizePolicy {
        match self.kind {
            ToolKind::Pen => SizePolicy::PressureBrush,
            ToolKind::Eraser => SizePolicy::FixedBrush,
            ToolKind::Bucket | ToolKind::LassoBucket | ToolKind::KomaRect => SizePolicy::SinglePixel,
        }
    }

    /// ブラシプレビュー (ホバー時の円) を描くツールか。
    ///
    /// ストロークジェスチャ (ペン/消しゴム) のみプレビューを持つ。
    pub const fn has_brush_preview(self) -> bool {
        matches!(self.gesture_kind(), GestureKind::Stroke)
    }

    /// 手ブレ補正 (stabilization) を適用するツールか。
    ///
    /// ペンのみ。消しゴム/塗り/コマは補正しない (既存挙動の維持)。
    pub const fn applies_stabilization(self) -> bool {
        matches!(self.kind, ToolKind::Pen)
    }

    /// 基準サイズ・プリセット・筆圧から実効ブラシサイズを解決する。
    ///
    /// `base_size` はアクティブペンサイズ (`active_pen_size`)。`preset` はアクティブ
    /// ペンプリセット (筆圧有効判定に使う)。`pressure` は 0.0..=1.0 にクランプされる。
    /// 筆圧カーブは本メソッドで 1 回だけ適用する (BL-030 で二重適用を解消済み)。
    pub fn resolve_size(self, base_size: u32, preset: Option<&PenPreset>, pressure: f32) -> u32 {
        match self.size_policy() {
            SizePolicy::FixedBrush => base_size.max(1),
            SizePolicy::SinglePixel => 1,
            SizePolicy::PressureBrush => {
                let base = base_size.max(1);
                let Some(preset) = preset else {
                    return base;
                };
                if !preset.pressure_enabled {
                    return base;
                }
                let clamped_pressure = pressure.clamp(0.0, 1.0);
                let scaled = (base as f32 * (0.2 + clamped_pressure * 0.8)).round() as u32;
                scaled.max(1)
            }
        }
    }
}

impl From<ToolKind> for ToolDescriptor {
    fn from(kind: ToolKind) -> Self {
        Self::for_kind(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressure_preset(pressure_enabled: bool) -> PenPreset {
        PenPreset {
            pressure_enabled,
            ..PenPreset::default()
        }
    }

    #[test]
    fn gesture_kind_classifies_each_tool() {
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::Pen).gesture_kind(),
            GestureKind::Stroke
        );
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::Eraser).gesture_kind(),
            GestureKind::Stroke
        );
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::Bucket).gesture_kind(),
            GestureKind::FloodFill
        );
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::LassoBucket).gesture_kind(),
            GestureKind::LassoFill
        );
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::KomaRect).gesture_kind(),
            GestureKind::KomaRect
        );
    }

    #[test]
    fn blend_mode_matches_stroke_mode() {
        assert_eq!(
            ToolDescriptor::for_kind(ToolKind::Eraser).blend_mode(),
            StrokeMode::Erase
        );
        for kind in [
            ToolKind::Pen,
            ToolKind::Bucket,
            ToolKind::LassoBucket,
            ToolKind::KomaRect,
        ] {
            assert_eq!(
                ToolDescriptor::for_kind(kind).blend_mode(),
                StrokeMode::Paint,
                "kind={kind:?}"
            );
        }
    }

    #[test]
    fn has_brush_preview_only_for_stroke_tools() {
        assert!(ToolDescriptor::for_kind(ToolKind::Pen).has_brush_preview());
        assert!(ToolDescriptor::for_kind(ToolKind::Eraser).has_brush_preview());
        assert!(!ToolDescriptor::for_kind(ToolKind::Bucket).has_brush_preview());
        assert!(!ToolDescriptor::for_kind(ToolKind::LassoBucket).has_brush_preview());
        assert!(!ToolDescriptor::for_kind(ToolKind::KomaRect).has_brush_preview());
    }

    #[test]
    fn stabilization_applies_only_to_pen() {
        assert!(ToolDescriptor::for_kind(ToolKind::Pen).applies_stabilization());
        for kind in [
            ToolKind::Eraser,
            ToolKind::Bucket,
            ToolKind::LassoBucket,
            ToolKind::KomaRect,
        ] {
            assert!(
                !ToolDescriptor::for_kind(kind).applies_stabilization(),
                "kind={kind:?}"
            );
        }
    }

    #[test]
    fn eraser_size_ignores_pressure() {
        let descriptor = ToolDescriptor::for_kind(ToolKind::Eraser);
        let preset = pressure_preset(true);
        assert_eq!(descriptor.resolve_size(10, Some(&preset), 0.0), 10);
        assert_eq!(descriptor.resolve_size(10, Some(&preset), 1.0), 10);
        // 0 はクランプして 1 にする。
        assert_eq!(descriptor.resolve_size(0, Some(&preset), 1.0), 1);
    }

    #[test]
    fn fill_and_koma_tools_resolve_to_single_pixel() {
        for kind in [ToolKind::Bucket, ToolKind::LassoBucket, ToolKind::KomaRect] {
            let descriptor = ToolDescriptor::for_kind(kind);
            assert_eq!(descriptor.resolve_size(50, None, 1.0), 1, "kind={kind:?}");
        }
    }

    #[test]
    fn pen_without_preset_uses_base_size() {
        let descriptor = ToolDescriptor::for_kind(ToolKind::Pen);
        assert_eq!(descriptor.resolve_size(8, None, 0.5), 8);
        assert_eq!(descriptor.resolve_size(0, None, 0.5), 1);
    }

    #[test]
    fn pen_with_pressure_disabled_uses_base_size() {
        let descriptor = ToolDescriptor::for_kind(ToolKind::Pen);
        let preset = pressure_preset(false);
        assert_eq!(descriptor.resolve_size(20, Some(&preset), 0.0), 20);
        assert_eq!(descriptor.resolve_size(20, Some(&preset), 1.0), 20);
    }

    #[test]
    fn pen_with_pressure_applies_curve_once() {
        let descriptor = ToolDescriptor::for_kind(ToolKind::Pen);
        let preset = pressure_preset(true);
        // base * (0.2 + pressure * 0.8) を 1 回だけ適用 (BL-030)。
        // pressure=1.0 → base のまま。
        assert_eq!(descriptor.resolve_size(100, Some(&preset), 1.0), 100);
        // pressure=0.0 → base * 0.2 = 20。
        assert_eq!(descriptor.resolve_size(100, Some(&preset), 0.0), 20);
        // pressure=0.5 → base * 0.6 = 60。
        assert_eq!(descriptor.resolve_size(100, Some(&preset), 0.5), 60);
        // クランプ: pressure>1.0 でも 1.0 と同じ。
        assert_eq!(descriptor.resolve_size(100, Some(&preset), 2.0), 100);
    }
}
