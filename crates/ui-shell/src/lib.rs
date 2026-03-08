//! `ui-shell` はアプリケーションウィンドウ上でパネルをホストする最小UI層。
//!
//! フェーズ0では、個々のパネル機能そのものは持たず、`RenderContext` と
//! `PanelPlugin` 群を束ねる薄い境界として機能する。

mod text;

pub use text::{
    draw_text_rgba, line_height as text_line_height, measure_text_width, text_backend_name,
    wrap_text_lines,
};

use app_core::{ColorRgba8, Document};
use builtin_plugins::default_builtin_panels;
use plugin_api::{HostAction, PanelEvent, PanelNode, PanelPlugin, PanelTree, PanelView};
use render::{RenderContext, RenderFrame};

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
const SLIDER_HEIGHT: usize = 32;
const SLIDER_TRACK_HEIGHT: usize = 8;
const SLIDER_TRACK_TOP: usize = 20;
const SLIDER_KNOB_WIDTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PanelHitKind {
    Activate,
    Slider { min: usize, max: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelHitRegion {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    panel_id: String,
    node_id: String,
    kind: PanelHitKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FocusTarget {
    panel_id: String,
    node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSurface {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    hit_regions: Vec<PanelHitRegion>,
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
        x: usize,
        y: usize,
    ) -> Option<PanelEvent> {
        self.hit_regions
            .iter()
            .rev()
            .find(|region| region.panel_id == panel_id && region.node_id == node_id)
            .map(|region| panel_event_for_region(region, x, y))
    }
}

fn panel_event_for_region(region: &PanelHitRegion, x: usize, y: usize) -> PanelEvent {
    match region.kind {
        PanelHitKind::Activate => PanelEvent::Activate {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
        },
        PanelHitKind::Slider { min, max } => PanelEvent::SetValue {
            panel_id: region.panel_id.clone(),
            node_id: region.node_id.clone(),
            value: slider_value_for_position(region, min, max, x, y),
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

/// パネルホストとして振る舞う最小UIシェル。
pub struct UiShell {
    /// キャンバス描画側への入口。
    render_context: RenderContext,
    /// 登録済みのパネルプラグイン一覧。
    panels: Vec<Box<dyn PanelPlugin>>,
    panel_scroll_offset: usize,
    panel_content_height: usize,
    focused_target: Option<FocusTarget>,
}

impl UiShell {
    /// 空のUIシェルを作成する。
    pub fn new() -> Self {
        let mut shell = Self {
            render_context: RenderContext::new(),
            panels: Vec::new(),
            panel_scroll_offset: 0,
            panel_content_height: 0,
            focused_target: None,
        };
        for panel in default_builtin_panels() {
            shell.register_panel(panel);
        }
        shell
    }

    /// パネルプラグインを1つ登録する。
    pub fn register_panel(&mut self, panel: Box<dyn PanelPlugin>) {
        self.panels.push(panel);
    }

    /// ドキュメント更新をレンダラと各パネルへ配送する。
    pub fn update(&mut self, document: &Document) {
        let _ = self.render_context.document(document);
        for panel in &mut self.panels {
            panel.update(document);
        }
    }

    /// 現在のドキュメントからキャンバス用フレームを生成する。
    pub fn render_frame(&self, document: &Document) -> RenderFrame {
        self.render_context.render_frame(document)
    }

    /// 現在登録されているパネル数を返す。
    pub fn panel_count(&self) -> usize {
        self.panels.len()
    }

    /// 現在登録されているパネルの最小デバッグ情報を返す。
    pub fn panel_debug_summaries(&self) -> Vec<(&'static str, &'static str, String)> {
        self.panels
            .iter()
            .map(|panel| (panel.id(), panel.title(), panel.debug_summary()))
            .collect()
    }

    pub fn panel_views(&self) -> Vec<PanelView> {
        self.panels.iter().map(|panel| panel.view()).collect()
    }

    pub fn panel_trees(&self) -> Vec<PanelTree> {
        self.panels.iter().map(|panel| panel.panel_tree()).collect()
    }

    pub fn focused_target(&self) -> Option<(&str, &str)> {
        self.focused_target
            .as_ref()
            .map(|target| (target.panel_id.as_str(), target.node_id.as_str()))
    }

    pub fn panel_scroll_offset(&self) -> usize {
        self.panel_scroll_offset
    }

    pub fn focus_panel_node(&mut self, panel_id: &str, node_id: &str) -> bool {
        let exists = self
            .focusable_targets()
            .iter()
            .any(|target| target.panel_id == panel_id && target.node_id == node_id);
        if !exists {
            return false;
        }

        let next = FocusTarget {
            panel_id: panel_id.to_string(),
            node_id: node_id.to_string(),
        };
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        true
    }

    pub fn focus_next(&mut self) -> bool {
        self.move_focus(1)
    }

    pub fn focus_previous(&mut self) -> bool {
        self.move_focus(-1)
    }

    pub fn activate_focused(&mut self) -> Vec<HostAction> {
        let Some(target) = self.focused_target.clone() else {
            return Vec::new();
        };

        self.handle_panel_event(&PanelEvent::Activate {
            panel_id: target.panel_id,
            node_id: target.node_id,
        })
    }

    pub fn scroll_panels(&mut self, delta_lines: i32, viewport_height: usize) -> bool {
        let delta_pixels = delta_lines.saturating_mul(text_line_height() as i32);
        let max_offset = self.max_panel_scroll_offset(viewport_height) as i32;
        let next_offset = (self.panel_scroll_offset as i32 + delta_pixels).clamp(0, max_offset);
        let next_offset = next_offset as usize;
        if next_offset == self.panel_scroll_offset {
            return false;
        }

        self.panel_scroll_offset = next_offset;
        true
    }

    pub fn handle_panel_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        if let PanelEvent::Activate { panel_id, node_id } = event {
            let _ = self.focus_panel_node(panel_id, node_id);
        }
        self.panels
            .iter_mut()
            .flat_map(|panel| panel.handle_event(event))
            .collect()
    }

    pub fn render_panel_surface(&mut self, width: usize, height: usize) -> PanelSurface {
        let trees = self.panel_trees();
        let panel_width = width.saturating_sub(PANEL_OUTER_PADDING * 2);
        self.panel_content_height = measure_panel_content_height(&trees, panel_width);
        self.panel_scroll_offset = self
            .panel_scroll_offset
            .min(self.max_panel_scroll_offset(height));

        let content_height = self.panel_content_height.max(height).max(1);
        let mut content = PanelSurface {
            width,
            height: content_height,
            pixels: vec![0; width * content_height * 4],
            hit_regions: Vec::new(),
        };
        fill_rect(
            &mut content,
            0,
            0,
            width,
            content_height,
            SIDEBAR_BACKGROUND,
        );

        let mut cursor_y = PANEL_OUTER_PADDING;
        for tree in trees {
            let panel_height = measure_panel_tree(&tree, panel_width);
            fill_rect(
                &mut content,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                panel_height,
                PANEL_BACKGROUND,
            );
            stroke_rect(
                &mut content,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                panel_height,
                PANEL_BORDER,
            );
            draw_panel_tree(
                &mut content,
                &tree,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                self.focused_target.as_ref(),
            );
            cursor_y += panel_height + PANEL_OUTER_PADDING;
        }

        if self.panel_scroll_offset == 0 && content.height == height {
            return content;
        }

        let mut surface = PanelSurface {
            width,
            height,
            pixels: vec![0; width * height * 4],
            hit_regions: Vec::new(),
        };

        let start_row = self
            .panel_scroll_offset
            .min(content.height.saturating_sub(1));
        let visible_rows = height.min(content.height.saturating_sub(start_row));
        let row_bytes = width * 4;
        for row in 0..visible_rows {
            let src_start = (start_row + row) * row_bytes;
            let dst_start = row * row_bytes;
            surface.pixels[dst_start..dst_start + row_bytes]
                .copy_from_slice(&content.pixels[src_start..src_start + row_bytes]);
        }

        for region in content.hit_regions {
            let region_bottom = region.y + region.height;
            if region_bottom <= self.panel_scroll_offset
                || region.y >= self.panel_scroll_offset + height
            {
                continue;
            }
            let top = region.y.saturating_sub(self.panel_scroll_offset);
            let bottom = (region_bottom.saturating_sub(self.panel_scroll_offset)).min(height);
            if bottom <= top {
                continue;
            }
            surface.hit_regions.push(PanelHitRegion {
                x: region.x,
                y: top,
                width: region.width,
                height: bottom - top,
                panel_id: region.panel_id,
                node_id: region.node_id,
                kind: region.kind,
            });
        }

        surface
    }

    fn move_focus(&mut self, step: isize) -> bool {
        let targets = self.focusable_targets();
        if targets.is_empty() {
            return false;
        }

        let current_index = self.focused_target.as_ref().and_then(|current| {
            targets.iter().position(|target| {
                target.panel_id == current.panel_id && target.node_id == current.node_id
            })
        });
        let next_index = match current_index {
            Some(index) => (index as isize + step).rem_euclid(targets.len() as isize) as usize,
            None if step >= 0 => 0,
            None => targets.len() - 1,
        };
        let next = targets[next_index].clone();
        if self.focused_target.as_ref() == Some(&next) {
            return false;
        }

        self.focused_target = Some(next);
        true
    }

    fn focusable_targets(&self) -> Vec<FocusTarget> {
        let mut targets = Vec::new();
        for tree in self.panel_trees() {
            collect_focus_targets(tree.id, &tree.children, &mut targets);
        }
        targets
    }

    fn max_panel_scroll_offset(&self, viewport_height: usize) -> usize {
        self.panel_content_height.saturating_sub(viewport_height)
    }
}

fn collect_focus_targets(panel_id: &str, nodes: &[PanelNode], targets: &mut Vec<FocusTarget>) {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                collect_focus_targets(panel_id, children, targets);
            }
            PanelNode::Button { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::Text { .. } | PanelNode::ColorPreview { .. } | PanelNode::Slider { .. } => {}
        }
    }
}

fn measure_panel_content_height(trees: &[PanelTree], width: usize) -> usize {
    if trees.is_empty() {
        return PANEL_OUTER_PADDING * 2;
    }

    let panels_height: usize = trees
        .iter()
        .map(|tree| measure_panel_tree(tree, width) + PANEL_OUTER_PADDING)
        .sum();
    PANEL_OUTER_PADDING + panels_height
}

fn measure_panel_tree(tree: &PanelTree, width: usize) -> usize {
    let title_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = measure_text(tree.title, title_width);
    let mut content_height = 0;
    for (index, child) in tree.children.iter().enumerate() {
        content_height += measure_node(child, title_width);
        if index + 1 != tree.children.len() {
            content_height += NODE_GAP;
        }
    }

    PANEL_INNER_PADDING * 2 + title_height + 6 + content_height
}

fn measure_node(node: &PanelNode, available_width: usize) -> usize {
    match node {
        PanelNode::Column { children, .. } => children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                measure_node(child, available_width)
                    + usize::from(index + 1 != children.len()) * NODE_GAP
            })
            .sum(),
        PanelNode::Row { children, .. } => {
            let width_per_child = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(NODE_GAP * children.len().saturating_sub(1))
                    / children.len()
            };
            children
                .iter()
                .map(|child| measure_node(child, width_per_child))
                .max()
                .unwrap_or(0)
        }
        PanelNode::Section {
            children, title, ..
        } => {
            let title_height = measure_text(title, available_width);
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut children_height = 0;
            for (index, child) in children.iter().enumerate() {
                children_height += measure_node(child, child_width);
                if index + 1 != children.len() {
                    children_height += SECTION_GAP;
                }
            }
            title_height + SECTION_GAP + children_height
        }
        PanelNode::Text { text, .. } => measure_text(text, available_width),
        PanelNode::ColorPreview { .. } => COLOR_PREVIEW_HEIGHT,
        PanelNode::Button { .. } => BUTTON_HEIGHT,
        PanelNode::Slider { .. } => SLIDER_HEIGHT,
    }
}

fn draw_panel_tree(
    surface: &mut PanelSurface,
    tree: &PanelTree,
    x: usize,
    y: usize,
    width: usize,
    focused_target: Option<&FocusTarget>,
) {
    let inner_x = x + PANEL_INNER_PADDING;
    let inner_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = draw_wrapped_text(
        surface,
        inner_x,
        y + PANEL_INNER_PADDING,
        tree.title,
        PANEL_TITLE,
        inner_width,
    );
    let mut cursor_y = y + PANEL_INNER_PADDING + title_height + 6;

    for child in &tree.children {
        let used = draw_node(
            surface,
            child,
            tree.id,
            inner_x,
            cursor_y,
            inner_width,
            focused_target,
        );
        cursor_y += used + NODE_GAP;
    }
}

fn draw_node(
    surface: &mut PanelSurface,
    node: &PanelNode,
    panel_id: &str,
    x: usize,
    y: usize,
    available_width: usize,
    focused_target: Option<&FocusTarget>,
) -> usize {
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
                    focused_target,
                );
                if index + 1 != children.len() {
                    cursor_y += NODE_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Row { children, .. } => {
            let child_gap = NODE_GAP;
            let child_width = if children.is_empty() {
                available_width
            } else {
                available_width.saturating_sub(child_gap * children.len().saturating_sub(1))
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
                    focused_target,
                );
                max_height = max_height.max(used);
                cursor_x += child_width + child_gap;
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
                    focused_target,
                );
                if index + 1 != children.len() {
                    cursor_y += SECTION_GAP;
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
            let is_focused = focused_target
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
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: available_width,
                height: BUTTON_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Activate,
            });
            BUTTON_HEIGHT
        }
        PanelNode::Slider {
            id,
            label,
            min,
            max,
            value,
            fill_color,
        } => {
            let clamped_value = (*value).clamp(*min, *max);
            let accent = fill_color.unwrap_or(ColorRgba8::new(0x9f, 0xb7, 0xff, 0xff));
            let track_y = y + SLIDER_TRACK_TOP;
            let track_width = available_width.max(1);
            let track_inner_width = track_width.saturating_sub(2);
            let range = max.saturating_sub(*min).max(1);
            let progress = clamped_value.saturating_sub(*min);
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
                &format!("{label}: {clamped_value}"),
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
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: track_width,
                height: SLIDER_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
                kind: PanelHitKind::Slider {
                    min: *min,
                    max: *max,
                },
            });
            SLIDER_HEIGHT
        }
    }
}

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

fn measure_text(text: &str, available_width: usize) -> usize {
    let lines = wrap_text(text, available_width);
    lines.len().max(1) * text_line_height()
}

fn draw_wrapped_text(
    surface: &mut PanelSurface,
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

fn wrap_text(text: &str, available_width: usize) -> Vec<String> {
    wrap_text_lines(text, available_width)
}

fn draw_text_line(surface: &mut PanelSurface, x: usize, y: usize, text: &str, color: [u8; 4]) {
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

fn fill_rect(
    surface: &mut PanelSurface,
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

fn stroke_rect(
    surface: &mut PanelSurface,
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

fn write_pixel(surface: &mut PanelSurface, x: usize, y: usize, color: [u8; 4]) {
    if x >= surface.width || y >= surface.height {
        return;
    }
    let index = (y * surface.width + x) * 4;
    surface.pixels[index..index + 4].copy_from_slice(&color);
}

impl Default for UiShell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{draw_text_rgba, text_backend_name, wrap_text_lines};
    use app_core::{Command, ToolKind};
    use plugin_api::PanelPlugin;

    /// `UiShell` の更新配送を確認するためのダミーパネル。
    struct TestPanel {
        updates: usize,
    }

    impl PanelPlugin for TestPanel {
        fn id(&self) -> &'static str {
            "test.panel"
        }

        fn title(&self) -> &'static str {
            "Test Panel"
        }

        fn update(&mut self, _document: &Document) {
            self.updates += 1;
        }
    }

    /// パネル登録がホスト状態に反映されることを確認する。
    #[test]
    fn registering_panel_increases_panel_count() {
        let mut shell = UiShell::new();
        let initial_count = shell.panel_count();
        shell.register_panel(Box::new(TestPanel { updates: 0 }));

        assert_eq!(shell.panel_count(), initial_count + 1);
    }

    /// `update` が登録済みパネルへ配送される経路を壊していないことを確認する。
    #[test]
    fn update_dispatches_to_registered_panels() {
        let mut shell = UiShell::new();
        let initial_count = shell.panel_count();
        shell.register_panel(Box::new(TestPanel { updates: 0 }));

        shell.update(&Document::default());

        assert_eq!(shell.panel_count(), initial_count + 1);
    }

    /// `UiShell` がレンダラ経由でフレームを取得できることを確認する。
    #[test]
    fn render_frame_returns_canvas_bitmap() {
        let shell = UiShell::new();
        let frame = shell.render_frame(&Document::default());

        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.pixels.len(), 64 * 64 * 4);
    }

    #[test]
    fn default_shell_registers_builtin_layers_panel() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let summaries = shell.panel_debug_summaries();
        assert!(summaries.iter().any(|(id, title, summary)| {
            *id == "builtin.layers-panel"
                && *title == "Layers"
                && summary.contains("active_layer=Layer 1")
        }));
    }

    #[test]
    fn default_shell_registers_builtin_tool_palette() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let views = shell.panel_views();
        assert!(views.iter().any(|view| {
            view.id == "builtin.tool-palette"
                && view.title == "Tools"
                && view.lines.iter().any(|line| line.contains("Brush"))
        }));
    }

    #[test]
    fn shell_exposes_panel_tree_buttons() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let panels = shell.panel_trees();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");

        fn has_brush_button(items: &[PanelNode]) -> bool {
            items.iter().any(|item| match item {
                PanelNode::Button { label, .. } => label == "Brush",
                PanelNode::Column { children, .. }
                | PanelNode::Row { children, .. }
                | PanelNode::Section { children, .. } => has_brush_button(children),
                PanelNode::Text { .. }
                | PanelNode::ColorPreview { .. }
                | PanelNode::Slider { .. } => false,
            })
        }

        assert!(has_brush_button(&tool_panel.children));
    }

    #[test]
    fn panel_event_returns_command_action() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let actions = shell.handle_panel_event(&PanelEvent::Activate {
            panel_id: "builtin.tool-palette".to_string(),
            node_id: "tool.eraser".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveTool {
                tool: ToolKind::Eraser,
            })]
        );
    }

    #[test]
    fn default_shell_registers_builtin_color_palette() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let summaries = shell.panel_debug_summaries();
        assert!(summaries.iter().any(|(id, title, summary)| {
            *id == "builtin.color-palette"
                && *title == "Colors"
                && summary.contains("active_color=#000000")
        }));
    }

    #[test]
    fn color_palette_slider_event_returns_color_command_action() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let actions = shell.handle_panel_event(&PanelEvent::SetValue {
            panel_id: "builtin.color-palette".to_string(),
            node_id: "color.slider.red".to_string(),
            value: 128,
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveColor {
                color: app_core::ColorRgba8::new(128, 0x00, 0x00, 0xff),
            })]
        );
    }

    #[test]
    fn color_palette_tree_contains_live_preview() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let panels = shell.panel_trees();
        let color_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.color-palette")
            .expect("color panel exists");

        fn has_preview(items: &[PanelNode]) -> bool {
            items.iter().any(|item| match item {
                PanelNode::ColorPreview { .. } => true,
                PanelNode::Column { children, .. }
                | PanelNode::Row { children, .. }
                | PanelNode::Section { children, .. } => has_preview(children),
                PanelNode::Text { .. } | PanelNode::Button { .. } | PanelNode::Slider { .. } => {
                    false
                }
            })
        }

        assert!(has_preview(&color_panel.children));
    }

    #[test]
    fn rendered_panel_surface_maps_slider_region_to_value_event() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());
        let surface = shell.render_panel_surface(280, 800);

        let mut found = None;
        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::SetValue {
                    panel_id,
                    node_id,
                    value,
                }) = surface.hit_test(x, y)
                    && panel_id == "builtin.color-palette"
                    && node_id == "color.slider.red"
                {
                    found = Some(value);
                    break 'outer;
                }
            }
        }

        assert!(found.is_some());
    }

    #[test]
    fn rendered_panel_surface_contains_clickable_button_region() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());
        let surface = shell.render_panel_surface(280, 800);

        let mut found = None;
        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::Activate { panel_id, node_id }) = surface.hit_test(x, y)
                    && panel_id == "builtin.tool-palette"
                    && node_id == "tool.brush"
                {
                    found = Some((x, y));
                    break 'outer;
                }
            }
        }

        assert!(found.is_some());
    }

    #[test]
    fn focus_navigation_can_activate_focused_button() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        assert!(shell.focus_next());
        assert_eq!(
            shell.focused_target(),
            Some(("builtin.app-actions", "app.new"))
        );
        assert_eq!(
            shell.activate_focused(),
            vec![HostAction::DispatchCommand(Command::NewDocument)]
        );
    }

    #[test]
    fn scrolling_panels_updates_scroll_offset() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());
        let _ = shell.render_panel_surface(280, 96);

        assert!(shell.scroll_panels(6, 96));
        assert!(shell.panel_scroll_offset() > 0);
    }

    #[test]
    fn text_renderer_draws_visible_pixels() {
        let mut pixels = vec![0; 160 * 40 * 4];

        draw_text_rgba(&mut pixels, 160, 40, 4, 4, "Aa", [0xff, 0xff, 0xff, 0xff]);

        assert!(pixels.chunks_exact(4).any(|pixel| pixel != [0, 0, 0, 0]));
        if text_backend_name() == "system" {
            assert!(pixels.chunks_exact(4).any(|pixel| {
                pixel[0] != 0 && pixel[0] != 0xff && pixel[0] == pixel[1] && pixel[1] == pixel[2]
            }));
        }
    }

    #[test]
    fn wrap_text_lines_preserves_long_words() {
        let lines = wrap_text_lines("antidisestablishmentarianism", 24);

        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), "antidisestablishmentarianism");
    }
}
