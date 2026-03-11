//! ポインタ・キーボード・パネル入力の解釈を `DesktopApp` へ追加する。
//!
//! OS 由来の生イベントをドキュメント編集やパネル操作へ変換し、
//! ランタイム側が UI 詳細を知らずに済むようにする。

use app_core::{CanvasPoint, Command, ToolKind, WindowPoint};
use canvas::{
    CanvasGestureUpdate, CanvasInputState, CanvasPointerAction, CanvasPointerEvent,
    advance_pointer_gesture, map_view_to_canvas_with_transform,
};

use super::DesktopApp;
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
        let Some(pointer_action) = pointer_action(action) else {
            return false;
        };
        let canvas_position = self.canvas_position_from_window(point).or_else(|| {
            (action != "down" && self.canvas_input.is_drawing)
                .then(|| self.canvas_position_from_window_clamped(point))
                .flatten()
        });
        let Some(page_point) = canvas_position else {
            if pointer_action == CanvasPointerAction::Up {
                self.canvas_input.reset();
            }
            return false;
        };

        let active_tool = self.document.active_tool;
        let active_panel_bounds = self.document.active_panel_bounds();

        let page_point = if action != "down" && self.canvas_input.is_drawing {
            active_panel_bounds
                .and_then(|bounds| bounds.clamp_canvas_point(page_point))
                .unwrap_or(page_point)
        } else {
            page_point
        };
        let inside_active_panel = active_panel_bounds
            .is_some_and(|bounds| bounds.contains_canvas_point(page_point));
        if active_tool != ToolKind::PanelRect && !inside_active_panel {
            if pointer_action == CanvasPointerAction::Up {
                self.canvas_input.reset();
            }
            return false;
        }

        let stabilization = self
            .document
            .active_pen_preset()
            .map(|preset| preset.stabilization)
            .unwrap_or_default();
        let update = advance_pointer_gesture(
            &mut self.canvas_input,
            pointer_action,
            page_point,
            active_tool,
            pressure,
            stabilization,
            |canvas_point| active_panel_bounds.and_then(|bounds| bounds.canvas_to_panel_local(canvas_point)),
        );

        match update {
            CanvasGestureUpdate::None => false,
            CanvasGestureUpdate::Paint(input) => {
                let changed = self.execute_paint_input(input);
                if active_tool == ToolKind::LassoBucket
                    && pointer_action == CanvasPointerAction::Up
                    && let Some(layout) = self.layout.as_ref()
                {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                }
                changed
            }
            CanvasGestureUpdate::LassoPreviewChanged => {
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                }
                false
            }
            CanvasGestureUpdate::PanelRectPreviewChanged => {
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                }
                true
            }
            CanvasGestureUpdate::PanelRectCommitted { anchor, current } => {
                let (page_width, page_height) = self.document.active_page_dimensions();
                let preview_state = CanvasInputState {
                    is_drawing: false,
                    last_position: Some(current),
                    last_smoothed_position: None,
                    lasso_points: Vec::new(),
                    panel_rect_anchor: Some(anchor),
                };
                let created = canvas::panel_creation_preview_bounds(
                    &preview_state,
                    page_width,
                    page_height,
                )
                .filter(|bounds| bounds.width >= 8 && bounds.height >= 8)
                .is_some_and(|bounds| {
                    self.execute_command(Command::CreatePanel {
                        x: bounds.x,
                        y: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    })
                });
                if let Some(layout) = self.layout.as_ref() {
                    self.append_canvas_host_dirty_rect(layout.canvas_host_rect);
                    return true;
                }
                created
            }
        }
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
}

fn pointer_action(action: &str) -> Option<CanvasPointerAction> {
    match action {
        "down" => Some(CanvasPointerAction::Down),
        "drag" => Some(CanvasPointerAction::Drag),
        "up" => Some(CanvasPointerAction::Up),
        _ => None,
    }
}
