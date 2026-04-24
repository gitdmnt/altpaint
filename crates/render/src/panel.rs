use std::collections::BTreeMap;

use app_core::ColorRgba8;
use panel_api::{LayerListItem, PanelNode, PanelTree};

use crate::PixelRect;
use crate::text::{
    draw_text_rgba, line_height as text_line_height, measure_text_width, wrap_text_lines,
};

const PANEL_BACKGROUND: [u8; 4] = [0x18, 0x1c, 0x24, 0xf6];
const PANEL_BORDER: [u8; 4] = [0x43, 0x4c, 0x5d, 0xff];
const PANEL_TITLE_BAR: [u8; 4] = [0x20, 0x28, 0x35, 0xff];
const PANEL_TITLE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
const SECTION_TITLE: [u8; 4] = [0x9d, 0xc7, 0xff, 0xff];
const BODY_TEXT: [u8; 4] = [0xd9, 0xe2, 0xf1, 0xff];
const BUTTON_FILL: [u8; 4] = [0x27, 0x30, 0x3d, 0xff];
const BUTTON_ACTIVE_FILL: [u8; 4] = [0x3b, 0x6b, 0xbd, 0xff];
const BUTTON_BORDER: [u8; 4] = [0x4d, 0x5a, 0x70, 0xff];
const BUTTON_ACTIVE_BORDER: [u8; 4] = [0xd6, 0xe4, 0xff, 0xff];
const BUTTON_FOCUS_BORDER: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
const BUTTON_TEXT: [u8; 4] = [0xf3, 0xf7, 0xff, 0xff];
const BUTTON_TEXT_DARK: [u8; 4] = [0x14, 0x14, 0x14, 0xff];
const SLIDER_TRACK_BACKGROUND: [u8; 4] = [0x1d, 0x25, 0x30, 0xff];
const SLIDER_TRACK_BORDER: [u8; 4] = [0x56, 0x69, 0x87, 0xff];
const SLIDER_KNOB: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const PREVIEW_SWATCH_BORDER: [u8; 4] = [0x74, 0x74, 0x74, 0xff];
const INPUT_BACKGROUND: [u8; 4] = [0x11, 0x17, 0x21, 0xff];
const INPUT_BORDER: [u8; 4] = [0x4d, 0x5a, 0x70, 0xff];
const INPUT_PLACEHOLDER: [u8; 4] = [0x88, 0x88, 0x88, 0xff];
const PANEL_INNER_PADDING: usize = 8;
const NODE_GAP: usize = 6;
const SECTION_GAP: usize = 4;
const SECTION_INDENT: usize = 10;
const BUTTON_HEIGHT: usize = 24;
const COLOR_PREVIEW_HEIGHT: usize = 52;
const COLOR_WHEEL_SIZE: usize = 160;
const INPUT_BOX_HEIGHT: usize = 24;
const SLIDER_HEIGHT: usize = 32;
const SLIDER_TRACK_HEIGHT: usize = 8;
const SLIDER_TRACK_TOP: usize = 20;
const SLIDER_KNOB_WIDTH: usize = 8;
const DROPDOWN_HEIGHT: usize = 24;
const LAYER_LIST_ITEM_HEIGHT: usize = 38;
const LAYER_LIST_DETAIL_OFFSET: usize = 18;
const LAYER_LIST_DRAG_HANDLE_WIDTH: usize = 14;
const TITLE_BAR_HEIGHT: usize = 28;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHitKind {
    MovePanel,
    Activate,
    Slider {
        min: i32,
        max: i32,
    },
    ColorWheel {
        hue_degrees: usize,
        saturation: usize,
        value: usize,
    },
    LayerListItem {
        value: i32,
    },
    DropdownOption {
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelHitRegion {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub panel_id: String,
    pub node_id: String,
    pub kind: PanelHitKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelFocusTarget<'a> {
    pub panel_id: &'a str,
    pub node_id: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelTextInputState<'a> {
    pub panel_id: &'a str,
    pub node_id: &'a str,
    pub cursor_chars: usize,
    pub preedit: Option<&'a str>,
}

#[derive(Clone, Copy, Default)]
pub struct PanelRenderState<'a> {
    pub focused_target: Option<PanelFocusTarget<'a>>,
    pub expanded_dropdown: Option<PanelFocusTarget<'a>>,
    pub text_input_states: &'a [PanelTextInputState<'a>],
}

#[derive(Debug, Clone, Copy)]
pub struct FloatingPanel<'a> {
    pub panel_id: &'a str,
    pub title: &'a str,
    pub rect: PixelRect,
    pub tree: &'a PanelTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterizedPanelLayer {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub hit_regions: Vec<PanelHitRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasuredPanelSize {
    pub width: usize,
    pub height: usize,
}

/// 現在の値を パネル レイヤー へ変換する。
pub fn rasterize_panel_layer(
    viewport: PixelRect,
    panels: &[FloatingPanel<'_>],
    render_state: PanelRenderState<'_>,
) -> RasterizedPanelLayer {
    let content_bounds = panel_content_bounds(viewport, panels).unwrap_or(PixelRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    });
    let width = content_bounds.width.max(1);
    let height = content_bounds.height.max(1);
    let mut layer = RasterizedPanelLayer {
        x: content_bounds.x,
        y: content_bounds.y,
        width,
        height,
        pixels: vec![0; width * height * 4],
        hit_regions: Vec::new(),
    };

    let text_input_states = render_state
        .text_input_states
        .iter()
        .map(|state| {
            (
                (state.panel_id.to_string(), state.node_id.to_string()),
                TextInputEditorState {
                    cursor_chars: state.cursor_chars,
                    preedit: state.preedit.map(ToOwned::to_owned),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let internal_render_state = InternalPanelRenderState {
        focused_target: render_state.focused_target.map(|target| FocusTarget {
            panel_id: target.panel_id.to_string(),
            node_id: target.node_id.to_string(),
        }),
        expanded_dropdown: render_state.expanded_dropdown.map(|target| FocusTarget {
            panel_id: target.panel_id.to_string(),
            node_id: target.node_id.to_string(),
        }),
        text_input_states,
    };

    for panel in panels {
        let Some(rect) = panel.rect.intersect(viewport) else {
            continue;
        };
        let translated = FloatingPanel {
            panel_id: panel.panel_id,
            title: panel.title,
            rect: PixelRect {
                x: rect.x.saturating_sub(content_bounds.x),
                y: rect.y.saturating_sub(content_bounds.y),
                width: rect.width,
                height: rect.height,
            },
            tree: panel.tree,
        };
        draw_panel_window(&mut layer, translated, &internal_render_state);
    }

    layer
}

/// 現在の measure パネル サイズ を返す。
pub fn measure_panel_size(
    title: &str,
    tree: &PanelTree,
    render_state: PanelRenderState<'_>,
    max_width: usize,
    max_height: usize,
) -> MeasuredPanelSize {
    let max_width = max_width.max(PANEL_INNER_PADDING * 2 + 32);
    let max_height = max_height.max(TITLE_BAR_HEIGHT + PANEL_INNER_PADDING * 2 + 16);
    let preferred_inner_width = tree
        .children
        .iter()
        .map(preferred_node_width)
        .max()
        .unwrap_or(0)
        .max(measure_text_width(title) + PANEL_INNER_PADDING * 2)
        .max(96);
    let width = preferred_inner_width
        .saturating_add(PANEL_INNER_PADDING * 2)
        .clamp(PANEL_INNER_PADDING * 2 + 32, max_width);
    let inner_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let height = TITLE_BAR_HEIGHT
        + PANEL_INNER_PADDING * 2
        + measure_children_height(tree.children.as_slice(), inner_width, render_state).clamp(
            16,
            max_height.saturating_sub(TITLE_BAR_HEIGHT + PANEL_INNER_PADDING * 2),
        );

    MeasuredPanelSize { width, height }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FocusTarget {
    panel_id: String,
    node_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TextInputEditorState {
    cursor_chars: usize,
    preedit: Option<String>,
}

struct InternalPanelRenderState {
    focused_target: Option<FocusTarget>,
    expanded_dropdown: Option<FocusTarget>,
    text_input_states: BTreeMap<(String, String), TextInputEditorState>,
}

/// 現在の値を content 範囲 へ変換する。
///
/// 値を生成できない場合は `None` を返します。
fn panel_content_bounds(viewport: PixelRect, panels: &[FloatingPanel<'_>]) -> Option<PixelRect> {
    panels
        .iter()
        .filter_map(|panel| panel.rect.intersect(viewport))
        .reduce(|acc, rect| acc.union(rect))
}

/// 現在の値を パネル ウィンドウ へ変換する。
fn draw_panel_window(
    surface: &mut RasterizedPanelLayer,
    panel: FloatingPanel<'_>,
    render_state: &InternalPanelRenderState,
) {
    if panel.rect.width < 32 || panel.rect.height < TITLE_BAR_HEIGHT + 16 {
        return;
    }

    fill_rect(
        surface,
        panel.rect.x,
        panel.rect.y,
        panel.rect.width,
        panel.rect.height,
        PANEL_BACKGROUND,
    );
    stroke_rect(
        surface,
        panel.rect.x,
        panel.rect.y,
        panel.rect.width,
        panel.rect.height,
        PANEL_BORDER,
    );
    fill_rect(
        surface,
        panel.rect.x,
        panel.rect.y,
        panel.rect.width,
        TITLE_BAR_HEIGHT.min(panel.rect.height),
        PANEL_TITLE_BAR,
    );
    draw_text_line(
        surface,
        panel.rect.x + PANEL_INNER_PADDING,
        panel.rect.y + 7,
        panel.title,
        PANEL_TITLE,
    );
    push_hit_region_clipped(
        surface,
        PanelHitRegion {
            x: panel.rect.x,
            y: panel.rect.y,
            width: panel.rect.width,
            height: TITLE_BAR_HEIGHT.min(panel.rect.height),
            panel_id: panel.panel_id.to_string(),
            node_id: String::new(),
            kind: PanelHitKind::MovePanel,
        },
    );

    let inner_x = panel.rect.x + PANEL_INNER_PADDING;
    let inner_y = panel.rect.y + TITLE_BAR_HEIGHT + PANEL_INNER_PADDING;
    let inner_width = panel.rect.width.saturating_sub(PANEL_INNER_PADDING * 2);
    let max_bottom = panel.rect.y + panel.rect.height.saturating_sub(PANEL_INNER_PADDING);
    let mut cursor_y = inner_y;
    for child in &panel.tree.children {
        let used = draw_node(
            surface,
            child,
            panel.panel_id,
            inner_x,
            cursor_y,
            inner_width,
            max_bottom,
            render_state,
        );
        cursor_y += used + NODE_GAP;
        if cursor_y >= max_bottom {
            break;
        }
    }
}

/// 現在の値を node へ変換する。
#[allow(clippy::too_many_arguments)]
fn draw_node(
    surface: &mut RasterizedPanelLayer,
    node: &PanelNode,
    panel_id: &str,
    x: usize,
    y: usize,
    available_width: usize,
    max_bottom: usize,
    render_state: &InternalPanelRenderState,
) -> usize {
    if y >= max_bottom || available_width == 0 {
        return 0;
    }

    match node {
        PanelNode::Column { children, .. } => {
            let mut cursor_y = y;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(
                    surface,
                    child,
                    panel_id,
                    x,
                    cursor_y,
                    available_width,
                    max_bottom,
                    render_state,
                );
                if index + 1 != children.len() {
                    cursor_y += NODE_GAP;
                }
                if cursor_y >= max_bottom {
                    break;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Row { children, .. } => {
            let child_width = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1))
                    / children.len()
            };
            let mut cursor_x = x;
            let mut max_height = 0;
            for child in children {
                let used = draw_node(
                    surface,
                    child,
                    panel_id,
                    cursor_x,
                    y,
                    child_width,
                    max_bottom,
                    render_state,
                );
                max_height = max_height.max(used);
                cursor_x += child_width + NODE_GAP;
            }
            max_height
        }
        PanelNode::Section {
            title, children, ..
        } => {
            let title_height =
                draw_wrapped_text(surface, x, y, title, SECTION_TITLE, available_width);
            let child_x = x + SECTION_INDENT;
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut cursor_y = y + title_height + SECTION_GAP;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(
                    surface,
                    child,
                    panel_id,
                    child_x,
                    cursor_y,
                    child_width,
                    max_bottom,
                    render_state,
                );
                if index + 1 != children.len() {
                    cursor_y += SECTION_GAP;
                }
                if cursor_y >= max_bottom {
                    break;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Text { text, .. } => {
            draw_wrapped_text(surface, x, y, text, BODY_TEXT, available_width)
        }
        PanelNode::ColorPreview { label, color, .. } => {
            let label_height = draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width);
            let swatch_y = y + label_height + 4;
            let swatch_height = COLOR_PREVIEW_HEIGHT
                .saturating_sub(label_height + 4)
                .max(12);
            fill_rect(
                surface,
                x,
                swatch_y,
                available_width,
                swatch_height,
                color.to_rgba8(),
            );
            stroke_rect(
                surface,
                x,
                swatch_y,
                available_width,
                swatch_height,
                PREVIEW_SWATCH_BORDER,
            );
            COLOR_PREVIEW_HEIGHT
        }
        PanelNode::ColorWheel {
            id,
            label,
            hue_degrees,
            saturation,
            value,
            ..
        } => {
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let wheel_size = COLOR_WHEEL_SIZE.min(available_width.max(96));
            let wheel_x = x + available_width.saturating_sub(wheel_size) / 2;
            let wheel_y = y + label_height;
            draw_color_wheel(
                surface,
                wheel_x,
                wheel_y,
                wheel_size,
                *hue_degrees,
                *saturation,
                *value,
            );
            push_hit_region_clipped(
                surface,
                PanelHitRegion {
                    x: wheel_x,
                    y: wheel_y,
                    width: wheel_size,
                    height: wheel_size,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::ColorWheel {
                        hue_degrees: *hue_degrees,
                        saturation: *saturation,
                        value: *value,
                    },
                },
            );
            label_height + wheel_size
        }
        PanelNode::Button {
            id,
            label,
            active,
            fill_color,
            ..
        } => {
            let fill = fill_color.map_or(
                if *active {
                    BUTTON_ACTIVE_FILL
                } else {
                    BUTTON_FILL
                },
                ColorRgba8::to_rgba8,
            );
            let is_focused = render_state
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            fill_rect(surface, x, y, available_width, BUTTON_HEIGHT, fill);
            stroke_rect(
                surface,
                x,
                y,
                available_width,
                BUTTON_HEIGHT,
                if *active {
                    BUTTON_ACTIVE_BORDER
                } else {
                    BUTTON_BORDER
                },
            );
            if is_focused && available_width > 2 && BUTTON_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    y + 1,
                    available_width - 2,
                    BUTTON_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            draw_wrapped_text(
                surface,
                x + 6,
                y + 7,
                label,
                button_text_color(*fill_color),
                available_width.saturating_sub(12),
            );
            push_hit_region_clipped(
                surface,
                PanelHitRegion {
                    x,
                    y,
                    width: available_width,
                    height: BUTTON_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::Activate,
                },
            );
            BUTTON_HEIGHT
        }
        PanelNode::Slider {
            id,
            label,
            min,
            max,
            value,
            display_value,
            fill_color,
            ..
        } => {
            let clamped_value = (*value).clamp(*min, *max);
            let shown_value = display_value.unwrap_or(clamped_value);
            let accent = fill_color.unwrap_or(ColorRgba8::new(0x9f, 0xb7, 0xff, 0xff));
            let track_y = y + SLIDER_TRACK_TOP;
            let track_width = available_width.max(1);
            let track_inner_width = track_width.saturating_sub(2);
            let range = (max - min).max(1) as usize;
            let progress = (clamped_value - min) as usize;
            let fill_width = if track_inner_width == 0 {
                0
            } else {
                ((progress * track_inner_width) / range).max(1)
            };
            let knob_offset = if track_inner_width <= 1 {
                0
            } else {
                (progress * (track_inner_width - 1)) / range
            };
            let knob_x = (x + 1 + knob_offset)
                .saturating_sub(SLIDER_KNOB_WIDTH / 2)
                .min(x + track_width.saturating_sub(SLIDER_KNOB_WIDTH.min(track_width)));
            draw_wrapped_text(
                surface,
                x,
                y,
                &format!("{label}: {shown_value}"),
                BODY_TEXT,
                available_width,
            );
            fill_rect(
                surface,
                x,
                track_y,
                track_width,
                SLIDER_TRACK_HEIGHT,
                SLIDER_TRACK_BACKGROUND,
            );
            stroke_rect(
                surface,
                x,
                track_y,
                track_width,
                SLIDER_TRACK_HEIGHT,
                SLIDER_TRACK_BORDER,
            );
            if fill_width > 0 {
                fill_rect(
                    surface,
                    x + 1,
                    track_y + 1,
                    fill_width.min(track_inner_width),
                    SLIDER_TRACK_HEIGHT.saturating_sub(2).max(1),
                    accent.to_rgba8(),
                );
            }
            fill_rect(
                surface,
                knob_x,
                track_y.saturating_sub(3),
                SLIDER_KNOB_WIDTH.min(track_width),
                SLIDER_TRACK_HEIGHT + 6,
                SLIDER_KNOB,
            );
            stroke_rect(
                surface,
                knob_x,
                track_y.saturating_sub(3),
                SLIDER_KNOB_WIDTH.min(track_width),
                SLIDER_TRACK_HEIGHT + 6,
                SLIDER_TRACK_BORDER,
            );
            push_hit_region_clipped(
                surface,
                PanelHitRegion {
                    x,
                    y,
                    width: track_width,
                    height: SLIDER_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::Slider {
                        min: *min,
                        max: *max,
                    }, // min/max は i32
                },
            );
            SLIDER_HEIGHT
        }
        PanelNode::Dropdown {
            id,
            label,
            value,
            options,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let is_expanded = render_state
                .expanded_dropdown
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let selected_label = options
                .iter()
                .find(|option| option.value == *value)
                .map(|option| option.label.as_str())
                .unwrap_or(value.as_str());
            let button_label = if label.is_empty() {
                selected_label.to_string()
            } else {
                format!("{label}: {selected_label}")
            };
            fill_rect(surface, x, y, available_width, DROPDOWN_HEIGHT, BUTTON_FILL);
            stroke_rect(
                surface,
                x,
                y,
                available_width,
                DROPDOWN_HEIGHT,
                if is_expanded {
                    BUTTON_ACTIVE_BORDER
                } else {
                    BUTTON_BORDER
                },
            );
            if is_focused && available_width > 2 && DROPDOWN_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    y + 1,
                    available_width - 2,
                    DROPDOWN_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            let arrow = "▾";
            let arrow_width = measure_text_width(arrow);
            draw_wrapped_text(
                surface,
                x + 6,
                y + 7,
                &button_label,
                BUTTON_TEXT,
                available_width.saturating_sub(arrow_width + 18),
            );
            draw_text_line(
                surface,
                x + available_width.saturating_sub(arrow_width + 6),
                y + 7,
                arrow,
                if is_expanded {
                    SECTION_TITLE
                } else {
                    BUTTON_TEXT
                },
            );
            push_hit_region_clipped(
                surface,
                PanelHitRegion {
                    x,
                    y,
                    width: available_width,
                    height: DROPDOWN_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::Activate,
                },
            );
            if !is_expanded {
                return DROPDOWN_HEIGHT;
            }
            let mut cursor_y = y + DROPDOWN_HEIGHT;
            for option in options {
                let active = option.value == *value;
                fill_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    DROPDOWN_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_FILL
                    } else {
                        PANEL_BACKGROUND
                    },
                );
                stroke_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    DROPDOWN_HEIGHT,
                    BUTTON_BORDER,
                );
                draw_wrapped_text(
                    surface,
                    x + 6,
                    cursor_y + 7,
                    &option.label,
                    if active { BUTTON_TEXT } else { BODY_TEXT },
                    available_width.saturating_sub(12),
                );
                push_hit_region_clipped(
                    surface,
                    PanelHitRegion {
                        x,
                        y: cursor_y,
                        width: available_width,
                        height: DROPDOWN_HEIGHT,
                        panel_id: panel_id.to_string(),
                        node_id: id.clone(),
                        kind: PanelHitKind::DropdownOption {
                            value: option.value.clone(),
                        },
                    },
                );
                cursor_y += DROPDOWN_HEIGHT;
            }
            DROPDOWN_HEIGHT + options.len() * DROPDOWN_HEIGHT
        }
        PanelNode::LayerList {
            id,
            label,
            selected_index,
            items,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let mut cursor_y = y + label_height;
            let item_count = items.len().max(1);
            for index in 0..item_count {
                let item = items.get(index).cloned().unwrap_or(LayerListItem {
                    label: "<no layers>".to_string(),
                    detail: String::new(),
                });
                let active = *selected_index == index;
                fill_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    LAYER_LIST_ITEM_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_FILL
                    } else {
                        BUTTON_FILL
                    },
                );
                stroke_rect(
                    surface,
                    x,
                    cursor_y,
                    available_width,
                    LAYER_LIST_ITEM_HEIGHT,
                    if active {
                        BUTTON_ACTIVE_BORDER
                    } else {
                        BUTTON_BORDER
                    },
                );
                if is_focused && active && available_width > 2 && LAYER_LIST_ITEM_HEIGHT > 2 {
                    stroke_rect(
                        surface,
                        x + 1,
                        cursor_y + 1,
                        available_width - 2,
                        LAYER_LIST_ITEM_HEIGHT - 2,
                        BUTTON_FOCUS_BORDER,
                    );
                }
                draw_text_rgba(
                    &mut surface.pixels,
                    surface.width,
                    surface.height,
                    x + 6,
                    cursor_y + 6,
                    &item.label,
                    BUTTON_TEXT,
                );
                if !item.detail.is_empty() {
                    draw_text_rgba(
                        &mut surface.pixels,
                        surface.width,
                        surface.height,
                        x + 6,
                        cursor_y + LAYER_LIST_DETAIL_OFFSET,
                        &item.detail,
                        BODY_TEXT,
                    );
                }
                let grip_x = x + available_width.saturating_sub(LAYER_LIST_DRAG_HANDLE_WIDTH);
                draw_text_line(
                    surface,
                    x + 6,
                    cursor_y + 20,
                    &format!("{:02}", index + 1),
                    SECTION_TITLE,
                );
                for offset in [6usize, 12, 18] {
                    for column in [0usize, 5] {
                        fill_rect(surface, grip_x + column, cursor_y + offset, 2, 2, BODY_TEXT);
                    }
                }
                push_hit_region_clipped(
                    surface,
                    PanelHitRegion {
                        x,
                        y: cursor_y,
                        width: available_width,
                        height: LAYER_LIST_ITEM_HEIGHT,
                        panel_id: panel_id.to_string(),
                        node_id: id.clone(),
                        kind: PanelHitKind::LayerListItem { value: index as i32 },
                    },
                );
                cursor_y += LAYER_LIST_ITEM_HEIGHT;
            }
            label_height + item_count * LAYER_LIST_ITEM_HEIGHT
        }
        PanelNode::TextInput {
            id,
            label,
            value,
            placeholder,
            ..
        } => {
            let is_focused = render_state
                .focused_target
                .as_ref()
                .is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let editor_state = render_state
                .text_input_states
                .get(&(panel_id.to_string(), id.clone()))
                .cloned()
                .unwrap_or(TextInputEditorState {
                    cursor_chars: text_char_len(value),
                    preedit: None,
                });
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let box_y = y + label_height;
            fill_rect(
                surface,
                x,
                box_y,
                available_width,
                INPUT_BOX_HEIGHT,
                INPUT_BACKGROUND,
            );
            stroke_rect(
                surface,
                x,
                box_y,
                available_width,
                INPUT_BOX_HEIGHT,
                INPUT_BORDER,
            );
            if is_focused && available_width > 2 && INPUT_BOX_HEIGHT > 2 {
                stroke_rect(
                    surface,
                    x + 1,
                    box_y + 1,
                    available_width - 2,
                    INPUT_BOX_HEIGHT - 2,
                    BUTTON_FOCUS_BORDER,
                );
            }
            let display_text = if let Some(preedit) = editor_state.preedit.as_deref() {
                insert_text_at_char_index(value, editor_state.cursor_chars, preedit)
            } else {
                value.clone()
            };
            let text_to_draw = if display_text.is_empty() {
                placeholder.clone()
            } else {
                display_text.clone()
            };
            draw_text_rgba(
                &mut surface.pixels,
                surface.width,
                surface.height,
                x + 6,
                box_y + 7,
                &text_to_draw,
                if display_text.is_empty() {
                    INPUT_PLACEHOLDER
                } else {
                    BUTTON_TEXT
                },
            );
            if is_focused {
                let caret_char_index = editor_state.cursor_chars
                    + editor_state
                        .preedit
                        .as_deref()
                        .map(text_char_len)
                        .unwrap_or(0);
                let caret_prefix = prefix_for_char_count(&display_text, caret_char_index);
                let caret_x = (x + 6 + measure_text_width(&caret_prefix))
                    .min(x + available_width.saturating_sub(3));
                fill_rect(
                    surface,
                    caret_x,
                    box_y + 4,
                    1,
                    INPUT_BOX_HEIGHT.saturating_sub(8).max(1),
                    BUTTON_FOCUS_BORDER,
                );
            }
            push_hit_region_clipped(
                surface,
                PanelHitRegion {
                    x,
                    y: box_y,
                    width: available_width,
                    height: INPUT_BOX_HEIGHT,
                    panel_id: panel_id.to_string(),
                    node_id: id.clone(),
                    kind: PanelHitKind::Activate,
                },
            );
            label_height + INPUT_BOX_HEIGHT
        }
    }
}

/// push hit 領域 clipped に必要な処理を行う。
fn push_hit_region_clipped(surface: &mut RasterizedPanelLayer, region: PanelHitRegion) {
    let viewport = PixelRect {
        x: 0,
        y: 0,
        width: surface.width,
        height: surface.height,
    };
    let Some(clipped) = viewport.intersect(PixelRect {
        x: region.x,
        y: region.y,
        width: region.width,
        height: region.height,
    }) else {
        return;
    };
    surface.hit_regions.push(PanelHitRegion {
        x: clipped.x,
        y: clipped.y,
        width: clipped.width,
        height: clipped.height,
        panel_id: region.panel_id,
        node_id: region.node_id,
        kind: region.kind,
    });
}

/// 描画 色 ホイール に必要な描画内容を組み立てる。
fn draw_color_wheel(
    surface: &mut RasterizedPanelLayer,
    x: usize,
    y: usize,
    size: usize,
    hue_degrees: usize,
    saturation: usize,
    value: usize,
) {
    let size = size.max(1);
    let center = (size as f32 - 1.0) * 0.5;
    let outer_radius = center.max(1.0);
    let inner_radius = outer_radius * 0.72;
    let square_half = inner_radius * 0.7;

    for local_y in 0..size {
        for local_x in 0..size {
            let dx = local_x as f32 - center;
            let dy = local_y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let pixel = if distance >= inner_radius && distance <= outer_radius {
                let hue = dy.atan2(dx).to_degrees().rem_euclid(360.0) as usize;
                hsv_to_rgba(hue, 100, 100)
            } else if dx.abs() <= square_half && dy.abs() <= square_half {
                let local_saturation = (((dx + square_half) / (square_half * 2.0)) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as usize;
                let local_value = ((1.0 - (dy + square_half) / (square_half * 2.0)) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as usize;
                hsv_to_rgba(hue_degrees, local_saturation, local_value)
            } else {
                continue;
            };
            fill_rect(surface, x + local_x, y + local_y, 1, 1, pixel);
        }
    }

    let selector_hue = hue_degrees % 360;
    let selector_angle = (selector_hue as f32).to_radians();
    let selector_radius = (inner_radius + outer_radius) * 0.5;
    let selector_x = x
        + (center + selector_angle.cos() * selector_radius)
            .round()
            .max(0.0) as usize;
    let selector_y = y
        + (center + selector_angle.sin() * selector_radius)
            .round()
            .max(0.0) as usize;
    stroke_rect(
        surface,
        selector_x.saturating_sub(2),
        selector_y.saturating_sub(2),
        5,
        5,
        BUTTON_FOCUS_BORDER,
    );

    let sv_x = x
        + (center - square_half + (square_half * 2.0) * (saturation as f32 / 100.0))
            .round()
            .max(0.0) as usize;
    let sv_y = y
        + (center - square_half + (square_half * 2.0) * (1.0 - value as f32 / 100.0))
            .round()
            .max(0.0) as usize;
    stroke_rect(
        surface,
        sv_x.saturating_sub(2),
        sv_y.saturating_sub(2),
        5,
        5,
        BUTTON_FOCUS_BORDER,
    );
}

/// 入力や種別に応じて処理を振り分ける。
fn hsv_to_rgba(hue_degrees: usize, saturation: usize, value: usize) -> [u8; 4] {
    let h = (hue_degrees % 360) as f32;
    let s = (saturation.min(100) as f32) / 100.0;
    let v = (value.min(100) as f32) / 100.0;
    if s <= f32::EPSILON {
        let gray = (v * 255.0).round() as u8;
        return [gray, gray, gray, 0xff];
    }

    let sector = (h / 60.0).floor();
    let fraction = h / 60.0 - sector;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * fraction);
    let t = v * (1.0 - s * (1.0 - fraction));
    let (r, g, b) = match sector as i32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };

    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        0xff,
    ]
}

/// button テキスト 色 を計算して返す。
///
/// 値を生成できない場合は `None` を返します。
fn button_text_color(fill_color: Option<ColorRgba8>) -> [u8; 4] {
    let Some(fill_color) = fill_color else {
        return BUTTON_TEXT;
    };
    let luminance = 0.2126 * f32::from(fill_color.r)
        + 0.7152 * f32::from(fill_color.g)
        + 0.0722 * f32::from(fill_color.b);
    if luminance >= 140.0 {
        BUTTON_TEXT_DARK
    } else {
        BUTTON_TEXT
    }
}

/// 推奨される node 幅 を返す。
fn preferred_node_width(node: &PanelNode) -> usize {
    match node {
        PanelNode::Column { children, .. } => {
            children.iter().map(preferred_node_width).max().unwrap_or(0)
        }
        PanelNode::Row { children, .. } => {
            let child_sum = children.iter().map(preferred_node_width).sum::<usize>();
            child_sum + NODE_GAP * children.len().saturating_sub(1)
        }
        PanelNode::Section {
            title, children, ..
        } => {
            let child_width = children.iter().map(preferred_node_width).max().unwrap_or(0);
            (child_width + SECTION_INDENT).max(measure_text_width(title))
        }
        PanelNode::Text { text, .. } => measure_text_width(text).max(96),
        PanelNode::ColorPreview { label, .. } => measure_text_width(label).max(140),
        PanelNode::ColorWheel { label, .. } => measure_text_width(label).max(COLOR_WHEEL_SIZE),
        PanelNode::Button { label, .. } => (measure_text_width(label) + 12).max(96),
        PanelNode::Slider { label, max, .. } => {
            let sample = format!("{label}: {max}");
            (measure_text_width(&sample) + 12).max(160)
        }
        PanelNode::Dropdown {
            label,
            value,
            options,
            ..
        } => {
            let selected_label = options
                .iter()
                .find(|option| option.value == *value)
                .map(|option| option.label.as_str())
                .unwrap_or(value.as_str());
            let button_label = if label.is_empty() {
                selected_label.to_string()
            } else {
                format!("{label}: {selected_label}")
            };
            let expanded_height_width = options
                .iter()
                .map(|option| measure_text_width(option.label.as_str()) + 12)
                .max()
                .unwrap_or(0);
            (measure_text_width(&button_label) + 12)
                .max(expanded_height_width)
                .max(120)
        }
        PanelNode::LayerList { label, items, .. } => {
            let items_width = items
                .iter()
                .map(|item| {
                    measure_text_width(item.label.as_str())
                        .max(measure_text_width(item.detail.as_str()))
                        + 18
                        + LAYER_LIST_DRAG_HANDLE_WIDTH
                })
                .max()
                .unwrap_or(120);
            items_width.max(measure_text_width(label))
        }
        PanelNode::TextInput {
            label,
            value,
            placeholder,
            ..
        } => {
            measure_text_width(label)
                .max(measure_text_width(value))
                .max(measure_text_width(placeholder))
                .max(96)
                + 12
        }
    }
}

/// 現在の measure children 高さ を返す。
fn measure_children_height(
    children: &[PanelNode],
    available_width: usize,
    render_state: PanelRenderState<'_>,
) -> usize {
    if children.is_empty() {
        return 0;
    }

    children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            measure_node_height(child, available_width, render_state)
                + if index + 1 != children.len() {
                    NODE_GAP
                } else {
                    0
                }
        })
        .sum()
}

/// 入力や種別に応じて処理を振り分ける。
fn measure_node_height(
    node: &PanelNode,
    available_width: usize,
    render_state: PanelRenderState<'_>,
) -> usize {
    match node {
        PanelNode::Column { children, .. } => {
            measure_children_height(children.as_slice(), available_width, render_state)
        }
        PanelNode::Row { children, .. } => {
            let child_width = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1))
                    / children.len().max(1)
            };
            children
                .iter()
                .map(|child| measure_node_height(child, child_width, render_state))
                .max()
                .unwrap_or(0)
        }
        PanelNode::Section {
            title, children, ..
        } => {
            let title_height = wrap_text(title, available_width).len().max(1) * text_line_height();
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            title_height
                + SECTION_GAP
                + children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| {
                        measure_node_height(child, child_width, render_state)
                            + if index + 1 != children.len() {
                                SECTION_GAP
                            } else {
                                0
                            }
                    })
                    .sum::<usize>()
        }
        PanelNode::Text { text, .. } => {
            wrap_text(text, available_width).len().max(1) * text_line_height()
        }
        PanelNode::ColorPreview { label, .. } => {
            wrap_text(label, available_width).len().max(1) * text_line_height()
                + 4
                + COLOR_PREVIEW_HEIGHT.saturating_sub(text_line_height() + 4)
        }
        PanelNode::ColorWheel { label, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                wrap_text(label, available_width).len().max(1) * text_line_height() + 4
            };
            label_height + COLOR_WHEEL_SIZE.min(available_width.max(96))
        }
        PanelNode::Button { .. } => BUTTON_HEIGHT,
        PanelNode::Slider { .. } => SLIDER_HEIGHT,
        PanelNode::Dropdown { options, id, .. } => {
            let expanded = render_state
                .expanded_dropdown
                .is_some_and(|target| target.node_id == id.as_str());
            DROPDOWN_HEIGHT
                + if expanded {
                    options.len() * DROPDOWN_HEIGHT
                } else {
                    0
                }
        }
        PanelNode::LayerList { label, items, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                wrap_text(label, available_width).len().max(1) * text_line_height() + 4
            };
            label_height + items.len().max(1) * LAYER_LIST_ITEM_HEIGHT
        }
        PanelNode::TextInput { label, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                wrap_text(label, available_width).len().max(1) * text_line_height() + 4
            };
            label_height + INPUT_BOX_HEIGHT
        }
    }
}

/// 描画 wrapped テキスト に必要な描画内容を組み立てる。
fn draw_wrapped_text(
    surface: &mut RasterizedPanelLayer,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
    available_width: usize,
) -> usize {
    let lines = wrap_text(text, available_width);
    for (index, line) in lines.iter().enumerate() {
        draw_text_line(surface, x, y + index * text_line_height(), line, color);
    }
    lines.len().max(1) * text_line_height()
}

/// 折り返し テキスト を計算して返す。
fn wrap_text(text: &str, available_width: usize) -> Vec<String> {
    wrap_text_lines(text, available_width)
}

/// 描画 テキスト line に必要な描画内容を組み立てる。
fn draw_text_line(
    surface: &mut RasterizedPanelLayer,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
) {
    draw_text_rgba(
        surface.pixels.as_mut_slice(),
        surface.width,
        surface.height,
        x,
        y,
        text,
        color,
    );
}

/// 塗りつぶし 矩形 に必要な描画内容を組み立てる。
fn fill_rect(
    surface: &mut RasterizedPanelLayer,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    let max_x = (x + width).min(surface.width);
    let max_y = (y + height).min(surface.height);
    for yy in y..max_y {
        for xx in x..max_x {
            write_pixel(surface, xx, yy, color);
        }
    }
}

/// ストローク 矩形 に必要な描画内容を組み立てる。
fn stroke_rect(
    surface: &mut RasterizedPanelLayer,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    if width == 0 || height == 0 {
        return;
    }
    fill_rect(surface, x, y, width, 1, color);
    fill_rect(surface, x, y + height.saturating_sub(1), width, 1, color);
    fill_rect(surface, x, y, 1, height, color);
    fill_rect(surface, x + width.saturating_sub(1), y, 1, height, color);
}

/// ピクセル を保存先へ書き出す。
fn write_pixel(surface: &mut RasterizedPanelLayer, x: usize, y: usize, color: [u8; 4]) {
    if x >= surface.width || y >= surface.height {
        return;
    }
    let index = (y * surface.width + x) * 4;
    surface.pixels[index..index + 4].copy_from_slice(&color);
}

/// テキスト char len を計算して返す。
fn text_char_len(text: &str) -> usize {
    text.chars().count()
}

/// 現在の byte インデックス for char インデックス を返す。
fn byte_index_for_char_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

/// 現在の insert テキスト at char インデックス を返す。
fn insert_text_at_char_index(text: &str, char_index: usize, inserted: &str) -> String {
    let split_at = byte_index_for_char_index(text, char_index);
    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..split_at]);
    next.push_str(inserted);
    next.push_str(&text[split_at..]);
    next
}

/// 現在の prefix for char 件数 を返す。
fn prefix_for_char_count(text: &str, char_count: usize) -> String {
    text.chars().take(char_count).collect()
}

#[cfg(test)]
mod tests {
    use app_core::Command;
    use panel_api::{HostAction, PanelNode, PanelTree};

    use super::*;

    /// rasterized パネル レイヤー contains move handle 領域 が期待どおりに動作することを検証する。
    #[test]
    fn rasterized_panel_layer_contains_move_handle_region() {
        let tree = PanelTree {
            id: "test.panel",
            title: "Test Panel",
            children: vec![PanelNode::Button {
                id: "button.ok".to_string(),
                label: "OK".to_string(),
                action: HostAction::DispatchCommand(Command::SaveProject),
                active: false,
                fill_color: None,
            }],
        };
        let layer = rasterize_panel_layer(
            PixelRect {
                x: 0,
                y: 0,
                width: 320,
                height: 240,
            },
            &[FloatingPanel {
                panel_id: tree.id,
                title: tree.title,
                rect: PixelRect {
                    x: 24,
                    y: 32,
                    width: 200,
                    height: 140,
                },
                tree: &tree,
            }],
            PanelRenderState::default(),
        );

        assert_eq!(layer.x, 24);
        assert_eq!(layer.y, 32);
        assert!(layer.hit_regions.iter().any(|region| {
            region.panel_id == "test.panel" && matches!(region.kind, PanelHitKind::MovePanel)
        }));
    }
}
