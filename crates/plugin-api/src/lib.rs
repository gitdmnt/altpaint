//! `plugin-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

use app_core::{Command, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelView {
    pub id: &'static str,
    pub title: &'static str,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelTree {
    pub id: &'static str,
    pub title: &'static str,
    pub children: Vec<PanelNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAction {
    DispatchCommand(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    Activate { panel_id: String, node_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelNode {
    Column {
        id: String,
        children: Vec<PanelNode>,
    },
    Row {
        id: String,
        children: Vec<PanelNode>,
    },
    Section {
        id: String,
        title: String,
        children: Vec<PanelNode>,
    },
    Text {
        id: String,
        text: String,
    },
    Button {
        id: String,
        label: String,
        action: HostAction,
        active: bool,
    },
}

/// パネル型プラグインの最小インターフェース。
///
/// フェーズ0では、識別子・表示名・ドキュメント更新通知・
/// コマンド返却の4点だけを共通契約として提供する。
pub trait PanelPlugin {
    /// プラグインを一意に識別する固定IDを返す。
    fn id(&self) -> &'static str;

    /// UI上で表示するパネル名を返す。
    fn title(&self) -> &'static str;

    /// ドキュメント更新時に呼ばれるフック。
    ///
    /// 現段階では読み取り前提で、内部状態の更新に使うことを想定する。
    fn update(&mut self, _document: &Document) {}

    /// パネルが発行したいコマンド列を返す。
    ///
    /// フェーズ0では空配列を既定値とし、後続フェーズで入力や操作結果を
    /// `Command` として返すルートを整備する。
    fn commands(&mut self) -> Vec<Command> {
        Vec::new()
    }

    /// デバッグや最小UI表示に使う要約文字列を返す。
    fn debug_summary(&self) -> String {
        String::new()
    }

    /// 最小可視UIに使う表示データを返す。
    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: Vec::new(),
        }
    }

    fn panel_tree(&self) -> PanelTree {
        PanelTree {
            id: self.id(),
            title: self.title(),
            children: self
                .view()
                .lines
                .into_iter()
                .enumerate()
                .map(|(index, text)| PanelNode::Text {
                    id: format!("line.{index}"),
                    text,
                })
                .collect(),
        }
    }

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        match event {
            PanelEvent::Activate { panel_id, node_id } if panel_id == self.id() => {
                find_actions_in_nodes(&self.panel_tree().children, node_id)
            }
            _ => Vec::new(),
        }
    }
}

fn find_actions_in_nodes(nodes: &[PanelNode], target_id: &str) -> Vec<HostAction> {
    for node in nodes {
        if let Some(actions) = find_actions_in_node(node, target_id) {
            return actions;
        }
    }
    Vec::new()
}

fn find_actions_in_node(node: &PanelNode, target_id: &str) -> Option<Vec<HostAction>> {
    match node {
        PanelNode::Column { children, .. }
        | PanelNode::Row { children, .. }
        | PanelNode::Section { children, .. } => children
            .iter()
            .find_map(|child| find_actions_in_node(child, target_id)),
        PanelNode::Text { .. } => None,
        PanelNode::Button { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Button { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::ToolKind;

    struct TestPanel;

    impl PanelPlugin for TestPanel {
        fn id(&self) -> &'static str {
            "test.panel"
        }

        fn title(&self) -> &'static str {
            "Test"
        }

        fn panel_tree(&self) -> PanelTree {
            PanelTree {
                id: self.id(),
                title: self.title(),
                children: vec![PanelNode::Button {
                    id: "tool.brush".to_string(),
                    label: "Brush".to_string(),
                    action: HostAction::DispatchCommand(Command::SetActiveTool {
                        tool: ToolKind::Brush,
                    }),
                    active: true,
                }],
            }
        }
    }

    #[test]
    fn panel_plugin_tree_can_expose_button_action() {
        let panel = TestPanel;
        let tree = panel.panel_tree();

        assert_eq!(tree.id, "test.panel");
        assert!(matches!(
            &tree.children[0],
            PanelNode::Button {
                label,
                action: HostAction::DispatchCommand(Command::SetActiveTool { tool: ToolKind::Brush }),
                active: true,
                ..
            } if label == "Brush"
        ));
    }

    #[test]
    fn activate_event_resolves_button_action() {
        let mut panel = TestPanel;

        let actions = panel.handle_event(&PanelEvent::Activate {
            panel_id: "test.panel".to_string(),
            node_id: "tool.brush".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveTool {
                tool: ToolKind::Brush,
            })]
        );
    }
}
