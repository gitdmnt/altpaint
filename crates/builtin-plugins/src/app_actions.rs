use app_core::{Command, Document};
use plugin_api::{PanelPlugin, PanelUi, PanelUiNode, PanelView};

#[derive(Debug, Default)]
pub struct AppActionsPlugin;

impl PanelPlugin for AppActionsPlugin {
    fn id(&self) -> &'static str {
        "builtin.app-actions"
    }

    fn title(&self) -> &'static str {
        "App"
    }

    fn update(&mut self, _document: &Document) {}

    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: vec!["New".to_string(), "Save".to_string(), "Load".to_string()],
        }
    }

    fn ui(&self) -> PanelUi {
        PanelUi {
            id: self.id(),
            title: self.title(),
            nodes: vec![PanelUiNode::Section {
                title: "Project".to_string(),
                children: vec![
                    PanelUiNode::CommandButton {
                        id: "app.new".to_string(),
                        label: "New".to_string(),
                        command: Command::NewDocument,
                        active: false,
                    },
                    PanelUiNode::CommandButton {
                        id: "app.save".to_string(),
                        label: "Save".to_string(),
                        command: Command::SaveProject,
                        active: false,
                    },
                    PanelUiNode::CommandButton {
                        id: "app.load".to_string(),
                        label: "Load".to_string(),
                        command: Command::LoadProject,
                        active: false,
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
    fn app_actions_exposes_save_load_commands() {
        let plugin = AppActionsPlugin;
        let ui = plugin.ui();

        assert!(matches!(
            &ui.nodes[0],
            PanelUiNode::Section { children, .. }
                if children.iter().any(|child| matches!(
                    child,
                    PanelUiNode::CommandButton { label, command: Command::SaveProject, .. }
                        if label == "Save"
                ))
        ));
    }
}