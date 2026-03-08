use app_core::{ColorRgba8, Command, Document};
use plugin_api::{HostAction, PanelNode, PanelPlugin, PanelTree, PanelView};

const PALETTE_COLORS: [(&str, ColorRgba8); 6] = [
    ("Black", ColorRgba8::new(0x00, 0x00, 0x00, 0xff)),
    ("Red", ColorRgba8::new(0xe5, 0x39, 0x35, 0xff)),
    ("Blue", ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff)),
    ("Green", ColorRgba8::new(0x43, 0xa0, 0x47, 0xff)),
    ("Gold", ColorRgba8::new(0xfb, 0x8c, 0x00, 0xff)),
    ("Violet", ColorRgba8::new(0x8e, 0x24, 0xaa, 0xff)),
];

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
            lines: PALETTE_COLORS
                .iter()
                .map(|(label, color)| {
                    let marker = if self.snapshot.active_color == *color {
                        ">"
                    } else {
                        " "
                    };
                    format!("{marker} {label} ({})", color.hex_rgb())
                })
                .collect(),
        }
    }

    fn panel_tree(&self) -> PanelTree {
        let mut rows = Vec::new();
        for (row_index, chunk) in PALETTE_COLORS.chunks(3).enumerate() {
            rows.push(PanelNode::Row {
                id: format!("palette.row.{row_index}"),
                children: chunk
                    .iter()
                    .map(|(label, color)| PanelNode::Button {
                        id: format!("color.{}", label.to_ascii_lowercase()),
                        label: (*label).to_string(),
                        action: HostAction::DispatchCommand(Command::SetActiveColor {
                            color: *color,
                        }),
                        active: self.snapshot.active_color == *color,
                        fill_color: Some(*color),
                    })
                    .collect(),
            });
        }

        PanelTree {
            id: self.id(),
            title: self.title(),
            children: vec![PanelNode::Section {
                id: "palette".to_string(),
                title: "Palette".to_string(),
                children: rows,
            }],
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

        assert_eq!(plugin.snapshot().active_color, ColorRgba8::new(0x1e, 0x88, 0xe5, 0xff));
        assert!(plugin.view().lines.iter().any(|line| line.contains("> Blue")));
    }

    #[test]
    fn color_palette_exposes_color_command_buttons() {
        let plugin = ColorPalettePlugin::default();

        let tree = plugin.panel_tree();

        assert!(matches!(
            &tree.children[0],
            PanelNode::Section { children, .. }
                if matches!(
                    &children[0],
                    PanelNode::Row { children, .. }
                        if matches!(
                            &children[1],
                            PanelNode::Button {
                                label,
                                action: HostAction::DispatchCommand(Command::SetActiveColor { color }),
                                fill_color: Some(fill_color),
                                ..
                            } if label == "Red"
                                && *color == ColorRgba8::new(0xe5, 0x39, 0x35, 0xff)
                                && *fill_color == ColorRgba8::new(0xe5, 0x39, 0x35, 0xff)
                        )
                )
        ));
    }
}