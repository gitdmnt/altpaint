//! `plugin-api` は、標準パネルや将来の拡張機能が従う最小インターフェースを定義する。

use app_core::{Command, Document};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelView {
    pub id: &'static str,
    pub title: &'static str,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelUi {
    pub id: &'static str,
    pub title: &'static str,
    pub nodes: Vec<PanelUiNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelUiNode {
    Section {
        title: String,
        children: Vec<PanelUiNode>,
    },
    Text(String),
    CommandButton {
        id: String,
        label: String,
        command: Command,
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

    fn ui(&self) -> PanelUi {
        PanelUi {
            id: self.id(),
            title: self.title(),
            nodes: self
                .view()
                .lines
                .into_iter()
                .map(PanelUiNode::Text)
                .collect(),
        }
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

        fn ui(&self) -> PanelUi {
            PanelUi {
                id: self.id(),
                title: self.title(),
                nodes: vec![PanelUiNode::CommandButton {
                    id: "tool.brush".to_string(),
                    label: "Brush".to_string(),
                    command: Command::SetActiveTool {
                        tool: ToolKind::Brush,
                    },
                    active: true,
                }],
            }
        }
    }

    #[test]
    fn panel_plugin_ui_can_expose_command_button() {
        let panel = TestPanel;
        let ui = panel.ui();

        assert_eq!(ui.id, "test.panel");
        assert!(matches!(
            &ui.nodes[0],
            PanelUiNode::CommandButton {
                label,
                command: Command::SetActiveTool { tool: ToolKind::Brush },
                active: true,
                ..
            } if label == "Brush"
        ));
    }
}
