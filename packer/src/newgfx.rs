//! `--newgfx`: the remaster's NewGfx pictures, from the player's v1.2 copy.
//!
//! v1.2 mounts `game/classic_newgfx` over `game/classic` and swaps resources
//! through `newgfx.txt` (`Application.cs:198-201,586-597`). Here that table is
//! evaluated once, on the host: every classic id the game draws is resolved
//! with [`burngfx::newgfx::resolve`] against the two directories —
//! `v1.2/newgfx/` (the `classic_newgfx` package) over `v1.2/` (the `classic`
//! one), names compared without case as the package system does — and what
//! it reaches is cut to the cells actually used and packed under
//! `ng/<classic key>` with its scale ([`burngfx::pack::ScaledImage`]). An id
//! that v1.2 would leave classic gets no entry, so the game falls back the
//! same way.
//!
//! The fonts follow `[noscale]`: `font.txt` → `font-highres.txt` (from the
//! classic package), `font-small.txt` → `font-small-highres.txt`.
//!
//! None of this is ours to ship: see `burngfx/testdata/README.md`.

use crate::{src, Entry};
use burngfx::newgfx::{self, Resolved, ResourceId, Table};
use burngfx::pack::{names, ScaledImage, NAME_LEN};
use burngfx::ratio::Pair;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// The two package directories, indexed by lower-case relative path.
struct Files {
    index: HashMap<String, PathBuf>,
    decoded: HashMap<PathBuf, Option<(u32, u32, Vec<u8>)>>,
}

impl Files {
    fn new(roots: &[&Path]) -> Files {
        let mut index = HashMap::new();
        // The classic package first, so the NewGfx one replaces what both
        // have (`PackageSystem.cs:403-470`: the later mount wins); each
        // version's over the one before.
        for root in roots {
            walk(root, root, &mut index);
        }
        Files { index, decoded: HashMap::new() }
    }

    fn exists(&self, file: &str) -> bool {
        self.index.contains_key(&file.to_lowercase())
    }

    fn path(&self, file: &str) -> Option<&PathBuf> {
        self.index.get(&file.to_lowercase())
    }

    /// A PNG as RGBA, once per file.
    fn png(&mut self, file: &str) -> Option<&(u32, u32, Vec<u8>)> {
        let path = self.path(file)?.clone();
        self.decoded.entry(path.clone()).or_insert_with(|| decode(&path)).as_ref()
    }
}

fn walk(root: &Path, dir: &Path, index: &mut HashMap<String, PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            // The NewGfx package lives inside the classic one's directory
            // here; it is not part of the classic package.
            if dir == root && e.file_name() == "newgfx" {
                continue;
            }
            walk(root, &p, index);
        } else if let Ok(rel) = p.strip_prefix(root) {
            index.insert(rel.to_string_lossy().replace('\\', "/").to_lowercase(), p);
        }
    }
}

fn decode(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = src::read(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => {
            eprintln!("{}: unsupported colour type {other:?}", path.display());
            return None;
        }
    };
    Some((info.width, info.height, rgba))
}

/// `(x, y, w, h)` of an image as a packed entry with its scale.
fn cut(img: &(u32, u32, Vec<u8>), rect: (u32, u32, u32, u32), scale: Pair) -> Option<Vec<u8>> {
    let (iw, ih, rgba) = img;
    let (x, y, w, h) = rect;
    if w == 0 || h == 0 || x + w > *iw || y + h > *ih {
        return None;
    }
    let header = ScaledImage::header(scale).unwrap_or_else(|| panic!("scale {scale:?} does not fit the header"));
    let mut out = Vec::with_capacity(8 + (w * h * 4) as usize);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    for row in y..y + h {
        let at = ((row * iw + x) * 4) as usize;
        out.extend_from_slice(&rgba[at..at + (w * 4) as usize]);
    }
    Some(out)
}

/// The frames a resolved id draws, each a packed entry. `pngsheet` cells
/// as `PngSpriteSheetProcessor` cuts them (row-major, `floor(width / w)`
/// columns), `pngani` one file per frame (`AniProcessorPng`: the file name
/// formatted with the frame), `png` the whole file.
fn frames(files: &mut Files, id: &ResourceId, scale: Pair) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    match id.format.as_str() {
        "pngsheet" => {
            let Some((cw, ch)) = id.cell() else { return out };
            let Some(img) = files.png(&id.file) else { return out };
            let cols = (img.0 / cw.max(1)).max(1);
            for f in id.frames() {
                let f = f as u32;
                let rect = ((f % cols) * cw, (f / cols) * ch, cw, ch);
                match cut(img, rect, scale) {
                    Some(e) => out.push(e),
                    None => {
                        println!("newgfx: {} frame {f} is outside the sheet", id.id);
                        return Vec::new();
                    }
                }
            }
        }
        "pngani" => {
            for f in id.frames() {
                let file = id.file.replace("{0}", &f.to_string());
                let Some(img) = files.png(&file) else { return Vec::new() };
                out.extend(cut(img, (0, 0, img.0, img.1), scale));
            }
        }
        "png" => {
            if let Some(img) = files.png(&id.file_for_index()) {
                out.extend(cut(img, (0, 0, img.0, img.1), scale));
            }
        }
        other => println!("newgfx: {} — no reader for {other}", id.id),
    }
    out
}

/// What the NewGfx part added: entries, and their pixels decoded.
pub struct Summary {
    pub entries: usize,
    pub pixels_bytes: usize,
    pub classic: usize,
    /// The keys v1.2 leaves classic.
    pub classic_keys: Vec<String>,
}

/// Resolve and pack. `tiles` are the packed maps' classic tiles, `sheets`
/// their own-sheet cells by map, `rooms` the interiors they reach.
pub fn pack(dir: &Path, tiles: &BTreeSet<(u8, u8)>, sheets: &BTreeMap<String, BTreeSet<usize>>,
            rooms: &BTreeSet<u8>, entries: &mut Vec<Entry>) -> Option<Summary> {
    let classic_dir = dir.join("v1.2");
    let newgfx_dir = classic_dir.join("newgfx");
    // v1.3's package over v1.2's (`v1.3/` and `v1.3/newgfx/`): its table —
    // the two new flags, and the new character artwork, an opt-in this port
    // leaves off (`burngfx::newgfx::replacement`) — and its `start_flags.png`.
    let classic13 = dir.join("v1.3");
    let newgfx13 = classic13.join("newgfx");
    let table_path = if newgfx13.join("newgfx.txt").is_file() { newgfx13.join("newgfx.txt") } else { newgfx_dir.join("newgfx.txt") };
    let table_text = match src::read_to_string(&table_path) {
        Ok(t) => t,
        Err(_) => {
            println!("newgfx: no v1.2/newgfx/newgfx.txt — the NewGfx look will have nothing to draw");
            return None;
        }
    };
    let Some(table) = Table::parse(&table_text) else {
        println!("newgfx: v1.2/newgfx/newgfx.txt does not read as a config file");
        return None;
    };
    let tables = [table];
    let mut files = Files::new(&[&classic_dir, &classic13, &newgfx_dir, &newgfx13]);

    // Every (key, id) the game draws.
    let mut wanted: Vec<(String, String)> =
        names::NEWGFX.iter().map(|(k, id)| (k.to_string(), id.to_string())).collect();
    for (stem, file, range) in [("munt", "munt.raw", 0..27), ("syst", "syst.raw", 0..112), ("inv", "inv.raw", 0..4),
                                 ("sta", "sta.ani", 8..17)] {
        for n in range {
            wanted.push((format!("f/{stem}/{n}"), format!("{file}?{n}")));
        }
    }
    for n in 0..30 {
        wanted.push((format!("f/ges_{n:02}"), format!("ges_{n:02}.ani")));
    }
    for (key, file) in names::V13_FLAGS {
        wanted.push((key.to_string(), file.to_string()));
    }
    // The body sheet's every cell (272 at v1.2, the classic 214 and the
    // remaster's extra frames), and the item sheet's.
    for n in 0..272 {
        wanted.push((format!("body/{n}"), format!("syssze.raw?{n}")));
    }
    for n in 0..64 {
        wanted.push((format!("item/{n}"), format!("gst.raw?{n}")));
    }
    // `BurnmapProcessor.cs:254,263`. The map number in a tile's id is not
    // read by the table, so a replaced tile is one entry for every map.
    for &(s, i) in tiles {
        wanted.push((format!("tile/{s:02}_{i:02}"), format!("burngfxtile@zei_{s:03}.raw?{i}?0")));
    }
    for (map, cells) in sheets {
        for c in cells {
            wanted.push((format!("sheet/{map}/{c}"), format!("pngsheet@maps/{}_tiles.png?{c}?32x32", map.to_lowercase())));
        }
    }
    for r in rooms {
        wanted.push((format!("room/{r}"), format!("raum_{r}.pac")));
    }

    let mut summary = Summary { entries: 0, pixels_bytes: 0, classic: 0, classic_keys: Vec::new() };
    let exists = |f: &str| files.exists(f);
    let resolved: Vec<(String, Resolved)> =
        wanted.into_iter().map(|(k, id)| { let r = newgfx::resolve(&tables, &id, &exists); (k, r) }).collect();
    for (key, r) in resolved {
        let Resolved::New(rep) = r else {
            summary.classic += 1;
            summary.classic_keys.push(key);
            continue;
        };
        let got = frames(&mut files, &rep.id, rep.scale);
        if got.is_empty() {
            println!("newgfx: {key} → {} drew nothing", rep.id.id);
            continue;
        }
        // One frame where the classic key names one; an animation keeps
        // every frame the replacement has, `/k` from 0.
        let single = got.len() == 1 && !key.starts_with("a/");
        for (k, data) in got.into_iter().enumerate() {
            let name = if single { key.clone() } else { format!("{key}/{k}") };
            let mut buf = [0u8; NAME_LEN];
            let n = names::newgfx(&mut buf, &name).unwrap_or_else(|| panic!("ng/{name}: name too long for the pack"));
            summary.pixels_bytes += data.len() - 8;
            summary.entries += 1;
            entries.push(Entry::new(&buf[..n], data));
        }
    }

    // The fonts: `LoadFont` takes the replacement without asking whether it
    // exists; the description and its atlas must both be here to pack.
    for (key, file) in names::NEWGFX_FONTS {
        let name = newgfx::resolve_file(&tables, file, &|_| true);
        let Some(path) = files.path(&name).cloned() else {
            println!("newgfx: no {name} for {file}");
            continue;
        };
        let Ok(text) = src::read(&path) else { continue };
        let image = burngfx::config::Config::parse(&String::from_utf8_lossy(&text))
            .and_then(|c| c.section("").and_then(|s| s.get("image")).map(|s| s.trim().to_string()));
        let Some(image) = image else {
            println!("newgfx: {name} names no image");
            continue;
        };
        let Some(img) = files.png(&image) else {
            println!("newgfx: no {image} for {name}");
            continue;
        };
        let (w, h, rgba) = img;
        let mut strip = Vec::with_capacity(4 + rgba.len());
        strip.extend_from_slice(&(*w as u16).to_le_bytes());
        strip.extend_from_slice(&(*h as u16).to_le_bytes());
        strip.extend_from_slice(rgba);
        if burngfx::font::ScaledFont::parse(&String::from_utf8_lossy(&text), *h as i64).is_none() {
            println!("newgfx: {name} does not read as a v1.2 font");
            continue;
        }
        summary.pixels_bytes += rgba.len();
        summary.entries += 2;
        entries.push(Entry::new(format!("ng/{key}").as_bytes(), text));
        entries.push(Entry::new(format!("ng/{key}.strip").as_bytes(), strip));
    }
    // Whole sheets the game cuts itself, by the classic cell, at their scale
    // (the cut follows: a cell is the classic one times the scale):
    // `font-indicators` 8 × 12 → 15 × 27, `font-icons` 12 × 12 → 22 × 27
    // (`newgfx.txt:136-137`), and `player_icons.png`, one face a 36 × 38 cell
    // (`:144`, `MapViewOverlayPlayer.cs:48`), which has no classic picture.
    for (key, id) in [("ui/indicators", "pngsheet@font-indicators.png?0?8x12"),
                      ("ui/font_icons", "pngsheet@font-icons.png?0?12x12"),
                      ("n/player_icons", "gfx/objects/player_icons.png?0")] {
        let exists = |f: &str| files.exists(f);
        let Resolved::New(r) = newgfx::resolve(&tables, id, &exists) else {
            summary.classic += 1;
            summary.classic_keys.push(key.to_string());
            continue;
        };
        let Some(img) = files.png(&r.id.file_for_index()) else { continue };
        let Some(data) = cut(img, (0, 0, img.0, img.1), r.scale) else { continue };
        summary.pixels_bytes += data.len() - 8;
        summary.entries += 1;
        entries.push(Entry::new(format!("ng/{key}").as_bytes(), data));
    }
    scenes(&mut files, &newgfx_dir, entries, &mut summary);
    Some(summary)
}

/// The films' NewGfx descriptions (`classic_newgfx/scenes/*.txt`, which
/// replace `classic/scenes/*.txt` by name, `ImageScene`'s `ResolveFileReplacement`):
/// each text under `ng/scenes/<stem>`, its background under `ng/fs/<stem>`
/// and animation `a`'s frames under `ng/fs/<stem>/<a>/<f>`, all at the
/// file's `sprite_scale` (`GuiImage.Reload`).
fn scenes(files: &mut Files, newgfx_dir: &Path, entries: &mut Vec<Entry>, summary: &mut Summary) {
    let Ok(rd) = std::fs::read_dir(newgfx_dir.join("scenes")) else { return };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    paths.sort();
    for path in paths {
        let stem = path.file_stem().unwrap().to_string_lossy().to_lowercase();
        let Ok(bytes) = src::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let Some(config) = burngfx::config::Config::parse(&text) else { continue };
        let Some(image) = config.section("image") else { continue };
        let scale = image.get("sprite_scale").and_then(Pair::parse).unwrap_or(Pair::ONE);
        let mut push = |name: String, data: Vec<u8>, summary: &mut Summary| {
            assert!(name.len() <= NAME_LEN, "{name}: name too long for the pack");
            summary.pixels_bytes += data.len().saturating_sub(8);
            summary.entries += 1;
            entries.push(Entry::new(name.as_bytes(), data));
        };
        let Some(bg) = image.get("background").map(|b| ResourceId::parse(b.trim())) else { continue };
        let got = frames(files, &bg, scale);
        let Some(first) = got.into_iter().next() else {
            println!("newgfx: scenes/{stem}.txt: no {}", bg.id);
            continue;
        };
        push(format!("ng/fs/{stem}"), first, summary);
        let count: usize = image.get("animations").and_then(|n| n.trim().parse().ok()).unwrap_or(0);
        for a in 0..count {
            let Some(id) = config.section(&format!("animation{a}")).and_then(|s| s.get("image")) else { continue };
            for (f, data) in frames(files, &ResourceId::parse(id.trim()), scale).into_iter().enumerate() {
                push(format!("ng/fs/{stem}/{a}/{f}"), data, summary);
            }
        }
        push(format!("ng/scenes/{stem}"), bytes, summary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **v1.2's own table, per scene** (the player's `newgfx.txt` in the
    /// shared test data, when it is there): what each scene's pictures
    /// resolve to, with the files of both packages, and what that gives in
    /// game pixels (`Sprite.Size`) — the 512 × 240 rooms, the 546 × 341
    /// start picture, the 360-wide films.
    #[test]
    fn v12s_table_chooses_per_scene() {
        let dir = crate::testdata().join("v1.2");
        let Ok(text) = std::fs::read_to_string(dir.join("newgfx/newgfx.txt")) else {
            eprintln!("no v1.2/newgfx in the test data; skipped");
            return;
        };
        let tables = [Table::parse(&text).unwrap()];
        let mut files = Files::new(&[&dir, &dir.join("newgfx")]);
        let s60 = Pair::parse("1.875x2.25").unwrap();
        let game = |img: &(u32, u32, Vec<u8>), s: Pair| {
            use burngfx::ratio::Ratio;
            let r = |n: u32, k: Ratio| (Ratio::int(n as i64) / k + Ratio::new(1, 2)).floor();
            (r(img.0, s.x), r(img.1, s.y))
        };
        // (id, the file it reaches, its scale, its size in game pixels)
        let cases: &[(&str, &str, &str, (i64, i64))] = &[
            ("sta.pac", "gfx/start.png", "1.875x2.25", (546, 341)),             // start
            ("opti.pac", "gfx/radio_background.png", "1.875x2.25", (536, 300)), // options
            ("info.pac", "gfx/backgrounds/info.png", "1.875x2.25", (540, 300)),  // info
            ("raum_3.pac", "gfx/backgrounds/room3_large.png", "1.875x2.25", (512, 240)), // a room
            ("hint2.pac", "gfx/backgrounds/hint2.png", "2x2.375", (320, 200)),  // a pile
            ("gfx/trader_background.png", "gfx/backgrounds/trader.png", "1.875x2.25", (546, 455)), // trader
            ("ges_04.ani", "gfx/faces/geometry_true/ges_04.png", "1.875x2.25", (68, 57)), // a face
        ];
        for &(id, file, scale, size) in cases {
            let exists = |f: &str| files.exists(f);
            let r = match newgfx::resolve(&tables, id, &exists) {
                Resolved::New(r) => r,
                other => panic!("{id}: {other:?}"),
            };
            assert_eq!((r.id.file_for_index().as_str(), r.scale), (file, Pair::parse(scale).unwrap()), "{id}");
            let img = files.png(file).unwrap().clone();
            assert_eq!(game(&img, r.scale), size, "{id}: {}x{}", img.0, img.1);
        }
        // Cells: a map tile, a body, an item — 60 × 72 or 30 × 36, one game
        // tile or one body each.
        for (id, cell) in [("burngfxtile@zei_004.raw?12?5", (60, 72)), ("syssze.raw?16", (30, 36)), ("gst.raw?7", (60, 72))] {
            let exists = |f: &str| files.exists(f);
            let Resolved::New(r) = newgfx::resolve(&tables, id, &exists) else { panic!("{id}") };
            assert_eq!((r.id.cell(), r.scale), (Some(cell), s60), "{id}");
        }
        // What v1.2 keeps classic: the map's daytime bar has no file
        // (`munt.raw?1`), nor have the inventory's first and last frames.
        for id in ["munt.raw?1", "inv.raw?0", "inv.raw?3"] {
            let exists = |f: &str| files.exists(f);
            assert!(matches!(newgfx::resolve(&tables, id, &exists), Resolved::Classic(_)), "{id}");
        }
        // The sheets the game cuts: by the classic cell, at their scale.
        for (id, file) in [("pngsheet@font-indicators.png?0?8x12", "font-indicators.png"),
                           ("gfx/objects/player_icons.png?0", "gfx/objects/player_icons.png")] {
            let exists = |f: &str| files.exists(f);
            let Resolved::New(r) = newgfx::resolve(&tables, id, &exists) else { panic!("{id}") };
            assert_eq!((r.id.file_for_index().as_str(), r.scale), (file, s60), "{id}");
        }
        // Fonts: `[noscale]`, the high-resolution pair.
        assert_eq!(newgfx::resolve_file(&tables, "font.txt", &|_| true), "font-highres.txt");
        assert_eq!(newgfx::resolve_file(&tables, "font-small.txt", &|_| true), "font-small-highres.txt");
        // A film: `scenes/film_05.txt` is the NewGfx package's, 360 × 200.
        let mut entries = Vec::new();
        let mut summary = Summary { entries: 0, pixels_bytes: 0, classic: 0, classic_keys: Vec::new() };
        scenes(&mut files, &dir.join("newgfx"), &mut entries, &mut summary);
        let bg = entries.iter().find(|e| e.name == b"ng/fs/film_05").expect("film 5's picture");
        let bg = ScaledImage::parse(&bg.data).unwrap();
        assert_eq!((bg.image.game_w(), bg.image.game_h()), (360, 200));
        assert!(entries.iter().any(|e| e.name == b"ng/fs/film_05/0/17"), "its first animation's 18 frames");
        assert!(entries.iter().any(|e| e.name == b"ng/scenes/film_05"));
    }

    fn write_png(path: &Path, w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(file, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&px(x, y));
            }
        }
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }

    /// A two-cell tile sheet in the classic package, its table in the
    /// NewGfx one: tile 04_01 is cut from cell 1 at 15/8 × 9/4; a road
    /// whose file is missing, and an id the table does not name, stay
    /// classic; a file in both packages is the NewGfx one's.
    #[test]
    fn the_table_is_evaluated_and_the_used_cells_cut() {
        let dir = std::env::temp_dir().join(format!("packer-newgfx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ng = dir.join("v1.2/newgfx");
        std::fs::create_dir_all(&ng).unwrap();
        std::fs::write(ng.join("newgfx.txt"), "[replacement_60x72]\nsprite_scale=1.875x2.25\n\
            zei_004.raw=pngsheet@maps/shared/set_4.png?{0}?6x4\nsyst.raw=gfx/ways/syst_{0}.png\n\
            gfx/up.png=gfx/up.png\n").unwrap();
        write_png(&dir.join("v1.2/maps/shared/SET_4.png"), 12, 4, |x, _| [if x < 6 { 1 } else { 2 }, 0, 0, 255]);
        write_png(&ng.join("gfx/ways/syst_30.png"), 2, 2, |_, _| [9, 9, 9, 255]);
        write_png(&dir.join("v1.2/gfx/up.png"), 1, 1, |_, _| [3, 3, 3, 255]);
        write_png(&ng.join("gfx/up.png"), 2, 1, |_, _| [4, 4, 4, 255]);

        let tiles: BTreeSet<(u8, u8)> = [(4, 1)].into_iter().collect();
        let mut entries = Vec::new();
        let s = pack(&dir, &tiles, &BTreeMap::new(), &BTreeSet::new(), &mut entries).unwrap();
        let get = |n: &str| entries.iter().find(|e| e.name == n.as_bytes()).map(|e| e.data.clone());

        let tile = get("ng/tile/04_01").expect("the tile is replaced");
        let img = ScaledImage::parse(&tile).unwrap();
        assert_eq!(img.scale, Pair::parse("1.875x2.25").unwrap());
        assert_eq!((img.image.width, img.image.height), (6, 4));
        assert_eq!(img.image.pixel(0, 0).unwrap()[0], 2, "cell 1 is the right-hand one");
        assert!(get("ng/f/syst/30").is_some());
        assert!(get("ng/f/syst/31").is_none());
        assert!(s.classic_keys.iter().any(|k| k == "f/syst/31"));
        let up_bytes = get("ng/ui/up").unwrap();
        let up = ScaledImage::parse(&up_bytes).unwrap().image;
        assert_eq!((up.width, up.pixel(0, 0).unwrap()[0]), (2, 4));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
