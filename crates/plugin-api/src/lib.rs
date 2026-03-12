//! `plugin-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

pub mod services;

use app_core::{ColorRgba8, Command, Document};
use serde_json::Value;

pub use services::ServiceRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelMoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelView {
    pub id: &'static str,
    pub title: &'static str,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelTree {
    pub id: &'static str,
    pub title: &'static str,
    pub children: Vec<PanelNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostAction {
    DispatchCommand(Command),
    RequestService(ServiceRequest),
    InvokePanelHandler {
        panel_id: String,
        handler_name: String,
        event_kind: String,
    },
    MovePanel {
        panel_id: String,
        direction: PanelMoveDirection,
    },
    SetPanelVisibility {
        panel_id: String,
        visible: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    Activate {
        panel_id: String,
        node_id: String,
    },
    SetValue {
        panel_id: String,
        node_id: String,
        value: usize,
    },
    DragValue {
        panel_id: String,
        node_id: String,
        from: usize,
        to: usize,
    },
    SetText {
        panel_id: String,
        node_id: String,
        value: String,
    },
    Keyboard {
        panel_id: String,
        shortcut: String,
        key: String,
        repeat: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputMode {
    Text,
    Numeric,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropdownOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerListItem {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
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
    ColorPreview {
        id: String,
        label: String,
        color: ColorRgba8,
    },
    ColorWheel {
        id: String,
        label: String,
        hue_degrees: usize,
        saturation: usize,
        value: usize,
        action: HostAction,
    },
    Button {
        id: String,
        label: String,
        action: HostAction,
        active: bool,
        fill_color: Option<ColorRgba8>,
    },
    Slider {
        id: String,
        label: String,
        action: HostAction,
        min: usize,
        max: usize,
        value: usize,
        fill_color: Option<ColorRgba8>,
    },
    TextInput {
        id: String,
        label: String,
        value: String,
        placeholder: String,
        binding_path: String,
        action: Option<HostAction>,
        input_mode: TextInputMode,
    },
    Dropdown {
        id: String,
        label: String,
        value: String,
        action: HostAction,
        options: Vec<DropdownOption>,
    },
    LayerList {
        id: String,
        label: String,
        selected_index: usize,
        action: HostAction,
        items: Vec<LayerListItem>,
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

    fn handles_keyboard_event(&self) -> bool {
        false
    }

    fn persistent_config(&self) -> Option<Value> {
        None
    }

    fn restore_persistent_config(&mut self, _config: &Value) {}

    fn handle_event(&mut self, event: &PanelEvent) -> Vec<HostAction> {
        match event {
            PanelEvent::Activate { panel_id, node_id }
            | PanelEvent::SetValue {
                panel_id, node_id, ..
            }
            | PanelEvent::DragValue {
                panel_id, node_id, ..
            }
            | PanelEvent::SetText {
                panel_id, node_id, ..
            } if panel_id == self.id() => {
                find_actions_in_nodes(&self.panel_tree().children, node_id)
            }
            PanelEvent::Keyboard { panel_id, .. } if panel_id == self.id() => Vec::new(),
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
        PanelNode::Text { .. } | PanelNode::ColorPreview { .. } | PanelNode::ColorWheel { .. } => {
            None
        }
        PanelNode::Button { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Button { .. } => None,
        PanelNode::Slider { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Slider { .. } => None,
        PanelNode::TextInput {
            id,
            action: Some(action),
            ..
        } if id == target_id => Some(vec![action.clone()]),
        PanelNode::TextInput { .. } => None,
        PanelNode::Dropdown { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::Dropdown { .. } => None,
        PanelNode::LayerList { id, action, .. } if id == target_id => Some(vec![action.clone()]),
        PanelNode::LayerList { .. } => None,
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
                    id: "tool.pen".to_string(),
                    label: "Pen".to_string(),
                    action: HostAction::DispatchCommand(Command::SetActiveTool {
                        tool: ToolKind::Pen,
                    }),
                    active: true,
                    fill_color: None,
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
                action: HostAction::DispatchCommand(Command::SetActiveTool { tool: ToolKind::Pen }),
                active: true,
                fill_color: None,
                ..
            } if label == "Pen"
        ));
    }

    #[test]
    fn activate_event_resolves_button_action() {
        let mut panel = TestPanel;

        let actions = panel.handle_event(&PanelEvent::Activate {
            panel_id: "test.panel".to_string(),
            node_id: "tool.pen".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::DispatchCommand(Command::SetActiveTool {
                tool: ToolKind::Pen,
            })]
        );
    }

    #[test]
    fn panel_tree_button_can_emit_service_request() {
        struct ServicePanel;

        impl PanelPlugin for ServicePanel {
            fn id(&self) -> &'static str {
                "test.service"
            }

            fn title(&self) -> &'static str {
                "Service"
            }

            fn panel_tree(&self) -> PanelTree {
                PanelTree {
                    id: self.id(),
                    title: self.title(),
                    children: vec![PanelNode::Button {
                        id: "project.save".to_string(),
                        label: "Save".to_string(),
                        action: HostAction::RequestService(ServiceRequest::new(
                            services::names::PROJECT_SAVE_CURRENT,
                        )),
                        active: false,
                        fill_color: None,
                    }],
                }
            }
        }

        let mut panel = ServicePanel;
        let actions = panel.handle_event(&PanelEvent::Activate {
            panel_id: "test.service".to_string(),
            node_id: "project.save".to_string(),
        });

        assert_eq!(
            actions,
            vec![HostAction::RequestService(ServiceRequest::new(
                services::names::PROJECT_SAVE_CURRENT,
            ))]
        );
    }

    #[test]
    fn set_value_event_resolves_slider_action() {
        struct SliderPanel;

        impl PanelPlugin for SliderPanel {
            fn id(&self) -> &'static str {
                "test.slider"
            }

            fn title(&self) -> &'static str {
                "Slider"
            }

            fn panel_tree(&self) -> PanelTree {
                PanelTree {
                    id: self.id(),
                    title: self.title(),
                    children: vec![PanelNode::Slider {
                        id: "color.red".to_string(),
                        label: "Red".to_string(),
                        action: HostAction::DispatchCommand(Command::Noop),
                        min: 0,
                        max: 255,
                        value: 0,
                        fill_color: None,
                    }],
                }
            }
        }

        let mut panel = SliderPanel;
        let actions = panel.handle_event(&PanelEvent::SetValue {
            panel_id: "test.slider".to_string(),
            node_id: "color.red".to_string(),
            value: 128,
        });

        assert_eq!(actions, vec![HostAction::DispatchCommand(Command::Noop)]);
    }
}
