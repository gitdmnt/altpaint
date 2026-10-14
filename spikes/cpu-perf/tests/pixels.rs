//! タイルストア、合成、ダブの単体テスト。小さいキャンバスで行う。
use std::sync::Arc;

use spike_cpu_perf::composite::{Layer, composite_tile, dirty_tiles, over, recomposite};
use spike_cpu_perf::dab::{Dab, apply_dabs, dab_tiles, stroke_dabs};
use spike_cpu_perf::tile::{TILE, Tile, TileStore};

/// 画素ごとに違う、正しい premultiplied (RGB ≤ A) の値で埋めたタイル。
fn noisy_tile(seed: u32) -> Tile {
    let mut tile = Tile::transparent();
    for (i, px) in tile.pixels_mut().chunks_exact_mut(4).enumerate() {
        let v = (i as u32)
            .wrapping_mul(2_654_435_761)
            .wrapping_add(seed.wrapping_mul(40_503));
        let a = 32 + (v >> 24) % 192;
        px.copy_from_slice(&[
            (v % (a + 1)) as u8,
            ((v >> 8) % (a + 1)) as u8,
            ((v >> 16) % (a + 1)) as u8,
            a as u8,
        ]);
    }
    tile
}

fn noisy_store(seed: u32, keys: &[(i32, i32)]) -> TileStore {
    keys.iter().fold(TileStore::default(), |store, &key| {
        store.with_tile(
            key,
            Arc::new(noisy_tile(seed.wrapping_add((key.0 * 31 + key.1) as u32))),
        )
    })
}

fn dab(cx: f32, cy: f32, diameter: f32) -> Dab {
    Dab {
        cx,
        cy,
        radius: diameter / 2.0,
        hardness: 0.5,
        color: [200, 40, 10],
        flow: 0.5,
    }
}

#[test]
fn store_clone_shares_tiles_and_write_copies_only_the_touched_tile() {
    let keys = [(0, 0), (1, 0)];
    let original = noisy_store(1, &keys);
    let snapshot = original.clone();
    assert!(
        keys.iter()
            .all(|k| Arc::ptr_eq(original.get(*k).unwrap(), snapshot.get(*k).unwrap()))
    );

    let drawn = apply_dabs(original, &[dab(100.0, 100.0, 20.0)], false);
    assert!(!Arc::ptr_eq(
        drawn.get((0, 0)).unwrap(),
        snapshot.get((0, 0)).unwrap()
    ));
    assert!(Arc::ptr_eq(
        drawn.get((1, 0)).unwrap(),
        snapshot.get((1, 0)).unwrap()
    ));
    assert_eq!(
        snapshot.pixel(100, 100),
        noisy_tile(1).pixels()[(100 * TILE + 100) * 4..][..4]
    );
}

#[test]
fn store_is_sparse_and_reads_transparent_where_nothing_was_written() {
    let store = apply_dabs(TileStore::default(), &[dab(300.0, 40.0, 10.0)], false);
    assert_eq!(store.len(), 1);
    assert!(store.get((0, 0)).is_none());
    assert_eq!(store.pixel(10, 10), [0, 0, 0, 0]);
    assert_eq!(store.pixel(-5, 900), [0, 0, 0, 0]);
}

#[test]
fn over_matches_the_premultiplied_formula_within_rounding() {
    let (src, dst, opacity) = ([100u8, 50, 25, 128], [60u8, 80, 100, 200], 204u32);
    let mut out = dst;
    over(&mut out, &src, opacity);
    let o = f64::from(opacity) / 255.0;
    let inv = 1.0 - f64::from(src[3]) / 255.0 * o;
    for c in 0..4 {
        let want = f64::from(src[c]) * o + f64::from(dst[c]) * inv;
        assert!(
            (f64::from(out[c]) - want).abs() < 1.5,
            "channel {c}: {} vs {want}",
            out[c]
        );
    }
}

#[test]
fn cached_recomposite_equals_compositing_all_layers() {
    let key = (0, 0);
    let stores: Vec<TileStore> = (0..10).map(|i| noisy_store(i, &[key])).collect();
    let layers: Vec<Layer> = stores
        .iter()
        .map(|store| Layer {
            store,
            opacity: 230,
        })
        .collect();
    let all = composite_tile(&layers, key);

    let cache = TileStore::default().with_tile(key, Arc::new(composite_tile(&layers[..5], key)));
    let cache_layer = Layer {
        store: &cache,
        opacity: 255,
    };
    let stack: Vec<Layer> = std::iter::once(cache_layer)
        .chain(layers[5..].iter().copied())
        .collect();
    let cached = recomposite(&stack, &[key]);
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].0, key);
    assert_eq!(cached[0].1.pixels(), all.pixels());
}

#[test]
fn dirty_rect_covers_nine_tiles_unaligned_and_four_aligned() {
    assert_eq!(dirty_tiles(37, 41, 512, 512).len(), 9);
    assert_eq!(
        dirty_tiles(256, 512, 512, 512),
        vec![(1, 2), (2, 2), (1, 3), (2, 3)]
    );
}

#[test]
fn dab_is_darkest_at_the_centre_symmetric_and_leaves_the_outside_untouched() {
    let store = apply_dabs(TileStore::default(), &[dab(100.5, 100.5, 40.0)], false);
    let alpha = |x, y| store.pixel(x, y)[3];
    assert!(alpha(100, 100) >= alpha(110, 100) && alpha(110, 100) > alpha(118, 100));
    assert_eq!(alpha(100 - 15, 100), alpha(100 + 15, 100));
    assert_eq!(alpha(100, 100 - 15), alpha(100, 100 + 15));
    assert_eq!(alpha(100, 100 - 15), alpha(100 - 15, 100));
    assert_eq!(store.pixel(100 + 21, 100), [0, 0, 0, 0]);
    assert_eq!(store.pixel(100, 100 - 21), [0, 0, 0, 0]);
}

#[test]
fn dab_across_a_tile_corner_matches_the_same_dab_inside_one_tile() {
    let across = apply_dabs(TileStore::default(), &[dab(256.5, 256.5, 60.0)], false);
    let inside = apply_dabs(TileStore::default(), &[dab(100.5, 100.5, 60.0)], false);
    assert_eq!(dab_tiles(&[dab(256.5, 256.5, 60.0)]).len(), 4);
    assert_eq!(across.len(), 4);
    for dy in -31..=31 {
        for dx in -31..=31 {
            assert_eq!(
                across.pixel(256 + dx, 256 + dy),
                inside.pixel(100 + dx, 100 + dy),
                "({dx}, {dy})"
            );
        }
    }
}

#[test]
fn parallel_and_sequential_dab_application_agree() {
    let dabs = stroke_dabs((200.0, 300.0), 200.0, 300.0, 0.1, [200, 40, 10]);
    let keys = dab_tiles(&dabs);
    let sequential = apply_dabs(noisy_store(7, &keys), &dabs, false);
    let parallel = apply_dabs(noisy_store(7, &keys), &dabs, true);
    assert!(keys.len() >= 4);
    for key in keys {
        assert_eq!(
            sequential.get(key).unwrap().pixels(),
            parallel.get(key).unwrap().pixels(),
            "{key:?}"
        );
    }
}

#[test]
fn one_frame_of_travel_yields_the_expected_number_of_dabs() {
    assert_eq!(
        stroke_dabs((0.0, 0.0), 200.0, 5.0, 0.1, [0, 0, 0]).len(),
        400
    );
    assert_eq!(
        stroke_dabs((0.0, 0.0), 200.0, 300.0, 0.1, [0, 0, 0]).len(),
        7
    );
}

/// 255 で割る四捨五入を、実装とは別の式で書いたもの。
fn round_div255(v: u32) -> u32 {
    (2 * v + 255) / 510
}

#[test]
fn composite_tile_matches_a_per_pixel_reference_over_the_whole_tile() {
    let key = (0, 0);
    let tiles: Vec<Tile> = (40..43).map(noisy_tile).collect();
    let store = |tile: &Tile| TileStore::default().with_tile(key, Arc::new(tile.clone()));
    let stores: Vec<TileStore> = tiles.iter().map(store).collect();
    let layer = |store| Layer {
        store,
        opacity: 230,
    };
    let out = composite_tile(&stores.iter().map(layer).collect::<Vec<_>>(), key);
    let reference = |i: usize| {
        tiles.iter().fold([0u32; 4], |dst, tile| {
            let src = &tile.pixels()[i * 4..][..4];
            let inv = 255 - round_div255(u32::from(src[3]) * 230);
            [0, 1, 2, 3].map(|c| round_div255(u32::from(src[c]) * 230) + round_div255(dst[c] * inv))
        })
    };
    for i in 0..TILE * TILE {
        assert_eq!(
            out.pixels()[i * 4..][..4],
            reference(i).map(|v| v as u8),
            "pixel {i}"
        );
    }
}

#[test]
fn dab_blends_its_colour_over_existing_pixels_as_the_formula_says() {
    let d = dab(130.3, 120.7, 90.0);
    let before = noisy_tile(9);
    let dense = TileStore::default().with_tile((0, 0), Arc::new(before.clone()));
    let store = apply_dabs(dense, &[d], false);
    let src = [200.0, 40.0, 10.0, 255.0];
    for (i, old) in before.pixels().chunks_exact(4).enumerate() {
        let (x, y) = ((i % TILE) as i32, (i / TILE) as i32);
        let (dx, dy) = (
            f64::from(x) + 0.5 - f64::from(d.cx),
            f64::from(y) + 0.5 - f64::from(d.cy),
        );
        let radius = f64::from(d.radius);
        let a = ((radius - (dx * dx + dy * dy).sqrt()) / (0.5 * radius)).clamp(0.0, 1.0) * 0.5;
        let new = store.pixel(x, y);
        for c in 0..4 {
            let want = src[c] * a + f64::from(old[c]) * (1.0 - a);
            assert!(
                (f64::from(new[c]) - want).abs() <= 1.5,
                "({x}, {y}) channel {c}: {} vs {want}",
                new[c]
            );
        }
    }
}

#[test]
fn dab_across_the_origin_matches_the_same_dab_at_positive_coordinates() {
    let across = apply_dabs(TileStore::default(), &[dab(0.5, 0.5, 60.0)], false);
    let inside = apply_dabs(TileStore::default(), &[dab(600.5, 600.5, 60.0)], false);
    assert_eq!(
        dab_tiles(&[dab(0.5, 0.5, 60.0)]),
        vec![(-1, -1), (-1, 0), (0, -1), (0, 0)]
    );
    for dy in -31..=31 {
        for dx in -31..=31 {
            assert_eq!(
                across.pixel(dx, dy),
                inside.pixel(600 + dx, 600 + dy),
                "({dx}, {dy})"
            );
        }
    }
}

#[test]
fn dab_tiles_include_the_next_tile_only_when_the_dab_reaches_into_it() {
    assert_eq!(dab_tiles(&[dab(246.0, 100.0, 20.0)]), vec![(0, 0)]);
    assert_eq!(dab_tiles(&[dab(246.5, 100.0, 20.0)]), vec![(0, 0), (1, 0)]);
    assert_eq!(dab_tiles(&[dab(100.0, 246.5, 20.0)]), vec![(0, 0), (0, 1)]);
}
