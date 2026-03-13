//! `panel-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

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
    /// ID を計算して返す。
    fn id(&self) -> &'static str;

    /// title を計算して返す。
    fn title(&self) -> &'static str;

    /// 更新 に必要な処理を行う。
    fn update(&mut self, _document: &Document) {}

    /// commands を計算して返す。
    fn commands(&mut self) -> Vec<Command> {
        Vec::new()
    }

    /// debug summary を計算して返す。
    fn debug_summary(&self) -> String {
        String::new()
    }

    /// ビュー を計算して返す。
    fn view(&self) -> PanelView {
        PanelView {
            id: self.id(),
            title: self.title(),
            lines: Vec::new(),
        }
    }

    /// パネル tree 用の表示文字列を組み立てる。
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

    /// handles キーボード イベント を計算して返す。
    fn handles_keyboard_event(&self) -> bool {
        false
    }

    /// 現在の persistent 設定 を返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    fn persistent_config(&self) -> Option<Value> {
        None
    }

    /// Persistent 設定 を更新する。
    fn restore_persistent_config(&mut self, _config: &Value) {}

    /// 入力や種別に応じて処理を振り分ける。
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

/// find actions in nodes を計算して返す。
fn find_actions_in_nodes(nodes: &[PanelNode], target_id: &str) -> Vec<HostAction> {
    for node in nodes {
        if let Some(actions) = find_actions_in_node(node, target_id) {
            return actions;
        }
    }
    Vec::new()
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
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
        /// ID を計算して返す。
        fn id(&self) -> &'static str {
            "test.panel"
        }

        /// 現在の値を output へ変換する。
        ///
        /// 内部でサービス要求を発行します。
        fn title(&self) -> &'static str {
            "Test"
        }

        /// 現在の値を tree へ変換する。
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

    /// パネル プラグイン tree can expose button action が期待どおりに動作することを検証する。
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

    /// activate イベント resolves button action が期待どおりに動作することを検証する。
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

    /// パネル tree button can emit サービス 要求 が期待どおりに動作することを検証する。
    ///
    /// 内部でサービス要求を発行します。
    #[test]
    fn panel_tree_button_can_emit_service_request() {
        struct ServicePanel;

        impl PanelPlugin for ServicePanel {
            /// ID を計算して返す。
            fn id(&self) -> &'static str {
                "test.service"
            }

            /// 現在の値を output へ変換する。
            fn title(&self) -> &'static str {
                "Service"
            }

            /// 現在の値を tree へ変換する。
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

    /// 設定 値 イベント resolves slider action が期待どおりに動作することを検証する。
    #[test]
    fn set_value_event_resolves_slider_action() {
        struct SliderPanel;

        impl PanelPlugin for SliderPanel {
            /// ID を計算して返す。
            fn id(&self) -> &'static str {
                "test.slider"
            }

            /// title を計算して返す。
            fn title(&self) -> &'static str {
                "Slider"
            }

            /// 現在の値を tree へ変換する。
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
