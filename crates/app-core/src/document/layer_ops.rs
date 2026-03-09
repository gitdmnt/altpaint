//! `Document` のレイヤー編集と合成処理をまとめる。
//!
//! 公開 command 境界の背後にある layer 操作・合成 helper をここへ集約し、
//! ドキュメント本体を状態遷移の入口として読みやすく保つ。

use std::iter::Peekable;
use std::str::Chars;

use super::{
    BlendMode, CanvasBitmap, DirtyRect, Document, LayerMask, LayerNodeId, Panel, RasterLayer,
};

impl Document {
    /// 先頭のコマのビットマップへ1点描画する。
    pub fn draw_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        self.draw_point_with_pressure(x, y, 1.0)
    }

    /// 先頭のコマのビットマップへ筆圧付きで1点描画する。
    pub fn draw_point_with_pressure(
        &mut self,
        x: usize,
        y: usize,
        pressure: f32,
    ) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size_with_pressure(pressure);
        let antialias = self
            .active_pen_preset()
            .map(|preset| preset.antialias)
            .unwrap_or(true);
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, color, false, size, antialias);
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
        self.draw_stroke_with_pressure(from_x, from_y, to_x, to_y, 1.0)
    }

    /// 先頭のコマのビットマップへ筆圧付きストロークを描画する。
    pub fn draw_stroke_with_pressure(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        pressure: f32,
    ) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let size = self.active_draw_size_with_pressure(pressure);
        let antialias = self
            .active_pen_preset()
            .map(|preset| preset.antialias)
            .unwrap_or(true);
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_line_on_active_layer(
                panel, from_x, from_y, to_x, to_y, color, false, size, antialias,
            );
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// アクティブレイヤー上の1点を消去する。
    pub fn erase_point(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        self.erase_point_with_pressure(x, y, 1.0)
    }

    /// アクティブレイヤー上の1点を筆圧付きで消去する。
    pub fn erase_point_with_pressure(
        &mut self,
        x: usize,
        y: usize,
        pressure: f32,
    ) -> Option<DirtyRect> {
        let size = self.active_draw_size_with_pressure(pressure);
        let antialias = self
            .active_pen_preset()
            .map(|preset| preset.antialias)
            .unwrap_or(true);
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            let dirty = draw_on_active_layer(panel, x, y, [0, 0, 0, 0], true, size, antialias);
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
        self.erase_stroke_with_pressure(from_x, from_y, to_x, to_y, 1.0)
    }

    /// アクティブレイヤー上の線分を筆圧付きで消去する。
    pub fn erase_stroke_with_pressure(
        &mut self,
        from_x: usize,
        from_y: usize,
        to_x: usize,
        to_y: usize,
        pressure: f32,
    ) -> Option<DirtyRect> {
        let size = self.active_draw_size_with_pressure(pressure);
        let antialias = self
            .active_pen_preset()
            .map(|preset| preset.antialias)
            .unwrap_or(true);
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
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
                antialias,
            );
            composite_panel_bitmap_region(panel, dirty);
            return Some(dirty);
        }

        None
    }

    /// 閉領域バケツ塗りを行う。
    pub fn fill_region(&mut self, x: usize, y: usize) -> Option<DirtyRect> {
        let color = self.active_color.to_rgba8();
        let panel = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())?;
        ensure_panel_layers(panel);
        let target = panel.bitmap.pixel_rgba(x, y)?;
        if target == color {
            return None;
        }
        let mut stack = vec![(x, y)];
        let mut visited = vec![false; panel.bitmap.width.saturating_mul(panel.bitmap.height)];
        let mut dirty: Option<DirtyRect> = None;

        while let Some((current_x, current_y)) = stack.pop() {
            if current_x >= panel.bitmap.width || current_y >= panel.bitmap.height {
                continue;
            }
            let index = current_y * panel.bitmap.width + current_x;
            if visited[index] {
                continue;
            }
            visited[index] = true;
            if panel.bitmap.pixel_rgba(current_x, current_y) != Some(target) {
                continue;
            }

            let rect = panel.layers[panel.active_layer_index]
                .bitmap
                .set_pixel_rgba(current_x, current_y, color);
            dirty = Some(match dirty {
                Some(current) => current.union(rect),
                None => rect,
            });

            if current_x > 0 {
                stack.push((current_x - 1, current_y));
            }
            if current_x + 1 < panel.bitmap.width {
                stack.push((current_x + 1, current_y));
            }
            if current_y > 0 {
                stack.push((current_x, current_y - 1));
            }
            if current_y + 1 < panel.bitmap.height {
                stack.push((current_x, current_y + 1));
            }
        }

        let dirty = dirty?;
        composite_panel_bitmap_region(panel, dirty);
        Some(dirty)
    }

    /// 投げ縄ポリゴン内部を塗り潰す。
    pub fn fill_lasso(&mut self, points: &[(usize, usize)]) -> Option<DirtyRect> {
        if points.len() < 3 {
            return None;
        }
        let color = self.active_color.to_rgba8();
        let panel = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())?;
        ensure_panel_layers(panel);
        let min_x = points.iter().map(|(x, _)| *x).min()?;
        let min_y = points.iter().map(|(_, y)| *y).min()?;
        let max_x = points.iter().map(|(x, _)| *x).max()?;
        let max_y = points.iter().map(|(_, y)| *y).max()?;
        let width_limit = panel.bitmap.width.saturating_sub(1);
        let height_limit = panel.bitmap.height.saturating_sub(1);
        let mut dirty: Option<DirtyRect> = None;

        for y in min_y..=max_y.min(height_limit) {
            for x in min_x..=max_x.min(width_limit) {
                if !point_in_polygon((x as f32) + 0.5, (y as f32) + 0.5, points) {
                    continue;
                }
                let rect = panel.layers[panel.active_layer_index]
                    .bitmap
                    .set_pixel_rgba(x, y, color);
                dirty = Some(match dirty {
                    Some(current) => current.union(rect),
                    None => rect,
                });
            }
        }

        let dirty = dirty?;
        composite_panel_bitmap_region(panel, dirty);
        Some(dirty)
    }

    /// 透過レイヤーを末尾へ追加して選択する。
    pub fn add_raster_layer(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
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
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
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

    /// アクティブレイヤーを指定 index へ切り替える。
    pub fn select_layer(&mut self, index: usize) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            panel.active_layer_index = index.min(panel.layers.len().saturating_sub(1));
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤー名を更新する。
    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.name = name.to_string();
                sync_root_layer_summary(panel);
            }
        }
    }

    /// レイヤー順序を入れ替え、選択 index も追従させる。
    pub fn move_layer(&mut self, from_index: usize, to_index: usize) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
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
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            panel.active_layer_index = (panel.active_layer_index + 1) % panel.layers.len().max(1);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤーの blend mode を次へ循環する。
    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの blend mode を明示設定する。
    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = mode;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの可視状態を反転する。
    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.visible = !layer.visible;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーにデモ用マスクを付与または解除する。
    pub fn toggle_active_layer_mask(&mut self) {
        if let Some(panel) = self
            .work
            .pages
            .first_mut()
            .and_then(|page| page.panels.first_mut())
        {
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
    antialias: bool,
) -> DirtyRect {
    let active_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer.bitmap.erase_point_sized(x, y, size, antialias)
        } else {
            layer
                .bitmap
                .draw_point_sized_rgba(x, y, color, size, antialias)
        }
    } else {
        layer
            .bitmap
            .draw_point_sized_rgba(x, y, color, size, antialias)
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
    antialias: bool,
) -> DirtyRect {
    let active_index = panel
        .active_layer_index
        .min(panel.layers.len().saturating_sub(1));
    let is_background = active_index == 0;
    let layer = &mut panel.layers[active_index];
    if erase {
        if is_background {
            layer
                .bitmap
                .erase_line_sized(from_x, from_y, to_x, to_y, size, antialias)
        } else {
            layer
                .bitmap
                .draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size, antialias)
        }
    } else {
        layer
            .bitmap
            .draw_line_sized_rgba(from_x, from_y, to_x, to_y, color, size, antialias)
    }
}

fn point_in_polygon(x: f32, y: f32, points: &[(usize, usize)]) -> bool {
    let mut inside = false;
    let mut previous = *points.last().expect("polygon has points");
    for &(current_x, current_y) in points {
        let (x1, y1) = (previous.0 as f32, previous.1 as f32);
        let (x2, y2) = (current_x as f32, current_y as f32);
        let intersects = ((y1 > y) != (y2 > y))
            && (x < (x2 - x1) * (y - y1) / ((y2 - y1).abs().max(f32::EPSILON)) + x1);
        if intersects {
            inside = !inside;
        }
        previous = (current_x, current_y);
    }
    inside
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
    let custom_formula = match &layer.blend_mode {
        BlendMode::Custom(formula) => CustomBlendFormula::parse(formula),
        BlendMode::Normal | BlendMode::Multiply | BlendMode::Screen | BlendMode::Add => None,
    };
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
            let blended = blend_pixel(dst, src, &layer.blend_mode, custom_formula.as_ref());
            target.pixels[index..index + 4].copy_from_slice(&blended);
        }
    }
}

/// blend mode を考慮して 1 ピクセルを合成する。
fn blend_pixel(
    dst: [u8; 4],
    src: [u8; 4],
    mode: &BlendMode,
    custom_formula: Option<&CustomBlendFormula>,
) -> [u8; 4] {
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
            BlendMode::Custom(_) => custom_formula
                .map(|formula| formula.evaluate(s, d))
                .unwrap_or(s),
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

#[derive(Debug, Clone)]
struct CustomBlendFormula {
    expression: BlendExpr,
}

impl CustomBlendFormula {
    fn parse(input: &str) -> Option<Self> {
        let expression = BlendExprParser::parse(input.trim())?;
        Some(Self { expression })
    }

    fn evaluate(&self, src: f32, dst: f32) -> f32 {
        self.expression.evaluate(src, dst).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone)]
enum BlendExpr {
    Number(f32),
    Variable(BlendVariable),
    UnaryMinus(Box<BlendExpr>),
    Binary {
        op: BlendBinaryOp,
        left: Box<BlendExpr>,
        right: Box<BlendExpr>,
    },
    Function {
        name: String,
        args: Vec<BlendExpr>,
    },
}

impl BlendExpr {
    fn evaluate(&self, src: f32, dst: f32) -> f32 {
        match self {
            Self::Number(value) => *value,
            Self::Variable(BlendVariable::Src) => src,
            Self::Variable(BlendVariable::Dst) => dst,
            Self::UnaryMinus(value) => -value.evaluate(src, dst),
            Self::Binary { op, left, right } => {
                let left = left.evaluate(src, dst);
                let right = right.evaluate(src, dst);
                match op {
                    BlendBinaryOp::Add => left + right,
                    BlendBinaryOp::Sub => left - right,
                    BlendBinaryOp::Mul => left * right,
                    BlendBinaryOp::Div => {
                        if right.abs() <= f32::EPSILON {
                            0.0
                        } else {
                            left / right
                        }
                    }
                }
            }
            Self::Function { name, args } => {
                let values = args
                    .iter()
                    .map(|arg| arg.evaluate(src, dst))
                    .collect::<Vec<_>>();
                match name.as_str() {
                    "min" if values.len() == 2 => values[0].min(values[1]),
                    "max" if values.len() == 2 => values[0].max(values[1]),
                    "clamp" if values.len() == 1 => values[0].clamp(0.0, 1.0),
                    "clamp" if values.len() == 3 => values[0].clamp(values[1], values[2]),
                    "abs" if values.len() == 1 => values[0].abs(),
                    "screen" if values.len() == 2 => 1.0 - (1.0 - values[0]) * (1.0 - values[1]),
                    "multiply" if values.len() == 2 => values[0] * values[1],
                    "add" if values.len() == 2 => (values[0] + values[1]).min(1.0),
                    "avg" if values.len() == 2 => (values[0] + values[1]) * 0.5,
                    _ => src,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BlendVariable {
    Src,
    Dst,
}

#[derive(Debug, Clone, Copy)]
enum BlendBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

struct BlendExprParser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> BlendExprParser<'a> {
    fn parse(input: &'a str) -> Option<BlendExpr> {
        let mut parser = Self {
            chars: input.chars().peekable(),
        };
        let expression = parser.parse_expression()?;
        parser.skip_whitespace();
        parser.chars.peek().is_none().then_some(expression)
    }

    fn parse_expression(&mut self) -> Option<BlendExpr> {
        let mut expr = self.parse_term()?;
        loop {
            self.skip_whitespace();
            let op = match self.chars.peek().copied() {
                Some('+') => BlendBinaryOp::Add,
                Some('-') => BlendBinaryOp::Sub,
                _ => break,
            };
            self.chars.next();
            let right = self.parse_term()?;
            expr = BlendExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Some(expr)
    }

    fn parse_term(&mut self) -> Option<BlendExpr> {
        let mut expr = self.parse_factor()?;
        loop {
            self.skip_whitespace();
            let op = match self.chars.peek().copied() {
                Some('*') => BlendBinaryOp::Mul,
                Some('/') => BlendBinaryOp::Div,
                _ => break,
            };
            self.chars.next();
            let right = self.parse_factor()?;
            expr = BlendExpr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Some(expr)
    }

    fn parse_factor(&mut self) -> Option<BlendExpr> {
        self.skip_whitespace();
        match self.chars.peek().copied()? {
            '(' => {
                self.chars.next();
                let expr = self.parse_expression()?;
                self.skip_whitespace();
                (self.chars.next() == Some(')')).then_some(expr)
            }
            '-' => {
                self.chars.next();
                self.parse_factor()
                    .map(|expr| BlendExpr::UnaryMinus(Box::new(expr)))
            }
            ch if ch.is_ascii_digit() || ch == '.' => self.parse_number().map(BlendExpr::Number),
            ch if ch.is_ascii_alphabetic() || ch == '_' => self.parse_identifier_or_function(),
            _ => None,
        }
    }

    fn parse_identifier_or_function(&mut self) -> Option<BlendExpr> {
        let identifier = self.parse_identifier()?;
        self.skip_whitespace();
        if self.chars.peek().copied() == Some('(') {
            self.chars.next();
            let mut args = Vec::new();
            loop {
                self.skip_whitespace();
                if self.chars.peek().copied() == Some(')') {
                    self.chars.next();
                    break;
                }
                args.push(self.parse_expression()?);
                self.skip_whitespace();
                match self.chars.peek().copied() {
                    Some(',') => {
                        self.chars.next();
                    }
                    Some(')') => {
                        self.chars.next();
                        break;
                    }
                    _ => return None,
                }
            }
            Some(BlendExpr::Function {
                name: identifier.to_ascii_lowercase(),
                args,
            })
        } else {
            match identifier.to_ascii_lowercase().as_str() {
                "src" | "s" => Some(BlendExpr::Variable(BlendVariable::Src)),
                "dst" | "d" => Some(BlendExpr::Variable(BlendVariable::Dst)),
                _ => None,
            }
        }
    }

    fn parse_identifier(&mut self) -> Option<String> {
        let mut identifier = String::new();
        while let Some(ch) = self.chars.peek().copied() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                identifier.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        (!identifier.is_empty()).then_some(identifier)
    }

    fn parse_number(&mut self) -> Option<f32> {
        let mut text = String::new();
        let mut has_digit = false;
        while let Some(ch) = self.chars.peek().copied() {
            if ch.is_ascii_digit() || ch == '.' {
                has_digit |= ch.is_ascii_digit();
                text.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }
        has_digit.then(|| text.parse::<f32>().ok()).flatten()
    }

    fn skip_whitespace(&mut self) {
        while self.chars.peek().copied().is_some_and(char::is_whitespace) {
            self.chars.next();
        }
    }
}
