//! 円形のダブと、ストロークの 1 フレーム分のダブの列。
use std::sync::Arc;

use rayon::prelude::*;

use crate::composite::div255;
use crate::tile::{TILE, Tile, TileKey, TileStore, tile_of};

/// 縁をぼかした円形のダブ。色は premultiplied でない RGB、流量は 0〜1。
#[derive(Clone, Copy)]
pub struct Dab {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub hardness: f32,
    pub color: [u8; 3],
    pub flow: f32,
}

/// 始点から右へ travel だけ進む間のダブの列。間隔は、直径に spacing を掛けた値。
pub fn stroke_dabs(
    start: (f32, f32),
    travel: f32,
    diameter: f32,
    spacing: f32,
    color: [u8; 3],
) -> Vec<Dab> {
    let step = diameter * spacing;
    let count = (travel / step).round() as usize;
    let dab = |i: usize| Dab {
        cx: start.0 + step * i as f32,
        cy: start.1,
        radius: diameter / 2.0,
        hardness: 0.5,
        color,
        flow: 0.5,
    };
    (0..count).map(dab).collect()
}

/// ダブの列が掛かるタイルの位置 (重複なし)。
pub fn dab_tiles(dabs: &[Dab]) -> Vec<TileKey> {
    let span = |d: &Dab| {
        let (c0, r0) = tile_of(
            (d.cx - d.radius).floor() as i32,
            (d.cy - d.radius).floor() as i32,
        );
        let (c1, r1) = tile_of(
            (d.cx + d.radius).ceil() as i32 - 1,
            (d.cy + d.radius).ceil() as i32 - 1,
        );
        (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| (c, r)))
    };
    let mut keys: Vec<TileKey> = dabs.iter().flat_map(span).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// 1 個のダブを、1 枚のタイルに source-over で重ねる。円の中心からの距離で、縁をぼかす。
fn blend_dab(tile: &mut Tile, key: TileKey, dab: &Dab) {
    let (ox, oy) = (key.0 * TILE as i32, key.1 * TILE as i32);
    let (x0, x1) = (
        ((dab.cx - dab.radius).floor() as i32).max(ox),
        ((dab.cx + dab.radius).ceil() as i32).min(ox + TILE as i32),
    );
    let (y0, y1) = (
        ((dab.cy - dab.radius).floor() as i32).max(oy),
        ((dab.cy + dab.radius).ceil() as i32).min(oy + TILE as i32),
    );
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let src = [dab.color[0], dab.color[1], dab.color[2], 255].map(u32::from);
    let falloff = (1.0 - dab.hardness).max(1e-3) * dab.radius;
    for y in y0..y1 {
        let dy = y as f32 + 0.5 - dab.cy;
        let row = ((y - oy) as usize * TILE + (x0 - ox) as usize) * 4;
        let pixels = &mut tile.pixels_mut()[row..row + (x1 - x0) as usize * 4];
        for (i, px) in pixels.chunks_exact_mut(4).enumerate() {
            let dx = (x0 + i as i32) as f32 + 0.5 - dab.cx;
            let coverage = ((dab.radius - (dx * dx + dy * dy).sqrt()) / falloff).clamp(0.0, 1.0);
            let a = (coverage * dab.flow * 255.0 + 0.5) as u32;
            for c in 0..4 {
                px[c] = (div255(src[c] * a) + div255(u32::from(px[c]) * (255 - a))) as u8;
            }
        }
    }
}

/// ダブの列を、掛かるタイルに順に重ねる。共有されているタイルは、書く前に複製する (copy-on-write)。
/// parallel が真のときは、タイルごとに並列に行う。
pub fn apply_dabs(mut store: TileStore, dabs: &[Dab], parallel: bool) -> TileStore {
    let draw = |(key, mut tile): (TileKey, Arc<Tile>)| {
        let pixels = Arc::make_mut(&mut tile);
        dabs.iter().for_each(|dab| blend_dab(pixels, key, dab));
        (key, tile)
    };
    let taken: Vec<(TileKey, Arc<Tile>)> = dab_tiles(dabs)
        .into_iter()
        .map(|key| (key, store.take(key)))
        .collect();
    let drawn: Vec<(TileKey, Arc<Tile>)> = if parallel {
        taken.into_par_iter().map(draw).collect()
    } else {
        taken.into_iter().map(draw).collect()
    };
    drawn
        .into_iter()
        .fold(store, |store, (key, tile)| store.with_tile(key, tile))
}
