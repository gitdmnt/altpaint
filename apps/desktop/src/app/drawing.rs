use std::collections::BTreeMap;

use app_core::{
	BitmapComposite, BitmapCompositor, BitmapEdit, CanvasBitmap, CanvasDirtyRect, PaintInput,
	PaintPlugin, PaintPluginContext, PanelLocalPoint, PenTipBitmap, ToolKind,
};

pub(super) type PaintPluginRegistry = BTreeMap<String, Box<dyn PaintPlugin>>;

pub(super) fn default_paint_plugins() -> PaintPluginRegistry {
	let mut plugins: PaintPluginRegistry = BTreeMap::new();
	plugins.insert(
		STANDARD_BITMAP_PLUGIN_ID.to_string(),
		Box::new(BuiltinBitmapPaintPlugin),
	);
	plugins
}

pub(super) const STANDARD_BITMAP_PLUGIN_ID: &str = "builtin.bitmap";

struct BuiltinBitmapPaintPlugin;

impl PaintPlugin for BuiltinBitmapPaintPlugin {
	fn id(&self) -> &'static str {
		STANDARD_BITMAP_PLUGIN_ID
	}

	fn process(&self, input: &PaintInput, context: &PaintPluginContext<'_>) -> Vec<BitmapEdit> {
		match input {
			PaintInput::Stamp { at, pressure } => stamp_edit(*at, *pressure, context).into_iter().collect(),
			PaintInput::StrokeSegment { from, to, pressure } => {
				stroke_segment_edit(*from, *to, *pressure, context).into_iter().collect()
			}
			PaintInput::FloodFill { at } => flood_fill_edit(*at, context).into_iter().collect(),
			PaintInput::LassoFill { points } => lasso_fill_edit(points, context).into_iter().collect(),
		}
	}
}

#[derive(Clone, Copy)]
struct EraseComposite;


impl BitmapCompositor for EraseComposite {
	fn compose(&self, bitmap_a: &CanvasBitmap, bitmap_b: &CanvasBitmap) -> CanvasBitmap {
		let width = bitmap_a.width.min(bitmap_b.width);
		let height = bitmap_a.height.min(bitmap_b.height);
		let mut out = CanvasBitmap::transparent(width, height);
		for y in 0..height {
			for x in 0..width {
				let index = (y * width + x) * 4;
				let erase = bitmap_a.pixels[index + 3] as f32 / 255.0;
				let previous = [
					bitmap_b.pixels[index],
					bitmap_b.pixels[index + 1],
					bitmap_b.pixels[index + 2],
					bitmap_b.pixels[index + 3],
				];
				if erase <= 0.0 {
					out.pixels[index..index + 4].copy_from_slice(&previous);
					continue;
				}
				let remaining_alpha = (previous[3] as f32 / 255.0) * (1.0 - erase);
				let mut result = previous;
				result[3] = (remaining_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
				if result[3] == 0 {
					result[0] = 0;
					result[1] = 0;
					result[2] = 0;
				}
				out.pixels[index..index + 4].copy_from_slice(&result);
			}
		}
		out
	}
}

fn stamp_edit(
	at: PanelLocalPoint,
	pressure: f32,
	context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
	stroke_like_edit(&[at], pressure, context)
}

fn stroke_segment_edit(
	from: PanelLocalPoint,
	to: PanelLocalPoint,
	pressure: f32,
	context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
	let size = effective_size(context, pressure).max(1);
	let spacing = effective_spacing(context, size);
	let dx = to.x as f32 - from.x as f32;
	let dy = to.y as f32 - from.y as f32;
	let distance = dx.hypot(dy);
	let steps = (distance / spacing).ceil().max(1.0) as usize;
	let mut points = Vec::with_capacity(steps + 1);
	for step in 0..=steps {
		let t = if steps == 0 {
			0.0
		} else {
			step as f32 / steps as f32
		};
		let x = from.x as f32 + dx * t;
		let y = from.y as f32 + dy * t;
		points.push(PanelLocalPoint::new(
			x.round().max(0.0) as usize,
			y.round().max(0.0) as usize,
		));
	}
	stroke_like_edit(&points, pressure, context)
}

fn stroke_like_edit(
	points: &[PanelLocalPoint],
	pressure: f32,
	context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
	if points.is_empty() {
		return None;
	}
	let stamp = build_stamp(context, pressure)?;
	let half_w = stamp.width as isize / 2;
	let half_h = stamp.height as isize / 2;
	let mut left = usize::MAX;
	let mut top = usize::MAX;
	let mut right = 0usize;
	let mut bottom = 0usize;
	for point in points {
		let stamp_left = point.x.saturating_sub(half_w.max(0) as usize);
		let stamp_top = point.y.saturating_sub(half_h.max(0) as usize);
		left = left.min(stamp_left);
		top = top.min(stamp_top);
		right = right.max(stamp_left.saturating_add(stamp.width));
		bottom = bottom.max(stamp_top.saturating_add(stamp.height));
	}
	if left == usize::MAX || right <= left || bottom <= top {
		return None;
	}

	let dirty_rect = CanvasDirtyRect {
		x: left,
		y: top,
		width: right - left,
		height: bottom - top,
	};
	let mut bitmap = CanvasBitmap::transparent(dirty_rect.width, dirty_rect.height);
	for point in points {
		let local_x = point.x.saturating_sub(left);
		let local_y = point.y.saturating_sub(top);
		blend_stamp(
			&mut bitmap,
			&stamp,
			local_x as isize - half_w,
			local_y as isize - half_h,
		);
	}

	Some(BitmapEdit::new(dirty_rect, bitmap, edit_composite(context)))
}

fn flood_fill_edit(at: PanelLocalPoint, context: &PaintPluginContext<'_>) -> Option<BitmapEdit> {
	let width = context.composited_bitmap.width;
	let height = context.composited_bitmap.height;
	if at.x >= width || at.y >= height {
		return None;
	}
	let target = context.composited_bitmap.pixel_rgba(at.x, at.y)?;
	let fill = fill_color(context);
	if target == fill {
		return None;
	}

	let mut visited = vec![false; width.saturating_mul(height)];
	let mut stack = vec![(at.x, at.y)];
	let mut points = Vec::new();
	let mut min_x = usize::MAX;
	let mut min_y = usize::MAX;
	let mut max_x = 0usize;
	let mut max_y = 0usize;

	while let Some((x, y)) = stack.pop() {
		if x >= width || y >= height {
			continue;
		}
		let index = y * width + x;
		if visited[index] {
			continue;
		}
		visited[index] = true;
		if context.composited_bitmap.pixel_rgba(x, y) != Some(target) {
			continue;
		}
		points.push((x, y));
		min_x = min_x.min(x);
		min_y = min_y.min(y);
		max_x = max_x.max(x);
		max_y = max_y.max(y);

		if x > 0 {
			stack.push((x - 1, y));
		}
		if x + 1 < width {
			stack.push((x + 1, y));
		}
		if y > 0 {
			stack.push((x, y - 1));
		}
		if y + 1 < height {
			stack.push((x, y + 1));
		}
	}

	bitmap_from_points(points, min_x, min_y, max_x, max_y, fill, context)
}

fn lasso_fill_edit(
	points: &[PanelLocalPoint],
	context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
	if points.len() < 3 {
		return None;
	}
	let min_x = points.iter().map(|point| point.x).min()?;
	let min_y = points.iter().map(|point| point.y).min()?;
	let max_x = points.iter().map(|point| point.x).max()?;
	let max_y = points.iter().map(|point| point.y).max()?;
	let width_limit = context.composited_bitmap.width.saturating_sub(1);
	let height_limit = context.composited_bitmap.height.saturating_sub(1);
	let fill = fill_color(context);
	let mut hits = Vec::new();

	for y in min_y..=max_y.min(height_limit) {
		for x in min_x..=max_x.min(width_limit) {
			if point_in_polygon((x as f32) + 0.5, (y as f32) + 0.5, points) {
				hits.push((x, y));
			}
		}
	}

	bitmap_from_points(hits, min_x, min_y, max_x, max_y, fill, context)
}

fn bitmap_from_points(
	points: Vec<(usize, usize)>,
	min_x: usize,
	min_y: usize,
	max_x: usize,
	max_y: usize,
	fill: [u8; 4],
	context: &PaintPluginContext<'_>,
) -> Option<BitmapEdit> {
	if points.is_empty() || min_x == usize::MAX || min_y == usize::MAX {
		return None;
	}
	let dirty_rect = CanvasDirtyRect {
		x: min_x,
		y: min_y,
		width: max_x.saturating_sub(min_x).saturating_add(1),
		height: max_y.saturating_sub(min_y).saturating_add(1),
	};
	let mut bitmap = CanvasBitmap::transparent(dirty_rect.width, dirty_rect.height);
	for (x, y) in points {
		let local_x = x.saturating_sub(min_x);
		let local_y = y.saturating_sub(min_y);
		let index = (local_y * bitmap.width + local_x) * 4;
		bitmap.pixels[index..index + 4].copy_from_slice(&fill);
	}
	Some(BitmapEdit::new(dirty_rect, bitmap, edit_composite(context)))
}

fn build_stamp(context: &PaintPluginContext<'_>, pressure: f32) -> Option<CanvasBitmap> {
	let size = effective_size(context, pressure).max(1) as usize;
	let opacity = (context.pen.opacity * context.pen.flow).clamp(0.0, 1.0);
	let color = stamp_color(context);
	match context.pen.tip.as_ref() {
		Some(PenTipBitmap::AlphaMask8 { width, height, data }) if !data.is_empty() => Some(
			resample_alpha_tip(*width as usize, *height as usize, data, size, color, opacity),
		),
		Some(PenTipBitmap::Rgba8 { width, height, data }) if !data.is_empty() => Some(
			resample_rgba_tip(*width as usize, *height as usize, data, size, color, opacity),
		),
		Some(PenTipBitmap::AlphaMask8 { .. }) | Some(PenTipBitmap::Rgba8 { .. }) => {
			Some(generated_round_stamp(size, color, opacity, context.pen.antialias))
		}
		Some(PenTipBitmap::PngBlob { .. }) | None => {
			Some(generated_round_stamp(size, color, opacity, context.pen.antialias))
		}
	}
}

fn effective_size(context: &PaintPluginContext<'_>, pressure: f32) -> u32 {
	match context.tool {
		ToolKind::Pen | ToolKind::Eraser => {
			let base = context.resolved_size.max(1);
			if !context.pen.pressure_enabled || context.tool == ToolKind::Eraser {
				return base;
			}
			let clamped = pressure.clamp(0.0, 1.0);
			(base as f32 * (0.2 + clamped * 0.8)).round().max(1.0) as u32
		}
		ToolKind::Bucket | ToolKind::LassoBucket | ToolKind::PanelRect => 1,
	}
}

fn effective_spacing(context: &PaintPluginContext<'_>, size: u32) -> f32 {
	(size as f32 * (context.pen.spacing_percent / 100.0))
		.clamp(1.0, size.max(1) as f32)
}

fn fill_color(context: &PaintPluginContext<'_>) -> [u8; 4] {
	stamp_color(context)
}

fn stamp_color(context: &PaintPluginContext<'_>) -> [u8; 4] {
	match context.tool {
		ToolKind::Eraser if context.active_layer_is_background => [255, 255, 255, 255],
		ToolKind::Eraser => [0, 0, 0, 255],
		_ => context.color.to_rgba8(),
	}
}

fn edit_composite(context: &PaintPluginContext<'_>) -> BitmapComposite {
	match context.tool {
		ToolKind::Pen => BitmapComposite::source_over(),
		ToolKind::Eraser if context.active_layer_is_background => BitmapComposite::source_over(),
		ToolKind::Eraser => BitmapComposite::custom(EraseComposite),
		ToolKind::Bucket | ToolKind::LassoBucket => BitmapComposite::source_over(),
		ToolKind::PanelRect => BitmapComposite::source_over(),
	}
}

fn generated_round_stamp(size: usize, color: [u8; 4], opacity: f32, antialias: bool) -> CanvasBitmap {
	let size = size.max(1);
	let radius = size as f32 * 0.5;
	let center = radius - 0.5;
	let mut bitmap = CanvasBitmap::transparent(size, size);
	for y in 0..size {
		for x in 0..size {
			let dx = x as f32 - center;
			let dy = y as f32 - center;
			let distance = (dx * dx + dy * dy).sqrt();
			let coverage = if antialias {
				(radius + 0.5 - distance).clamp(0.0, 1.0)
			} else if distance <= radius {
				1.0
			} else {
				0.0
			};
			if coverage <= 0.0 {
				continue;
			}
			let alpha = ((color[3] as f32) * opacity * coverage)
				.round()
				.clamp(0.0, 255.0) as u8;
			let index = (y * bitmap.width + x) * 4;
			bitmap.pixels[index] = color[0];
			bitmap.pixels[index + 1] = color[1];
			bitmap.pixels[index + 2] = color[2];
			bitmap.pixels[index + 3] = alpha;
		}
	}
	bitmap
}

fn resample_alpha_tip(
	source_width: usize,
	source_height: usize,
	data: &[u8],
	target_size: usize,
	color: [u8; 4],
	opacity: f32,
) -> CanvasBitmap {
	let aspect = if source_width == 0 {
		1.0
	} else {
		source_height.max(1) as f32 / source_width.max(1) as f32
	};
	let target_width = target_size.max(1);
	let target_height = ((target_size as f32 * aspect).round() as usize).max(1);
	let mut bitmap = CanvasBitmap::transparent(target_width, target_height);
	for y in 0..target_height {
		for x in 0..target_width {
			let src_x = x * source_width.max(1) / target_width.max(1);
			let src_y = y * source_height.max(1) / target_height.max(1);
			let src_index = src_y.saturating_mul(source_width.max(1)).saturating_add(src_x);
			if src_index >= data.len() {
				continue;
			}
			let alpha = ((data[src_index] as f32) * opacity).round().clamp(0.0, 255.0) as u8;
			let index = (y * bitmap.width + x) * 4;
			bitmap.pixels[index] = color[0];
			bitmap.pixels[index + 1] = color[1];
			bitmap.pixels[index + 2] = color[2];
			bitmap.pixels[index + 3] = alpha;
		}
	}
	bitmap
}

fn resample_rgba_tip(
	source_width: usize,
	source_height: usize,
	data: &[u8],
	target_size: usize,
	tint: [u8; 4],
	opacity: f32,
) -> CanvasBitmap {
	let aspect = if source_width == 0 {
		1.0
	} else {
		source_height.max(1) as f32 / source_width.max(1) as f32
	};
	let target_width = target_size.max(1);
	let target_height = ((target_size as f32 * aspect).round() as usize).max(1);
	let mut bitmap = CanvasBitmap::transparent(target_width, target_height);
	for y in 0..target_height {
		for x in 0..target_width {
			let src_x = x * source_width.max(1) / target_width.max(1);
			let src_y = y * source_height.max(1) / target_height.max(1);
			let src_index = (src_y.saturating_mul(source_width.max(1)).saturating_add(src_x)) * 4;
			if src_index + 3 >= data.len() {
				continue;
			}
			let index = (y * bitmap.width + x) * 4;
			bitmap.pixels[index] = ((data[src_index] as u16 * tint[0] as u16) / 255) as u8;
			bitmap.pixels[index + 1] = ((data[src_index + 1] as u16 * tint[1] as u16) / 255) as u8;
			bitmap.pixels[index + 2] = ((data[src_index + 2] as u16 * tint[2] as u16) / 255) as u8;
			bitmap.pixels[index + 3] = ((data[src_index + 3] as f32) * opacity)
				.round()
				.clamp(0.0, 255.0) as u8;
		}
	}
	bitmap
}

fn blend_stamp(target: &mut CanvasBitmap, stamp: &CanvasBitmap, offset_x: isize, offset_y: isize) {
	for y in 0..stamp.height {
		for x in 0..stamp.width {
			let target_x = offset_x + x as isize;
			let target_y = offset_y + y as isize;
			if target_x < 0
				|| target_y < 0
				|| target_x as usize >= target.width
				|| target_y as usize >= target.height
			{
				continue;
			}
			let src_index = (y * stamp.width + x) * 4;
			let incoming = [
				stamp.pixels[src_index],
				stamp.pixels[src_index + 1],
				stamp.pixels[src_index + 2],
				stamp.pixels[src_index + 3],
			];
			if incoming[3] == 0 {
				continue;
			}
			let dst_index = (target_y as usize * target.width + target_x as usize) * 4;
			let previous = [
				target.pixels[dst_index],
				target.pixels[dst_index + 1],
				target.pixels[dst_index + 2],
				target.pixels[dst_index + 3],
			];
			target.pixels[dst_index..dst_index + 4].copy_from_slice(&source_over_pixel(previous, incoming));
		}
	}
}

fn source_over_pixel(previous: [u8; 4], incoming: [u8; 4]) -> [u8; 4] {
	let src_a = incoming[3] as f32 / 255.0;
	if src_a <= 0.0 {
		return previous;
	}
	let dst_a = previous[3] as f32 / 255.0;
	let out_a = src_a + dst_a * (1.0 - src_a);
	let mut out = [0_u8; 4];
	for channel in 0..3 {
		let src = incoming[channel] as f32 / 255.0;
		let dst = previous[channel] as f32 / 255.0;
		let value = src * src_a + dst * (1.0 - src_a);
		out[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
	}
	out[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
	out
}

fn point_in_polygon(x: f32, y: f32, points: &[PanelLocalPoint]) -> bool {
	let mut inside = false;
	let mut previous = *points.last().expect("polygon has points");
	for current in points {
		let (x1, y1) = (previous.x as f32, previous.y as f32);
		let (x2, y2) = (current.x as f32, current.y as f32);
		let intersects = ((y1 > y) != (y2 > y))
			&& (x < (x2 - x1) * (y - y1) / ((y2 - y1).abs().max(f32::EPSILON)) + x1);
		if intersects {
			inside = !inside;
		}
		previous = *current;
	}
	inside
}

