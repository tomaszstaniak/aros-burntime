//! Compose a `.burnmap` into a PNG, so a map can be looked at without a window.
//!
//!     cargo run -p burngfx --example render_map -- MAT_000
//!
//! Tiles come from the remaster's rendered originals in `testdata/tiles`
//! (`<section>_<item>.png`) and, for a map's own artwork, from its
//! `<map>_tiles.png` sheet. A development aid: the engine will draw from the
//! player's own data, not from these.

use burngfx::map;
use std::collections::HashMap;
use std::path::PathBuf;

const TILE: usize = 32;

/// Width, height and RGBA pixels of a loaded PNG.
type Image = (usize, usize, Vec<u8>);

fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

/// Any PNG, as RGBA plus its size.
fn load_png(path: &PathBuf) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(file);
    // The map sheets are palette PNGs; expand them so every source is 8-bit
    // RGB or RGBA and the blit below has one shape to handle.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    // The rendered tiles are 8-bit RGB; the sprite references are RGBA.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => {
            eprintln!("{}: unsupported colour type {other:?}", path.display());
            return None;
        }
    };
    Some((info.width as usize, info.height as usize, rgba))
}

/// Mark a tile we have no artwork for, so it cannot be mistaken for scenery.
/// Sections 14 and 15 have no rendered PNGs in the remaster at all, and 28 and
/// 29 are partial; those live in the player's own `zei_<section>.raw`.
fn mark_missing(dst: &mut [u8], dst_w: usize, at_x: usize, at_y: usize) {
    for y in 0..TILE {
        for x in 0..TILE {
            let mark = x == y || x + y == TILE - 1 || x == 0 || y == 0 || x == TILE - 1 || y == TILE - 1;
            let d = ((at_y + y) * dst_w + at_x + x) * 4;
            dst[d..d + 4].copy_from_slice(if mark {
                &[0xA0, 0x38, 0x78, 0xFF]
            } else {
                &[0x2A, 0x1E, 0x2E, 0xFF]
            });
        }
    }
}

fn blit(dst: &mut [u8], dst_w: usize, src: &[u8], src_w: usize, at_x: usize, at_y: usize) {
    for y in 0..TILE {
        for x in 0..TILE {
            let s = (y * src_w + x) * 4;
            if src.len() < s + 4 {
                continue;
            }
            let d = ((at_y + y) * dst_w + at_x + x) * 4;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "MAT_000".into());
    let dir = testdata();

    let bytes = std::fs::read(dir.join(format!("maps/{name}.burnmap")))
        .unwrap_or_else(|e| panic!("{name}.burnmap: {e} — see testdata/README.md"));
    let m = map::parse(&bytes).expect("parse");

    let (w, h) = m.pixel_size();
    let (w, h) = (w as usize, h as usize);
    let mut canvas = vec![0u8; w * h * 4];

    // The map's own sheet, if it has one: a flat run of 32x32 cells.
    let sheet = load_png(&dir.join(format!("maps/{name}_tiles.png")));

    let mut cache: HashMap<(u8, u8), Option<Image>> = HashMap::new();
    let mut drawn = 0;
    let mut missing = Vec::new();

    for y in 0..m.height {
        for x in 0..m.width {
            let tile = m.tile(x, y);
            let (px, py) = (x * TILE, y * TILE);

            if let Some((section, item)) = tile.classic_name() {
                let entry = cache.entry((section, item)).or_insert_with(|| {
                    load_png(&dir.join(format!("tiles/{section:02}_{item:02}.png")))
                });
                match entry {
                    Some((tw, _, rgba)) => {
                        blit(&mut canvas, w, rgba, *tw, px, py);
                        drawn += 1;
                    }
                    None => {
                        mark_missing(&mut canvas, w, px, py);
                        missing.push(format!("{section:02}_{item:02}"));
                    }
                }
            } else if let Some(index) = tile.custom_sheet_index() {
                match &sheet {
                    Some((sw, _, rgba)) => {
                        let cols = sw / TILE;
                        let (cx, cy) = ((index % cols) * TILE, (index / cols) * TILE);
                        // Offset into the sheet, reusing the same blit.
                        let mut cell = vec![0u8; TILE * TILE * 4];
                        for ty in 0..TILE {
                            for tx in 0..TILE {
                                let s = ((cy + ty) * sw + cx + tx) * 4;
                                if s + 4 <= rgba.len() {
                                    let d = (ty * TILE + tx) * 4;
                                    cell[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                                }
                            }
                        }
                        blit(&mut canvas, w, &cell, TILE, px, py);
                        drawn += 1;
                    }
                    None => missing.push(format!("sheet index {index}")),
                }
            }
        }
    }

    let out = dir.join(format!("{name}.png"));
    let file = std::fs::File::create(&out).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&canvas).unwrap();

    println!("{name}: {}x{} tiles, {w}x{h} px, {drawn} drawn", m.width, m.height);
    println!("  {} entrances", m.entrances.len());
    if missing.is_empty() {
        println!("  no missing tiles");
    } else {
        missing.sort();
        missing.dedup();
        println!("  MISSING {} tiles: {:?}", missing.len(), &missing[..missing.len().min(8)]);
    }
    println!("  -> {}", out.display());
}
