//! `ZEI_*.RAW`: the map tiles and their collision masks.
//!
//! The synthetic cases build tiles byte by byte. The rest read the real files
//! out of `burntime.pak` and check them against two independent oracles the
//! remaster ships: rendered PNGs of the same tiles, and hand-written mask
//! files. Both skip when the data is absent.

use burngfx::pak::ClassicPak;
use burngfx::zei::{TileSet, WalkGrid, WalkMask, ZeiError, MASK_CELL, TILES_PER_FILE, TILE_SIZE};
use burngfx::Palette;
use std::path::PathBuf;

const RECORD_LEN: usize = 16 + TILE_SIZE * TILE_SIZE;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

/// A file of 63 tiles; the first is `first`, the rest are zeroed.
fn build(first_mask: u16, first_pixels: &[u8; 1024]) -> Vec<u8> {
    let mut out = vec![0u8; TILES_PER_FILE * RECORD_LEN];
    out[2..4].copy_from_slice(&first_mask.to_le_bytes());
    out[16..16 + 1024].copy_from_slice(first_pixels);
    out
}

#[test]
fn a_file_holds_exactly_sixty_three_tiles() {
    let set = TileSet::parse(&build(0, &[0; 1024])).unwrap();
    assert_eq!(set.len(), TILES_PER_FILE);
    assert!(!set.is_empty());
    assert!(set.tile(62).is_some());
    assert!(set.tile(63).is_none());

    assert_eq!(TileSet::parse(&[]).unwrap_err(), ZeiError::WrongSize(0));
    let short = vec![0u8; TILES_PER_FILE * RECORD_LEN - 1];
    assert_eq!(TileSet::parse(&short).unwrap_err(), ZeiError::WrongSize(short.len()));
}

/// A set bit blocks, and cell 0 is the most significant bit — so the mask
/// reads left to right, top to bottom, the way it is written down.
#[test]
fn the_mask_reads_most_significant_bit_first() {
    let open = WalkMask(0);
    assert!(open.is_open());
    for y in 0..4 {
        for x in 0..4 {
            assert!(open.walkable(x, y), "({x},{y}) should be open");
        }
    }

    let solid = WalkMask(0xFFFF);
    assert!(solid.is_solid());
    assert!(!solid.walkable(0, 0));

    // Only the top-left cell blocked.
    let corner = WalkMask(0x8000);
    assert!(!corner.walkable(0, 0));
    assert!(corner.walkable(1, 0));
    assert!(corner.walkable(0, 1));

    // Only the bottom-right cell blocked.
    let far = WalkMask(0x0001);
    assert!(!far.walkable(3, 3));
    assert!(far.walkable(2, 3));

    // Off the grid is not walkable, rather than a panic.
    assert!(!open.walkable(4, 0));
    assert!(!open.walkable(0, 4));
}

/// The pixels are four interleaved passes, not a straight run.
#[test]
fn pixels_are_de_interleaved_into_the_tile() {
    let mut pixels = [0u8; 1024];
    // Pass p holds every fourth pixel starting at p, so the first byte of each
    // pass lands at 0, 1, 2 and 3.
    for p in 0..4 {
        pixels[p * 256] = 10 + p as u8;
        pixels[p * 256 + 1] = 20 + p as u8;
    }

    let set = TileSet::parse(&build(0, &pixels)).unwrap();
    let t = set.tile(0).unwrap();
    assert_eq!(&t.indices[0..4], &[10, 11, 12, 13], "first pixel of each pass");
    assert_eq!(&t.indices[4..8], &[20, 21, 22, 23], "second pixel of each pass");
}

#[test]
fn a_walk_grid_is_four_cells_per_tile_and_closed_at_the_edges() {
    let mut grid = WalkGrid::new(3, 2);
    assert_eq!((grid.width, grid.height), (12, 8));
    assert_eq!(grid.open_cells(), 0, "starts blocked");

    for ty in 0..2 {
        for tx in 0..3 {
            grid.set_tile(tx, ty, WalkMask(0));
        }
    }
    assert_eq!(grid.open_cells(), 12 * 8);

    // One mask cell covers 8x8 map pixels.
    assert_eq!(MASK_CELL, 8);
    assert!(grid.walkable_at(0, 0));
    assert!(grid.walkable_at(95, 63), "the last pixel of the last cell");
    assert!(!grid.walkable_at(96, 0), "one past the right edge");
    assert!(!grid.walkable_at(-1, 0));

    grid.close_the_edges();
    assert!(!grid.walkable_at(0, 0), "the outer ring is blocked");
    assert!(grid.walkable_at(8, 8), "but the inside is not");
    assert_eq!(grid.open_cells(), 10 * 6);
}

fn pak_bytes() -> Option<Vec<u8>> {
    std::fs::read(testdata("burntime.pak")).ok()
}

fn load_png(path: &PathBuf) -> Option<Vec<[u8; 3]>> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    Some(match info.color_type {
        png::ColorType::Rgb => buf.chunks(3).map(|p| [p[0], p[1], p[2]]).collect(),
        png::ColorType::Rgba => buf.chunks(4).map(|p| [p[0], p[1], p[2]]).collect(),
        _ => return None,
    })
}

/// The rendered PNGs are an oracle for *placement*, not for colour.
///
/// The remaster's ripper exports a tile the first time any map uses it, in
/// that map's palette (TileExporter walks maps 0..37, 80..84, 89 in order), so
/// there is no single palette that reproduces all of them — 17_19 is in map
/// 17's because map 17 is the first to use it.
///
/// So the check is in two parts. Indices are palette-independent: if the
/// four-pass de-interleave put a pixel in the wrong place, some index would
/// have to be two different colours inside one tile. And then, more strongly,
/// every referenced tile must reproduce the PNG *exactly* under one of the map
/// palettes — consistency alone could survive a transform that permuted whole
/// runs of equal pixels, and an exact match cannot.
#[test]
fn decoded_tiles_agree_with_the_remasters_own_renders() {
    let Some(bytes) = pak_bytes() else {
        eprintln!("skipping: no burntime.pak — see testdata/README.md");
        return;
    };
    let pak = ClassicPak::open(&bytes).unwrap();

    // The maps the exporter walks, in its order — one of these palettes drew
    // each rendered tile.
    let palettes: Vec<Palette> = (0..=37)
        .chain([80, 81, 82, 83, 84, 89])
        .filter_map(|m| pak.get(&format!("burn_gfx/mat_{m:03}.raw")))
        .filter_map(Palette::from_mat_raw)
        .collect();
    assert!(palettes.len() > 30, "only found {} map palettes", palettes.len());

    let mut compared = 0;
    let mut exact = 0;
    for section in 1u8..=50 {
        let Some(raw) = pak.get(&format!("burn_gfx/zei_{section:03}.raw")) else { continue };
        let set = TileSet::parse(raw).unwrap_or_else(|e| panic!("zei_{section:03}: {e:?}"));

        for index in 0..set.len() {
            let Some(want) = load_png(&testdata(&format!("tiles/{section:02}_{index:02}.png")))
            else {
                continue;
            };
            let tile = set.tile(index).unwrap();
            assert_eq!(want.len(), tile.indices.len(), "{section:02}_{index:02}: size");

            // One colour per index, throughout the tile.
            let mut seen: std::collections::HashMap<u8, [u8; 3]> = std::collections::HashMap::new();
            for (&i, &colour) in tile.indices.iter().zip(&want) {
                match seen.get(&i) {
                    Some(&first) => assert_eq!(
                        first, colour,
                        "{section:02}_{index:02}: index {i} appears as two colours"
                    ),
                    None => {
                        seen.insert(i, colour);
                    }
                }
            }
            compared += 1;

            // Stronger than consistency: every referenced tile must come out
            // pixel for pixel under *some* map's palette. Which one is not
            // knowable without replaying the exporter's order, but that one
            // exists is, and a wrong transform would not survive it.
            let matched = palettes.iter().any(|palette| {
                let mine: Vec<[u8; 3]> =
                    tile.to_rgba(palette).rgba.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
                mine == want
            });
            assert!(matched, "{section:02}_{index:02}: no map palette reproduces it");
            exact += 1;
        }
    }
    assert!(compared > 800, "only compared {compared} tiles");
    assert_eq!(exact, compared, "every referenced tile matched exactly");
}

/// The sections with no rendered PNG anywhere decode with the same code and no
/// special case — which is the whole reason for reading these files.
#[test]
fn the_sections_with_no_artwork_decode_too() {
    let Some(bytes) = pak_bytes() else { return };
    let pak = ClassicPak::open(&bytes).unwrap();

    for section in [14u8, 15] {
        let raw = pak
            .get(&format!("burn_gfx/zei_{section:03}.raw"))
            .unwrap_or_else(|| panic!("zei_{section:03} should be in the archive"));
        assert_eq!(raw.len(), 65_520);

        let set = TileSet::parse(raw).expect("should parse like any other");
        assert_eq!(set.len(), TILES_PER_FILE);

        // Real artwork, not a blank run: several distinct colours per tile and
        // a mixture of open and blocked masks across the file.
        let distinct: std::collections::HashSet<u8> =
            set.tile(0).unwrap().indices.iter().copied().collect();
        assert!(distinct.len() > 4, "section {section} tile 0 has {} colours", distinct.len());

        let open = (0..set.len()).filter(|&i| set.tile(i).unwrap().mask.is_open()).count();
        assert!(open > 0 && open < set.len(), "section {section}: {open} open of {}", set.len());
    }
}

/// The masks in the file are the game's own. The remaster additionally ships a
/// handful of hand-written `.txt` masks which `BurnmapProcessor` applies *after*
/// them — corrections, not a description. Nine of the ten differ, and two even
/// unblock a cell the file blocks, so they must not be used as an oracle here.
/// This pins that they are a separate layer rather than a disagreement.
#[test]
fn the_hand_written_masks_are_corrections_not_a_second_opinion() {
    let Some(bytes) = pak_bytes() else { return };
    let Ok(dir) = std::fs::read_dir(testdata("masks")) else {
        eprintln!("skipping: no testdata/masks — see testdata/README.md");
        return;
    };
    let pak = ClassicPak::open(&bytes).unwrap();

    let mut differing = 0;
    let mut checked = 0;
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".txt") else { continue };
        let Some((section, index)) = stem.split_once('_') else { continue };
        let (Ok(section), Ok(index)) = (section.parse::<u8>(), index.parse::<usize>()) else {
            continue;
        };
        let Some(raw) = pak.get(&format!("burn_gfx/zei_{section:03}.raw")) else { continue };

        let mask = TileSet::parse(raw).unwrap().tile(index).unwrap().mask;
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let mut same = true;
        for (y, line) in text.lines().take(4).enumerate() {
            for (x, c) in line.chars().take(4).enumerate() {
                if mask.walkable(x, y) != (c != '1') {
                    same = false;
                }
            }
        }
        checked += 1;
        if !same {
            differing += 1;
        }
    }
    assert!(checked >= 8, "only found {checked} mask files");
    assert!(
        differing >= checked - 1,
        "only {differing} of {checked} differ — if these now agree, they may have stopped \
         being corrections and this test should become an equality check"
    );
}
