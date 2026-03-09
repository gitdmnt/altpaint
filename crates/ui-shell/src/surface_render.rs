//! `UiShell` の software panel rendering をまとめる。
//!
//! panel tree から panel surface を組み立てる測定・描画ロジックを集約し、
//! runtime 管理層から presentation 実装を分離する。

use super::focus::{insert_text_at_char_index, prefix_for_char_count, text_char_len};
use super::*;

const SIDEBAR_BACKGROUND: [u8; 4] = [0x2a, 0x2a, 0x2a, 0xff];
const PANEL_BACKGROUND: [u8; 4] = [0x1f, 0x1f, 0x1f, 0xff];
const PANEL_BORDER: [u8; 4] = [0x3f, 0x3f, 0x3f, 0xff];
const PANEL_TITLE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
const SECTION_TITLE: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
const BODY_TEXT: [u8; 4] = [0xd8, 0xd8, 0xd8, 0xff];
const BUTTON_FILL: [u8; 4] = [0x32, 0x32, 0x32, 0xff];
const BUTTON_ACTIVE_FILL: [u8; 4] = [0x44, 0x5f, 0xb0, 0xff];
const BUTTON_BORDER: [u8; 4] = [0x56, 0x56, 0x56, 0xff];
const BUTTON_ACTIVE_BORDER: [u8; 4] = [0xc6, 0xd4, 0xff, 0xff];
const BUTTON_FOCUS_BORDER: [u8; 4] = [0x9f, 0xb7, 0xff, 0xff];
const BUTTON_TEXT: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const BUTTON_TEXT_DARK: [u8; 4] = [0x14, 0x14, 0x14, 0xff];
const SLIDER_TRACK_BACKGROUND: [u8; 4] = [0x2c, 0x2c, 0x2c, 0xff];
const SLIDER_TRACK_BORDER: [u8; 4] = [0x5f, 0x5f, 0x5f, 0xff];
const SLIDER_KNOB: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const PREVIEW_SWATCH_BORDER: [u8; 4] = [0x74, 0x74, 0x74, 0xff];
const PANEL_OUTER_PADDING: usize = 8;
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
const INPUT_BACKGROUND: [u8; 4] = [0x15, 0x15, 0x15, 0xff];
const INPUT_BORDER: [u8; 4] = [0x56, 0x56, 0x56, 0xff];
const INPUT_PLACEHOLDER: [u8; 4] = [0x88, 0x88, 0x88, 0xff];
pub(super) const PANEL_SCROLL_PIXELS_PER_LINE: i32 = 48;

impl UiShell {
    /// 現在の panel trees から viewport 向け panel surface を構築する。
    pub fn render_panel_surface(&mut self, width: usize, height: usize) -> PanelSurface {
        let width = width.max(1);
        let height = height.max(1);
        let panel_width = width.saturating_sub(PANEL_OUTER_PADDING * 2);
        let needs_rebuild = self.panel_content_dirty
            || self.panel_content_cache.as_ref().is_none_or(|content| content.width != width);
        if needs_rebuild {
            self.panel_content_cache = Some(self.build_panel_content_surface(width, panel_width));
            self.panel_content_dirty = false;
        }

        self.panel_content_height = self.panel_content_cache.as_ref().map(|content| content.height).unwrap_or(0);
        self.panel_scroll_offset = self.panel_scroll_offset.min(self.max_panel_scroll_offset(height));

        viewport_panel_surface(
            self.panel_content_cache.as_ref().expect("panel content cache exists"),
            height,
            self.panel_scroll_offset,
        )
    }

    /// panel tree 全体をスクロール前提の content surface へ描画する。
    fn build_panel_content_surface(&mut self, width: usize, panel_width: usize) -> PanelSurface {
        let trees = self.panel_trees();
        let focused_target = self.focused_target.clone();
        let expanded_dropdown = self.expanded_dropdown.clone();
        let text_input_states = self.text_input_states.clone();
        let render_state = PanelRenderState {
            focused_target: focused_target.as_ref(),
            expanded_dropdown: expanded_dropdown.as_ref(),
            text_input_states: &text_input_states,
        };
        self.panel_content_height = measure_panel_content_height(&trees, panel_width, render_state);

        let content_height = self.panel_content_height.max(1);
        let mut content = PanelSurface {
            width,
            height: content_height,
            pixels: vec![0; width * content_height * 4],
            hit_regions: Vec::new(),
        };
        fill_rect(&mut content, 0, 0, width, content_height, SIDEBAR_BACKGROUND);

        let mut cursor_y = PANEL_OUTER_PADDING;
        for tree in trees {
            let panel_height = measure_panel_tree(&tree, panel_width, render_state);
            fill_rect(&mut content, PANEL_OUTER_PADDING, cursor_y, panel_width, panel_height, PANEL_BACKGROUND);
            stroke_rect(&mut content, PANEL_OUTER_PADDING, cursor_y, panel_width, panel_height, PANEL_BORDER);
            draw_panel_tree(&mut content, &tree, PANEL_OUTER_PADDING, cursor_y, panel_width, render_state);
            cursor_y += panel_height + PANEL_OUTER_PADDING;
        }

        content
    }

    /// 現在の content height と viewport height から最大スクロール量を返す。
    pub(super) fn max_panel_scroll_offset(&self, viewport_height: usize) -> usize {
        self.panel_content_height.saturating_sub(viewport_height)
    }
}

fn viewport_panel_surface(content: &PanelSurface, height: usize, scroll_offset: usize) -> PanelSurface {
    if scroll_offset == 0 && content.height == height {
        return content.clone();
    }

    let mut surface = PanelSurface {
        width: content.width,
        height,
        pixels: vec![0; content.width * height * 4],
        hit_regions: Vec::new(),
    };
    fill_rect(&mut surface, 0, 0, content.width, height, SIDEBAR_BACKGROUND);

    let start_row = scroll_offset.min(content.height.saturating_sub(1));
    let visible_rows = height.min(content.height.saturating_sub(start_row));
    let row_bytes = content.width * 4;
    for row in 0..visible_rows {
        let src_start = (start_row + row) * row_bytes;
        let dst_start = row * row_bytes;
        surface.pixels[dst_start..dst_start + row_bytes]
            .copy_from_slice(&content.pixels[src_start..src_start + row_bytes]);
    }

    for region in &content.hit_regions {
        let region_bottom = region.y + region.height;
        if region_bottom <= scroll_offset || region.y >= scroll_offset + height {
            continue;
        }
        let top = region.y.saturating_sub(scroll_offset);
        let bottom = (region_bottom.saturating_sub(scroll_offset)).min(height);
        if bottom <= top {
            continue;
        }
        surface.hit_regions.push(PanelHitRegion {
            x: region.x,
            y: top,
            width: region.width,
            height: bottom - top,
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            kind: region.kind.clone(),
        });
    }

    surface
}

fn measure_panel_content_height(trees: &[PanelTree], width: usize, render_state: PanelRenderState<'_>) -> usize {
    if trees.is_empty() {
        return PANEL_OUTER_PADDING * 2;
    }

    let panels_height: usize = trees
        .iter()
        .map(|tree| measure_panel_tree(tree, width, render_state) + PANEL_OUTER_PADDING)
        .sum();
    PANEL_OUTER_PADDING + panels_height
}

fn measure_panel_tree(tree: &PanelTree, width: usize, render_state: PanelRenderState<'_>) -> usize {
    let title_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = measure_text(tree.title, title_width);
    let mut content_height = 0;
    for (index, child) in tree.children.iter().enumerate() {
        content_height += measure_node(child, tree.id, title_width, render_state);
        if index + 1 != tree.children.len() {
            content_height += NODE_GAP;
        }
    }

    PANEL_INNER_PADDING * 2 + title_height + 6 + content_height
}

fn measure_node(node: &PanelNode, panel_id: &str, available_width: usize, render_state: PanelRenderState<'_>) -> usize {
    match node {
        PanelNode::Column { children, .. } => children
            .iter()
            .enumerate()
            .map(|(index, child)| measure_node(child, panel_id, available_width, render_state) + usize::from(index + 1 != children.len()) * NODE_GAP)
            .sum(),
        PanelNode::Row { children, .. } => {
            let width_per_child = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1)) / children.len()
            };
            children.iter().map(|child| measure_node(child, panel_id, width_per_child, render_state)).max().unwrap_or(0)
        }
        PanelNode::Section { children, title, .. } => {
            let title_height = measure_text(title, available_width);
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut children_height = 0;
            for (index, child) in children.iter().enumerate() {
                children_height += measure_node(child, panel_id, child_width, render_state);
                if index + 1 != children.len() {
                    children_height += SECTION_GAP;
                }
            }
            title_height + SECTION_GAP + children_height
        }
        PanelNode::Text { text, .. } => measure_text(text, available_width),
        PanelNode::ColorPreview { .. } => COLOR_PREVIEW_HEIGHT,
        PanelNode::ColorWheel { label, .. } => {
            let label_height = if label.is_empty() { 0 } else { measure_text(label, available_width) + 4 };
            label_height + COLOR_WHEEL_SIZE.min(available_width.max(96))
        }
        PanelNode::Button { .. } => BUTTON_HEIGHT,
        PanelNode::Slider { .. } => SLIDER_HEIGHT,
        PanelNode::TextInput { label, .. } => {
            let label_height = if label.is_empty() { 0 } else { measure_text(label, available_width) + 4 };
            label_height + INPUT_BOX_HEIGHT
        }
        PanelNode::Dropdown { id, options, .. } => {
            let mut height = DROPDOWN_HEIGHT;
            if render_state.expanded_dropdown.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str()) {
                height += options.len() * DROPDOWN_HEIGHT;
            }
            height
        }
        PanelNode::LayerList { label, items, .. } => {
            let label_height = if label.is_empty() { 0 } else { measure_text(label, available_width) + 4 };
            label_height + items.len().max(1) * LAYER_LIST_ITEM_HEIGHT
        }
    }
}

fn draw_panel_tree(surface: &mut PanelSurface, tree: &PanelTree, x: usize, y: usize, width: usize, render_state: PanelRenderState<'_>) {
    let inner_x = x + PANEL_INNER_PADDING;
    let inner_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = draw_wrapped_text(surface, inner_x, y + PANEL_INNER_PADDING, tree.title, PANEL_TITLE, inner_width);
    let mut cursor_y = y + PANEL_INNER_PADDING + title_height + 6;

    for child in &tree.children {
        let used = draw_node(surface, child, tree.id, inner_x, cursor_y, inner_width, render_state);
        cursor_y += used + NODE_GAP;
    }
}

fn draw_node(surface: &mut PanelSurface, node: &PanelNode, panel_id: &str, x: usize, y: usize, available_width: usize, render_state: PanelRenderState<'_>) -> usize {
    match node {
        PanelNode::Column { children, .. } => {
            let mut cursor_y = y;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(surface, child, panel_id, x, cursor_y, available_width, render_state);
                if index + 1 != children.len() {
                    cursor_y += NODE_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Row { children, .. } => {
            let child_width = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1)) / children.len()
            };
            let mut cursor_x = x;
            let mut max_height = 0;
            for child in children {
                let used = draw_node(surface, child, panel_id, cursor_x, y, child_width, render_state);
                max_height = max_height.max(used);
                cursor_x += child_width + NODE_GAP;
            }
            max_height
        }
        PanelNode::Section { title, children, .. } => {
            let title_height = draw_wrapped_text(surface, x, y, title, SECTION_TITLE, available_width);
            let child_x = x + SECTION_INDENT;
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut cursor_y = y + title_height + SECTION_GAP;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(surface, child, panel_id, child_x, cursor_y, child_width, render_state);
                if index + 1 != children.len() {
                    cursor_y += SECTION_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Text { text, .. } => draw_wrapped_text(surface, x, y, text, BODY_TEXT, available_width),
        PanelNode::ColorPreview { label, color, .. } => {
            let label_height = draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width);
            let swatch_y = y + label_height + 4;
            let swatch_height = COLOR_PREVIEW_HEIGHT.saturating_sub(label_height + 4).max(12);
            fill_rect(surface, x, swatch_y, available_width, swatch_height, color.to_rgba8());
            stroke_rect(surface, x, swatch_y, available_width, swatch_height, PREVIEW_SWATCH_BORDER);
            COLOR_PREVIEW_HEIGHT
        }
        PanelNode::ColorWheel { id, label, hue_degrees, saturation, value, .. } => {
            let label_height = if label.is_empty() {
                0
            } else {
                draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4
            };
            let wheel_size = COLOR_WHEEL_SIZE.min(available_width.max(96));
            let wheel_x = x + available_width.saturating_sub(wheel_size) / 2;
            let wheel_y = y + label_height;
            draw_color_wheel(surface, wheel_x, wheel_y, wheel_size, *hue_degrees, *saturation, *value);
            surface.hit_regions.push(PanelHitRegion {
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
            });
            label_height + wheel_size
        }
        PanelNode::Button { id, label, active, fill_color, .. } => {
            let fill = fill_color.map_or(if *active { BUTTON_ACTIVE_FILL } else { BUTTON_FILL }, ColorRgba8::to_rgba8);
            let is_focused = render_state.focused_target.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            fill_rect(surface, x, y, available_width, BUTTON_HEIGHT, fill);
            stroke_rect(surface, x, y, available_width, BUTTON_HEIGHT, if *active { BUTTON_ACTIVE_BORDER } else { BUTTON_BORDER });
            if is_focused && available_width > 2 && BUTTON_HEIGHT > 2 {
                stroke_rect(surface, x + 1, y + 1, available_width - 2, BUTTON_HEIGHT - 2, BUTTON_FOCUS_BORDER);
            }
            draw_wrapped_text(surface, x + 6, y + 7, label, button_text_color(*fill_color), available_width.saturating_sub(12));
            surface.hit_regions.push(PanelHitRegion { x, y, width: available_width, height: BUTTON_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::Activate });
            BUTTON_HEIGHT
        }
        PanelNode::Slider { id, label, min, max, value, fill_color, .. } => {
            let clamped_value = (*value).clamp(*min, *max);
            let accent = fill_color.unwrap_or(ColorRgba8::new(0x9f, 0xb7, 0xff, 0xff));
            let track_y = y + SLIDER_TRACK_TOP;
            let track_width = available_width.max(1);
            let track_inner_width = track_width.saturating_sub(2);
            let range = max.saturating_sub(*min).max(1);
            let progress = clamped_value.saturating_sub(*min);
            let fill_width = if track_inner_width == 0 { 0 } else { ((progress * track_inner_width) / range).max(1) };
            let knob_offset = if track_inner_width <= 1 { 0 } else { (progress * (track_inner_width - 1)) / range };
            let knob_x = (x + 1 + knob_offset).saturating_sub(SLIDER_KNOB_WIDTH / 2).min(x + track_width.saturating_sub(SLIDER_KNOB_WIDTH.min(track_width)));
            draw_wrapped_text(surface, x, y, &format!("{label}: {clamped_value}"), BODY_TEXT, available_width);
            fill_rect(surface, x, track_y, track_width, SLIDER_TRACK_HEIGHT, SLIDER_TRACK_BACKGROUND);
            stroke_rect(surface, x, track_y, track_width, SLIDER_TRACK_HEIGHT, SLIDER_TRACK_BORDER);
            if fill_width > 0 {
                fill_rect(surface, x + 1, track_y + 1, fill_width.min(track_inner_width), SLIDER_TRACK_HEIGHT.saturating_sub(2).max(1), accent.to_rgba8());
            }
            fill_rect(surface, knob_x, track_y.saturating_sub(3), SLIDER_KNOB_WIDTH.min(track_width), SLIDER_TRACK_HEIGHT + 6, SLIDER_KNOB);
            stroke_rect(surface, knob_x, track_y.saturating_sub(3), SLIDER_KNOB_WIDTH.min(track_width), SLIDER_TRACK_HEIGHT + 6, SLIDER_TRACK_BORDER);
            surface.hit_regions.push(PanelHitRegion { x, y, width: track_width, height: SLIDER_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::Slider { min: *min, max: *max } });
            SLIDER_HEIGHT
        }
        PanelNode::Dropdown { id, label, value, options, .. } => {
            let is_focused = render_state.focused_target.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let is_expanded = render_state.expanded_dropdown.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let selected_label = options.iter().find(|option| option.value == *value).map(|option| option.label.as_str()).unwrap_or(value.as_str());
            let button_label = if label.is_empty() { format!("{selected_label} ▾") } else { format!("{label}: {selected_label} ▾") };
            fill_rect(surface, x, y, available_width, DROPDOWN_HEIGHT, BUTTON_FILL);
            stroke_rect(surface, x, y, available_width, DROPDOWN_HEIGHT, if is_expanded { BUTTON_ACTIVE_BORDER } else { BUTTON_BORDER });
            if is_focused && available_width > 2 && DROPDOWN_HEIGHT > 2 {
                stroke_rect(surface, x + 1, y + 1, available_width - 2, DROPDOWN_HEIGHT - 2, BUTTON_FOCUS_BORDER);
            }
            draw_wrapped_text(surface, x + 6, y + 7, &button_label, BUTTON_TEXT, available_width.saturating_sub(12));
            surface.hit_regions.push(PanelHitRegion { x, y, width: available_width, height: DROPDOWN_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::Activate });
            if !is_expanded { return DROPDOWN_HEIGHT; }
            let mut cursor_y = y + DROPDOWN_HEIGHT;
            for option in options {
                let active = option.value == *value;
                fill_rect(surface, x, cursor_y, available_width, DROPDOWN_HEIGHT, if active { BUTTON_ACTIVE_FILL } else { PANEL_BACKGROUND });
                stroke_rect(surface, x, cursor_y, available_width, DROPDOWN_HEIGHT, BUTTON_BORDER);
                draw_wrapped_text(surface, x + 6, cursor_y + 7, &option.label, if active { BUTTON_TEXT } else { BODY_TEXT }, available_width.saturating_sub(12));
                surface.hit_regions.push(PanelHitRegion { x, y: cursor_y, width: available_width, height: DROPDOWN_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::DropdownOption { value: option.value.clone() } });
                cursor_y += DROPDOWN_HEIGHT;
            }
            DROPDOWN_HEIGHT + options.len() * DROPDOWN_HEIGHT
        }
        PanelNode::LayerList { id, label, selected_index, items, .. } => {
            let is_focused = render_state.focused_target.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let label_height = if label.is_empty() { 0 } else { draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4 };
            let mut cursor_y = y + label_height;
            let item_count = items.len().max(1);
            for index in 0..item_count {
                let item = items.get(index).cloned().unwrap_or(LayerListItem { label: "<no layers>".to_string(), detail: String::new() });
                let active = *selected_index == index;
                fill_rect(surface, x, cursor_y, available_width, LAYER_LIST_ITEM_HEIGHT, if active { BUTTON_ACTIVE_FILL } else { BUTTON_FILL });
                stroke_rect(surface, x, cursor_y, available_width, LAYER_LIST_ITEM_HEIGHT, if active { BUTTON_ACTIVE_BORDER } else { BUTTON_BORDER });
                if is_focused && active && available_width > 2 && LAYER_LIST_ITEM_HEIGHT > 2 {
                    stroke_rect(surface, x + 1, cursor_y + 1, available_width - 2, LAYER_LIST_ITEM_HEIGHT - 2, BUTTON_FOCUS_BORDER);
                }
                draw_text_rgba(&mut surface.pixels, surface.width, surface.height, x + 6, cursor_y + 6, &item.label, BUTTON_TEXT);
                if !item.detail.is_empty() {
                    draw_text_rgba(&mut surface.pixels, surface.width, surface.height, x + 6, cursor_y + LAYER_LIST_DETAIL_OFFSET, &item.detail, BODY_TEXT);
                }
                let grip_x = x + available_width.saturating_sub(LAYER_LIST_DRAG_HANDLE_WIDTH);
                for offset in [8usize, 14, 20] {
                    fill_rect(surface, grip_x, cursor_y + offset, 8, 1, BODY_TEXT);
                }
                surface.hit_regions.push(PanelHitRegion { x, y: cursor_y, width: available_width, height: LAYER_LIST_ITEM_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::LayerListItem { value: index } });
                cursor_y += LAYER_LIST_ITEM_HEIGHT;
            }
            label_height + item_count * LAYER_LIST_ITEM_HEIGHT
        }
        PanelNode::TextInput { id, label, value, placeholder, binding_path: _, .. } => {
            let is_focused = render_state.focused_target.is_some_and(|target| target.panel_id == panel_id && target.node_id == id.as_str());
            let editor_state = render_state.text_input_states.get(&(panel_id.to_string(), id.clone())).cloned().unwrap_or(TextInputEditorState { cursor_chars: text_char_len(value), preedit: None });
            let label_height = if label.is_empty() { 0 } else { draw_wrapped_text(surface, x, y, label, BODY_TEXT, available_width) + 4 };
            let box_y = y + label_height;
            fill_rect(surface, x, box_y, available_width, INPUT_BOX_HEIGHT, INPUT_BACKGROUND);
            stroke_rect(surface, x, box_y, available_width, INPUT_BOX_HEIGHT, INPUT_BORDER);
            if is_focused && available_width > 2 && INPUT_BOX_HEIGHT > 2 {
                stroke_rect(surface, x + 1, box_y + 1, available_width - 2, INPUT_BOX_HEIGHT - 2, BUTTON_FOCUS_BORDER);
            }
            let display_text = if let Some(preedit) = editor_state.preedit.as_deref() { insert_text_at_char_index(value, editor_state.cursor_chars, preedit) } else { value.clone() };
            let text_to_draw = if display_text.is_empty() { placeholder.clone() } else { display_text.clone() };
            draw_text_rgba(&mut surface.pixels, surface.width, surface.height, x + 6, box_y + 7, &text_to_draw, if display_text.is_empty() { INPUT_PLACEHOLDER } else { BUTTON_TEXT });
            if is_focused {
                let caret_char_index = editor_state.cursor_chars + editor_state.preedit.as_deref().map(text_char_len).unwrap_or(0);
                let caret_prefix = prefix_for_char_count(&display_text, caret_char_index);
                let caret_x = (x + 6 + measure_text_width(&caret_prefix)).min(x + available_width.saturating_sub(3));
                fill_rect(surface, caret_x, box_y + 4, 1, INPUT_BOX_HEIGHT.saturating_sub(8).max(1), BUTTON_FOCUS_BORDER);
            }
            surface.hit_regions.push(PanelHitRegion { x, y: box_y, width: available_width, height: INPUT_BOX_HEIGHT, panel_id: panel_id.to_string(), node_id: id.clone(), kind: PanelHitKind::Activate });
            label_height + INPUT_BOX_HEIGHT
        }
    }
}

fn draw_color_wheel(
    surface: &mut PanelSurface,
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
    let selector_x = x + (center + selector_angle.cos() * selector_radius).round().max(0.0) as usize;
    let selector_y = y + (center + selector_angle.sin() * selector_radius).round().max(0.0) as usize;
    stroke_rect(surface, selector_x.saturating_sub(2), selector_y.saturating_sub(2), 5, 5, BUTTON_FOCUS_BORDER);

    let sv_x = x + (center - square_half + (square_half * 2.0) * (saturation as f32 / 100.0)).round().max(0.0) as usize;
    let sv_y = y + (center - square_half + (square_half * 2.0) * (1.0 - value as f32 / 100.0)).round().max(0.0) as usize;
    stroke_rect(surface, sv_x.saturating_sub(2), sv_y.saturating_sub(2), 5, 5, BUTTON_FOCUS_BORDER);
}

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

fn button_text_color(fill_color: Option<ColorRgba8>) -> [u8; 4] {
    let Some(fill_color) = fill_color else { return BUTTON_TEXT; };
    let luminance = 0.2126 * f32::from(fill_color.r) + 0.7152 * f32::from(fill_color.g) + 0.0722 * f32::from(fill_color.b);
    if luminance >= 140.0 { BUTTON_TEXT_DARK } else { BUTTON_TEXT }
}

fn measure_text(text: &str, available_width: usize) -> usize {
    let lines = wrap_text(text, available_width);
    lines.len().max(1) * text_line_height()
}

fn draw_wrapped_text(surface: &mut PanelSurface, x: usize, y: usize, text: &str, color: [u8; 4], available_width: usize) -> usize {
    let lines = wrap_text(text, available_width);
    for (index, line) in lines.iter().enumerate() {
        draw_text_line(surface, x, y + index * text_line_height(), line, color);
    }
    lines.len().max(1) * text_line_height()
}

fn wrap_text(text: &str, available_width: usize) -> Vec<String> {
    wrap_text_lines(text, available_width)
}

fn draw_text_line(surface: &mut PanelSurface, x: usize, y: usize, text: &str, color: [u8; 4]) {
    draw_text_rgba(surface.pixels.as_mut_slice(), surface.width, surface.height, x, y, text, color);
}

fn fill_rect(surface: &mut PanelSurface, x: usize, y: usize, width: usize, height: usize, color: [u8; 4]) {
    let max_x = (x + width).min(surface.width);
    let max_y = (y + height).min(surface.height);
    for yy in y..max_y {
        for xx in x..max_x {
            write_pixel(surface, xx, yy, color);
        }
    }
}

fn stroke_rect(surface: &mut PanelSurface, x: usize, y: usize, width: usize, height: usize, color: [u8; 4]) {
    if width == 0 || height == 0 { return; }
    fill_rect(surface, x, y, width, 1, color);
    fill_rect(surface, x, y + height.saturating_sub(1), width, 1, color);
    fill_rect(surface, x, y, 1, height, color);
    fill_rect(surface, x + width.saturating_sub(1), y, 1, height, color);
}

fn write_pixel(surface: &mut PanelSurface, x: usize, y: usize, color: [u8; 4]) {
    if x >= surface.width || y >= surface.height { return; }
    let index = (y * surface.width + x) * 4;
    surface.pixels[index..index + 4].copy_from_slice(&color);
}
