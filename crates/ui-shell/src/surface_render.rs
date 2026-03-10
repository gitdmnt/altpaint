//! `UiShell` のパネルレイヤー構築を render へ委譲する。
//!
//! ここでは panel tree / focus / 永続化済みレイアウトを render 向け DTO へ変換し、
//! hit-test は `ui-shell` 側へ残したまま、ラスタライズ責務だけを `render` へ移す。

use app_core::{WorkspacePanelPosition, WorkspacePanelSize};
use render::{FloatingPanel, PanelFocusTarget, PanelRenderState as RenderPanelState, PanelTextInputState, PixelRect};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use super::*;

pub(super) const PANEL_SCROLL_PIXELS_PER_LINE: i32 = 48;
const DEFAULT_PANEL_WIDTH: usize = 300;
const DEFAULT_PANEL_HEIGHT: usize = 220;
const DEFAULT_PANEL_ORIGIN_X: usize = 24;
const DEFAULT_PANEL_ORIGIN_Y: usize = 72;
const PANEL_CASCADE_X: usize = 28;
const PANEL_CASCADE_Y: usize = 36;
const PANEL_MIN_WIDTH: usize = 180;
const PANEL_MIN_HEIGHT: usize = 120;

struct OwnedPanelTextInputState {
    panel_id: String,
    node_id: String,
    cursor_chars: usize,
    preedit: Option<String>,
}

impl UiShell {
    /// 現在の panel trees から viewport 向け panel layer を構築する。
    pub fn render_panel_surface(&mut self, width: usize, height: usize) -> PanelSurface {
        let width = width.max(1);
        let height = height.max(1);
        let viewport_changed = self
            .panel_content_viewport
            .is_none_or(|viewport| viewport != (width, height));
        let needs_raster = self.panel_content_dirty || viewport_changed || self.panel_bitmap_cache.is_empty();
        let needs_compose = needs_raster || self.panel_layout_dirty || self.panel_content_cache.is_none();

        self.last_panel_rasterized_panels = 0;
        self.last_panel_composited_panels = 0;
        self.last_panel_raster_duration_ms = 0.0;
        self.last_panel_compose_duration_ms = 0.0;
        self.last_panel_surface_dirty_rect = None;
        if viewport_changed {
            self.full_panel_raster_dirty = true;
            self.dirty_panel_ids.clear();
        }

        let trees = self.panel_trees();
        let focused_target = self.focused_target.clone();
        let expanded_dropdown = self.expanded_dropdown.clone();
        let text_input_state_snapshot = self.collect_panel_text_input_states();
        let text_input_states = text_input_state_snapshot
            .iter()
            .map(|state| PanelTextInputState {
                panel_id: state.panel_id.as_str(),
                node_id: state.node_id.as_str(),
                cursor_chars: state.cursor_chars,
                preedit: state.preedit.as_deref(),
            })
            .collect::<Vec<_>>();
        let render_state = RenderPanelState {
            focused_target: focused_target.as_ref().map(|target| PanelFocusTarget {
                panel_id: target.panel_id.as_str(),
                node_id: target.node_id.as_str(),
            }),
            expanded_dropdown: expanded_dropdown.as_ref().map(|target| PanelFocusTarget {
                panel_id: target.panel_id.as_str(),
                node_id: target.node_id.as_str(),
            }),
            text_input_states: &text_input_states,
        };
        let floating_panels = self.collect_floating_panels(trees.as_slice(), width, height, render_state);
        let dirty_ids = self.dirty_panel_ids.clone();
        let can_incremental_compose = !viewport_changed && self.panel_content_cache.is_some();
        let incremental_dirty = if self.panel_layout_dirty {
            Some(panel_layout_dirty_rect(
                &self.rendered_panel_rects,
                &panel_rect_map(floating_panels.as_slice()),
            ))
        } else if needs_raster && !self.full_panel_raster_dirty && !dirty_ids.is_empty() {
            Some(panel_subset_dirty_rect(
                &self.rendered_panel_rects,
                &panel_rect_map(floating_panels.as_slice()),
                &dirty_ids,
            ))
        } else {
            None
        }
        .flatten();
        let use_incremental_compose = can_incremental_compose && incremental_dirty.is_some();
        let mut result_surface = None;
        if needs_raster {
            let started = Instant::now();
            self.rebuild_panel_bitmaps(floating_panels.as_slice(), render_state);
            self.last_panel_raster_duration_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        if needs_compose {
            let started = Instant::now();
            let composed_surface = if use_incremental_compose {
                self.compose_panel_surface_incremental(floating_panels.as_slice(), incremental_dirty)
            } else {
                self.compose_panel_surface(floating_panels.as_slice())
            };
            self.panel_content_cache = Some(composed_surface.clone());
            result_surface = Some(composed_surface);
            self.last_panel_compose_duration_ms = started.elapsed().as_secs_f64() * 1000.0;
            self.panel_content_viewport = Some((width, height));
            self.panel_content_dirty = false;
            self.full_panel_raster_dirty = false;
            self.dirty_panel_ids.clear();
            self.panel_layout_dirty = false;
        }

        self.panel_content_height = height;
        self.panel_scroll_offset = 0;
        result_surface.unwrap_or_else(|| {
            self.panel_content_cache
                .clone()
                .unwrap_or_else(|| self.compose_panel_surface(floating_panels.as_slice()))
        })
    }

    fn collect_panel_text_input_states(&self) -> Vec<OwnedPanelTextInputState> {
        self
            .text_input_states
            .iter()
            .map(|((panel_id, node_id), state)| OwnedPanelTextInputState {
                panel_id: panel_id.clone(),
                node_id: node_id.clone(),
                cursor_chars: state.cursor_chars,
                preedit: state.preedit.clone(),
            })
            .collect()
    }

    fn collect_floating_panels<'a>(
        &self,
        trees: &'a [plugin_api::PanelTree],
        width: usize,
        height: usize,
        render_state: RenderPanelState<'a>,
    ) -> Vec<FloatingPanel<'a>> {
        trees
            .iter()
            .enumerate()
            .map(|(index, tree)| FloatingPanel {
                panel_id: tree.id,
                title: tree.title,
                rect: self.panel_rect_for_tree(tree, index, width, height, render_state),
                tree,
            })
            .collect()
    }

    fn rebuild_panel_bitmaps(
        &mut self,
        floating_panels: &[FloatingPanel<'_>],
        render_state: RenderPanelState<'_>,
    ) {
        let valid_ids = floating_panels
            .iter()
            .map(|panel| panel.panel_id.to_string())
            .collect::<BTreeSet<_>>();
        self.rendered_panel_rects = floating_panels
            .iter()
            .map(|panel| (panel.panel_id.to_string(), panel.rect))
            .collect();
        self.panel_bitmap_cache.retain(|panel_id, _| valid_ids.contains(panel_id));

        let reraster_all = self.full_panel_raster_dirty || self.panel_bitmap_cache.is_empty();
        let dirty_ids = self.dirty_panel_ids.clone();
        let mut rasterized = 0usize;

        for panel in floating_panels {
            if !reraster_all && !dirty_ids.contains(panel.panel_id) {
                continue;
            }
                let layer = render::rasterize_panel_layer(
                    PixelRect {
                        x: 0,
                        y: 0,
                        width: panel.rect.width.max(1),
                        height: panel.rect.height.max(1),
                    },
                    &[FloatingPanel {
                        panel_id: panel.panel_id,
                        title: panel.title,
                        rect: PixelRect {
                            x: 0,
                            y: 0,
                            width: panel.rect.width,
                            height: panel.rect.height,
                        },
                        tree: panel.tree,
                    }],
                    render_state,
                );
                self.panel_bitmap_cache.insert(
                    panel.panel_id.to_string(),
                    PanelSurface {
                        x: 0,
                        y: 0,
                        width: layer.width,
                        height: layer.height,
                        pixels: layer.pixels,
                        hit_regions: layer.hit_regions,
                    },
                );
                rasterized += 1;
        }
        self.last_panel_rasterized_panels = rasterized;
    }

    fn compose_panel_surface(&mut self, floating_panels: &[FloatingPanel<'_>]) -> PanelSurface {
        let next_rects = panel_rect_map(floating_panels);
        let Some(bounds) = panel_bounds(floating_panels) else {
            self.last_panel_composited_panels = 0;
            self.rendered_panel_rects = next_rects;
            self.last_panel_surface_dirty_rect = None;
            return PanelSurface {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                pixels: vec![0; 4],
                hit_regions: Vec::new(),
            };
        };
        self.rendered_panel_rects = next_rects;
        self.last_panel_composited_panels = floating_panels.len();
        self.last_panel_surface_dirty_rect = Some(bounds);

        let mut surface = PanelSurface {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width.max(1),
            height: bounds.height.max(1),
            pixels: vec![0; bounds.width.max(1) * bounds.height.max(1) * 4],
            hit_regions: Vec::new(),
        };
        for panel in floating_panels {
            let Some(bitmap) = self.panel_bitmap_cache.get(panel.panel_id) else {
                continue;
            };
            let offset_x = panel.rect.x.saturating_sub(bounds.x);
            let offset_y = panel.rect.y.saturating_sub(bounds.y);
            blend_panel_bitmap(&mut surface, bitmap, offset_x, offset_y);
            surface.hit_regions.extend(bitmap.hit_regions.iter().cloned().map(|mut region| {
                region.x += offset_x;
                region.y += offset_y;
                region
            }));
        }

        surface
    }

    fn compose_panel_surface_incremental(
        &mut self,
        floating_panels: &[FloatingPanel<'_>],
        dirty_global: Option<PixelRect>,
    ) -> PanelSurface {
        let next_rects = panel_rect_map(floating_panels);
        let Some(bounds) = panel_bounds(floating_panels) else {
            self.last_panel_composited_panels = 0;
            self.rendered_panel_rects = next_rects;
            self.last_panel_surface_dirty_rect = None;
            return PanelSurface {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                pixels: vec![0; 4],
                hit_regions: Vec::new(),
            };
        };

        let previous_surface = self.panel_content_cache.take();
        let previous_rects = self.rendered_panel_rects.clone();
        let dirty_global = dirty_global.or_else(|| panel_layout_dirty_rect(&previous_rects, &next_rects));
        self.last_panel_surface_dirty_rect = dirty_global;

        let mut surface = if let Some(mut previous_surface) = previous_surface {
            if previous_surface.x == bounds.x
                && previous_surface.y == bounds.y
                && previous_surface.width == bounds.width.max(1)
                && previous_surface.height == bounds.height.max(1)
            {
                previous_surface.hit_regions.clear();
                previous_surface
            } else {
                let mut surface = PanelSurface {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width.max(1),
                    height: bounds.height.max(1),
                    pixels: vec![0; bounds.width.max(1) * bounds.height.max(1) * 4],
                    hit_regions: Vec::new(),
                };
                copy_surface_overlap(&mut surface, &previous_surface);
                surface
            }
        } else {
            PanelSurface {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width.max(1),
                height: bounds.height.max(1),
                pixels: vec![0; bounds.width.max(1) * bounds.height.max(1) * 4],
                hit_regions: Vec::new(),
            }
        };

        let mut composited_panels = 0usize;
        if let Some(dirty_global) = dirty_global
            && let Some(local_dirty) = global_rect_to_surface_rect(&surface, dirty_global)
        {
            clear_surface_rect(&mut surface, local_dirty);
            for panel in floating_panels {
                if panel.rect.intersect(dirty_global).is_none() {
                    continue;
                }
                let Some(bitmap) = self.panel_bitmap_cache.get(panel.panel_id) else {
                    continue;
                };
                let offset_x = panel.rect.x.saturating_sub(bounds.x);
                let offset_y = panel.rect.y.saturating_sub(bounds.y);
                blend_panel_bitmap_clipped(
                    &mut surface,
                    bitmap,
                    offset_x,
                    offset_y,
                    local_dirty,
                );
                composited_panels += 1;
            }
        }

        surface.hit_regions = build_hit_regions(&self.panel_bitmap_cache, floating_panels, bounds);
        self.rendered_panel_rects = next_rects;
        self.last_panel_composited_panels = composited_panels;
        surface
    }

    fn panel_rect_for_tree(
        &self,
        tree: &plugin_api::PanelTree,
        index: usize,
        viewport_width: usize,
        viewport_height: usize,
        render_state: RenderPanelState<'_>,
    ) -> PixelRect {
        let fallback_position = WorkspacePanelPosition {
            x: DEFAULT_PANEL_ORIGIN_X + PANEL_CASCADE_X * index,
            y: DEFAULT_PANEL_ORIGIN_Y + PANEL_CASCADE_Y * index,
        };
        let fallback_size = WorkspacePanelSize {
            width: DEFAULT_PANEL_WIDTH,
            height: DEFAULT_PANEL_HEIGHT,
        };
        let state = self
            .workspace_layout
            .panels
            .iter()
            .find(|entry| entry.id == tree.id);
        let position = state
            .and_then(|entry| entry.position)
            .unwrap_or(fallback_position);
        let size = state.and_then(|entry| entry.size).unwrap_or(fallback_size);
        let measured = render::measure_panel_size(
            tree.title,
            tree,
            render_state,
            viewport_width.max(1),
            viewport_height.max(1),
        );
        let width = size
            .width
            .max(measured.width)
            .max(PANEL_MIN_WIDTH)
            .min(viewport_width.max(1));
        let height = size
            .height
            .max(measured.height)
            .max(PANEL_MIN_HEIGHT)
            .min(viewport_height.max(1));
        let max_x = viewport_width.saturating_sub(width);
        let max_y = viewport_height.saturating_sub(height);

        PixelRect {
            x: position.x.min(max_x),
            y: position.y.min(max_y),
            width,
            height,
        }
    }

    /// 浮動パネルでは共通スクロールを持たないため常に 0 を返す。
    pub(super) fn max_panel_scroll_offset(&self, _viewport_height: usize) -> usize {
        0
    }
}

fn blend_panel_bitmap(surface: &mut PanelSurface, bitmap: &PanelSurface, offset_x: usize, offset_y: usize) {
    blend_panel_bitmap_clipped(
        surface,
        bitmap,
        offset_x,
        offset_y,
        PixelRect {
            x: 0,
            y: 0,
            width: surface.width,
            height: surface.height,
        },
    );
}

fn blend_panel_bitmap_clipped(
    surface: &mut PanelSurface,
    bitmap: &PanelSurface,
    offset_x: usize,
    offset_y: usize,
    clip_rect: PixelRect,
) {
    let panel_rect = PixelRect {
        x: offset_x,
        y: offset_y,
        width: bitmap.width,
        height: bitmap.height,
    };
    let Some(clipped) = panel_rect.intersect(clip_rect) else {
        return;
    };
    let src_start_x = clipped.x.saturating_sub(offset_x);
    let src_start_y = clipped.y.saturating_sub(offset_y);
    let row_bytes = clipped.width * 4;

    for row in 0..clipped.height {
        let src_y = src_start_y + row;
        let dst_y = clipped.y + row;
        let src_row_start = (src_y * bitmap.width + src_start_x) * 4;
        let dst_row_start = (dst_y * surface.width + clipped.x) * 4;
        surface.pixels[dst_row_start..dst_row_start + row_bytes]
            .copy_from_slice(&bitmap.pixels[src_row_start..src_row_start + row_bytes]);
    }
}

fn panel_rect_map(floating_panels: &[FloatingPanel<'_>]) -> BTreeMap<String, PixelRect> {
    floating_panels
        .iter()
        .map(|panel| (panel.panel_id.to_string(), panel.rect))
        .collect()
}

fn panel_bounds(floating_panels: &[FloatingPanel<'_>]) -> Option<PixelRect> {
    floating_panels
        .iter()
        .map(|panel| panel.rect)
        .reduce(|acc, rect| acc.union(rect))
}

fn panel_layout_dirty_rect(
    previous_rects: &BTreeMap<String, PixelRect>,
    next_rects: &BTreeMap<String, PixelRect>,
) -> Option<PixelRect> {
    let mut dirty = None;
    for (panel_id, next_rect) in next_rects {
        let changed_rect = match previous_rects.get(panel_id) {
            Some(previous_rect) if previous_rect == next_rect => None,
            Some(previous_rect) => Some(previous_rect.union(*next_rect)),
            None => Some(*next_rect),
        };
        if let Some(rect) = changed_rect {
            dirty = Some(dirty.map_or(rect, |existing: PixelRect| existing.union(rect)));
        }
    }
    for (panel_id, previous_rect) in previous_rects {
        if !next_rects.contains_key(panel_id) {
            dirty = Some(dirty.map_or(*previous_rect, |existing: PixelRect| existing.union(*previous_rect)));
        }
    }
    dirty
}

fn panel_subset_dirty_rect(
    previous_rects: &BTreeMap<String, PixelRect>,
    next_rects: &BTreeMap<String, PixelRect>,
    dirty_ids: &BTreeSet<String>,
) -> Option<PixelRect> {
    let mut dirty = None;
    for panel_id in dirty_ids {
        let rect = match (previous_rects.get(panel_id), next_rects.get(panel_id)) {
            (Some(previous), Some(current)) => previous.union(*current),
            (Some(previous), None) => *previous,
            (None, Some(current)) => *current,
            (None, None) => continue,
        };
        dirty = Some(dirty.map_or(rect, |existing: PixelRect| existing.union(rect)));
    }
    dirty
}

fn global_rect_to_surface_rect(surface: &PanelSurface, rect: PixelRect) -> Option<PixelRect> {
    PixelRect {
        x: surface.x,
        y: surface.y,
        width: surface.width,
        height: surface.height,
    }
    .intersect(rect)
    .map(|intersection| PixelRect {
        x: intersection.x.saturating_sub(surface.x),
        y: intersection.y.saturating_sub(surface.y),
        width: intersection.width,
        height: intersection.height,
    })
}

fn copy_surface_overlap(destination: &mut PanelSurface, source: &PanelSurface) {
    let Some(overlap) = PixelRect {
        x: destination.x,
        y: destination.y,
        width: destination.width,
        height: destination.height,
    }
    .intersect(PixelRect {
        x: source.x,
        y: source.y,
        width: source.width,
        height: source.height,
    }) else {
        return;
    };

    let src_start_x = overlap.x.saturating_sub(source.x);
    let src_start_y = overlap.y.saturating_sub(source.y);
    let dst_start_x = overlap.x.saturating_sub(destination.x);
    let dst_start_y = overlap.y.saturating_sub(destination.y);
    let row_bytes = overlap.width * 4;
    for row in 0..overlap.height {
        let src_row_start = ((src_start_y + row) * source.width + src_start_x) * 4;
        let dst_row_start = ((dst_start_y + row) * destination.width + dst_start_x) * 4;
        destination.pixels[dst_row_start..dst_row_start + row_bytes]
            .copy_from_slice(&source.pixels[src_row_start..src_row_start + row_bytes]);
    }
}

fn clear_surface_rect(surface: &mut PanelSurface, rect: PixelRect) {
    for row in 0..rect.height {
        let row_start = ((rect.y + row) * surface.width + rect.x) * 4;
        let row_end = row_start + rect.width * 4;
        surface.pixels[row_start..row_end].fill(0);
    }
}

fn build_hit_regions(
    bitmap_cache: &BTreeMap<String, PanelSurface>,
    floating_panels: &[FloatingPanel<'_>],
    bounds: PixelRect,
) -> Vec<render::PanelHitRegion> {
    let mut hit_regions = Vec::new();
    for panel in floating_panels {
        let Some(bitmap) = bitmap_cache.get(panel.panel_id) else {
            continue;
        };
        let offset_x = panel.rect.x.saturating_sub(bounds.x);
        let offset_y = panel.rect.y.saturating_sub(bounds.y);
        hit_regions.extend(bitmap.hit_regions.iter().cloned().map(|mut region| {
            region.x += offset_x;
            region.y += offset_y;
            region
        }));
    }
    hit_regions
}
