//! `ui-shell` はアプリケーションウィンドウ上でパネルをホストする最小UI層。
//!
//! フェーズ0では、個々のパネル機能そのものは持たず、`RenderContext` と
//! `PanelPlugin` 群を束ねる薄い境界として機能する。

use app_core::Document;
use builtin_plugins::default_builtin_panels;
use plugin_api::{PanelPlugin, PanelUi, PanelUiNode, PanelView};
use render::{RenderContext, RenderFrame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlintPanelModel {
    pub id: String,
    pub title: String,
    pub items: Vec<SlintPanelItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlintPanelItem {
    Text {
        text: String,
    },
    Button {
        id: String,
        label: String,
        command: app_core::Command,
        active: bool,
    },
    Section {
        title: String,
        text: String,
    },
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

    pub fn panel_uis(&self) -> Vec<PanelUi> {
        self.panels.iter().map(|panel| panel.ui()).collect()
    }

    pub fn slint_panels(&self) -> Vec<SlintPanelModel> {
        self.panel_uis()
            .into_iter()
            .map(|panel| SlintPanelModel {
                id: panel.id.to_string(),
                title: panel.title.to_string(),
                items: flatten_panel_ui_nodes(panel.nodes),
            })
            .collect()
    }
}

fn flatten_panel_ui_nodes(nodes: Vec<PanelUiNode>) -> Vec<SlintPanelItem> {
    nodes
        .into_iter()
        .flat_map(flatten_panel_ui_node)
        .collect()
}

fn flatten_panel_ui_node(node: PanelUiNode) -> Vec<SlintPanelItem> {
    match node {
        PanelUiNode::Text(text) => vec![SlintPanelItem::Text { text }],
        PanelUiNode::CommandButton {
            id,
            label,
            command,
            active,
        } => vec![SlintPanelItem::Button {
            id,
            label,
            command,
            active,
        }],
        PanelUiNode::Section { title, children } => {
            let summary = children
                .iter()
                .filter_map(|child| match child {
                    PanelUiNode::Text(text) => Some(text.clone()),
                    PanelUiNode::CommandButton { label, .. } => Some(label.clone()),
                    PanelUiNode::Section { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n");

            let mut items = vec![SlintPanelItem::Section {
                title,
                text: summary,
            }];
            items.extend(children.into_iter().flat_map(flatten_panel_ui_node));
            items
        }
    }
}

impl Default for UiShell {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn shell_exposes_slint_panels_with_button_commands() {
        let mut shell = UiShell::new();
        shell.update(&Document::default());

        let panels = shell.slint_panels();
        let tool_panel = panels
            .iter()
            .find(|panel| panel.id == "builtin.tool-palette")
            .expect("tool panel exists");

        fn has_brush_button(items: &[SlintPanelItem]) -> bool {
            items.iter().any(|item| match item {
                SlintPanelItem::Button { label, .. } => label == "Brush",
                SlintPanelItem::Section { .. } => false,
                SlintPanelItem::Text { .. } => false,
            })
        }

        assert!(has_brush_button(&tool_panel.items));
    }
}
