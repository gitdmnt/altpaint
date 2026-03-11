//! ポインタ・キーボード・パネル入力の解釈を `DesktopApp` へ追加する。
//!
//! OS 由来の生イベントをドキュメント編集やパネル操作へ変換し、
//! ランタイム側が UI 詳細を知らずに済むようにする。

use app_core::{CanvasPoint, Command, PaintInput, ToolKind, WindowPoint};

use super::DesktopApp;
use crate::canvas_bridge::{
    CanvasInputState, CanvasPointerEvent, map_view_to_canvas_with_transform,
};
use crate::frame::brush_preview_rect;

impl DesktopApp {
    /// 現在のポインタ位置からキャンバスホバー状態を更新する。
    pub(crate) fn update_canvas_hover(&mut self, x: i32, y: i32) -> bool {
        let previous = self.hover_canvas_position;
        let next = self.hover_canvas_position_from_window(WindowPoint::new(x, y));
        if next == self.hover_canvas_position {
            return false;
        }
        self.hover_canvas_position = next;

        let Some(layout) = self.layout.as_ref().map(|layout| layout.canvas_host_rect) else {
            self.rebuild_present_frame();
            return true;
        };
        let (bitmap_width, bitmap_height) = self.canvas_dimensions();

        let transform = self.document.view_transform;
        if let Some(previous) = previous.and_then(|position| {
            brush_preview_rect(
                layout,
                bitmap_width,
                bitmap_height,
                transform,
                position,
                self.brush_preview_size().unwrap_or(1),
            )
        }) {
            self.append_canvas_host_dirty_rect(previous);
        }
        if let Some(next) = next.and_then(|position| {
            brush_preview_rect(
                layout,
                bitmap_width,
                bitmap_height,
                transform,
                position,
                self.brush_preview_size().unwrap_or(1),
            )
        }) {
            self.append_canvas_host_dirty_rect(next);
        }
        true
    }

    /// ポインタ押下をキャンバスまたはパネル操作へ振り分ける。
    #[allow(dead_code)]
    pub(crate) fn handle_pointer_pressed(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_pressed_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_pressed_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        let point = WindowPoint::new(x, y);
        if self.begin_panel_interaction(point) {
            return true;
        }

        if self.canvas_display_contains_window(point) {
            return self.handle_canvas_pointer("down", point, pressure);
        }

        if self.canvas_position_from_window(point).is_some() {
            return self.handle_canvas_pointer("down", point, pressure);
        }

        false
    }

    /// ポインタ解放を現在の操作状態に応じて処理する。
    #[allow(dead_code)]
    pub(crate) fn handle_pointer_released(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_released_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_released_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        let point = WindowPoint::new(x, y);
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("up", point, pressure);
        }
        if self.panel_interaction.active_panel_drag.take().is_some() {
            self.panel_interaction.pending_panel_press = None;
            self.persist_session_state();
            return false;
        }
        self.handle_panel_pointer(point)
    }

    /// ポインタ移動をドラッグ中の対象へ配送する。
    pub(crate) fn handle_pointer_dragged(&mut self, x: i32, y: i32) -> bool {
        self.handle_pointer_dragged_with_pressure(x, y, 1.0)
    }

    pub(crate) fn handle_pointer_dragged_with_pressure(
        &mut self,
        x: i32,
        y: i32,
        pressure: f32,
    ) -> bool {
        let point = WindowPoint::new(x, y);
        if self.canvas_input.is_drawing {
            return self.handle_canvas_pointer("drag", point, pressure);
        }

        if self.panel_interaction.active_panel_drag.is_some() {
            return self.drag_panel_interaction(point);
        }

        false
    }

    /// キャンバス上のポインタ操作を描画コマンドへ変換して適用する。
    pub(crate) fn handle_canvas_pointer(
        &mut self,
        action: &str,
        point: WindowPoint,
        pressure: f32,
    ) -> bool {
        let canvas_position = self.canvas_position_from_window(point).or_else(|| {
            (action != "down" && self.canvas_input.is_drawing)
                .then(|| self.canvas_position_from_window_clamped(point))
                .flatten()
        });
        let Some(page_point) = canvas_position else {
            if action == "up" {
                self.canvas_input = CanvasInputState::default();
            }
            return false;
        };

        let active_tool = self.document.active_tool;
        if active_tool == ToolKind::PanelRect {
            return self.handle_panel_rect_pointer(action, page_point);
        }

        let page_point = if action != "down" && self.canvas_input.is_drawing {
            self.clamp_page_position_to_active_panel(page_point)
                .unwrap_or(page_point)
        } else {
            page_point
        };
        let inside_active_panel = self.page_position_in_active_panel(page_point).is_some();
        if !inside_active_panel {
            if action == "up" {
                self.canvas_input = CanvasInputState::default();
            }
            return false;
        }

        match action {
            "down" => {
                if active_tool == ToolKind::Bucket {
                    let Some(local_point) = self.page_to_active_panel_local(page_point) else {
                        return false;
                    };
                    return self.execute_paint_input(PaintInput::FloodFill { at: local_point });
                }
                self.canvas_input.is_drawing = true;
                self.canvas_input.last_position = Some(page_point);
                self.canvas_input.last_smoothed_position =
                    Some((page_point.x as f32, page_point.y as f32));
                if active_tool == ToolKind::LassoBucket {
                    self.canvas_input.lasso_points.clear();
                    self.canvas_input.lasso_points.push(page_point);
                    if let Some(layout) = self.layout.as_ref() {
                        self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    }
                    return false;
                }
                let Some(local_point) = self.page_to_active_panel_local(page_point) else {
                    return false;
                };
                self.execute_paint_input(PaintInput::Stamp {
                    at: local_point,
                    pressure,
                })
            }
            "drag" if self.canvas_input.is_drawing => {
                if active_tool == ToolKind::LassoBucket {
                    if self.canvas_input.lasso_points.last().copied() != Some(page_point) {
                        self.canvas_input.lasso_points.push(page_point);
                        if let Some(layout) = self.layout.as_ref() {
                            self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                        }
                    }
                    self.canvas_input.last_position = Some(page_point);
                    return false;
                }
                let next_position = self.stabilized_canvas_position(page_point);
                let from = self.canvas_input.last_position;
                if from == Some(next_position) {
                    return false;
                }
                let changed = from
                    .and_then(|previous| {
                        Some((
                            self.page_to_active_panel_local(previous)?,
                            self.page_to_active_panel_local(next_position)?,
                        ))
                    })
                    .is_some_and(|(from_local, to_local)| {
                        self.execute_paint_input(PaintInput::StrokeSegment {
                            from: from_local,
                            to: to_local,
                            pressure,
                        })
                    });
                self.canvas_input.last_position = Some(next_position);
                changed
            }
            "up" => {
                if active_tool == ToolKind::LassoBucket {
                    let changed = if self.canvas_input.lasso_points.len() >= 3 {
                        let Some(local_points) = self
                            .canvas_input
                            .lasso_points
                            .iter()
                            .map(|&point| self.page_to_active_panel_local(point))
                            .collect::<Option<Vec<_>>>()
                        else {
                            self.canvas_input = CanvasInputState::default();
                            return false;
                        };
                        self.execute_paint_input(PaintInput::LassoFill {
                            points: local_points,
                        })
                    } else {
                        false
                    };
                    if let Some(layout) = self.layout.as_ref() {
                        self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    }
                    self.canvas_input = CanvasInputState::default();
                    return changed;
                }
                let from = self.canvas_input.last_position;
                let changed = if self.canvas_input.is_drawing && from != Some(page_point) {
                    from.and_then(|previous| {
                        Some((
                            self.page_to_active_panel_local(previous)?,
                            self.page_to_active_panel_local(page_point)?,
                        ))
                    })
                    .is_some_and(|(from_local, to_local)| {
                        self.execute_paint_input(PaintInput::StrokeSegment {
                            from: from_local,
                            to: to_local,
                            pressure,
                        })
                    })
                } else {
                    false
                };
                self.canvas_input = CanvasInputState::default();
                changed
            }
            _ => false,
        }
    }

    fn stabilized_canvas_position(&mut self, point: CanvasPoint) -> CanvasPoint {
        if self.document.active_tool != ToolKind::Pen {
            self.canvas_input.last_smoothed_position = Some((point.x as f32, point.y as f32));
            return point;
        }
        let stabilization = self
            .document
            .active_pen_preset()
            .map(|preset| preset.stabilization)
            .unwrap_or_default();
        if stabilization == 0 {
            self.canvas_input.last_smoothed_position = Some((point.x as f32, point.y as f32));
            return point;
        }

        let blend = (1.0 / (1.0 + stabilization as f32 / 12.0)).clamp(0.05, 1.0);
        let previous = self
            .canvas_input
            .last_smoothed_position
            .unwrap_or((point.x as f32, point.y as f32));
        let next = (
            previous.0 + (point.x as f32 - previous.0) * blend,
            previous.1 + (point.y as f32 - previous.1) * blend,
        );
        self.canvas_input.last_smoothed_position = Some(next);
        CanvasPoint::new(
            next.0.round().max(0.0) as usize,
            next.1.round().max(0.0) as usize,
        )
    }

    /// ウィンドウ座標をキャンバスビットマップ座標へ変換する。
    fn canvas_position_from_window(&self, point: WindowPoint) -> Option<CanvasPoint> {
        let layout = self.layout.as_ref()?;
        if !layout.canvas_host_rect.contains(point.x, point.y) {
            return None;
        }

        self.canvas_position_from_window_clamped(point)
    }

    fn canvas_display_contains_window(&self, point: WindowPoint) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|layout| layout.canvas_display_rect.contains(point.x, point.y))
    }

    fn hover_canvas_position_from_window(&self, point: WindowPoint) -> Option<CanvasPoint> {
        let position = self.canvas_position_from_window(point)?;
        match self.document.active_tool {
            ToolKind::PanelRect => Some(position),
            ToolKind::Pen | ToolKind::Eraser | ToolKind::Bucket | ToolKind::LassoBucket => self
                .page_position_in_active_panel(position)
                .map(|_| position),
        }
    }

    /// ウィンドウ座標をキャンバスビットマップ座標へクランプ付きで変換する。
    pub(crate) fn canvas_position_from_window_clamped(
        &self,
        point: WindowPoint,
    ) -> Option<CanvasPoint> {
        let layout = self.layout.as_ref()?;
        let window_rect = app_core::WindowRect::new(
            layout.canvas_host_rect.x,
            layout.canvas_host_rect.y,
            layout.canvas_host_rect.width,
            layout.canvas_host_rect.height,
        );
        let viewport_point = window_rect.clamp_to_canvas_viewport_point(point)?;
        map_view_to_canvas_with_transform(
            &render::RenderFrame {
                width: self.canvas_dimensions().0,
                height: self.canvas_dimensions().1,
                pixels: Vec::new(),
            },
            CanvasPointerEvent {
                position: viewport_point,
                width: layout.canvas_host_rect.width as i32,
                height: layout.canvas_host_rect.height as i32,
            },
            self.document.view_transform,
        )
    }

    fn page_position_in_active_panel(&self, point: CanvasPoint) -> Option<CanvasPoint> {
        let bounds = self.document.active_panel_bounds()?;
        bounds.contains_canvas_point(point).then_some(point)
    }

    fn page_to_active_panel_local(&self, point: CanvasPoint) -> Option<app_core::PanelLocalPoint> {
        let bounds = self.document.active_panel_bounds()?;
        bounds.canvas_to_panel_local(point)
    }

    fn clamp_page_position_to_active_panel(&self, point: CanvasPoint) -> Option<CanvasPoint> {
        let bounds = self.document.active_panel_bounds()?;
        bounds.clamp_canvas_point(point)
    }

    fn handle_panel_rect_pointer(&mut self, action: &str, point: CanvasPoint) -> bool {
        match action {
            "down" => {
                self.canvas_input.is_drawing = true;
                self.canvas_input.panel_rect_anchor = Some(point);
                self.canvas_input.last_position = Some(point);
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                }
                true
            }
            "drag" if self.canvas_input.is_drawing => {
                if self.canvas_input.last_position == Some(point) {
                    return false;
                }
                self.canvas_input.last_position = Some(point);
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                }
                true
            }
            "up" => {
                let preview = self.panel_creation_preview_bounds();
                let created = preview
                    .filter(|bounds| bounds.width >= 8 && bounds.height >= 8)
                    .is_some_and(|bounds| {
                        self.execute_command(Command::CreatePanel {
                            x: bounds.x,
                            y: bounds.y,
                            width: bounds.width,
                            height: bounds.height,
                        })
                    });
                self.canvas_input = CanvasInputState::default();
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    return true;
                }
                created
            }
            _ => false,
        }
    }

}
