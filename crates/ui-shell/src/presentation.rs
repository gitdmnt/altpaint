use std::collections::BTreeMap;

use app_core::{PanelSurfacePoint, PanelSurfaceRect};
use panel_api::PanelEvent;

pub(crate) use render::{PanelHitKind, PanelHitRegion};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusTarget {
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextInputEditorState {
    pub(crate) cursor_chars: usize,
    pub(crate) preedit: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) struct PanelRenderState<'a> {
    pub(crate) focused_target: Option<&'a FocusTarget>,
    pub(crate) expanded_dropdown: Option<&'a FocusTarget>,
    pub(crate) text_input_states: &'a BTreeMap<(String, String), TextInputEditorState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSurface {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub(crate) hit_regions: Vec<PanelHitRegion>,
}

impl PanelSurface {
    /// 入力値を束ねた新しいインスタンスを生成する。
    pub fn from_pixels(x: usize, y: usize, width: usize, height: usize, pixels: Vec<u8>) -> Self {
        Self {
            x,
            y,
            width,
            height,
            pixels,
            hit_regions: Vec::new(),
        }
    }

    /// 現在の hit 領域 件数 を返す。
    pub fn hit_region_count(&self) -> usize {
        self.hit_regions.len()
    }

    /// 現在の global 範囲 を返す。
    pub fn global_bounds(&self) -> PanelSurfaceRect {
        PanelSurfaceRect::new(self.x, self.y, self.width, self.height)
    }

    /// 既存データを走査して hit test at を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn hit_test_at(&self, point: PanelSurfacePoint) -> Option<PanelEvent> {
        self.hit_regions
            .iter()
            .rev()
            .find(|region| {
                point.x >= region.x
                    && point.y >= region.y
                    && point.x < region.x + region.width
                    && point.y < region.y + region.height
            })
            .and_then(|region| panel_event_for_region(region, point))
    }

    /// 既存データを走査して move パネル hit test at を組み立てる。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn move_panel_hit_test_at(&self, point: PanelSurfacePoint) -> Option<String> {
        self.hit_regions
            .iter()
            .rev()
            .find(|region| {
                point.x >= region.x
                    && point.y >= region.y
                    && point.x < region.x + region.width
                    && point.y < region.y + region.height
                    && matches!(region.kind, PanelHitKind::MovePanel)
            })
            .map(|region| region.panel_id.clone())
    }

    /// 現在の値を イベント at へ変換する。
    pub fn drag_event_at(
        &self,
        panel_id: &str,
        node_id: &str,
        source_value: i32,
        point: PanelSurfacePoint,
    ) -> Option<PanelEvent> {
        let source_region = self.hit_regions.iter().rev().find(|region| {
            region.panel_id == panel_id
                && region.node_id == node_id
                && (matches!(&region.kind, PanelHitKind::Slider { .. })
                    || matches!(&region.kind, PanelHitKind::ColorWheel { .. })
                    || matches!(
                        &region.kind,
                        PanelHitKind::LayerListItem { value } if *value == source_value
                    ))
        })?;

        match &source_region.kind {
            PanelHitKind::Slider { .. } => self
                .hit_regions
                .iter()
                .rev()
                .find(|region| region.panel_id == panel_id && region.node_id == node_id)
                .and_then(|region| panel_event_for_region(region, point)),
            PanelHitKind::ColorWheel { .. } => self
                .hit_regions
                .iter()
                .rev()
                .find(|region| region.panel_id == panel_id && region.node_id == node_id)
                .and_then(|region| panel_event_for_region(region, point)),
            PanelHitKind::LayerListItem { .. } => self
                .hit_regions
                .iter()
                .rev()
                .find(|region| {
                    region.panel_id == panel_id
                        && region.node_id == node_id
                        && point.x >= region.x
                        && point.y >= region.y
                        && point.x < region.x + region.width
                        && point.y < region.y + region.height
                        && matches!(&region.kind, PanelHitKind::LayerListItem { .. })
                })
                .and_then(|region| match &region.kind {
                    PanelHitKind::LayerListItem { value } => Some(PanelEvent::DragValue {
                        panel_id: panel_id.to_string(),
                        node_id: node_id.to_string(),
                        from: source_value,
                        to: *value,
                    }),
                    _ => None,
                }),
            PanelHitKind::MovePanel
            | PanelHitKind::Activate
            | PanelHitKind::DropdownOption { .. } => None,
        }
    }
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn panel_event_for_region(region: &PanelHitRegion, point: PanelSurfacePoint) -> Option<PanelEvent> {
    Some(match &region.kind {
        PanelHitKind::MovePanel => return None,
        PanelHitKind::Activate => PanelEvent::Activate {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
        },
        PanelHitKind::Slider { min, max } => PanelEvent::SetValue {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: slider_value_for_position(region, *min, *max, point),
        },
        PanelHitKind::ColorWheel {
            hue_degrees,
            saturation,
            value,
        } => PanelEvent::SetText {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: color_wheel_value_for_position(region, point, *hue_degrees, *saturation, *value),
        },
        PanelHitKind::LayerListItem { value } => PanelEvent::SetValue {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: *value,
        },
        PanelHitKind::DropdownOption { value } => PanelEvent::SetText {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: value.clone(),
        },
    })
}

/// Slider 値 for position を有効範囲へ補正して返す。
fn slider_value_for_position(
    region: &PanelHitRegion,
    min: i32,
    max: i32,
    point: PanelSurfacePoint,
) -> i32 {
    if max <= min || region.width <= 1 {
        return min;
    }

    let local_x = point.x.clamp(region.x, region.x + region.width - 1) - region.x;
    let range = (max - min) as usize;
    min + (range * local_x / (region.width - 1)) as i32
}

/// 色 ホイール 値 for position を有効範囲へ補正して返す。
fn color_wheel_value_for_position(
    region: &PanelHitRegion,
    point: PanelSurfacePoint,
    hue_degrees: usize,
    saturation: usize,
    value: usize,
) -> String {
    let local_x = point.x.saturating_sub(region.x) as f32;
    let local_y = point.y.saturating_sub(region.y) as f32;
    let size = region.width.min(region.height).max(1) as f32;
    let center = (size - 1.0) * 0.5;
    let dx = local_x - center;
    let dy = local_y - center;
    let distance = (dx * dx + dy * dy).sqrt();
    let outer_radius = center.max(1.0);
    let inner_radius = outer_radius * 0.72;
    let square_half = inner_radius * 0.7;

    let (mut next_hue, mut next_saturation, mut next_value) = (hue_degrees, saturation, value);
    if distance >= inner_radius && distance <= outer_radius {
        let angle = dy.atan2(dx).to_degrees().rem_euclid(360.0);
        next_hue = angle.round() as usize % 360;
    } else {
        let square_min_x = center - square_half;
        let square_max_x = center + square_half;
        let square_min_y = center - square_half;
        let square_max_y = center + square_half;
        if local_x >= square_min_x
            && local_x <= square_max_x
            && local_y >= square_min_y
            && local_y <= square_max_y
        {
            next_saturation = (((local_x - square_min_x) / (square_max_x - square_min_x)) * 100.0)
                .round()
                .clamp(0.0, 100.0) as usize;
            next_value = ((1.0 - (local_y - square_min_y) / (square_max_y - square_min_y)) * 100.0)
                .round()
                .clamp(0.0, 100.0) as usize;
        }
    }

    format!("{next_hue},{next_saturation},{next_value}")
}
