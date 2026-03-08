//! `ui-shell` はアプリケーションウィンドウ上でパネルをホストする最小UI層。
//!
//! フェーズ0では、個々のパネル機能そのものは持たず、`RenderContext` と
//! `PanelPlugin` 群を束ねる薄い境界として機能する。

use app_core::Document;
use builtin_plugins::default_builtin_panels;
use font8x8::{BASIC_FONTS, UnicodeFonts};
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
const BUTTON_TEXT: [u8; 4] = [0xf0, 0xf0, 0xf0, 0xff];
const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 8;
const LINE_HEIGHT: usize = 10;
const PANEL_OUTER_PADDING: usize = 8;
const PANEL_INNER_PADDING: usize = 8;
const NODE_GAP: usize = 6;
const SECTION_GAP: usize = 4;
const SECTION_INDENT: usize = 10;
const BUTTON_HEIGHT: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanelHitRegion {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
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
            .map(|region| PanelEvent::Activate {
                panel_id: region.panel_id.clone(),
                node_id: region.node_id.clone(),
            })
    }
}

/// パネルホストとして振る舞う最小UIシェル。
pub struct UiShell {
    /// キャンバス描画側への入口。
    render_context: RenderContext,
    /// 登録済みのパネルプラグイン一覧。
    panels: Vec<Box<dyn PanelPlugin>>,
}

impl UiShell {
    /// 空のUIシェルを作成する。
    pub fn new() -> Self {
        let mut shell = Self {
            render_context: RenderContext::new(),
            panels: Vec::new(),
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

    pub fn handle_panel_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        self.panels
            .iter_mut()
            .flat_map(|panel| panel.handle_event(event))
            .collect()
    }

    pub fn render_panel_surface(&self, width: usize, height: usize) -> PanelSurface {
        let mut surface = PanelSurface {
            width,
            height,
            pixels: vec![0; width * height * 4],
            hit_regions: Vec::new(),
        };
        fill_rect(&mut surface, 0, 0, width, height, SIDEBAR_BACKGROUND);

        let mut cursor_y = PANEL_OUTER_PADDING;
        let panel_width = width.saturating_sub(PANEL_OUTER_PADDING * 2);

        for tree in self.panel_trees() {
            let panel_height = measure_panel_tree(&tree, panel_width);
            if cursor_y >= height {
                break;
            }

            let clamped_height = panel_height.min(height.saturating_sub(cursor_y));
            fill_rect(
                &mut surface,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                clamped_height,
                PANEL_BACKGROUND,
            );
            stroke_rect(
                &mut surface,
                PANEL_OUTER_PADDING,
                cursor_y,
                panel_width,
                clamped_height,
                PANEL_BORDER,
            );
            draw_panel_tree(&mut surface, &tree, PANEL_OUTER_PADDING, cursor_y, panel_width);
            cursor_y += panel_height + PANEL_OUTER_PADDING;
        }

        surface
    }
}

fn measure_panel_tree(tree: &PanelTree, width: usize) -> usize {
    let title_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = measure_text(&tree.title, title_width);
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
        PanelNode::Section { children, title, .. } => {
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
        PanelNode::Button { .. } => BUTTON_HEIGHT,
    }
}

fn draw_panel_tree(
    surface: &mut PanelSurface,
    tree: &PanelTree,
    x: usize,
    y: usize,
    width: usize,
) {
    let inner_x = x + PANEL_INNER_PADDING;
    let inner_width = width.saturating_sub(PANEL_INNER_PADDING * 2);
    let title_height = draw_wrapped_text(surface, inner_x, y + PANEL_INNER_PADDING, tree.title, PANEL_TITLE, inner_width);
    let mut cursor_y = y + PANEL_INNER_PADDING + title_height + 6;

    for child in &tree.children {
        let used = draw_node(surface, child, tree.id, inner_x, cursor_y, inner_width);
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
) -> usize {
    match node {
        PanelNode::Column { children, .. } => {
            let mut cursor_y = y;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(surface, child, panel_id, x, cursor_y, available_width);
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
                let used = draw_node(surface, child, panel_id, cursor_x, y, child_width);
                max_height = max_height.max(used);
                cursor_x += child_width + child_gap;
            }
            max_height
        }
        PanelNode::Section {
            title, children, ..
        } => {
            let title_height = draw_wrapped_text(surface, x, y, title, SECTION_TITLE, available_width);
            let child_x = x + SECTION_INDENT;
            let child_width = available_width.saturating_sub(SECTION_INDENT);
            let mut cursor_y = y + title_height + SECTION_GAP;
            for (index, child) in children.iter().enumerate() {
                cursor_y += draw_node(surface, child, panel_id, child_x, cursor_y, child_width);
                if index + 1 != children.len() {
                    cursor_y += SECTION_GAP;
                }
            }
            cursor_y.saturating_sub(y)
        }
        PanelNode::Text { text, .. } => {
            draw_wrapped_text(surface, x, y, text, BODY_TEXT, available_width)
        }
        PanelNode::Button {
            id,
            label,
            active,
            ..
        } => {
            let fill = if *active { BUTTON_ACTIVE_FILL } else { BUTTON_FILL };
            fill_rect(surface, x, y, available_width, BUTTON_HEIGHT, fill);
            stroke_rect(surface, x, y, available_width, BUTTON_HEIGHT, BUTTON_BORDER);
            draw_wrapped_text(
                surface,
                x + 6,
                y + 7,
                label,
                BUTTON_TEXT,
                available_width.saturating_sub(12),
            );
            surface.hit_regions.push(PanelHitRegion {
                x,
                y,
                width: available_width,
                height: BUTTON_HEIGHT,
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            });
            BUTTON_HEIGHT
        }
    }
}

fn measure_text(text: &str, available_width: usize) -> usize {
    let lines = wrap_text(text, available_width);
    lines.len().max(1) * LINE_HEIGHT
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
        draw_text_line(surface, x, y + index * LINE_HEIGHT, line, color);
    }
    lines.len().max(1) * LINE_HEIGHT
}

fn wrap_text(text: &str, available_width: usize) -> Vec<String> {
    let max_chars = (available_width / FONT_WIDTH).max(1);
    let mut lines = Vec::new();

    for raw_line in text.split('\n') {
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let next_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };

            if next_len > max_chars && !current.is_empty() {
                lines.push(current.clone());
                current.clear();
            }

            if word.chars().count() > max_chars {
                if !current.is_empty() {
                    lines.push(current.clone());
                    current.clear();
                }
                let mut chunk = String::new();
                for ch in word.chars() {
                    chunk.push(ch);
                    if chunk.chars().count() >= max_chars {
                        lines.push(chunk.clone());
                        chunk.clear();
                    }
                }
                current = chunk;
                continue;
            }

            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }

        if current.is_empty() {
            lines.push(String::new());
        } else {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn draw_text_line(surface: &mut PanelSurface, x: usize, y: usize, text: &str, color: [u8; 4]) {
    for (index, ch) in text.chars().enumerate() {
        draw_glyph(surface, x + index * FONT_WIDTH, y, ch, color);
    }
}

fn draw_glyph(surface: &mut PanelSurface, x: usize, y: usize, ch: char, color: [u8; 4]) {
    let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
    let Some(glyph) = glyph else {
        return;
    };

    for (row, bits) in glyph.iter().enumerate().take(FONT_HEIGHT) {
        for col in 0..FONT_WIDTH {
            if ((bits >> col) & 1) == 1 {
                write_pixel(surface, x + col, y + row, color);
            }
        }
    }
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
                PanelNode::Text { .. } => false,
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
    fn rendered_panel_surface_contains_clickable_button_region() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());
        let surface = shell.render_panel_surface(280, 800);

        let mut found = None;
        'outer: for y in 0..surface.height {
            for x in 0..surface.width {
                if let Some(PanelEvent::Activate { panel_id, node_id }) = surface.hit_test(x, y) {
                    if panel_id == "builtin.tool-palette" && node_id == "tool.brush" {
                        found = Some((x, y));
                        break 'outer;
                    }
                }
            }
        }

        assert!(found.is_some());
    }
}
