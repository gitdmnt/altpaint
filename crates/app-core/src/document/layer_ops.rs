//! `Document` のレイヤー編集と合成処理をまとめる。
//!
//! 公開 command 境界の背後にある layer 操作・合成 helper をここへ集約し、
//! ドキュメント本体を状態遷移の入口として読みやすく保つ。

use std::iter::Peekable;
use std::str::Chars;

use crate::{BitmapEdit, CanvasDirtyRect, ClampToCanvasBounds, MergeInSpace};

use super::{
    BlendMode, CanvasBitmap, Document, LayerMask, LayerNodeId, Panel, RasterLayer,
};

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
    /// 描画プラグインが生成したビットマップ差分をアクティブレイヤーへ反映する。
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

    /// 透過レイヤーを末尾へ追加して選択する。
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

    /// アクティブレイヤーを削除するが、最後の1枚は残す。
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

    /// アクティブレイヤーを指定 index へ切り替える。
    pub fn select_layer(&mut self, index: usize) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            panel.active_layer_index = index.min(panel.layers.len().saturating_sub(1));
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤー名を更新する。
    pub fn rename_active_layer(&mut self, name: &str) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.name = name.to_string();
                sync_root_layer_summary(panel);
            }
        }
    }

    /// レイヤー順序を入れ替え、選択 index も追従させる。
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

    /// 次のレイヤーへ循環選択する。
    pub fn select_next_layer(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            panel.active_layer_index = (panel.active_layer_index + 1) % panel.layers.len().max(1);
            sync_root_layer_summary(panel);
        }
    }

    /// アクティブレイヤーの blend mode を次へ循環する。
    pub fn cycle_active_layer_blend_mode(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = layer.blend_mode.next();
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの blend mode を明示設定する。
    pub fn set_active_layer_blend_mode(&mut self, mode: BlendMode) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.blend_mode = mode;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーの可視状態を反転する。
    pub fn toggle_active_layer_visibility(&mut self) {
        if let Some(panel) = self.active_panel_mut() {
            ensure_panel_layers(panel);
            if let Some(layer) = panel.layers.get_mut(panel.active_layer_index) {
                layer.visible = !layer.visible;
                panel.bitmap = composite_panel_bitmap(panel);
            }
        }
    }

    /// アクティブレイヤーにデモ用マスクを付与または解除する。
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
        let incoming = extract_bitmap_region(&edit.bitmap, source_x, source_y, dirty.width, dirty.height)?;
        let previous = extract_bitmap_region(&layer.bitmap, dirty.x, dirty.y, dirty.width, dirty.height)?;
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

fn write_bitmap_region(target: &mut CanvasBitmap, start_x: usize, start_y: usize, region: &CanvasBitmap) {
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

/// 全レイヤーを合成して panel bitmap を再構築する。
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

/// dirty rect に限定して panel bitmap を再合成する。
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

/// 単一レイヤーの dirty rect だけを target bitmap へ合成する。
fn composite_layer_region_into(
    target: &mut CanvasBitmap,
    layer: &RasterLayer,
    dirty: CanvasDirtyRect,
) {
    let custom_formula = match &layer.blend_mode {
        BlendMode::Custom(formula) => CustomBlendFormula::parse(formula),
        BlendMode::Normal | BlendMode::Multiply | BlendMode::Screen | BlendMode::Add => None,
    };
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
            let blended = blend_pixel(dst, src, &layer.blend_mode, custom_formula.as_ref());
            target.pixels[target_index..target_index + 4].copy_from_slice(&blended);
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
