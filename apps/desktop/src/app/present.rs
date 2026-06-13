//! フレーム生成と差分更新の責務を `DesktopApp` へ追加する。
//!
//! Phase 9F 完了後、`PresentFrame` のレイヤーは
//! `background_quads` / `canvas_surface` / `overlay_*_quads` / `panel_quads` /
//! `foreground_quads` / `status_quad` のみ。CPU 合成経路は完全撤去済み。

use std::time::Instant;

use desktop_support::FrameProfiler;

use super::{DesktopApp, PresentFrameUpdate};
use crate::present_quads::DesktopLayout;

impl DesktopApp {
    pub(crate) fn prepare_present_frame(
        &mut self,
        window_width: usize,
        window_height: usize,
        profiler: &mut FrameProfiler,
    ) -> PresentFrameUpdate {
        self.poll_background_tasks();
        let (canvas_width, canvas_height) = self.canvas_dimensions();
        let next_layout = profiler.measure("layout", || {
            DesktopLayout::new(window_width, window_height, canvas_width, canvas_height)
        });

        if self.layout.as_ref() != Some(&next_layout) {
            self.layout = Some(next_layout.clone());
            self.request_panel_reconcile();
            self.rebuild_present_frame();
        }

        // workspace_layout パネル用に、登録パネル一覧 (id/title/visible) を JSON 化して runtime に注入。
        // 値が変わっていれば workspace-layout が dirty 扱いとなり次の sync_dirty_panels で再描画される。
        let workspace_panels_json = self.build_workspace_panels_json();
        self.panel_runtime
            .set_workspace_panels_json(workspace_panels_json);

        if self.panel_runtime.has_dirty_panels() {
            profiler.record_value("ui_update_panels", self.panel_runtime.dirty_panel_count() as f64);
            let host_state = panel_runtime::HostState {
                can_undo: self.history.can_undo(),
                can_redo: self.history.can_redo(),
                active_jobs: self.background_jobs.len(),
                snapshot_count: self.snapshots.len(),
            };
            let sync_t = Instant::now();
            let changed = self
                .panel_runtime
                .sync_dirty_panels(&self.document, host_state);
            profiler.record("ui_sync_panels", sync_t.elapsed());
            let reconcile_t = Instant::now();
            self.panel_workspace
                .reconcile_panels(self.panel_runtime.panel_ids());
            profiler.record("ui_reconcile", reconcile_t.elapsed());
            if !changed.is_empty() {
                self.request_panel_reconcile();
            }
        }

        // HTML パネルの hit / move handle / full rect テーブルを CPU 側で更新する。
        // レイアウト解決は GPU 非依存 (`collect_panel_hits`) のため、GPU 提示の有無
        // (headless テスト含む) にかかわらずフォーカス巡回・キーボード操作・
        // pointer hit が機能する。GPU ループ (event_loop.rs) は quad 組み立てのみを担う。
        profiler.measure("panel_hits", || {
            self.refresh_panel_hit_tables(window_width, window_height);
        });

        if self.invalidation.needs_panel_reconcile {
            profiler.measure("panel_reconcile", || {
                self.panel_workspace
                    .reconcile_panels(self.panel_runtime.panel_ids());
            });
            self.invalidation.needs_panel_reconcile = false;
        }

        if self.invalidation.needs_full_present_rebuild {
            self.invalidation.canvas_dirty_rect = None;
            self.invalidation.temp_overlay_dirty_rect = None;
            self.invalidation.ui_panel_dirty_rect = None;
            self.invalidation.canvas_transform_update = false;
            self.invalidation.needs_status_refresh = false;
            self.invalidation.needs_full_present_rebuild = false;
            let bitmap = self.cpu_canvas_snapshot.as_ref();
            let window_rect = geometry::WindowRect {
                x: 0,
                y: 0,
                width: window_width,
                height: window_height,
            };
            return PresentFrameUpdate {
                background_dirty_rect: Some(window_rect),
                temp_overlay_dirty_rect: Some(window_rect),
                ui_panel_dirty_rect: Some(window_rect),
                canvas_dirty_rect: bitmap.map(|bitmap| geometry::PageDirtyRect {
                    x: 0,
                    y: 0,
                    width: bitmap.width,
                    height: bitmap.height,
                }),
                canvas_transform_changed: true,
                canvas_updated: true,
            };
        }

        let mut layer_dirty = crate::present_quads::LayerDirtyAccumulator::default();

        // ステータス更新 — HtmlPanelView 化されたため、毎フレーム
        // status_bar.update() を呼んで snapshot を view に流す（差分なら no-op）。
        // 実際の GPU 描画は event_loop.rs の RedrawRequested で行う。
        if self.invalidation.needs_status_refresh {
            self.invalidation.needs_status_refresh = false;
        }

        // 一時オーバーレイは GPU quad で毎フレーム描画されるため CPU 合成は不要。
        if let Some(dirty_rect) = self.invalidation.temp_overlay_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            layer_dirty.mark_temp_overlay(dirty_rect);
        }

        // UIパネル dirty
        if let Some(dirty_rect) = self.invalidation.ui_panel_dirty_rect.take()
            && dirty_rect.width > 0
            && dirty_rect.height > 0
        {
            layer_dirty.mark_ui_panel(dirty_rect);
        }

        let canvas_dirty_rect = self.invalidation.canvas_dirty_rect.take();
        let canvas_transform_changed = std::mem::take(&mut self.invalidation.canvas_transform_update);
        if let Some(canvas_dirty_rect) = canvas_dirty_rect {
            use geometry::ClampToCanvasBounds;
            let dirty = canvas_dirty_rect.clamp_to_canvas_bounds(canvas_width, canvas_height);
            let canvas_area = (canvas_width.max(1) * canvas_height.max(1)) as f64;
            profiler.record_value("canvas_upload_area_px", (dirty.width * dirty.height) as f64);
            profiler.record_value("canvas_upload_width_px", dirty.width as f64);
            profiler.record_value("canvas_upload_height_px", dirty.height as f64);
            profiler.record_value(
                "canvas_upload_coverage_pct",
                ((dirty.width * dirty.height) as f64 / canvas_area) * 100.0,
            );
        }
        if let Some(temp_overlay_dirty_rect) = layer_dirty.temp_overlay {
            let window_area = (window_width.max(1) * window_height.max(1)) as f64;
            profiler.record_value(
                "overlay_upload_area_px",
                (temp_overlay_dirty_rect.width * temp_overlay_dirty_rect.height) as f64,
            );
            profiler.record_value(
                "overlay_upload_width_px",
                temp_overlay_dirty_rect.width as f64,
            );
            profiler.record_value(
                "overlay_upload_height_px",
                temp_overlay_dirty_rect.height as f64,
            );
            profiler.record_value(
                "overlay_upload_coverage_pct",
                ((temp_overlay_dirty_rect.width * temp_overlay_dirty_rect.height) as f64
                    / window_area)
                    * 100.0,
            );
        }
        if canvas_dirty_rect.is_some() || canvas_transform_changed {
            profiler.measure("compute_canvas_view_geometry", || {
                let _ = self.canvas_view_geometry();
            });
        }

        PresentFrameUpdate {
            background_dirty_rect: layer_dirty.background,
            temp_overlay_dirty_rect: layer_dirty.temp_overlay,
            ui_panel_dirty_rect: layer_dirty.ui_panel,
            canvas_dirty_rect,
            canvas_transform_changed,
            canvas_updated: canvas_dirty_rect.is_some() || canvas_transform_changed,
        }
    }

    /// HTML パネルの hit / move handle / full rect テーブルを更新する。
    ///
    /// パネル位置は workspace_layout、サイズは View の `panel_size` が権威。
    /// hit 矩形は `collect_panel_hits` が GPU 描画と同一のクランプ規則で
    /// レイアウト解決して返すため、実描画と常に一致する。
    fn refresh_panel_hit_tables(&mut self, window_width: usize, window_height: usize) {
        let all_panel_ids = self.panel_runtime.panel_ids_with_gpu();
        let (panel_ids, hidden_ids): (Vec<String>, Vec<String>) = all_panel_ids
            .into_iter()
            .partition(|id| self.panel_workspace.is_panel_visible(id));
        // 不可視パネルのジオメトリは掃除する
        for id in &hidden_ids {
            self.panel_workspace.remove_panel_geometry(id);
        }
        if panel_ids.is_empty() {
            return;
        }

        let chrome_h = super::PANEL_CHROME_HEIGHT as usize;
        let measured = self.panel_runtime.panel_sizes();
        let mut sized: Vec<(String, u32, u32)> = Vec::with_capacity(panel_ids.len());
        let mut panel_rects: Vec<geometry::WindowRect> = Vec::with_capacity(panel_ids.len());
        for id in &panel_ids {
            let (mw, mh) = measured
                .iter()
                .find(|(pid, _, _)| pid == id)
                .map(|(_, w, h)| (*w, *h))
                .unwrap_or((1, 1));
            // 位置は workspace_layout の position を使う（サイズは measured で上書き）
            let position_rect = self
                .panel_workspace
                .panel_rect(id, window_width, window_height)
                .unwrap_or(geometry::WindowRect {
                    x: 0,
                    y: 0,
                    width: mw as usize,
                    height: mh as usize,
                });
            panel_rects.push(geometry::WindowRect {
                x: position_rect.x,
                y: position_rect.y,
                width: mw as usize,
                height: mh as usize,
            });
            // viewport はクランプ上限としてそのまま渡し、View 側でクランプさせる
            sized.push((id.clone(), window_width as u32, window_height as u32));
        }

        let hits_by_panel = self.panel_runtime.collect_panel_hits(
            &sized,
            1.0,
            super::PANEL_CHROME_HEIGHT,
        );
        for (panel_id, hits) in hits_by_panel {
            let Some(index) = panel_ids.iter().position(|id| id == &panel_id) else {
                continue;
            };
            let panel_rect = panel_rects[index];
            let hit_rects: Vec<(String, geometry::WindowRect)> = hits
                .into_iter()
                .filter_map(|hit| {
                    let element_id = hit.element_id?;
                    Some((
                        element_id,
                        geometry::WindowRect {
                            x: hit.rect.x as usize,
                            y: hit.rect.y as usize,
                            width: hit.rect.width as usize,
                            height: hit.rect.height as usize,
                        },
                    ))
                })
                .collect();
            // chrome/body 分割は panel-workspace 側で full_rect から導出される (BL-096)。
            self.panel_workspace
                .update_panel_geometry(&panel_id, panel_rect, chrome_h, hit_rects);
        }
    }
}
