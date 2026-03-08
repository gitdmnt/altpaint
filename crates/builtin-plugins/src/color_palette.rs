use app_core::{ColorRgba8, Command, Document};
use plugin_api::{HostAction, PanelNode, PanelPlugin, PanelTree, PanelView};

const RED_SLIDER_ID: &str = "color.slider.red";
const GREEN_SLIDER_ID: &str = "color.slider.green";
const BLUE_SLIDER_ID: &str = "color.slider.blue";
const PREVIEW_NODE_ID: &str = "color.preview";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColorPaletteSnapshot {
    pub active_color: ColorRgba8,
}

#[derive(Debug, Default)]
pub struct ColorPalettePlugin {
    snapshot: ColorPaletteSnapshot,
}

impl ColorPalettePlugin {
    pub fn snapshot(&self) -> &ColorPaletteSnapshot {
        &self.snapshot
    }

    fn updated_color_for_slider(&self, node_id: &str, value: usize) -> Option<ColorRgba8> {
        let value = value.min(u8::MAX as usize) as u8;
        let mut color = self.snapshot.active_color;
        match node_id {
            RED_SLIDER_ID => color.r = value,
            GREEN_SLIDER_ID => color.g = value,
            BLUE_SLIDER_ID => color.b = value,
            _ => return None,
        }
        Some(color)
    }
}

impl PanelPlugin for ColorPalettePlugin {
    fn id(&self) -> &'static str {
        "builtin.color-palette"
    }

    fn title(&self) -> &'static str {
        "Colors"
    }

    fn update(&mut self, document: &Document) {
        self.snapshot.active_color = document.active_color;
    }

    fn debug_summary(&self) -> String {
        format!("active_color={}", self.snapshot.active_color.hex_rgb())
    }

    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec![format!(
                "Preview {} / R:{} G:{} B:{}",
                self.snapshot.active_color.hex_rgb(),
                self.snapshot.active_color.r,
                self.snapshot.active_color.g,
                self.snapshot.active_color.b,
            )],
        }
    }

    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(),
            title: self.title(),
            children: vec![PanelNode::Section {
                id: "custom".to_string(),
                title: "Custom".to_string(),
                children: vec![
                    PanelNode::ColorPreview {
                        id: PREVIEW_NODE_ID.to_string(),
                        label: format!("Live Preview {}", self.snapshot.active_color.hex_rgb()),
                        color: self.snapshot.active_color,
                    },
                    PanelNode::Text {
                        id: "color.current".to_string(),
                        text: format!(
                            "R:{} G:{} B:{}",
                            self.snapshot.active_color.r,
                            self.snapshot.active_color.g,
                            self.snapshot.active_color.b,
                        ),
                    },
                    PanelNode::Slider {
                        id: RED_SLIDER_ID.to_string(),
                        label: "Red".to_string(),
                        min: 0,
                        max: 255,
                        value: self.snapshot.active_color.r as usize,
                        fill_color: Some(ColorRgba8::new(0xd3, 0x2f, 0x2f, 0xff)),
                    },
                    PanelNode::Slider {
                        id: GREEN_SLIDER_ID.to_string(),
                        label: "Green".to_string(),
                        min: 0,
                        max: 255,
                        value: self.snapshot.active_color.g as usize,
                        fill_color: Some(ColorRgba8::new(0x38, 0x8e, 0x3c, 0xff)),
                    },
                    PanelNode::Slider {
                        id: BLUE_SLIDER_ID.to_string(),
                        label: "Blue".to_string(),
                        min: 0,
                        max: 255,
                        value: self.snapshot.active_color.b as usize,
                        fill_color: Some(ColorRgba8::new(0x19, 0x76, 0xd2, 0xff)),
                    },
                ],
            }],
        }
    }

    fn handle_event(&mut self, event: &plugin_api::PanelEvent) -> Vec<HostAction> {
        match event {
            plugin_api::PanelEvent::SetValue {
                panel_id,
                node_id,
                value,
            } if panel_id == self.id() => {
                let Some(color) = self.updated_color_for_slider(node_id, *value) else {
                    return Vec::new();
                };
                self.snapshot.active_color = color;
                vec![HostAction::DispatchCommand(Command::SetActiveColor {
                    color,
                })]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_palette_tracks_active_color() {
        let mut plugin = ColorPalettePlugin::default();
        let mut document = Document::default();
        document.set_active_color(ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff));

        plugin.update(&document);

        assert_eq!(
            plugin.snapshot().active_color,
            ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)
        );
        assert!(
            plugin
                .view()
                .lines
                .iter()
                .any(|line| line.contains("Preview #1E88E5 / R:30 G:136 B:229"))
        );
    }

    #[test]
    fn color_palette_exposes_live_preview_node() {
        let plugin = ColorPalettePlugin::default();

        let tree = plugin.panel_tree();

        assert!(matches!(
            &tree.children[0],
            PanelNode::Section { children, .. }
                if matches!(
                    &children[0],
                    PanelNode::ColorPreview { label, color, .. }
                        if label == "Live Preview #000000"
                            && *color == ColorRgba8::new(0x00, 0x00, 0x00, 0xff)
                )
        ));
    }

    #[test]
    fn color_palette_ignores_activate_events() {
        let mut plugin = ColorPalettePlugin::default();

        let actions = plugin.handle_event(&plugin_api::PanelEvent::Activate {
            panel_id: "builtin.color-palette".to_string(),
            node_id: PREVIEW_NODE_ID.to_string(),
        });

        assert!(actions.is_empty());
    }

    #[test]
    fn color_palette_slider_event_updates_single_channel() {
        let mut plugin = ColorPalettePlugin::default();
        let mut document = Document::default();
        document.set_active_color(ColorRgba8::new(10, 20, 30, 255));
        plugin.update(&document);

        let actions = plugin.handle_event(&plugin_api::PanelEvent::SetValue {
            panel_id: "builtin.color-palette".to_string(),
            node_id: RED_SLIDER_ID.to_string(),
            value: 128,
        });

        assert_eq!(
            plugin.snapshot().active_color,
            ColorRgba8::new(128, 20, 30, 255)
        );
        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveColor {
                color: ColorRgba8::new(128, 20, 30, 255),
            })]
        );
    }
}
