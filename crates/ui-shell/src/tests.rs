use super::*;
use panel_api::{HostAction, PanelNode, PanelPlugin, PanelTree};
use panel_runtime::PanelRuntime;

struct MockPanel {
    id: &'static str,
    title: &'static str,
    tree: PanelTree,
}

impl PanelPlugin for MockPanel {
    /// ID を計算して返す。
    fn id(&self) -> &'static str {
        self.id
    }

    /// title を計算して返す。
    fn title(&self) -> &'static str {
        self.title
    }

    /// 現在の パネル tree を返す。
    fn panel_tree(&self) -> PanelTree {
        self.tree.clone()
    }
}

/// 現在の値を runtime へ変換する。
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

/// パネル trees include ワークスペース manager and runtime panels が期待どおりに動作することを検証する。
#[test]
fn panel_trees_include_workspace_manager_and_runtime_panels() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    let trees = presentation.panel_trees(&runtime);

    assert_eq!(trees[0].id, workspace::WORKSPACE_PANEL_ID);
    assert!(trees.iter().any(|tree| tree.id == "builtin.mock"));
}

/// フォーカス moves to runtime パネル node が期待どおりに動作することを検証する。
#[test]
fn focus_moves_to_runtime_panel_node() {
    let runtime = mock_runtime();
    let mut presentation = PanelPresentation::new();
    presentation.reconcile_runtime_panels(&runtime);

    assert!(presentation.focus_panel_node(&runtime, "builtin.mock", "mock.button"));
    assert_eq!(
        presentation.focused_target(),
        Some(("builtin.mock", "mock.button"))
    );
}
