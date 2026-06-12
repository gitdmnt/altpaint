//! `Document` のレイヤー編集と合成処理をまとめる。
//!
//! 公開 command 境界の背後にある layer 操作・合成 helper をここへ集約し、
//! ドキュメント本体を状態遷移の入口として読みやすく保つ。

use crate::{BitmapEdit, CanvasDirtyRect, ClampToCanvasBounds, MergeInSpace, KomaId};

use super::{BlendMode, CanvasBitmap, Document, LayerNodeId, Koma, RasterLayer};

fn local_dirty_to_page_dirty(
    dirty: CanvasDirtyRect,
    koma_bounds: super::KomaBounds,
    page_width: usize,
    page_height: usize,
) -> CanvasDirtyRect {
    CanvasDirtyRect {
        x: dirty.x.saturating_add(koma_bounds.x),
        y: dirty.y.saturating_add(koma_bounds.y),
        width: dirty.width,
        height: dirty.height,
    }
    .clamp_to_canvas_bounds(page_width.max(1), page_height.max(1))
}

impl Document {
    pub fn apply_bitmap_edits_to_active_layer(
        &mut self,
        edits: &[BitmapEdit],
    ) -> Option<CanvasDirtyRect> {
        if edits.is_empty() {
            return None;
        }
        let koma_bounds = self.active_koma_bounds()?;
        let (page_width, page_height) = self.active_page_dimensions();
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            let dirty = apply_bitmap_edits(koma, edits)?;
            composite_koma_bitmap_region(koma, dirty);
            return Some(local_dirty_to_page_dirty(
                dirty,
                koma_bounds,
                page_width,
                page_height,
            ));
        }

        None
    }

    /// 指定 `KomaId` のページ・コマインデックスを返す。
    pub fn find_koma_location(&self, koma_id: KomaId) -> Option<(usize, usize)> {
        for (page_index, page) in self.work.pages.iter().enumerate() {
            for (koma_index, koma) in page.komas.iter().enumerate() {
                if koma.id == koma_id {
                    return Some((page_index, koma_index));
                }
            }
        }
        None
    }

    /// 指定 koma/layer のビットマップ全体を複製して返す。
    pub fn clone_koma_layer_bitmap(
        &self,
        koma_id: KomaId,
        layer_index: usize,
    ) -> Option<CanvasBitmap> {
        let (page_idx, koma_idx) = self.find_koma_location(koma_id)?;
        let koma = &self.work.pages[page_idx].komas[koma_idx];
        koma.layers.get(layer_index).map(|layer| layer.bitmap.clone())
    }

    /// 指定 koma/layer の指定領域を複製して返す（コマローカル座標系）。
    pub fn capture_koma_layer_region(
        &self,
        koma_id: KomaId,
        layer_index: usize,
        dirty: CanvasDirtyRect,
    ) -> Option<CanvasBitmap> {
        let (page_idx, koma_idx) = self.find_koma_location(koma_id)?;
        let koma = &self.work.pages[page_idx].komas[koma_idx];
        let layer = koma.layers.get(layer_index)?;
        extract_bitmap_region(&layer.bitmap, dirty.x, dirty.y, dirty.width, dirty.height)
    }

    /// 指定 koma/layer の指定位置にビットマップを復元し、コマ合成も更新する。
    ///
    /// 返値はページ座標系の dirty rect。
    pub fn restore_koma_layer_region(
        &mut self,
        koma_id: KomaId,
        layer_index: usize,
        x: usize,
        y: usize,
        bitmap: &CanvasBitmap,
    ) -> Option<CanvasDirtyRect> {
        let (page_idx, koma_idx) = self.find_koma_location(koma_id)?;
        let koma_bounds = self.work.pages[page_idx].komas[koma_idx].bounds;
        let (page_width, page_height) = {
            let page = &self.work.pages[page_idx];
            (page.width, page.height)
        };
        let koma = &mut self.work.pages[page_idx].komas[koma_idx];
        if let Some(layer) = koma.layers.get_mut(layer_index) {
            write_bitmap_region(&mut layer.bitmap, x, y, bitmap);
        }
        let dirty = CanvasDirtyRect {
            x,
            y,
            width: bitmap.width,
            height: bitmap.height,
        };
        composite_koma_bitmap_region(koma, dirty);
        Some(local_dirty_to_page_dirty(
            dirty,
            koma_bounds,
            page_width,
            page_height,
        ))
    }

    /// 指定 koma の指定 layer をビットマップを透明にリセットし、コマ合成も更新する。
    pub fn reset_koma_layer_to_transparent(&mut self, koma_id: KomaId, layer_index: usize) {
        let Some((page_idx, koma_idx)) = self.find_koma_location(koma_id) else {
            return;
        };
        let koma = &mut self.work.pages[page_idx].komas[koma_idx];
        if let Some(layer) = koma.layers.get_mut(layer_index) {
            let (w, h) = (layer.bitmap.width, layer.bitmap.height);
            layer.bitmap = CanvasBitmap::transparent(w, h);
        }
        let new_bitmap = composite_koma_bitmap(&self.work.pages[page_idx].komas[koma_idx]);
        self.work.pages[page_idx].komas[koma_idx].bitmap = new_bitmap;
    }

    /// 指定 koma の指定 layer に `BitmapEdit` を適用し、コマ合成も更新する。
    pub fn apply_bitmap_edits_to_koma_layer(
        &mut self,
        koma_id: KomaId,
        layer_index: usize,
        edits: &[BitmapEdit],
    ) -> Option<CanvasDirtyRect> {
        if edits.is_empty() {
            return None;
        }
        let (page_idx, koma_idx) = self.find_koma_location(koma_id)?;
        let koma_bounds = self.work.pages[page_idx].komas[koma_idx].bounds;
        let (page_width, page_height) = {
            let page = &self.work.pages[page_idx];
            (page.width, page.height)
        };
        let koma = &mut self.work.pages[page_idx].komas[koma_idx];
        // layer_index override: set active_layer_index temporarily
        let saved_index = koma.active_layer_index;
        koma.active_layer_index = layer_index.min(koma.layers.len().saturating_sub(1));
        let dirty = apply_bitmap_edits(koma, edits);
        if let Some(dirty) = dirty {
            composite_koma_bitmap_region(koma, dirty);
        }
        koma.active_layer_index = saved_index;
        let dirty = dirty?;
        Some(local_dirty_to_page_dirty(dirty, koma_bounds, page_width, page_height))
    }

    pub fn add_raster_layer(&mut self) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            koma.created_layer_count = koma.created_layer_count.saturating_add(1);
            let next_index = koma.created_layer_count;
            let (width, height) = (koma.bitmap.width, koma.bitmap.height);
            koma.layers.push(RasterLayer::transparent(
                LayerNodeId(next_index),
                format!("Layer {next_index}"),
                width,
                height,
            ));
            koma.active_layer_index = koma.layers.len().saturating_sub(1);
        }
    }

    pub fn remove_active_layer(&mut self) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if koma.layers.len() <= 1 {
                return;
            }
            koma.layers.remove(koma.active_layer_index);
            koma.active_layer_index = koma
                .active_layer_index
                .min(koma.layers.len().saturating_sub(1));
            koma.bitmap = composite_koma_bitmap(koma);
        }
    }

    pub fn select_layer(&mut self, index: usize) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            koma.active_layer_index = index.min(koma.layers.len().saturating_sub(1));
        }
    }

    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if let Some(layer) = koma.layers.get_mut(koma.active_layer_index) {
                layer.name = name.to_string();
            }
        }
    }

    pub fn move_layer(&mut self, from_index: usize, to_index: usize) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if koma.layers.len() <= 1 {
                return;
            }

            let last_index = koma.layers.len().saturating_sub(1);
            let from_index = from_index.min(last_index);
            let to_index = to_index.min(last_index);
            if from_index == to_index {
                return;
            }

            let moved = koma.layers.remove(from_index);
            koma.layers.insert(to_index, moved);

            koma.active_layer_index = match koma.active_layer_index {
                index if index == from_index => to_index,
                index if from_index < index && index <= to_index => index.saturating_sub(1),
                index if to_index <= index && index < from_index => index + 1,
                index => index,
            };

            koma.bitmap = composite_koma_bitmap(koma);
        }
    }

    pub fn select_next_layer(&mut self) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            koma.active_layer_index = (koma.active_layer_index + 1) % koma.layers.len().max(1);
        }
    }

    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if let Some(layer) = koma.layers.get_mut(koma.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                koma.bitmap = composite_koma_bitmap(koma);
            }
        }
    }

    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if let Some(layer) = koma.layers.get_mut(koma.active_layer_index) {
                layer.blend_mode = mode;
                koma.bitmap = composite_koma_bitmap(koma);
            }
        }
    }

    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(koma) = self.active_koma_mut() {
            ensure_koma_layers(koma);
            if let Some(layer) = koma.layers.get_mut(koma.active_layer_index) {
                layer.visible = !layer.visible;
                koma.bitmap = composite_koma_bitmap(koma);
            }
        }
    }

}

pub(super) fn ensure_koma_layers(koma: &mut Koma) {
    let mut repaired = false;
    if koma.layers.is_empty() {
        koma.layers.push(RasterLayer::background(
            LayerNodeId(1),
            "Layer 1".to_string(),
            koma.bitmap.width,
            koma.bitmap.height,
        ));
        if let Some(layer) = koma.layers.first_mut() {
            layer.bitmap = koma.bitmap.clone();
        }
        repaired = true;
    }
    koma.created_layer_count = koma
        .created_layer_count
        .max(koma.layers.len() as u64)
        .max(1);
    koma.active_layer_index = koma
        .active_layer_index
        .min(koma.layers.len().saturating_sub(1));
    if repaired {
        koma.bitmap = composite_koma_bitmap(koma);
    }
}


fn apply_bitmap_edits(koma: &mut Koma, edits: &[BitmapEdit]) -> Option<CanvasDirtyRect> {
    let active_index = koma
        .active_layer_index
        .min(koma.layers.len().saturating_sub(1));
    let layer = &mut koma.layers[active_index];
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

pub(super) fn composite_koma_bitmap(koma: &Koma) -> CanvasBitmap {
    let width = koma
        .layers
        .first()
        .map(|layer| layer.bitmap.width.max(1))
        .unwrap_or_else(|| koma.bitmap.width.max(1));
    let height = koma
        .layers
        .first()
        .map(|layer| layer.bitmap.height.max(1))
        .unwrap_or_else(|| koma.bitmap.height.max(1));
    let mut result = CanvasBitmap::transparent(width, height);
    for layer in &koma.layers {
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

fn composite_koma_bitmap_region(koma: &mut Koma, dirty: CanvasDirtyRect) {
    let dirty = dirty.clamp_to_canvas_bounds(koma.bitmap.width.max(1), koma.bitmap.height.max(1));
    if let Some(layer_index) = single_passthrough_layer_index(koma) {
        copy_bitmap_region(&koma.layers[layer_index].bitmap, &mut koma.bitmap, dirty);
        return;
    }

    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let index = (y * koma.bitmap.width + x) * 4;
            koma.bitmap.pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
    }

    for layer in &koma.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(&mut koma.bitmap, layer, dirty);
    }
}

fn single_passthrough_layer_index(koma: &Koma) -> Option<usize> {
    let mut visible_layers = koma
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
    if layer.bitmap.width != koma.bitmap.width || layer.bitmap.height != koma.bitmap.height {
        return None;
    }
    Some(index)
}

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
