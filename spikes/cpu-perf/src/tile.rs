//! 256px のタイルと、疎なタイルストア。画素は RGBA8 premultiplied。
use std::collections::HashMap;
use std::sync::Arc;

pub const TILE: usize = 256;
pub const TILE_BYTES: usize = TILE * TILE * 4;
/// タイルの位置 (列、行)。画素の座標を 256 で割った値。
pub type TileKey = (i32, i32);

#[derive(Clone)]
pub struct Tile(Box<[u8]>);

impl Tile {
    pub fn transparent() -> Self {
        Self(vec![0; TILE_BYTES].into_boxed_slice())
    }
    pub fn pixels(&self) -> &[u8] {
        &self.0
    }
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// 画素の座標から、タイルの位置を求める。
pub fn tile_of(x: i32, y: i32) -> TileKey {
    (x.div_euclid(TILE as i32), y.div_euclid(TILE as i32))
}

/// 空のタイルを持たないタイルストア。複製してもタイルは共有され、書いたタイルだけが別の実体になる。
#[derive(Clone, Default)]
pub struct TileStore(HashMap<TileKey, Arc<Tile>>);

impl TileStore {
    pub fn get(&self, key: TileKey) -> Option<&Arc<Tile>> {
        self.0.get(&key)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn with_tile(mut self, key: TileKey, tile: Arc<Tile>) -> Self {
        self.0.insert(key, tile);
        self
    }
    /// タイルをストアから取り出す。無ければ透明のタイルを作る。
    pub fn take(&mut self, key: TileKey) -> Arc<Tile> {
        self.0
            .remove(&key)
            .unwrap_or_else(|| Arc::new(Tile::transparent()))
    }
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let at =
            (y.rem_euclid(TILE as i32) as usize * TILE + x.rem_euclid(TILE as i32) as usize) * 4;
        self.get(tile_of(x, y))
            .map_or([0; 4], |tile| [0, 1, 2, 3].map(|c| tile.pixels()[at + c]))
    }
}
