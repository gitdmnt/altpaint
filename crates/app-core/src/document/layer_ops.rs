//! `Document` のレイヤー編集と合成処理をまとめる。
//!
//! 公開 command 境界の背後にある layer 操作・合成 helper をここへ集約し、
//! ドキュメント本体を状態遷移の入口として読みやすく保つ。

use crate::{BitmapEdit, CanvasDirtyRect, ClampToCanvasBounds, MergeInSpace, PanelId};

use super::{BlendMode, CanvasBitmap, Document, LayerMask, LayerNodeId, Panel, RasterLayer};

/// Local 差分 to ページ 差分 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn local_dirty_to_page_dirty(
    dirty: CanvasDirtyRect,
    panel_bounds: super::PanelBounds,
    page_width: usize,
    page_height: usize,
) -> CanvasDirtyRect {
    CanvasDirtyRect {
        x: dirty.x.saturating_add(panel_bounds.x),
        y: dirty.y.saturating_add(panel_bounds.y),
        width: dirty.width,
        height: dirty.height,
    }
    .clamp_to_canvas_bounds(page_width.max(1), page_height.max(1))
}

impl Document {
    /// ビットマップ edits to アクティブ レイヤー を更新し、必要な dirty 状態も記録する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub fn apply_bitmap_edits_to_active_layer(
        &mut self,
        edits: &[BitmapEdit],
    ) -> Option<CanvasDirtyRect> {
        if edits.is_empty() {
            return None;
        }
        let panel_bounds = self.active_panel_bounds()?;
        let (page_width, page_height) = self.active_page_dimensions();
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            let dirty = apply_bitmap_edits(panel, edits)?;
            composite_panel_bitmap_region(panel, dirty);
            return Some(local_dirty_to_page_dirty(
                dirty,
                panel_bounds,
                page_width,
                page_height,
            ));
        }

        None
    }

    /// 指定 `PanelId` のページ・パネルインデックスを返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn find_panel_location(&self, panel_id: PanelId) -> Option<(usize, usize)> {
        for (page_index, page) in self.work.pages.iter().enumerate() {
            for (panel_index, panel) in page.panels.iter().enumerate() {
                if panel.id == panel_id {
                    return Some((page_index, panel_index));
                }
            }
        }
        None
    }

    /// 指定 panel/layer のビットマップ全体を複製して返す。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn clone_panel_layer_bitmap(
        &self,
        panel_id: PanelId,
        layer_index: usize,
    ) -> Option<CanvasBitmap> {
        let (page_idx, panel_idx) = self.find_panel_location(panel_id)?;
        let panel = &self.work.pages[page_idx].panels[panel_idx];
        panel.layers.get(layer_index).map(|layer| layer.bitmap.clone())
    }

    /// 指定 panel/layer の指定領域を複製して返す（パネルローカル座標系）。
    ///
    /// 値を生成できない場合は `None` を返します。
    pub fn capture_panel_layer_region(
        &self,
        panel_id: PanelId,
        layer_index: usize,
        dirty: CanvasDirtyRect,
    ) -> Option<CanvasBitmap> {
        let (page_idx, panel_idx) = self.find_panel_location(panel_id)?;
        let panel = &self.work.pages[page_idx].panels[panel_idx];
        let layer = panel.layers.get(layer_index)?;
        extract_bitmap_region(&layer.bitmap, dirty.x, dirty.y, dirty.width, dirty.height)
    }

    /// 指定 panel/layer の指定位置にビットマップを復元し、パネル合成も更新する。
    ///
    /// 必要に応じて dirty 状態も更新します。返値はページ座標系の dirty rect。
    pub fn restore_panel_layer_region(
        &mut self,
        panel_id: PanelId,
        layer_index: usize,
        x: usize,
        y: usize,
        bitmap: &CanvasBitmap,
    ) -> Option<CanvasDirtyRect> {
        let (page_idx, panel_idx) = self.find_panel_location(panel_id)?;
        let panel_bounds = self.work.pages[page_idx].panels[panel_idx].bounds;
        let (page_width, page_height) = {
            let page = &self.work.pages[page_idx];
            (page.width, page.height)
        };
        let panel = &mut self.work.pages[page_idx].panels[panel_idx];
        if let Some(layer) = panel.layers.get_mut(layer_index) {
            write_bitmap_region(&mut layer.bitmap, x, y, bitmap);
        }
        let dirty = CanvasDirtyRect {
            x,
            y,
            width: bitmap.width,
            height: bitmap.height,
        };
        composite_panel_bitmap_region(panel, dirty);
        Some(local_dirty_to_page_dirty(
            dirty,
            panel_bounds,
            page_width,
            page_height,
        ))
    }

    /// 指定 panel の指定 layer をビットマップを透明にリセットし、パネル合成も更新する。
    pub fn reset_panel_layer_to_transparent(&mut self, panel_id: PanelId, layer_index: usize) {
        let Some((page_idx, panel_idx)) = self.find_panel_location(panel_id) else {
            return;
        };
        let panel = &mut self.work.pages[page_idx].panels[panel_idx];
        if let Some(layer) = panel.layers.get_mut(layer_index) {
            let (w, h) = (layer.bitmap.width, layer.bitmap.height);
            layer.bitmap = CanvasBitmap::transparent(w, h);
        }
        let new_bitmap = composite_panel_bitmap(&self.work.pages[page_idx].panels[panel_idx]);
        self.work.pages[page_idx].panels[panel_idx].bitmap = new_bitmap;
    }

    /// 指定 panel の指定 layer に `BitmapEdit` を適用し、パネル合成も更新する。
    pub fn apply_bitmap_edits_to_panel_layer(
        &mut self,
        panel_id: PanelId,
        layer_index: usize,
        edits: &[BitmapEdit],
    ) -> Option<CanvasDirtyRect> {
        if edits.is_empty() {
            return None;
        }
        let (page_idx, panel_idx) = self.find_panel_location(panel_id)?;
        let panel_bounds = self.work.pages[page_idx].panels[panel_idx].bounds;
        let (page_width, page_height) = {
            let page = &self.work.pages[page_idx];
            (page.width, page.height)
        };
        let panel = &mut self.work.pages[page_idx].panels[panel_idx];
        // layer_index override: set active_layer_index temporarily
        let saved_index = panel.active_layer_index;
        panel.active_layer_index = layer_index.min(panel.layers.len().saturating_sub(1));
        let dirty = apply_bitmap_edits(panel, edits);
        if let Some(dirty) = dirty {
            composite_panel_bitmap_region(panel, dirty);
        }
        panel.active_layer_index = saved_index;
        let dirty = dirty?;
        Some(local_dirty_to_page_dirty(dirty, panel_bounds, page_width, page_height))
    }

    /// Raster レイヤー を追加する。
    pub fn add_raster_layer(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            panel.created_layer_count = panel.created_layer_count.saturating_add(1);
            let next_index = panel.created_layer_count;
            let (width, height) = (panel.bitmap.width, panel.bitmap.height);
            panel.layers.push(RasterLayer::transparent(
                LayerNodeId(next_index),
                format!("Layer {next_index}"),
                width,
                height,
            ));
            panel.active_layer_index = panel.layers.len().saturating_sub(1);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブ レイヤー を削除する。
    pub fn remove_active_layer(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if panel.layers.len() <= 1 {
                return;
            }
            panel.layers.remove(panel.active_layer_index);
            panel.active_layer_index = panel
                .active_layer_index
                .min(panel.layers.len().saturating_sub(1));
            panel.bitmap = composite_panel_bitmap(panel);
            sync_root_layer_summary(panel);
        }
    }

    /// レイヤー を選択状態へ更新する。
    pub fn select_layer(&mut self, index: usize) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            panel.active_layer_index = index.min(panel.layers.len().saturating_sub(1));
            sync_root_layer_summary(panel);
        }
    }

    /// 現在の値を アクティブ レイヤー へ変換する。
    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.name = name.to_string();
                sync_root_layer_summary(panel);
            }
        }
    }

    /// 入力や種別に応じて処理を振り分ける。
    pub fn move_layer(&mut self, from_index: usize, to_index: usize) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if panel.layers.len() <= 1 {
                return;
            }

            let last_index = panel.layers.len().saturating_sub(1);
            let from_index = from_index.min(last_index);
            let to_index = to_index.min(last_index);
            if from_index == to_index {
                return;
            }

            let moved = panel.layers.remove(from_index);
            panel.layers.insert(to_index, moved);

            panel.active_layer_index = match panel.active_layer_index {
                index if index == from_index => to_index,
                index if from_index < index && index <= to_index => index.saturating_sub(1),
                index if to_index <= index && index < from_index => index + 1,
                index => index,
            };

            panel.bitmap = composite_panel_bitmap(panel);
            sync_root_layer_summary(panel);
        }
    }

    /// 次 レイヤー を選択状態へ更新する。
    pub fn select_next_layer(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            panel.active_layer_index = (panel.active_layer_index + 1) % panel.layers.len().max(1);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブ レイヤー ブレンド モード を順送りで切り替える。
    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブ レイヤー ブレンド モード を設定する。
    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = mode;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブ レイヤー visibility の有効状態を切り替える。
    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.visible = !layer.visible;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブ レイヤー マスク の有効状態を切り替える。
    pub fn toggle_active_layer_mask(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.mask = if layer.mask.is_some() {
                    None
                } else {
                    Some(LayerMask::demo(layer.bitmap.width, layer.bitmap.height))
                };
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }
}

/// パネル layers が満たされるよう整える。
pub(super) fn ensure_panel_layers(panel: &mut Panel) {
    let mut repaired = false;
    if panel.layers.is_empty() {
        panel.layers.push(RasterLayer::background(
            panel.root_layer.id,
            panel.root_layer.name.clone(),
            panel.bitmap.width,
            panel.bitmap.height,
        ));
        if let Some(layer) = panel.layers.first_mut() {
            layer.bitmap = panel.bitmap.clone();
        }
        repaired = true;
    }
    panel.created_layer_count = panel
        .created_layer_count
        .max(panel.layers.len() as u64)
        .max(1);
    panel.active_layer_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    sync_root_layer_summary(panel);
    if repaired {
        panel.bitmap = composite_panel_bitmap(panel);
    }
}

/// Root レイヤー summary を現在の状態へ同期する。
fn sync_root_layer_summary(panel: &mut Panel) {
    if let Some(layer) = panel.layers.get(panel.active_layer_index) {
        panel.root_layer.id = layer.id;
        panel.root_layer.name = layer.name.clone();
    }
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 値を生成できない場合は `None` を返します。
fn apply_bitmap_edits(panel: &mut Panel, edits: &[BitmapEdit]) -> Option<CanvasDirtyRect> {
    let active_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    let layer = &mut panel.layers[active_index];
    let mut dirty_union: Option<CanvasDirtyRect> = None;

    for edit in edits {
        let dirty = edit
            .dirty_rect
            .clamp_to_canvas_bounds(layer.bitmap.width.max(1), layer.bitmap.height.max(1));
        if dirty.width == 0 || dirty.height == 0 {
            continue;
        }
        if edit.bitmap.width == 0 || edit.bitmap.height == 0 {
            continue;
        }

        let source_x = dirty.x.saturating_sub(edit.dirty_rect.x);
        let source_y = dirty.y.saturating_sub(edit.dirty_rect.y);
        let incoming =
            extract_bitmap_region(&edit.bitmap, source_x, source_y, dirty.width, dirty.height)?;
        let previous =
            extract_bitmap_region(&layer.bitmap, dirty.x, dirty.y, dirty.width, dirty.height)?;
        let merged = edit.composite.compose(&incoming, &previous);
        write_bitmap_region(&mut layer.bitmap, dirty.x, dirty.y, &merged);

        dirty_union = Some(match dirty_union {
            Some(current) => current.merge(dirty),
            None => dirty,
        });
    }

    dirty_union
}

/// Extract ビットマップ 領域 に対応するビットマップ処理を行う。
fn extract_bitmap_region(
    bitmap: &CanvasBitmap,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
) -> Option<CanvasBitmap> {
    if width == 0
        || height == 0
        || start_x >= bitmap.width
        || start_y >= bitmap.height
        || start_x.saturating_add(width) > bitmap.width
        || start_y.saturating_add(height) > bitmap.height
    {
        return None;
    }

    let mut region = CanvasBitmap::transparent(width, height);
    for row in 0..height {
        let src_row_start = ((start_y + row) * bitmap.width + start_x) * 4;
        let src_row_end = src_row_start + width * 4;
        let dst_row_start = row * width * 4;
        let dst_row_end = dst_row_start + width * 4;
        region.pixels[dst_row_start..dst_row_end]
            .copy_from_slice(&bitmap.pixels[src_row_start..src_row_end]);
    }
    Some(region)
}

/// ビットマップ 領域 を保存先へ書き出す。
fn write_bitmap_region(
    target: &mut CanvasBitmap,
    start_x: usize,
    start_y: usize,
    region: &CanvasBitmap,
) {
    if region.width == 0 || region.height == 0 {
        return;
    }
    for row in 0..region.height {
        let dst_row_start = ((start_y + row) * target.width + start_x) * 4;
        let dst_row_end = dst_row_start + region.width * 4;
        let src_row_start = row * region.width * 4;
        let src_row_end = src_row_start + region.width * 4;
        target.pixels[dst_row_start..dst_row_end]
            .copy_from_slice(&region.pixels[src_row_start..src_row_end]);
    }
}

/// Composite パネル ビットマップ に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
pub(super) fn composite_panel_bitmap(panel: &Panel) -> CanvasBitmap {
    let width = panel
        .layers
        .first()
        .map(|layer| layer.bitmap.width.max(1))
        .unwrap_or_else(|| panel.bitmap.width.max(1));
    let height = panel
        .layers
        .first()
        .map(|layer| layer.bitmap.height.max(1))
        .unwrap_or_else(|| panel.bitmap.height.max(1));
    let mut result = CanvasBitmap::transparent(width, height);
    for layer in &panel.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(
            &mut result,
            layer,
            CanvasDirtyRect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
    }
    result
}

/// Composite パネル ビットマップ 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn composite_panel_bitmap_region(panel: &mut Panel, dirty: CanvasDirtyRect) {
    let dirty = dirty.clamp_to_canvas_bounds(panel.bitmap.width.max(1), panel.bitmap.height.max(1));
    if let Some(layer_index) = single_passthrough_layer_index(panel) {
        copy_bitmap_region(&panel.layers[layer_index].bitmap, &mut panel.bitmap, dirty);
        return;
    }

    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let index = (y * panel.bitmap.width + x) * 4;
            panel.bitmap.pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
    }

    for layer in &panel.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(&mut panel.bitmap, layer, dirty);
    }
}

/// 現在の single passthrough レイヤー インデックス を返す。
///
/// 値を生成できない場合は `None` を返します。
fn single_passthrough_layer_index(panel: &Panel) -> Option<usize> {
    let mut visible_layers = panel
        .layers
        .iter()
        .enumerate()
        .filter(|(_, layer)| layer.visible);
    let (index, layer) = visible_layers.next()?;
    if visible_layers.next().is_some() {
        return None;
    }
    if layer.mask.is_some() || !matches!(layer.blend_mode, BlendMode::Normal) {
        return None;
    }
    if layer.bitmap.width != panel.bitmap.width || layer.bitmap.height != panel.bitmap.height {
        return None;
    }
    Some(index)
}

/// Copy ビットマップ 領域 に必要な差分領域だけを描画または合成する。
///
/// 必要に応じて dirty 状態も更新します。
fn copy_bitmap_region(source: &CanvasBitmap, target: &mut CanvasBitmap, dirty: CanvasDirtyRect) {
    let dirty = dirty.clamp_to_canvas_bounds(
        target.width.min(source.width),
        target.height.min(source.height),
    );
    for y in dirty.y..dirty.y + dirty.height {
        let source_row_start = (y * source.width + dirty.x) * 4;
        let source_row_end = source_row_start + dirty.width * 4;
        let target_row_start = (y * target.width + dirty.x) * 4;
        let target_row_end = target_row_start + dirty.width * 4;
        target.pixels[target_row_start..target_row_end]
            .copy_from_slice(&source.pixels[source_row_start..source_row_end]);
    }
}

/// 入力や種別に応じて処理を振り分ける。
///
/// 必要に応じて dirty 状態も更新します。
fn composite_layer_region_into(
    target: &mut CanvasBitmap,
    layer: &RasterLayer,
    dirty: CanvasDirtyRect,
) {
    let dirty = dirty.clamp_to_canvas_bounds(
        target.width.min(layer.bitmap.width).max(1),
        target.height.min(layer.bitmap.height).max(1),
    );
    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let target_index = (y * target.width + x) * 4;
            let source_index = (y * layer.bitmap.width + x) * 4;
            let mut src = [
                layer.bitmap.pixels[source_index],
                layer.bitmap.pixels[source_index + 1],
                layer.bitmap.pixels[source_index + 2],
                layer.bitmap.pixels[source_index + 3],
            ];
            if let Some(mask) = &layer.mask {
                src[3] = ((src[3] as u16 * mask.alpha_at(x, y) as u16) / 255) as u8;
            }
            let dst = [
                target.pixels[target_index],
                target.pixels[target_index + 1],
                target.pixels[target_index + 2],
                target.pixels[target_index + 3],
            ];
            let blended = blend_pixel(dst, src, &layer.blend_mode);
            target.pixels[target_index..target_index + 4].copy_from_slice(&blended);
        }
    }
}

/// 入力や種別に応じて処理を振り分ける。
fn blend_pixel(dst: [u8; 4], src: [u8; 4], mode: &BlendMode) -> [u8; 4] {
    let src_a = src[3] as f32 / 255.0;
    if src_a <= 0.0 {
        return dst;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let blend_channel = |dst_c: u8, src_c: u8| -> f32 {
        let d = dst_c as f32 / 255.0;
        let s = src_c as f32 / 255.0;
        match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d,
            BlendMode::Screen => 1.0 - (1.0 - s) * (1.0 - d),
            BlendMode::Add => (s + d).min(1.0),
        }
    };
    let out_a = src_a + dst_a * (1.0 - src_a);
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let dst_c = dst[channel] as f32 / 255.0;
        let mixed = blend_channel(dst[channel], src[channel]);
        let out_c = mixed * src_a + dst_c * (1.0 - src_a);
        out[channel] = (out_c * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}
