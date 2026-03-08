use app_core::{Command, Document, ToolKind};
use plugin_api::{PanelPlugin, PanelUi, PanelUiNode, PanelView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPaletteSnapshot {
    pub active_tool: ToolKind,
}

impl Default for ToolPaletteSnapshot {
    fn default() -> Self {
        Self {
            active_tool: ToolKind::Brush,
        }
    }
}

#[derive(Debug, Default)]
pub struct ToolPalettePlugin {
    snapshot: ToolPaletteSnapshot,
}

impl ToolPalettePlugin {
    pub fn snapshot(&self) -> &ToolPaletteSnapshot {
        &self.snapshot
    }
}

impl PanelPlugin for ToolPalettePlugin {
    fn id(&self) -> &'static str {
        "builtin.tool-palette"
    }

    fn title(&self) -> &'static str {
        "Tools"
    }

    fn update(&mut self, document: &Document) {
        self.snapshot.active_tool = document.active_tool;
    }

    fn debug_summary(&self) -> String {
        format!("active_tool={:?}", self.snapshot.active_tool)
    }

    fn view(&self) -> PanelView {
        let brush_marker = if self.snapshot.active_tool == ToolKind::Brush {
            ">"
        } else {
            " "
        };
        let eraser_marker = if self.snapshot.active_tool == ToolKind::Eraser {
            ">"
        } else {
            " "
        };

        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec![
                format!("{} [B] Brush", brush_marker),
                format!("{} [E] Eraser", eraser_marker),
            ],
        }
    }

    fn ui(&self) -> PanelUi {
        PanelUi {
            id: self.id(),
            title: self.title(),
            nodes: vec![
                PanelUiNode::Section {
                    title: "Tools".to_string(),
                    children: vec![
                        PanelUiNode::CommandButton {
                            id: "tool.brush".to_string(),
                            label: "Brush".to_string(),
                            command: Command::SetActiveTool {
                                tool: ToolKind::Brush,
                            },
                            active: self.snapshot.active_tool == ToolKind::Brush,
                        },
                        PanelUiNode::CommandButton {
                            id: "tool.eraser".to_string(),
                            label: "Eraser".to_string(),
                            command: Command::SetActiveTool {
                                tool: ToolKind::Eraser,
                            },
                            active: self.snapshot.active_tool == ToolKind::Eraser,
                        },
                    ],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_palette_tracks_active_tool() {
        let mut plugin = ToolPalettePlugin::default();
        let mut document = Document::default();
        document.set_active_tool(ToolKind::Eraser);

        plugin.update(&document);

        assert_eq!(plugin.snapshot().active_tool, ToolKind::Eraser);
        let view = plugin.view();
        assert!(view.lines.iter().any(|line| line.contains("> [E] Eraser")));
    }

    #[test]
    fn tool_palette_exposes_command_buttons() {
        let plugin = ToolPalettePlugin::default();

        let ui = plugin.ui();

        assert!(matches!(
            &ui.nodes[0],
            PanelUiNode::Section { children, .. }
                if matches!(
                    &children[0],
                    PanelUiNode::CommandButton {
                        label,
                        command: Command::SetActiveTool { tool: ToolKind::Brush },
                        ..
                    } if label == "Brush"
                )
        ));
    }
}
