use app_core::{Command, Document};
use plugin_api::{HostAction, PanelNode, PanelPlugin, PanelTree, PanelView};

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

    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(),
            title: self.title(),
            children: vec![PanelNode::Section {
                id: "project".to_string(),
                title: "Project".to_string(),
                children: vec![
                    PanelNode::Button {
                        id: "app.new".to_string(),
                        label: "New".to_string(),
                        action: HostAction::DispatchCommand(Command::NewDocument),
                        active: false,
                        fill_color: None,
                    },
                    PanelNode::Button {
                        id: "app.save".to_string(),
                        label: "Save".to_string(),
                        action: HostAction::DispatchCommand(Command::SaveProject),
                        active: false,
                        fill_color: None,
                    },
                    PanelNode::Button {
                        id: "app.load".to_string(),
                        label: "Load".to_string(),
                        action: HostAction::DispatchCommand(Command::LoadProject),
                        active: false,
                        fill_color: None,
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
        let tree = plugin.panel_tree();

        assert!(matches!(
            &tree.children[0],
            PanelNode::Section { children, .. }
                if children.iter().any(|child| matches!(
                    child,
                    PanelNode::Button {
                        label,
                        action: HostAction::DispatchCommand(Command::SaveProject),
                        ..
                    }
                        if label == "Save"
                ))
        ));
    }
}
