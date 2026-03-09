use std::collections::BTreeMap;

use plugin_api::PanelEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PanelHitKind {
    Activate,
    Slider { min: usize, max: usize },
    LayerListItem { value: usize },
    DropdownOption { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PanelHitRegion {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) panel_id: String,
    pub(crate) node_id: String,
    pub(crate) kind: PanelHitKind,
}

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

#[derive(Clone, Copy)]
pub(crate) struct PanelRenderState<'a> {
    pub(crate) focused_target: Option<&'a FocusTarget>,
    pub(crate) expanded_dropdown: Option<&'a FocusTarget>,
    pub(crate) text_input_states: &'a BTreeMap<(String, String), TextInputEditorState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSurface {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub(crate) hit_regions: Vec<PanelHitRegion>,
}

impl PanelSurface {
    pub fn hit_test(&self, x: usize, y: usize) -> Option<PanelEvent> {
        self.hit_regions
            .iter()
            .rev()
            .find(|region| {
                x >= region.x
                    && y >= region.y
                    && x < region.x + region.width
                    && y < region.y + region.height
            })
            .map(|region| panel_event_for_region(region, x, y))
    }

    pub fn drag_event(
        &self,
        panel_id: &str,
        node_id: &str,
        source_value: usize,
        x: usize,
        y: usize,
    ) -> Option<PanelEvent> {
        let source_region = self
            .hit_regions
            .iter()
            .rev()
            .find(|region| {
                region.panel_id == panel_id
                    && region.node_id == node_id
                    && (matches!(&region.kind, PanelHitKind::Slider { .. })
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
                .map(|region| panel_event_for_region(region, x, y)),
            PanelHitKind::LayerListItem { .. } => self
                .hit_regions
                .iter()
                .rev()
                .find(|region| {
                    region.panel_id == panel_id
                        && region.node_id == node_id
                        && x >= region.x
                        && y >= region.y
                        && x < region.x + region.width
                        && y < region.y + region.height
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
            PanelHitKind::Activate | PanelHitKind::DropdownOption { .. } => None,
        }
    }
}

fn panel_event_for_region(region: &PanelHitRegion, x: usize, y: usize) -> PanelEvent {
    match &region.kind {
        PanelHitKind::Activate => PanelEvent::Activate {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
        },
        PanelHitKind::Slider { min, max } => PanelEvent::SetValue {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: slider_value_for_position(region, *min, *max, x, y),
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
    }
}

fn slider_value_for_position(
    region: &PanelHitRegion,
    min: usize,
    max: usize,
    x: usize,
    _y: usize,
) -> usize {
    if max <= min || region.width <= 1 {
        return min;
    }

    let local_x = x.clamp(region.x, region.x + region.width - 1) - region.x;
    min + ((max - min) * local_x) / (region.width - 1)
}
