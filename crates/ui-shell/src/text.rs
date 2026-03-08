use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont, point};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use fontdb::{Database, Family, ID, Query};
use std::sync::OnceLock;

const BITMAP_FONT_WIDTH: usize = 8;
const BITMAP_FONT_HEIGHT: usize = 8;
const BITMAP_LINE_HEIGHT: usize = 10;
const SYSTEM_FONT_SIZE: f32 = 13.0;

static TEXT_RENDERER: OnceLock<TextRenderer> = OnceLock::new();

#[allow(clippy::too_many_arguments)]
pub fn draw_text_rgba(
    pixels: &mut [u8],
    surface_width: usize,
    surface_height: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
) {
    shared_text_renderer().draw_text_rgba(pixels, surface_width, surface_height, x, y, text, color);
}

pub fn line_height() -> usize {
    shared_text_renderer().line_height()
}

pub fn measure_text_width(text: &str) -> usize {
    shared_text_renderer().measure_text_width(text)
}

pub fn text_backend_name() -> &'static str {
    shared_text_renderer().backend_name()
}

pub fn wrap_text_lines(text: &str, available_width: usize) -> Vec<String> {
    shared_text_renderer().wrap_text_lines(text, available_width)
}

fn shared_text_renderer() -> &'static TextRenderer {
    TEXT_RENDERER.get_or_init(TextRenderer::new)
}

#[derive(Debug)]
struct TextRenderer {
    backend: TextBackend,
}

#[derive(Debug)]
enum TextBackend {
    System(SystemFontRenderer),
    Bitmap,
}

#[derive(Debug)]
struct SystemFontRenderer {
    fonts: Vec<LoadedFont>,
    scale: PxScale,
    ascent: f32,
    line_height: usize,
}

#[derive(Debug)]
struct LoadedFont {
    font: FontVec,
}

impl TextRenderer {
    fn new() -> Self {
        let backend = SystemFontRenderer::load()
            .map(TextBackend::System)
            .unwrap_or(TextBackend::Bitmap);
        Self { backend }
    }

    fn backend_name(&self) -> &'static str {
        match self.backend {
            TextBackend::System(_) => "system",
            TextBackend::Bitmap => "bitmap",
        }
    }

    fn line_height(&self) -> usize {
        match &self.backend {
            TextBackend::System(renderer) => renderer.line_height,
            TextBackend::Bitmap => BITMAP_LINE_HEIGHT,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_rgba(
        &self,
        pixels: &mut [u8],
        surface_width: usize,
        surface_height: usize,
        x: usize,
        y: usize,
        text: &str,
        color: [u8; 4],
    ) {
        match &self.backend {
            TextBackend::System(renderer) => {
                renderer.draw_text_rgba(pixels, surface_width, surface_height, x, y, text, color)
            }
            TextBackend::Bitmap => {
                draw_bitmap_text(pixels, surface_width, surface_height, x, y, text, color)
            }
        }
    }

    fn measure_text_width(&self, text: &str) -> usize {
        match &self.backend {
            TextBackend::System(renderer) => renderer.measure_text_width(text),
            TextBackend::Bitmap => text.chars().count() * BITMAP_FONT_WIDTH,
        }
    }

    fn wrap_text_lines(&self, text: &str, available_width: usize) -> Vec<String> {
        let max_width = available_width.max(1);
        let mut lines = Vec::new();

        for raw_line in text.split('\n') {
            if raw_line.trim().is_empty() {
                lines.push(String::new());
                continue;
            }

            let mut current = String::new();
            for word in raw_line.split_whitespace() {
                let candidate = if current.is_empty() {
                    word.to_string()
                } else {
                    format!("{current} {word}")
                };

                if self.measure_text_width(&candidate) <= max_width {
                    current = candidate;
                    continue;
                }

                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }

                if self.measure_text_width(word) <= max_width {
                    current.push_str(word);
                    continue;
                }

                let mut chunks = self.wrap_long_word(word, max_width).into_iter().peekable();
                while let Some(chunk) = chunks.next() {
                    if chunks.peek().is_some() {
                        lines.push(chunk);
                    } else {
                        current = chunk;
                    }
                }
            }

            if current.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(current);
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }

    fn wrap_long_word(&self, word: &str, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut chunk = String::new();

        for ch in word.chars() {
            let mut candidate = chunk.clone();
            candidate.push(ch);
            if !chunk.is_empty() && self.measure_text_width(&candidate) > max_width {
                lines.push(std::mem::take(&mut chunk));
            }
            chunk.push(ch);
        }

        if !chunk.is_empty() {
            lines.push(chunk);
        }

        lines
    }
}

impl SystemFontRenderer {
    fn load() -> Option<Self> {
        let mut database = Database::new();
        database.load_system_fonts();
        if database.is_empty() {
            return None;
        }

        let fonts: Vec<LoadedFont> = candidate_font_ids(&database)
            .into_iter()
            .filter_map(|id| load_font(&database, id))
            .collect();
        if fonts.is_empty() {
            return None;
        }

        let scale = PxScale::from(SYSTEM_FONT_SIZE);
        let (ascent, line_height) = {
            let scaled = fonts[0].font.as_scaled(scale);
            (
                scaled.ascent(),
                ((scaled.height() + scaled.line_gap()).ceil() as usize).max(14),
            )
        };
        Some(Self {
            fonts,
            scale,
            ascent,
            line_height,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_rgba(
        &self,
        pixels: &mut [u8],
        surface_width: usize,
        surface_height: usize,
        x: usize,
        y: usize,
        text: &str,
        color: [u8; 4],
    ) {
        let mut cursor_x = x as f32;
        let baseline = y as f32 + self.ascent.ceil();
        let mut previous: Option<(usize, GlyphId)> = None;

        for ch in text.chars() {
            let font_index = self.font_index_for_char(ch);
            let font = &self.fonts[font_index].font;
            let scaled = font.as_scaled(self.scale);
            let glyph_id = font.glyph_id(ch);

            if let Some((previous_index, previous_glyph)) = previous
                && previous_index == font_index
            {
                cursor_x += scaled.kern(previous_glyph, glyph_id);
            }

            let glyph = glyph_id.with_scale_and_position(self.scale, point(cursor_x, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let offset_x = bounds.min.x.floor().max(0.0) as usize;
                let offset_y = bounds.min.y.floor().max(0.0) as usize;
                outlined.draw(|px, py, coverage| {
                    blend_pixel(
                        pixels,
                        surface_width,
                        surface_height,
                        offset_x + px as usize,
                        offset_y + py as usize,
                        color,
                        coverage,
                    );
                });
            }

            cursor_x += scaled.h_advance(glyph_id);
            previous = Some((font_index, glyph_id));
        }
    }

    fn font_index_for_char(&self, ch: char) -> usize {
        self.fonts
            .iter()
            .position(|loaded| ch.is_whitespace() || loaded.font.glyph_id(ch).0 != 0)
            .unwrap_or(0)
    }

    fn measure_text_width(&self, text: &str) -> usize {
        let mut width = 0.0;
        let mut previous: Option<(usize, GlyphId)> = None;

        for ch in text.chars() {
            let font_index = self.font_index_for_char(ch);
            let font = &self.fonts[font_index].font;
            let scaled = font.as_scaled(self.scale);
            let glyph_id = font.glyph_id(ch);

            if let Some((previous_index, previous_glyph)) = previous
                && previous_index == font_index
            {
                width += scaled.kern(previous_glyph, glyph_id);
            }

            width += scaled.h_advance(glyph_id);
            previous = Some((font_index, glyph_id));
        }

        width.ceil() as usize
    }
}

fn candidate_font_ids(database: &Database) -> Vec<ID> {
    let mut ids = Vec::new();

    push_query(
        database,
        &mut ids,
        &[Family::Name("Segoe UI"), Family::SansSerif],
    );
    push_query(
        database,
        &mut ids,
        &[Family::Name("Yu Gothic UI"), Family::SansSerif],
    );
    push_query(
        database,
        &mut ids,
        &[Family::Name("Meiryo UI"), Family::SansSerif],
    );
    push_query(
        database,
        &mut ids,
        &[Family::Name("Noto Sans"), Family::SansSerif],
    );
    push_query(
        database,
        &mut ids,
        &[Family::Name("DejaVu Sans"), Family::SansSerif],
    );
    push_query(
        database,
        &mut ids,
        &[Family::Name("Arial"), Family::SansSerif],
    );
    push_query(database, &mut ids, &[Family::SansSerif]);

    if ids.is_empty()
        && let Some(face) = database.faces().next()
    {
        ids.push(face.id);
    }

    ids
}

fn push_query(database: &Database, ids: &mut Vec<ID>, families: &[Family<'_>]) {
    let query = Query {
        families,
        ..Query::default()
    };
    if let Some(id) = database.query(&query)
        && !ids.contains(&id)
    {
        ids.push(id);
    }
}

fn load_font(database: &Database, id: ID) -> Option<LoadedFont> {
    database
        .with_face_data(id, |data, index| {
            FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
        })
        .flatten()
        .map(|font| LoadedFont { font })
}

fn draw_bitmap_text(
    pixels: &mut [u8],
    surface_width: usize,
    surface_height: usize,
    x: usize,
    y: usize,
    text: &str,
    color: [u8; 4],
) {
    for (index, ch) in text.chars().enumerate() {
        draw_bitmap_glyph(
            pixels,
            surface_width,
            surface_height,
            x + index * BITMAP_FONT_WIDTH,
            y,
            ch,
            color,
        );
    }
}

fn draw_bitmap_glyph(
    pixels: &mut [u8],
    surface_width: usize,
    surface_height: usize,
    x: usize,
    y: usize,
    ch: char,
    color: [u8; 4],
) {
    let glyph = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?'));
    let Some(glyph) = glyph else {
        return;
    };

    for (row, bits) in glyph.iter().enumerate().take(BITMAP_FONT_HEIGHT) {
        for col in 0..BITMAP_FONT_WIDTH {
            if ((bits >> col) & 1) == 1 {
                write_pixel(
                    pixels,
                    surface_width,
                    surface_height,
                    x + col,
                    y + row,
                    color,
                );
            }
        }
    }
}

fn blend_pixel(
    pixels: &mut [u8],
    surface_width: usize,
    surface_height: usize,
    x: usize,
    y: usize,
    color: [u8; 4],
    coverage: f32,
) {
    if x >= surface_width || y >= surface_height {
        return;
    }

    let alpha = ((color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }

    let index = (y * surface_width + x) * 4;
    let inverse = 1.0 - alpha;
    for channel in 0..3 {
        let blended = color[channel] as f32 * alpha + pixels[index + channel] as f32 * inverse;
        pixels[index + channel] = blended.round().clamp(0.0, 255.0) as u8;
    }
    let blended_alpha = 255.0 * alpha + pixels[index + 3] as f32 * inverse;
    pixels[index + 3] = blended_alpha.round().clamp(0.0, 255.0) as u8;
}

fn write_pixel(
    pixels: &mut [u8],
    surface_width: usize,
    surface_height: usize,
    x: usize,
    y: usize,
    color: [u8; 4],
) {
    if x >= surface_width || y >= surface_height {
        return;
    }

    let index = (y * surface_width + x) * 4;
    pixels[index..index + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_text_respects_requested_origin() {
        let width = 160;
        let height = 64;
        let x = 40;
        let y = 20;
        let mut pixels = vec![0; width * height * 4];

        draw_text_rgba(
            &mut pixels,
            width,
            height,
            x,
            y,
            "H",
            [0xff, 0xff, 0xff, 0xff],
        );

        let mut min_x = usize::MAX;
        let mut min_y = usize::MAX;
        for yy in 0..height {
            for xx in 0..width {
                let index = (yy * width + xx) * 4;
                if pixels[index..index + 4] != [0, 0, 0, 0] {
                    min_x = min_x.min(xx);
                    min_y = min_y.min(yy);
                }
            }
        }

        assert_ne!(min_x, usize::MAX);
        assert_ne!(min_y, usize::MAX);
        assert!(min_x >= x.saturating_sub(4));
        assert!(min_y >= y.saturating_sub(4));
    }
}
