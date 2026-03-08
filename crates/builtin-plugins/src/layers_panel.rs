use app_core::Document;
use plugin_api::{PanelPlugin, PanelUi, PanelUiNode, PanelView};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayersPanelSnapshot {
    pub work_title: String,
    pub page_count: usize,
    pub panel_count: usize,
    pub active_panel_layer_name: String,
}

#[derive(Debug, Default)]
pub struct LayersPanelPlugin {
    snapshot: LayersPanelSnapshot,
}

impl LayersPanelPlugin {
    pub fn snapshot(&self) -> &LayersPanelSnapshot {
        &self.snapshot
    }

    fn snapshot_from_document(document: &Document) -> LayersPanelSnapshot {
        let page_count = document.work.pages.len();
        let panel_count = document
            .work
            .pages
            .iter()
            .map(|page| page.panels.len())
            .sum();
        let active_panel_layer_name = document
            .work
            .pages
            .first()
            .and_then(|page| page.panels.first())
            .map(|panel| panel.root_layer.name.clone())
            .unwrap_or_else(|| "<no layer>".to_string());

        LayersPanelSnapshot {
            work_title: document.work.title.clone(),
            page_count,
            panel_count,
            active_panel_layer_name,
        }
    }
}

impl PanelPlugin for LayersPanelPlugin {
    fn id(&self) -> &'static str {
        "builtin.layers-panel"
    }

    fn title(&self) -> &'static str {
        "Layers"
    }

    fn update(&mut self, document: &Document) {
        self.snapshot = Self::snapshot_from_document(document);
    }

    fn debug_summary(&self) -> String {
        format!(
            "title={} pages={} panels={} active_layer={}",
            self.snapshot.work_title,
            self.snapshot.page_count,
            self.snapshot.panel_count,
            self.snapshot.active_panel_layer_name
        )
    }

    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec![
                format!("work: {}", self.snapshot.work_title),
                format!("pages: {}", self.snapshot.page_count),
                format!("panels: {}", self.snapshot.panel_count),
                format!("layer: {}", self.snapshot.active_panel_layer_name),
            ],
        }
    }

    fn ui(&self) -> PanelUi {
        PanelUi {
            id: self.id(),
            title: self.title(),
            nodes: vec![
                PanelUiNode::Section {
                    title: "Document".to_string(),
                    children: vec![
                        PanelUiNode::Text(format!("work: {}", self.snapshot.work_title)),
                        PanelUiNode::Text(format!("pages: {}", self.snapshot.page_count)),
                        PanelUiNode::Text(format!("panels: {}", self.snapshot.panel_count)),
                    ],
                },
                PanelUiNode::Section {
                    title: "Active Layer".to_string(),
                    children: vec![PanelUiNode::Text(format!(
                        "layer: {}",
                        self.snapshot.active_panel_layer_name
                    ))],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Document;

    #[test]
    fn layers_panel_tracks_document_summary() {
        let mut plugin = LayersPanelPlugin::default();
        let document = Document::default();

        plugin.update(&document);

        assert_eq!(plugin.snapshot().work_title, "Untitled");
        assert_eq!(plugin.snapshot().page_count, 1);
        assert_eq!(plugin.snapshot().panel_count, 1);
        assert_eq!(plugin.snapshot().active_panel_layer_name, "Layer 1");
    }

    #[test]
    fn layers_panel_exposes_declarative_sections() {
        let mut plugin = LayersPanelPlugin::default();
        let document = Document::default();
        plugin.update(&document);

        let ui = plugin.ui();

        assert!(matches!(
            &ui.nodes[0],
            PanelUiNode::Section { title, .. } if title == "Document"
        ));
    }
}
