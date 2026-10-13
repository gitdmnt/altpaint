//! ポインタ・キーボード・パネル入力の解釈を `DesktopApp` へ追加する。
//!
//! OS 由来の生イベントをドキュメント編集やパネル操作へ変換し、
//! ランタイム側が UI 詳細を知らずに済むようにする。

use document_model::DocumentCommand;
use editor_state::GestureKind;
use geometry::{PagePoint, WindowPoint, WindowRect};
use paint_engine::{CanvasGestureUpdate, CanvasPointerAction, advance_pointer_gesture};

use super::DesktopApp;
use crate::features::koma::{KomaGesture, KomaGestureUpdate, advance_koma_gesture};

impl DesktopApp {
    pub(crate) fn update_canvas_hover(&mut self, x: i32, y: i32) -> bool {
        let previous = self.paint.hover_canvas_position;
        let next = self.hover_canvas_position_from_window(WindowPoint::new(x, y));
        if next == self.paint.hover_canvas_position {
            return false;
        }
        self.paint.hover_canvas_position = next;

        let Some(layout) = self.layout.as_ref().map(|layout| layout.canvas_host_rect) else {
            self.rebuild_present_frame();
            return true;
        };
        let (bitmap_width, bitmap_height) = self.canvas_dimensions();

        let transform = self.document.session.view_transform;
        let geometry = canvas_geometry::CanvasViewGeometry::compute(
            layout,
            bitmap_width,
            bitmap_height,
            transform,
        );
        let brush_diameter = self.brush_preview_size().unwrap_or(1) as f32;
        if let Some(previous) = previous.and_then(|position| {
            geometry.and_then(|geometry| {
                geometry.brush_preview_rect_for_diameter(position, brush_diameter)
            })
        }) {
            self.append_temp_overlay_dirty_rect(previous);
        }
        if let Some(next) = next.and_then(|position| {
            geometry.and_then(|geometry| {
                geometry.brush_preview_rect_for_diameter(position, brush_diameter)
            })
        }) {
            self.append_temp_overlay_dirty_rect(next);
        }
        true
    }

    /// テスト専用: 筆圧 1.0 固定の押下ショートカット。
    #[cfg(test)]
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
            return self.handle_canvas_pointer(CanvasPointerAction::Down, point, pressure);
        }

        if self.canvas_position_from_window(point).is_some() {
            return self.handle_canvas_pointer(CanvasPointerAction::Down, point, pressure);
        }

        false
    }

    /// テスト専用: 筆圧 1.0 固定の解放ショートカット。
    #[cfg(test)]
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
        if self.is_canvas_interacting() {
            return self.handle_canvas_pointer(CanvasPointerAction::Up, point, pressure);
        }
        if self.panel_interaction.active_panel_resize.take().is_some() {
            self.panel_interaction.pending_panel_press = None;
            self.persist_session_state();
            return false;
        }
        if self.panel_interaction.active_panel_drag.take().is_some() {
            self.panel_interaction.pending_panel_press = None;
            self.persist_session_state();
            return false;
        }
        self.handle_panel_pointer(point)
    }

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
        if self.is_canvas_interacting() {
            return self.handle_canvas_pointer(CanvasPointerAction::Drag, point, pressure);
        }

        if self.panel_interaction.active_panel_drag.is_some()
            || self.panel_interaction.active_panel_resize.is_some()
        {
            return self.drag_panel_interaction(point);
        }

        false
    }

    pub(crate) fn handle_canvas_pointer(
        &mut self,
        action: CanvasPointerAction,
        point: WindowPoint,
        pressure: f32,
    ) -> bool {
        let canvas_position = self.canvas_position_from_window(point).or_else(|| {
            (action != CanvasPointerAction::Down && self.is_canvas_interacting())
                .then(|| self.canvas_position_from_window_clamped(point))
                .flatten()
        });
        let Some(page_point) = canvas_position else {
            if action == CanvasPointerAction::Up {
                self.reset_canvas_gestures();
            }
            return false;
        };

        let active_tool = self.document.session.active_tool();
        let tool = self.document.session.active_tool_descriptor();
        let active_koma_bounds = self.document.active_koma_bounds();

        // コマ作成 (KomaRect) は別経路で処理する (BL-081)。ジェスチャ進行中の
        // drag/up はアクティブコマ境界へクランプする (分離前の挙動を維持)。
        if tool.gesture_kind() == GestureKind::KomaRect {
            let page_point = if action != CanvasPointerAction::Down && self.koma_gesture.is_drawing {
                active_koma_bounds
                    .and_then(|bounds| bounds.clamp_canvas_point(page_point))
                    .unwrap_or(page_point)
            } else {
                page_point
            };
            return self.handle_koma_rect_pointer(action, page_point);
        }

        let page_point = if action != CanvasPointerAction::Down && self.paint.canvas_input.is_drawing {
            active_koma_bounds
                .and_then(|bounds| bounds.clamp_canvas_point(page_point))
                .unwrap_or(page_point)
        } else {
            page_point
        };
        let inside_active_koma =
            active_koma_bounds.is_some_and(|bounds| bounds.contains_canvas_point(page_point));
        if !inside_active_koma {
            if action == CanvasPointerAction::Up {
                self.paint.canvas_input.reset();
            }
            return false;
        }

        let stabilization = self
            .document
            .session
            .active_pen_preset()
            .map(|preset| preset.stabilization)
            .unwrap_or_default();
        let update = advance_pointer_gesture(
            &mut self.paint.canvas_input,
            action,
            page_point,
            active_tool,
            pressure,
            stabilization,
            |page_point| {
                active_koma_bounds.and_then(|bounds| bounds.canvas_to_koma_local(page_point))
            },
        );

        match update {
            CanvasGestureUpdate::None => false,
            CanvasGestureUpdate::Paint(input) => {
                let changed = self.apply_paint_input(input);
                if action == CanvasPointerAction::Up {
                    self.commit_stroke_to_history();
                }
                if tool.gesture_kind() == GestureKind::LassoFill
                    && action == CanvasPointerAction::Up
                    && let Some(layout) = self.layout.as_ref()
                {
                    self.append_temp_overlay_dirty_rect(layout.canvas_host_rect);
                }
                changed
            }
            CanvasGestureUpdate::LassoPreviewChanged => {
                if let Some(layout) = self.layout.as_ref() {
                    self.append_temp_overlay_dirty_rect(layout.canvas_host_rect);
                }
                true
            }
        }
    }

    /// コマ作成 (KomaRect) ジェスチャを処理する (BL-081)。
    fn handle_koma_rect_pointer(
        &mut self,
        pointer_action: CanvasPointerAction,
        page_point: PagePoint,
    ) -> bool {
        match advance_koma_gesture(&mut self.koma_gesture, pointer_action, page_point) {
            KomaGestureUpdate::None => false,
            KomaGestureUpdate::PreviewChanged => {
                if let Some(layout) = self.layout.as_ref() {
                    self.append_temp_overlay_dirty_rect(layout.canvas_host_rect);
                }
                true
            }
            KomaGestureUpdate::Committed { anchor, current } => {
                let (page_width, page_height) = self.document.active_page_dimensions();
                let preview_state = KomaGesture {
                    is_drawing: false,
                    anchor: Some(anchor),
                    last_position: Some(current),
                };
                let created = crate::features::koma::koma_creation_preview_bounds(
                    &preview_state,
                    page_width,
                    page_height,
                )
                .filter(|bounds| bounds.width >= 8 && bounds.height >= 8)
                .is_some_and(|bounds| {
                    self.apply_document_command(&DocumentCommand::CreateKoma {
                        x: bounds.x,
                        y: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    })
                });
                if let Some(layout) = self.layout.as_ref() {
                    self.append_temp_overlay_dirty_rect(layout.canvas_host_rect);
                    return true;
                }
                created
            }
        }
    }

    /// 両ジェスチャ状態 (ペイント系 + コマ作成) を破棄する。
    fn reset_canvas_gestures(&mut self) {
        self.paint.canvas_input.reset();
        self.koma_gesture.reset();
    }

    fn canvas_position_from_window(&self, point: WindowPoint) -> Option<PagePoint> {
        let layout = self.layout.as_ref()?;
        if !layout.canvas_host_rect.contains(point) {
            return None;
        }

        self.canvas_position_from_window_clamped(point)
    }

    fn canvas_display_contains_window(&self, point: WindowPoint) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|layout| layout.canvas_display_rect.contains(point))
    }

    fn hover_canvas_position_from_window(&self, point: WindowPoint) -> Option<PagePoint> {
        let position = self.canvas_position_from_window(point)?;
        // コマ作成は生のページ座標を使う。ペイント系はアクティブコマ内のみホバー有効。
        match self.document.session.active_tool_descriptor().gesture_kind() {
            GestureKind::KomaRect => Some(position),
            GestureKind::Stroke | GestureKind::FloodFill | GestureKind::LassoFill => self
                .page_position_in_active_panel(position)
                .map(|_| position),
        }
    }

    pub(crate) fn canvas_position_from_window_clamped(
        &self,
        point: WindowPoint,
    ) -> Option<PagePoint> {
        let layout = self.layout.as_ref()?;
        let window_rect = geometry::WindowRect::new(
            layout.canvas_host_rect.x,
            layout.canvas_host_rect.y,
            layout.canvas_host_rect.width,
            layout.canvas_host_rect.height,
        );
        let viewport_point = window_rect.clamp_to_canvas_viewport_point(point)?;
        let (canvas_width, canvas_height) = self.canvas_dimensions();
        canvas_geometry::CanvasViewGeometry::compute(
            WindowRect::new(
                0,
                0,
                layout.canvas_host_rect.width,
                layout.canvas_host_rect.height,
            ),
            canvas_width,
            canvas_height,
            self.document.session.view_transform,
        )
        .and_then(|geometry| geometry.map_view_to_canvas(viewport_point))
    }

    fn page_position_in_active_panel(&self, point: PagePoint) -> Option<PagePoint> {
        let bounds = self.document.active_koma_bounds()?;
        bounds.contains_canvas_point(point).then_some(point)
    }
}
