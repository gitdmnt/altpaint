//! source-over の合成と、dirty 領域に掛かるタイルの再合成。
use rayon::prelude::*;

use crate::tile::{Tile, TileKey, TileStore, tile_of};

/// 合成するレイヤー。不透明度は 0〜255。
#[derive(Clone, Copy)]
pub struct Layer<'a> {
    pub store: &'a TileStore,
    pub opacity: u32,
}

/// 255 で割って四捨五入する (0〜65025 の範囲で正しい)。
#[inline]
pub fn div255(v: u32) -> u32 {
    (v + 128 + ((v + 128) >> 8)) >> 8
}

/// dst の上に src を重ねる (premultiplied の source-over)。src には不透明度を掛ける。
pub fn over(dst: &mut [u8], src: &[u8], opacity: u32) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let inv = 255 - div255(u32::from(s[3]) * opacity);
        for c in 0..4 {
            d[c] = (div255(u32::from(s[c]) * opacity) + div255(u32::from(d[c]) * inv)) as u8;
        }
    }
}

/// レイヤーを下から順に重ねた 1 枚のタイルを作る。その位置にタイルを持たないレイヤーは飛ばす。
pub fn composite_tile(layers: &[Layer], key: TileKey) -> Tile {
    layers.iter().fold(Tile::transparent(), |mut out, layer| {
        if let Some(tile) = layer.store.get(key) {
            over(out.pixels_mut(), tile.pixels(), layer.opacity);
        }
        out
    })
}

/// 指定したタイルを、タイルごとに並列に合成する。
pub fn recomposite(layers: &[Layer], keys: &[TileKey]) -> Vec<(TileKey, Tile)> {
    keys.par_iter()
        .map(|&key| (key, composite_tile(layers, key)))
        .collect()
}

/// 矩形に掛かるタイルの位置を、行の順に返す。
pub fn dirty_tiles(x: i32, y: i32, width: i32, height: i32) -> Vec<TileKey> {
    let (c0, r0) = tile_of(x, y);
    let (c1, r1) = tile_of(x + width - 1, y + height - 1);
    (r0..=r1)
        .flat_map(|r| (c0..=c1).map(move |c| (c, r)))
        .collect()
}
