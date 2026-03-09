//! `Document` のレイヤー編集と合成処理をまとめる。
//!
//! 公開 command 境界の背後にある layer 操作・合成 helper をここへ集約し、
//! ドキュメント本体を状態遷移の入口として読みやすく保つ。

use super::{BlendMode, CanvasBitmap, DirtyRect, Document, LayerMask, LayerNodeId, Panel, RasterLayer};

impl Document {
    /// 先頭のコマのビットマップへ1点描画する。
    pub fn draw_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size();
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, color, false, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// 先頭のコマのビットマップへ最小ストロークを描画する。
    pub fn draw_stroke(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size();
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            let dirty = draw_line_on_active_layer(panel, from_x, from_y, to_x, to_y, color, false, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// アクティブレイヤー上の1点を消去する。
    pub fn erase_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        let size = self.active_draw_size();
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, [0, 0, 0, 0], true, size);
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// アクティブレイヤー上の線分を消去する。
    pub fn erase_stroke(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
    ) -> Option<DirtyRect> {
        let size = self.active_draw_size();
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            let dirty = draw_line_on_active_layer(
                panel,
                from_x,
                from_y,
                to_x,
                to_y,
                [0, 0, 0, 0],
                true,
                size,
            );
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// 透過レイヤーを末尾へ追加して選択する。
    pub fn add_raster_layer(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
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

    /// アクティブレイヤーを削除するが、最後の1枚は残す。
    pub fn remove_active_layer(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            if panel.layers.len() <= 1 {
                return;
            }
            panel.layers.remove(panel.active_layer_index);
            panel.active_layer_index = panel.active_layer_index.min(panel.layers.len().saturating_sub(1));
            panel.bitmap = composite_panel_bitmap(panel);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤーを指定 index へ切り替える。
    pub fn select_layer(&mut self, index: usize) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            panel.active_layer_index = index.min(panel.layers.len().saturating_sub(1));
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤー名を更新する。
    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.name = name.to_string();
                sync_root_layer_summary(panel);
            }
        }
    }

    /// レイヤー順序を入れ替え、選択 index も追従させる。
    pub fn move_layer(&mut self, from_index: usize, to_index: usize) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
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

    /// 次のレイヤーへ循環選択する。
    pub fn select_next_layer(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            panel.active_layer_index = (panel.active_layer_index + 1) % panel.layers.len().max(1);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤーの blend mode を次へ循環する。
    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの blend mode を明示設定する。
    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = mode;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの可視状態を反転する。
    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.visible = !layer.visible;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーにデモ用マスクを付与または解除する。
    pub fn toggle_active_layer_mask(&mut self) {
        if let Some(panel) = self.work.pages.first_mut().and_then(|page| page.panels.first_mut()) {
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

/// 最低1枚のレイヤーが存在するよう panel 状態を補正する。
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
    panel.created_layer_count = panel.created_layer_count.max(panel.layers.len() as u64).max(1);
    panel.active_layer_index = panel.active_layer_index.min(panel.layers.len().saturating_sub(1));
    sync_root_layer_summary(panel);
    if repaired {
        panel.bitmap = composite_panel_bitmap(panel);
    }
}

/// UI 用の root layer summary を現在の選択レイヤーへ同期する。
fn sync_root_layer_summary(panel: &mut Panel) {
    if let Some(layer) = panel.layers.get(panel.active_layer_index) {
        panel.root_layer.id = layer.id;
        panel.root_layer.name = layer.name.clone();
    }
}

/// アクティブレイヤーへ単一点の描画または消去を適用する。
fn draw_on_active_layer(
    panel: &mut Panel,
    x: usize,
    y: usize,
    color: [u8; 4],
    erase: bool,
    size: u32,
) -> DirtyRect {
    let active_index = panel.active_layer_index.min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer.bitmap.erase_point_sized(x, y, size)
        } else {
            layer.bitmap.draw_point_sized_rgba(x, y, color, size)
        }
    } else {
        layer.bitmap.draw_point_sized_rgba(x, y, color, size)
    }
}

/// アクティブレイヤーへ線描画または線消去を適用する。
#[allow(clippy::too_many_arguments)]
fn draw_line_on_active_layer(
    panel: &mut Panel,
    from_x: usize,
    from_y: usize,
    to_x: usize,
    to_y: usize,
    color: [u8; 4],
    erase: bool,
    size: u32,
) -> DirtyRect {
    let active_index = panel.active_layer_index.min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer.bitmap.erase_line_sized(from_x, from_y, to_x, to_y, size)
        } else {
            layer.bitmap.draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size)
        }
    } else {
        layer.bitmap.draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size)
    }
}

/// 全レイヤーを合成して panel bitmap を再構築する。
fn composite_panel_bitmap(panel: &Panel) -> CanvasBitmap {
    let width = panel.bitmap.width.max(1);
    let height = panel.bitmap.height.max(1);
    let mut result = CanvasBitmap::transparent(width, height);
    for layer in &panel.layers {
        if !layer.visible {
            continue;
        }
        composite_layer_region_into(
            &mut result,
            layer,
            DirtyRect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
    }
    result
}

/// dirty rect に限定して panel bitmap を再合成する。
fn composite_panel_bitmap_region(panel: &mut Panel, dirty: DirtyRect) {
    let dirty = dirty.clamp_to_bitmap(panel.bitmap.width.max(1), panel.bitmap.height.max(1));
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

/// 単一レイヤーの dirty rect だけを target bitmap へ合成する。
fn composite_layer_region_into(target: &mut CanvasBitmap, layer: &RasterLayer, dirty: DirtyRect) {
    let dirty = dirty.clamp_to_bitmap(
        target.width.min(layer.bitmap.width).max(1),
        target.height.min(layer.bitmap.height).max(1),
    );
    for y in dirty.y..dirty.y + dirty.height {
        for x in dirty.x..dirty.x + dirty.width {
            let index = (y * target.width + x) * 4;
            let mut src = [
                layer.bitmap.pixels[index],
                layer.bitmap.pixels[index + 1],
                layer.bitmap.pixels[index + 2],
                layer.bitmap.pixels[index + 3],
            ];
            if let Some(mask) = &layer.mask {
                src[3] = ((src[3] as u16 * mask.alpha_at(x, y) as u16) / 255) as u8;
            }
            let dst = [
                target.pixels[index],
                target.pixels[index + 1],
                target.pixels[index + 2],
                target.pixels[index + 3],
            ];
            let blended = blend_pixel(dst, src, layer.blend_mode);
            target.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
}

/// blend mode を考慮して 1 ピクセルを合成する。
fn blend_pixel(dst: [u8; 4], src: [u8; 4], mode: BlendMode) -> [u8; 4] {
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
