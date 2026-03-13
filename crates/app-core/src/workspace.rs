use serde::{Deserialize, Serialize};

use crate::{WindowPoint, WindowRect};

/// Is 表示状態 by 既定 かどうかを返す。
fn is_visible_by_default() -> bool {
    true
}

/// 既定の 表示状態 を返す。
fn default_visible() -> bool {
    is_visible_by_default()
}

/// 既定の パネル 幅 を返す。
fn default_panel_width() -> usize {
    300
}

/// 既定の パネル 高さ を返す。
fn default_panel_height() -> usize {
    220
}

/// 既定の パネル anchor を返す。
fn default_panel_anchor() -> WorkspacePanelAnchor {
    WorkspacePanelAnchor::TopLeft
}

/// 浮動パネルのアンカー基準オフセット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspacePanelPosition {
    pub x: usize,
    pub y: usize,
}

/// 浮動パネルの配置基準となる画面隅。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspacePanelAnchor {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// 浮動パネルのサイズ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePanelSize {
    #[serde(default = "default_panel_width")]
    pub width: usize,
    #[serde(default = "default_panel_height")]
    pub height: usize,
}

impl Default for WorkspacePanelSize {
    /// 既定値を持つインスタンスを返す。
    fn default() -> Self {
        Self {
            width: default_panel_width(),
            height: default_panel_height(),
        }
    }
}

/// パネル配置と表示状態を保存する最小ワークスペース設定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceLayout {
    #[serde(default)]
    pub panels: Vec<WorkspacePanelState>,
}

/// 個々のパネルの並び順と表示状態。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePanelState {
    pub id: String,
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default = "default_panel_anchor")]
    pub anchor: WorkspacePanelAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<WorkspacePanelPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<WorkspacePanelSize>,
}

impl WorkspacePanelState {
    /// 解決済みの position を返す。
    pub fn resolved_position(
        &self,
        viewport_width: usize,
        viewport_height: usize,
        panel_size: WorkspacePanelSize,
        fallback: WorkspacePanelPosition,
    ) -> WorkspacePanelPosition {
        let offset = self.position.unwrap_or(fallback);
        let width = panel_size.width.min(viewport_width);
        let height = panel_size.height.min(viewport_height);
        let max_x = viewport_width.saturating_sub(width);
        let max_y = viewport_height.saturating_sub(height);

        match self.anchor {
            WorkspacePanelAnchor::TopLeft => WorkspacePanelPosition {
                x: offset.x.min(max_x),
                y: offset.y.min(max_y),
            },
            WorkspacePanelAnchor::TopRight => WorkspacePanelPosition {
                x: viewport_width
                    .saturating_sub(width)
                    .saturating_sub(offset.x)
                    .min(max_x),
                y: offset.y.min(max_y),
            },
            WorkspacePanelAnchor::BottomLeft => WorkspacePanelPosition {
                x: offset.x.min(max_x),
                y: viewport_height
                    .saturating_sub(height)
                    .saturating_sub(offset.y)
                    .min(max_y),
            },
            WorkspacePanelAnchor::BottomRight => WorkspacePanelPosition {
                x: viewport_width
                    .saturating_sub(width)
                    .saturating_sub(offset.x)
                    .min(max_x),
                y: viewport_height
                    .saturating_sub(height)
                    .saturating_sub(offset.y)
                    .min(max_y),
            },
        }
    }

    /// 解決済みの ウィンドウ position を返す。
    pub fn resolved_window_position(
        &self,
        viewport: WindowRect,
        panel_size: WorkspacePanelSize,
        fallback: WorkspacePanelPosition,
    ) -> WindowPoint {
        let position =
            self.resolved_position(viewport.width, viewport.height, panel_size, fallback);
        WindowPoint::new(position.x as i32, position.y as i32)
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub fn set_position_from_absolute(
        &mut self,
        x: usize,
        y: usize,
        viewport_width: usize,
        viewport_height: usize,
        panel_size: WorkspacePanelSize,
    ) {
        let width = panel_size.width.min(viewport_width);
        let height = panel_size.height.min(viewport_height);
        let max_x = viewport_width.saturating_sub(width);
        let max_y = viewport_height.saturating_sub(height);
        let absolute_x = x.min(max_x);
        let absolute_y = y.min(max_y);

        let anchor = nearest_panel_anchor(
            absolute_x,
            absolute_y,
            viewport_width,
            viewport_height,
            width,
            height,
        );
        let position = match anchor {
            WorkspacePanelAnchor::TopLeft => WorkspacePanelPosition {
                x: absolute_x,
                y: absolute_y,
            },
            WorkspacePanelAnchor::TopRight => WorkspacePanelPosition {
                x: viewport_width
                    .saturating_sub(width)
                    .saturating_sub(absolute_x),
                y: absolute_y,
            },
            WorkspacePanelAnchor::BottomLeft => WorkspacePanelPosition {
                x: absolute_x,
                y: viewport_height
                    .saturating_sub(height)
                    .saturating_sub(absolute_y),
            },
            WorkspacePanelAnchor::BottomRight => WorkspacePanelPosition {
                x: viewport_width
                    .saturating_sub(width)
                    .saturating_sub(absolute_x),
                y: viewport_height
                    .saturating_sub(height)
                    .saturating_sub(absolute_y),
            },
        };

        self.anchor = anchor;
        self.position = Some(position);
        self.size = Some(panel_size);
    }

    /// Position from ウィンドウ 点 を設定する。
    pub fn set_position_from_window_point(
        &mut self,
        point: WindowPoint,
        viewport: WindowRect,
        panel_size: WorkspacePanelSize,
    ) {
        self.set_position_from_absolute(
            point.x.max(0) as usize,
            point.y.max(0) as usize,
            viewport.width,
            viewport.height,
            panel_size,
        );
    }
}

/// nearest パネル anchor に必要な処理を行う。
fn nearest_panel_anchor(
    x: usize,
    y: usize,
    viewport_width: usize,
    viewport_height: usize,
    panel_width: usize,
    panel_height: usize,
) -> WorkspacePanelAnchor {
    let right_gap = viewport_width.saturating_sub(panel_width).saturating_sub(x);
    let bottom_gap = viewport_height
        .saturating_sub(panel_height)
        .saturating_sub(y);
    let mut best = (WorkspacePanelAnchor::TopLeft, x.saturating_add(y));
    for candidate in [
        (WorkspacePanelAnchor::TopRight, right_gap.saturating_add(y)),
        (
            WorkspacePanelAnchor::BottomLeft,
            x.saturating_add(bottom_gap),
        ),
        (
            WorkspacePanelAnchor::BottomRight,
            right_gap.saturating_add(bottom_gap),
        ),
    ] {
        if candidate.1 < best.1 {
            best = candidate;
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ワークスペース パネル visibility defaults to true when missing が期待どおりに動作することを検証する。
    #[test]
    fn workspace_panel_visibility_defaults_to_true_when_missing() {
        let panel: WorkspacePanelState = serde_json::from_str(r#"{"id":"builtin.tool-palette"}"#)
            .expect("panel should deserialize");

        assert!(panel.visible);
        assert_eq!(panel.position, None);
        assert_eq!(panel.size, None);
    }

    /// ワークスペース レイアウト roundtrip preserves order and visibility が期待どおりに動作することを検証する。
    #[test]
    fn workspace_layout_roundtrip_preserves_order_and_visibility() {
        let layout = WorkspaceLayout {
            panels: vec![
                WorkspacePanelState {
                    id: "builtin.tool-palette".to_string(),
                    visible: false,
                    anchor: WorkspacePanelAnchor::TopLeft,
                    position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
                    size: Some(WorkspacePanelSize {
                        width: 280,
                        height: 320,
                    }),
                },
                WorkspacePanelState {
                    id: "builtin.layers-panel".to_string(),
                    visible: true,
                    anchor: WorkspacePanelAnchor::TopLeft,
                    position: Some(WorkspacePanelPosition { x: 340, y: 72 }),
                    size: Some(WorkspacePanelSize {
                        width: 320,
                        height: 360,
                    }),
                },
            ],
        };

        let json = serde_json::to_string(&layout).expect("layout should serialize");
        let restored: WorkspaceLayout =
            serde_json::from_str(&json).expect("layout should deserialize");

        assert_eq!(restored, layout);
    }

    /// ワークスペース パネル anchor defaults to top left when missing が期待どおりに動作することを検証する。
    #[test]
    fn workspace_panel_anchor_defaults_to_top_left_when_missing() {
        let panel: WorkspacePanelState =
            serde_json::from_str(r#"{"id":"builtin.tool-palette","position":{"x":24,"y":72}}"#)
                .expect("panel should deserialize");

        assert_eq!(panel.anchor, WorkspacePanelAnchor::TopLeft);
    }

    /// resolved position uses anchor relative offsets が期待どおりに動作することを検証する。
    #[test]
    fn resolved_position_uses_anchor_relative_offsets() {
        let panel = WorkspacePanelState {
            id: "builtin.layers-panel".to_string(),
            visible: true,
            anchor: WorkspacePanelAnchor::TopRight,
            position: Some(WorkspacePanelPosition { x: 24, y: 72 }),
            size: Some(WorkspacePanelSize {
                width: 300,
                height: 220,
            }),
        };

        assert_eq!(
            panel.resolved_position(
                1280,
                800,
                panel.size.expect("size exists"),
                WorkspacePanelPosition::default(),
            ),
            WorkspacePanelPosition { x: 956, y: 72 }
        );
    }

    /// 設定 position from absolute picks nearest corner anchor が期待どおりに動作することを検証する。
    #[test]
    fn set_position_from_absolute_picks_nearest_corner_anchor() {
        let mut panel = WorkspacePanelState {
            id: "builtin.layers-panel".to_string(),
            visible: true,
            anchor: WorkspacePanelAnchor::TopLeft,
            position: None,
            size: Some(WorkspacePanelSize {
                width: 300,
                height: 220,
            }),
        };

        panel.set_position_from_absolute(930, 68, 1280, 800, panel.size.expect("size exists"));

        assert_eq!(panel.anchor, WorkspacePanelAnchor::TopRight);
        assert_eq!(
            panel.position,
            Some(WorkspacePanelPosition { x: 50, y: 68 })
        );
        assert_eq!(
            panel.resolved_position(
                1280,
                800,
                panel.size.expect("size exists"),
                WorkspacePanelPosition::default(),
            ),
            WorkspacePanelPosition { x: 930, y: 68 }
        );
    }
}
