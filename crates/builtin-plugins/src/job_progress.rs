use app_core::Document;
use plugin_api::{PanelNode, PanelPlugin, PanelTree, PanelView};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JobProgressSnapshot {
    pub active_jobs: usize,
    pub queued_jobs: usize,
    pub status_line: String,
}

#[derive(Debug, Default)]
pub struct JobProgressPanelPlugin {
    snapshot: JobProgressSnapshot,
}

impl JobProgressPanelPlugin {
    pub fn snapshot(&self) -> &JobProgressSnapshot {
        &self.snapshot
    }

    fn snapshot_from_document(document: &Document) -> JobProgressSnapshot {
        JobProgressSnapshot {
            active_jobs: 0,
            queued_jobs: 0,
            status_line: format!("idle / work={}", document.work.title),
        }
    }
}

impl PanelPlugin for JobProgressPanelPlugin {
    fn id(&self) -> &'static str {
        "builtin.job-progress"
    }

    fn title(&self) -> &'static str {
        "Jobs"
    }

    fn update(&mut self, document: &Document) {
        self.snapshot = Self::snapshot_from_document(document);
    }

    fn debug_summary(&self) -> String {
        format!(
            "active_jobs={} queued_jobs={} status={}",
            self.snapshot.active_jobs, self.snapshot.queued_jobs, self.snapshot.status_line
        )
    }

    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec![
                format!("active: {}", self.snapshot.active_jobs),
                format!("queued: {}", self.snapshot.queued_jobs),
                self.snapshot.status_line.clone(),
            ],
        }
    }

    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(),
            title: self.title(),
            children: vec![PanelNode::Section {
                id: "jobs.summary".to_string(),
                title: "Queue".to_string(),
                children: vec![
                    PanelNode::Text {
                        id: "jobs.active".to_string(),
                        text: format!("active: {}", self.snapshot.active_jobs),
                    },
                    PanelNode::Text {
                        id: "jobs.queued".to_string(),
                        text: format!("queued: {}", self.snapshot.queued_jobs),
                    },
                    PanelNode::Text {
                        id: "jobs.status".to_string(),
                        text: self.snapshot.status_line.clone(),
                    },
                ],
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_progress_panel_tracks_idle_state() {
        let mut plugin = JobProgressPanelPlugin::default();
        let document = Document::default();

        plugin.update(&document);

        assert_eq!(plugin.snapshot().active_jobs, 0);
        assert_eq!(plugin.snapshot().queued_jobs, 0);
        assert!(plugin.snapshot().status_line.contains("Untitled"));
    }
}
