//! コマ作成 (KomaRect) ジェスチャの状態機械。
//!
//! NOTE: BL-081 で paint-engine のペイント系ステートマシンから分離した。
//! コマ作成はペイント入力解釈ではなく desktop feature の責務であり、
//! `CanvasGestureUpdate` を Paint 系に縮小するために別経路へ切り出した。
//! 最終配置 (features/koma) は B7 で確定する。挙動は分離前と不変。

use document_model::KomaBounds;
use geometry::PagePoint;

use paint_engine::CanvasPointerAction;

/// コマ作成ジェスチャの進行中状態。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct KomaGesture {
    pub(crate) is_drawing: bool,
    pub(crate) anchor: Option<PagePoint>,
    pub(crate) last_position: Option<PagePoint>,
}

impl KomaGesture {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// コマ作成ジェスチャの 1 ステップ進行結果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KomaGestureUpdate {
    None,
    PreviewChanged,
    Committed {
        anchor: PagePoint,
        current: PagePoint,
    },
}

/// コマ作成ジェスチャを 1 ステップ進める。
///
/// `point` はクランプ済みのページ座標。down で anchor を確定し、drag でプレビューを更新し、
/// up でコマ矩形を確定する。
pub(crate) fn advance_koma_gesture(
    state: &mut KomaGesture,
    action: CanvasPointerAction,
    point: PagePoint,
) -> KomaGestureUpdate {
    match action {
        CanvasPointerAction::Down => {
            state.is_drawing = true;
            state.anchor = Some(point);
            state.last_position = Some(point);
            KomaGestureUpdate::PreviewChanged
        }
        CanvasPointerAction::Drag => {
            if !state.is_drawing {
                return KomaGestureUpdate::None;
            }
            if state.last_position == Some(point) {
                return KomaGestureUpdate::None;
            }
            state.last_position = Some(point);
            KomaGestureUpdate::PreviewChanged
        }
        CanvasPointerAction::Up => {
            let anchor = state.anchor;
            let current = state.last_position.or(Some(point));
            state.reset();
            match (anchor, current) {
                (Some(anchor), Some(current)) => KomaGestureUpdate::Committed { anchor, current },
                _ => KomaGestureUpdate::None,
            }
        }
    }
}

/// anchor と現在位置からコマ作成プレビューの矩形を導出する。
///
/// 矩形はページ境界内へクランプされる。幅・高さが 0 の場合は `None`。
pub(crate) fn koma_creation_preview_bounds(
    state: &KomaGesture,
    page_width: usize,
    page_height: usize,
) -> Option<KomaBounds> {
    let anchor = state.anchor?;
    let current = state.last_position?;
    let left = anchor.x.min(current.x).min(page_width.saturating_sub(1));
    let top = anchor.y.min(current.y).min(page_height.saturating_sub(1));
    let right = anchor.x.max(current.x).min(page_width.saturating_sub(1));
    let bottom = anchor.y.max(current.y).min(page_height.saturating_sub(1));
    let width = right.saturating_sub(left).saturating_add(1);
    let height = bottom.saturating_sub(top).saturating_add(1);
    (width > 0 && height > 0).then_some(KomaBounds {
        x: left,
        y: top,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koma_rect_preview_bounds_are_derived_from_gesture_state() {
        let state = KomaGesture {
            anchor: Some(PagePoint::new(80, 50)),
            last_position: Some(PagePoint::new(20, 30)),
            ..KomaGesture::default()
        };

        let bounds = koma_creation_preview_bounds(&state, 200, 200).expect("preview bounds");

        assert_eq!(bounds.x, 20);
        assert_eq!(bounds.y, 30);
        assert_eq!(bounds.width, 61);
        assert_eq!(bounds.height, 21);
    }

    #[test]
    fn koma_gesture_commits_anchor_and_current_on_release() {
        let mut state = KomaGesture::default();

        assert_eq!(
            advance_koma_gesture(&mut state, CanvasPointerAction::Down, PagePoint::new(10, 10)),
            KomaGestureUpdate::PreviewChanged
        );
        assert_eq!(
            advance_koma_gesture(&mut state, CanvasPointerAction::Drag, PagePoint::new(40, 30)),
            KomaGestureUpdate::PreviewChanged
        );
        let update =
            advance_koma_gesture(&mut state, CanvasPointerAction::Up, PagePoint::new(40, 30));
        assert_eq!(
            update,
            KomaGestureUpdate::Committed {
                anchor: PagePoint::new(10, 10),
                current: PagePoint::new(40, 30),
            }
        );
        assert_eq!(state, KomaGesture::default());
    }
}
