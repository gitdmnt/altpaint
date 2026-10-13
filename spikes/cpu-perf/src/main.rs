//! 計測のハーネス。10 枚の密なレイヤーにストロークを描き、フレームごとの時間を Markdown の表で出す。
use std::hint::black_box;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use spike_cpu_perf::composite::{Layer, dirty_tiles, over, recomposite};
use spike_cpu_perf::dab::{apply_dabs, stroke_dabs};
use spike_cpu_perf::stats::{summarize, verdict};
use spike_cpu_perf::tile::{TILE, Tile, TileKey, TileStore};

const OPACITY: u32 = 230;
/// 1 つの条件で測るフレームの数 (慣らしの 2 本を除く)。
const FRAMES: usize = 300;

struct Case {
    width: i32,
    height: i32,
    active: usize,
    diameter: f32,
    paced: bool,
}

/// 画素ごとに違う、正しい premultiplied (RGB ≤ A) の値で埋めたタイル。
fn noisy_tile(seed: u64) -> Tile {
    let mut tile = Tile::transparent();
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for px in tile.pixels_mut().chunks_exact_mut(4) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let v = (state >> 16) as u32;
        let a = 32 + (v >> 24) % 192;
        let rgb = [v, v >> 8, v >> 16].map(|c| (c % (a + 1)) as u8);
        px.copy_from_slice(&[rgb[0], rgb[1], rgb[2], a as u8]);
    }
    tile
}

fn store_of(tiles: Vec<(TileKey, Tile)>) -> TileStore {
    let insert = |store: TileStore, (key, tile)| store.with_tile(key, Arc::new(tile));
    tiles.into_iter().fold(TileStore::default(), insert)
}

fn dense_layer(index: u64, keys: &[TileKey]) -> TileStore {
    let seed = |key: TileKey| (index << 40) ^ ((key.0 as u64) << 20) ^ key.1 as u64;
    store_of(
        keys.par_iter()
            .map(|&key| (key, noisy_tile(seed(key))))
            .collect(),
    )
}

fn layer(store: &TileStore) -> Layer<'_> {
    Layer {
        store,
        opacity: OPACITY,
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

/// ストロークを左から右へ引き、フレームごとの (ダブ描画, 再合成) の時間を ms で返す。
/// 1 フレームは 200px の移動。dirty 領域は 512×512 で、タイルの境界に揃えない (必ず 9 枚に掛かる)。
fn measure(layers: &[TileStore], cache: &TileStore, case: &Case) -> Vec<(f64, f64)> {
    let per_stroke = ((case.width - 549) / 200 + 1) as usize;
    let rows = (case.height - 553) / 170 + 1;
    let below = (case.active > 0).then_some(Layer {
        store: cache,
        opacity: 255,
    });
    let frame = |active: &mut TileStore, (x, y): (i32, i32)| {
        if case.paced {
            sleep(Duration::from_millis(15));
        }
        let start = Instant::now();
        let from = (x as f32 + 156.5, y as f32 + 256.5);
        let dabs = stroke_dabs(from, 200.0, case.diameter, 0.1, [200, 40, 10]);
        *active = apply_dabs(std::mem::take(active), &dabs, case.diameter > 64.0);
        let drawn = Instant::now();
        let above = layers[case.active + 1..].iter().map(layer);
        let stack: Vec<Layer> = below
            .into_iter()
            .chain([layer(active)])
            .chain(above)
            .collect();
        let out = recomposite(&stack, &dirty_tiles(x, y, 512, 512));
        let composited = Instant::now();
        assert_eq!(black_box(out).len(), 9);
        Some((ms(drawn - start), ms(composited - drawn)))
    };
    // ストロークごとに、描く前のレイヤーを layers の側に残す。最初に触れたタイルは複製される。
    let stroke = |s: usize| -> Vec<(f64, f64)> {
        let y = 41 + 170 * (s as i32 % rows);
        let xs = (0..per_stroke as i32).map(|i| (37 + 200 * i, y));
        xs.scan(layers[case.active].clone(), frame).collect()
    };
    let strokes: Vec<_> = (0..FRAMES.div_ceil(per_stroke) + 2).map(stroke).collect();
    strokes.into_iter().skip(2).flatten().collect()
}

/// 1 スレッドで、1 枚のタイルを 1 回重ねる時間 (ns/px)。
fn over_ns_per_pixel() -> f64 {
    let (src, mut dst) = (noisy_tile(1), noisy_tile(2));
    let start = Instant::now();
    (0..200).for_each(|_| {
        over(
            black_box(dst.pixels_mut()),
            black_box(src.pixels()),
            OPACITY,
        )
    });
    start.elapsed().as_secs_f64() * 1e9 / (200 * TILE * TILE) as f64
}

fn main() {
    let ns = over_ns_per_pixel();
    let per_tile = ns * (TILE * TILE) as f64 / 1e6;
    let threads = rayon::current_num_threads();
    println!(
        "スレッド {threads} / 1 スレッドの over: {ns:.2} ns/px、タイル 1 枚 {per_tile:.3} ms\n"
    );
    println!(
        "| 寸法 | 計測 | 描く層 | 直径 | ダブ p50 | ダブ p95 | ダブ最大 | 再合成 p50 | 再合成 p95 | 再合成最大 | p95 の和 | 判定 |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (width, height) in [(2894, 4093), (4441, 6213)] {
        let keys = dirty_tiles(0, 0, width, height);
        let layers: Vec<TileStore> = (0..10).map(|i| dense_layer(i, &keys)).collect();
        for (name, active) in [("中", 5), ("上", 9), ("下", 0)] {
            let lower: Vec<Layer> = layers[..active].iter().map(layer).collect();
            let cache = store_of(recomposite(&lower, &keys));
            for (paced, diameter) in [(true, 5.0), (true, 300.0), (false, 5.0), (false, 300.0)] {
                let case = Case {
                    width,
                    height,
                    active,
                    diameter,
                    paced,
                };
                let (dabs, composites): (Vec<f64>, Vec<f64>) =
                    measure(&layers, &cache, &case).into_iter().unzip();
                let (d, c) = (summarize(&dabs), summarize(&composites));
                let mode = if paced { "休みあり" } else { "連続" };
                println!(
                    "| {width}×{height} | {mode} | {name} | {diameter} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} |",
                    d.p50,
                    d.p95,
                    d.max,
                    c.p50,
                    c.p95,
                    c.max,
                    d.p95 + c.p95,
                    verdict(d.p95, c.p95)
                );
            }
        }
    }
}
