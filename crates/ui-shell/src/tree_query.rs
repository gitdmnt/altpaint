//! `PanelNode` 木の問い合わせ helper をまとめる。
//!
//! focus・event 解決・text input binding 探索のような再帰走査をここへ集約し、
//! `UiShell` 本体から木走査の詳細を分離する。

use super::*;

/// node id に対応する `HostAction` を再帰探索する。
pub(super) fn find_panel_action(nodes: &[PanelNode], target_id: &str) -> Option<HostAction> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(action) = find_panel_action(children, target_id) {
                    return Some(action);
                }
            }
            PanelNode::ColorWheel { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::Button { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::Slider { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::TextInput {
                id,
                action: Some(action),
                ..
            } if id == target_id => return Some(action.clone()),
            PanelNode::Dropdown { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::LayerList { id, action, .. } if id == target_id => return Some(action.clone()),
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::ColorWheel { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

/// dropdown node を再帰探索する。
pub(super) fn find_dropdown_node<'a>(nodes: &'a [PanelNode], target_id: &str) -> Option<&'a PanelNode> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(found) = find_dropdown_node(children, target_id) {
                    return Some(found);
                }
            }
            PanelNode::Dropdown { id, .. } if id == target_id => return Some(node),
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::ColorWheel { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

/// text input binding を探索する。
pub(super) fn find_text_input_binding(
    nodes: &[PanelNode],
    target_id: &str,
) -> Option<(String, TextInputMode)> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(binding) = find_text_input_binding(children, target_id) {
                    return Some(binding);
                }
            }
            PanelNode::TextInput {
                id,
                binding_path,
                input_mode,
                ..
            } if id == target_id => {
                if binding_path.is_empty() {
                    return None;
                }
                return Some((binding_path.clone(), *input_mode));
            }
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::ColorWheel { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

/// text input の現在値を探索する。
pub(super) fn find_text_input_value(
    nodes: &[PanelNode],
    target_id: &str,
) -> Option<(String, TextInputMode)> {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => {
                if let Some(value) = find_text_input_value(children, target_id) {
                    return Some(value);
                }
            }
            PanelNode::TextInput {
                id,
                value,
                input_mode,
                ..
            } if id == target_id => return Some((value.clone(), *input_mode)),
            PanelNode::Text { .. }
            | PanelNode::ColorPreview { .. }
            | PanelNode::ColorWheel { .. }
            | PanelNode::Button { .. }
            | PanelNode::Slider { .. }
            | PanelNode::TextInput { .. }
            | PanelNode::Dropdown { .. }
            | PanelNode::LayerList { .. } => {}
        }
    }
    None
}

/// focus 対象になり得る node を走査して収集する。
pub(super) fn collect_focus_targets(panel_id: &str, nodes: &[PanelNode], targets: &mut Vec<FocusTarget>) {
    for node in nodes {
        match node {
            PanelNode::Column { children, .. }
            | PanelNode::Row { children, .. }
            | PanelNode::Section { children, .. } => collect_focus_targets(panel_id, children, targets),
            PanelNode::Button { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::TextInput { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::Dropdown { id, .. } | PanelNode::LayerList { id, .. } | PanelNode::ColorWheel { id, .. } => targets.push(FocusTarget {
                panel_id: panel_id.to_string(),
                node_id: id.clone(),
            }),
            PanelNode::Text { .. } | PanelNode::ColorPreview { .. } | PanelNode::Slider { .. } => {}
        }
    }
}
