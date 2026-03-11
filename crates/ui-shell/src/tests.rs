use super::*;
use panel_runtime::PanelRuntime;
use plugin_api::{HostAction, PanelNode, PanelPlugin, PanelTree};

struct MockPanel {
    id: &'static str,
    title: &'static str,
    tree: PanelTree,
}

impl PanelPlugin for MockPanel {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn panel_tree(&self) -> PanelTree {
        self.tree.clone()
    }
}

fn mock_runtime() -> PanelRuntime {
    let mut runtime = PanelRuntime::new();
    runtime.register_panel(Box::new(MockPanel {
        id: "builtin.mock",
        title: "Mock",
        tree: PanelTree {
            id: "builtin.mock",
            title: "Mock",
            children: vec![PanelNode::Button {
                id: "mock.button".to_string(),
                label: "Push".to_string(),
                action: HostAction::DispatchCommand(app_core::Command::Noop),
                active: false,
                fill_color: None,
            }],
        },
    }));
    runtime
}

#[test]
fn panel_trees_include_workspace_manager_and_runtime_panels() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    let trees = presentation.panel_trees(&runtime);

    assert_eq!(trees[0].id, workspace::WORKSPACE_PANEL_ID);
    assert!(trees.iter().any(|tree| tree.id == "builtin.mock"));
}

#[test]
fn focus_moves_to_runtime_panel_node() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    assert!(presentation.focus_panel_node(&runtime, "builtin.mock", "mock.button"));
    assert_eq!(presentation.focused_target(), Some(("builtin.mock", "mock.button")));
}
