use app_core::{Document, ToolKind};
use plugin_api::{PanelNode, PanelPlugin, PanelTree, PanelView};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnapshotPanelSnapshot {
    pub work_title: String,
    pub page_count: usize,
    pub panel_count: usize,
    pub active_tool: ToolKind,
}

#[derive(Debug, Default)]
pub struct SnapshotPanelPlugin {
    snapshot: SnapshotPanelSnapshot,
}

impl SnapshotPanelPlugin {
    pub fn snapshot(&self) -> &SnapshotPanelSnapshot {
        &self.snapshot
    }

    fn snapshot_from_document(document: &Document) -> SnapshotPanelSnapshot {
        SnapshotPanelSnapshot {
            work_title: document.work.title.clone(),
            page_count: document.work.pages.len(),
            panel_count: document.work.pages.iter().map(|page| page.panels.len()).sum(),
            active_tool: document.active_tool,
        }
    }
}

impl PanelPlugin for SnapshotPanelPlugin {
    fn id(&self) -> &'static str {
        "builtin.snapshot-panel"
    }

    fn title(&self) -> &'static str {
        "Snapshots"
    }

    fn update(&mut self, document: &Document) {
        self.snapshot = Self::snapshot_from_document(document);
    }

    fn debug_summary(&self) -> String {
        format!(
            "work={} pages={} panels={} tool={:?}",
            self.snapshot.work_title,
            self.snapshot.page_count,
            self.snapshot.panel_count,
            self.snapshot.active_tool
        )
    }

    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec![
                format!("work: {}", self.snapshot.work_title),
                format!("pages: {} / panels: {}", self.snapshot.page_count, self.snapshot.panel_count),
                format!("current tool: {:?}", self.snapshot.active_tool),
                "snapshot storage: pending".to_string(),
            ],
        }
    }

    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(),
            title: self.title(),
            children: vec![
                PanelNode::Section {
                    id: "snapshot.current".to_string(),
                    title: "Current".to_string(),
                    children: vec![
                        PanelNode::Text {
                            id: "snapshot.work".to_string(),
                            text: format!("work: {}", self.snapshot.work_title),
                        },
                        PanelNode::Text {
                            id: "snapshot.counts".to_string(),
                            text: format!(
                                "pages: {} / panels: {}",
                                self.snapshot.page_count, self.snapshot.panel_count
                            ),
                        },
                        PanelNode::Text {
                            id: "snapshot.tool".to_string(),
                            text: format!("current tool: {:?}", self.snapshot.active_tool),
                        },
                    ],
                },
                PanelNode::Section {
                    id: "snapshot.status".to_string(),
                    title: "Status".to_string(),
                    children: vec![PanelNode::Text {
                        id: "snapshot.pending".to_string(),
                        text: "snapshot storage: pending".to_string(),
                    }],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_panel_tracks_document_summary() {
        let mut plugin = SnapshotPanelPlugin::default();
        let document = Document::default();

        plugin.update(&document);

        assert_eq!(plugin.snapshot().work_title, "Untitled");
        assert_eq!(plugin.snapshot().page_count, 1);
        assert_eq!(plugin.snapshot().panel_count, 1);
        assert_eq!(plugin.snapshot().active_tool, ToolKind::Brush);
    }
}