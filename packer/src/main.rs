//! Build the game's asset archive, `burntime.pack`, from the player's own
//! Burntime files.
//!
//!     packer --from DIR --out burntime.pack            # world + Nob Hill
//!     packer --from DIR --out burntime.pack full       # every map
//!     scripts/pack.sh full                             # the developer's, over burngfx/testdata
//!
//! Everything that needs decoding is decoded here: PNG tiles and the font
//! strip become raw RGBA, so the AROS side needs no image library. Maps, room
//! settings and string tables go in as they are, because `burngfx` parses
//! those already and they are small.
//!
//! Nothing about this is redistributable. It reads files that come from an
//! installation of the game and from the remaster's repository, laid out as
//! `burngfx/testdata/README.md` describes, and writes an archive that is
//! equally not ours to publish. Beside it, `<out>.sources` lists every file
//! read with its size and SHA-256: what the pack was made of.

mod newgfx;
mod sha256;

use burngfx::map;
use burngfx::pack::{names, NAME_LEN, VERSION};
use burngfx::pak::ClassicPak;
use burngfx::text::MapSettings;
use burngfx::zei::{TileSet, WalkGrid};
use burngfx::Palette;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const TILE: u32 = 32;


/// The repository's own test data: for the tests only. The program itself
/// is told where the files are (`--from`).
#[cfg(test)]
fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata")
}

/// Every source file read, for the list written beside the pack: its path
/// under `--from`, its size and its SHA-256. What a pack was made of, and
/// so whose it is.
/// The remaster's files: `DIR/v1.3/` over `DIR/v1.2/`. A file v1.3 has is
/// read from there, every other one from v1.2 — the files v1.3 changed or
/// added (its rule presets, `newburn.txt`, `tooltip.txt`, `BURN.TXT`,
/// `setupnotes.txt`, `MEN_054.TXT`, their German copies), taken at the tag
/// `DIR/v1.3/SOURCE` names. Pictures, fonts and NewGfx stay v1.2's: v1.3's
/// experimental character artwork is not included.
struct Remaster {
    v12: PathBuf,
    v13: PathBuf,
}

impl Remaster {
    fn new(dir: &Path) -> Remaster {
        Remaster { v12: dir.join("v1.2"), v13: dir.join("v1.3") }
    }

    fn join(&self, rel: impl AsRef<Path>) -> PathBuf {
        let newer = self.v13.join(rel.as_ref());
        if newer.is_file() { newer } else { self.v12.join(rel) }
    }

    /// The file names in `rel` of either, once each, sorted. A name v1.3
    /// dropped by renaming it (`-MEN_040.TXT` → `MEN_054.TXT`) is still
    /// listed from v1.2; the callers take only `MEN_NNN.TXT`.
    fn names(&self, rel: &str) -> Vec<String> {
        let mut out: Vec<String> = [&self.v12, &self.v13]
            .iter()
            .filter_map(|d| std::fs::read_dir(d.join(rel)).ok())
            .flat_map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect::<Vec<_>>())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

mod src {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    thread_local! {
        static READ: RefCell<(PathBuf, Vec<(String, usize, [u8; 32])>)> = RefCell::new((PathBuf::new(), Vec::new()));
    }

    pub fn root(dir: &Path) {
        READ.with(|r| r.borrow_mut().0 = dir.to_path_buf());
    }

    fn note(path: &Path, bytes: &[u8]) {
        READ.with(|r| {
            let mut r = r.borrow_mut();
            let rel = path.strip_prefix(&r.0).unwrap_or(path).to_string_lossy().replace('\\', "/");
            if !r.1.iter().any(|(p, _, _)| *p == rel) {
                r.1.push((rel, bytes.len(), super::sha256::digest(bytes)));
            }
        });
    }

    pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        let bytes = std::fs::read(path.as_ref())?;
        note(path.as_ref(), &bytes);
        Ok(bytes)
    }

    pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
        let bytes = read(path)?;
        String::from_utf8(bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// `sha256  size  path` a line, sorted by path.
    pub fn listing() -> String {
        READ.with(|r| {
            let mut v = r.borrow().1.clone();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v.iter().map(|(p, n, d)| format!("{}  {n}  {p}\n", super::sha256::hex(d))).collect()
        })
    }
}

const USAGE: &str = "usage: packer --from DIR [--out FILE] [--newgfx] [PROFILE | MAT_NNN ...]\n\n\
DIR holds the files the pack is made from, laid out as\n\
burngfx/testdata/README.md describes. FILE is burntime.pack in the current\n\
directory unless given. Beside it, FILE.sources lists every file read, with\n\
its size and SHA-256. --newgfx adds the remaster's NewGfx pictures from\n\
DIR/v1.2/newgfx (v1.2's game/classic_newgfx): a much bigger pack.";

/// An entry waiting to be written.
struct Entry {
    name: Vec<u8>,
    data: Vec<u8>,
}

impl Entry {
    fn new(name: &[u8], data: Vec<u8>) -> Entry {
        assert!(name.len() <= NAME_LEN, "name {:?} is too long", String::from_utf8_lossy(name));
        Entry { name: name.to_vec(), data }
    }
}

/// A PNG as `u16` width, `u16` height, then RGBA — the packed image format.
fn decode_png(path: &PathBuf) -> Option<Vec<u8>> {
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
        other => {
            eprintln!("{}: unsupported colour type {other:?}", path.display());
            return None;
        }
    };

    let mut out = Vec::with_capacity(4 + rgba.len());
    out.extend_from_slice(&(info.width as u16).to_le_bytes());
    out.extend_from_slice(&(info.height as u16).to_le_bytes());
    out.extend_from_slice(&rgba);
    Some(out)
}

/// The named map sets, so "the demo pack" means one thing to everybody.
///
/// `minimal` is the default and the smallest pack the AROS build can walk
/// through. `demo` adds Acid Town, which is what makes the three room cases
/// reachable in one run: an ordinary interior, a water source, and a doctor —
/// a room with no `raum_` picture at all, which must fall back to the panel
/// rather than leave the previous room on screen. Muarab has no such door.
///
/// It also carries `MAT_001`, which is where the 1993 save puts the first
/// player. Travelling needs only the road graph, so any location can be
/// reached without its map; going *inside* one needs the map, and the start
/// is the one place the player is before they have travelled anywhere.
const PROFILES: &[(&str, &[&str])] = &[
    ("minimal", &["MAT_000", "MAT_005"]),
    ("demo", &["MAT_000", "MAT_001", "MAT_005", "MAT_019"]),
    ("full", FULL),
];

/// The world, the thirty seven locations the 1993 save knows, and New
/// Village (`MAT_038`), the location the remaster adds: `gam.dat` has no
/// record of it, and the game builds one from its `.txt`
/// (`burngfx::gamdat::add_remaster_location`, `LocationCreator.Create` since
/// v1.2: a road of three days to location 8). It has no `mat_038.raw`
/// either: its rooms' kinds come from the `.txt` and its tiles from the
/// remaster's rendered `tiles/` pictures, which is how the remaster draws
/// every map.
///
/// Written out rather than generated from a range so that the list is a
/// list — a map that stops parsing should be a failure, not a gap nobody
/// notices.
const FULL: &[&str] = &[
    "MAT_000", "MAT_001", "MAT_002", "MAT_003", "MAT_004", "MAT_005", "MAT_006", "MAT_007",
    "MAT_008", "MAT_009", "MAT_010", "MAT_011", "MAT_012", "MAT_013", "MAT_014", "MAT_015",
    "MAT_016", "MAT_017", "MAT_018", "MAT_019", "MAT_020", "MAT_021", "MAT_022", "MAT_023",
    "MAT_024", "MAT_025", "MAT_026", "MAT_027", "MAT_028", "MAT_029", "MAT_030", "MAT_031",
    "MAT_032", "MAT_033", "MAT_034", "MAT_035", "MAT_036", "MAT_037", "MAT_038",
];

fn profile(name: &str) -> Option<Vec<String>> {
    PROFILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, maps)| maps.iter().map(|m| String::from(*m)).collect())
}

/// Pull the price table out of the remaster's `ConstValues.cs`.
///
/// The file holds one `int[]` whose entries are `value,//a,b,c,value,name}`
/// — the stored number, then the original's own four columns and the item's
/// name. Only the leading number matters; the rest is carried through as a
/// comment so the emitted file can be read by a person.
///
/// **This is a conversion, not a copy of the source file**, and where the
/// numbers should ultimately come from is a question this does not answer.
/// The provenance line it writes is there so nobody has to reconstruct it.
fn prices_from_const_values(text: &str) -> String {
    let mut out = String::from(
        "' trade values, one per burngfx sprite id\n         ' converted by packer from the remaster's ConstValues.cs (tag v0.9.1)\n         ' each is ConstValues.GetValue(id); the game divides by four\n",
    );
    let Some(start) = text.find("int[] values") else { return out };
    let body = &text[start..];
    let Some(end) = body.find("};") else { return out };
    let mut id = 0usize;
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(digits) = line.split(|c: char| !c.is_ascii_digit()).next() else { continue };
        if digits.is_empty() || !line.starts_with(digits) {
            continue;
        }
        let Ok(value) = digits.parse::<i32>() else { continue };
        // The comment after `//` names the item; keep it, trimmed of the
        // stray brace the original leaves on the end.
        let note = line.split("//").nth(1).unwrap_or("").trim_end_matches('}').trim();
        out.push_str(&format!("{id} = {value}"));
        if !note.is_empty() {
            out.push_str(&format!("   ' {note}"));
        }
        out.push('\n');
        id += 1;
    }
    out
}

fn main() {
    // `--from DIR` and `--out FILE`; the rest is what to pack.
    let mut from: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut with_newgfx = false;
    let mut rest: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--from" => from = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--newgfx" => with_newgfx = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => rest.push(a),
        }
    }
    let Some(dir) = from else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if !dir.is_dir() {
        eprintln!("packer: {} is not a directory", dir.display());
        std::process::exit(2);
    }
    src::root(&dir);
    let out_path = out.unwrap_or_else(|| PathBuf::from("burntime.pack"));
    let maps: Vec<String> = {
        let args: Vec<String> = rest;
        match args.split_first() {
            None => profile("minimal").expect("minimal is a profile"),
            // A single word that is not a map name is a profile name. Anything
            // else is the map list itself, as it always was.
            Some((first, rest)) if rest.is_empty() && !first.starts_with("MAT_") => {
                match profile(first) {
                    Some(maps) => {
                        println!("profile {first}: {maps:?}");
                        maps
                    }
                    None => {
                        let names: Vec<&str> = PROFILES.iter().map(|(n, _)| *n).collect();
                        println!("no profile called {first} — try one of {names:?}, or name maps");
                        return;
                    }
                }
            }
            Some(_) => args,
        }
    };

    let mut entries: Vec<Entry> = Vec::new();
    let mut wanted_tiles: BTreeSet<(u8, u8)> = BTreeSet::new();
    // The intro's tiles, for NewGfx's replacements of them.
    let mut intro_tiles: BTreeSet<(u8, u8)> = BTreeSet::new();
    // The `raum_N.pac` interiors the packed maps actually reach, so a two-map
    // pack does not carry all nine.
    let mut wanted_rooms: BTreeSet<u8> = BTreeSet::new();
    let mut missing: Vec<String> = Vec::new();
    let mut parsed_maps: Vec<(String, burngfx::map::Map)> = Vec::new();

    // The classic archive, opened once: the map loop needs each map's room
    // ids out of `mat_NNN.raw`, and the sections below need tiles and gam.dat.
    let pak_bytes = src::read(dir.join("burntime.pak")).ok();
    let classic = pak_bytes.as_ref().and_then(|b| burngfx::pak::ClassicPak::open(b).ok());
    if classic.is_none() {
        println!("no burntime.pak — rooms will fall back to the .txt files");
    }

    // Every map is parsed, not only the ones being packed: the walkability
    // grid of a location is part of the world rather than part of its
    // artwork, because that is where the generator drops things on the
    // ground. A pack with four maps and a pack with thirty eight must lay out
    // the same world.
    let all: Vec<String> = FULL.iter().map(|m| String::from(*m)).collect();

    for name in &all {
        let selected = maps.contains(name);
        let Ok(bytes) = src::read(dir.join(format!("maps/{name}.burnmap"))) else {
            if selected {
                panic!("{name}.burnmap is missing — see burngfx/testdata/README.md");
            }
            continue;
        };
        let parsed = map::parse(&bytes).expect("map should parse");

        if selected {
            for tile in &parsed.tiles {
                if let Some(pair) = tile.classic_name() {
                    wanted_tiles.insert(pair);
                }
            }

            let mut key = [0u8; NAME_LEN];
            let n = names::map(&mut key, name)
                .unwrap_or_else(|| panic!("{name}: name too long for the pack"));
            entries.push(Entry::new(&key[..n], bytes));
        }

        // Room settings are optional: only three maps ship one.
        // New Village's is the remaster's own file: v1.3's over v1.2's,
        // else the one beside the maps.
        let remaster_txt = (name == "MAT_038").then(|| Remaster::new(&dir).join(format!("maps/{name}.txt")));
        let settings_text = remaster_txt.and_then(|p| src::read_to_string(p).ok())
            .or_else(|| src::read_to_string(dir.join(format!("maps/{name}.txt"))).ok())
            .filter(|_| selected);
        if let Some(text) = settings_text {
            let settings = MapSettings::parse(&text);
            // The `.txt` overrides the interior where it has one, the same way
            // it overrides the title: `BurnmapProcessor` reads the classic
            // door first and lets the file correct it.
            wanted_rooms.extend(settings.rooms.iter().filter_map(|r| r.interior()));
            println!(
                "{name}: {}x{} tiles, {} entrances, {} rooms, entry_point {:?}",
                parsed.width,
                parsed.height,
                parsed.entrances.len(),
                settings.rooms.len(),
                settings.entry_point
            );
            let mut key = [0u8; NAME_LEN];
            let n = names::settings(&mut key, name)
                .unwrap_or_else(|| panic!("{name}: settings name too long for the pack"));
            entries.push(Entry::new(&key[..n], text.into_bytes()));
        } else if selected {
            println!("{name}: no .txt — rooms will have no names");
        }

        // A map's own tile sheet, when it has one.
        if selected {
            if let Some(sheet) = decode_png(&dir.join(format!("maps/{name}_tiles.png"))) {
                entries.push(Entry::new(format!("sheet/{name}").as_bytes(), sheet));
            }
        }

        parsed_maps.push((name.clone(), parsed));
    }

    // Tiles and walkability come from the classic archive when it is here.
    // That is the only source for sections 14 and 15, and the only source of
    // collision at all — the rendered PNGs carry no mask.
    let archive = src::read(dir.join("burntime.pak")).ok();
    let classic = archive.as_ref().and_then(|b| ClassicPak::open(b).ok());

    match &classic {
        Some(pak) => {
            for (name, parsed) in &parsed_maps {
                let selected = maps.contains(name);
                let map_id = map_id_of(name);
                // A map with no `mat_NNN.raw` (New Village) has no palette of
                // its own: its tiles are the remaster's rendered pictures,
                // packed by section and item; its walk grid comes from the
                // same masks as every other map's.
                let palette = pak
                    .get(&format!("burn_gfx/mat_{map_id:03}.raw"))
                    .and_then(Palette::from_mat_raw);
                if palette.is_none() {
                    println!("{name}: no mat_{map_id:03}.raw, its tiles from tiles/*.png");
                }

                // Follow BurnmapProcessor: every non-empty tile starts fully
                // walkable, then the masks narrow it. A tile the remaster drew
                // itself has no ZEI mask, and must not become a wall by
                // default — which is what skipping it did.
                let mut grid = WalkGrid::new(parsed.width, parsed.height);
                let sheet_mask = src::read_to_string(dir.join(format!("maps/{name}_tiles.txt")))
                    .ok()
                    .map(|t| t.lines().map(String::from).collect::<Vec<_>>());

                let mut sections: BTreeSet<u8> = BTreeSet::new();
                for t in &parsed.tiles {
                    if let Some((section, _)) = t.classic_name() {
                        sections.insert(section);
                    }
                }
                let sets: std::collections::BTreeMap<u8, TileSet> = sections
                    .iter()
                    .filter_map(|&s| {
                        let raw = pak.get(&format!("burn_gfx/zei_{s:03}.raw"))?;
                        Some((s, TileSet::parse(raw).ok()?))
                    })
                    .collect();

                for ty in 0..parsed.height {
                    for tx in 0..parsed.width {
                        let t = parsed.tile(tx, ty);
                        if t.is_empty() {
                            continue; // stays blocked
                        }
                        grid.fill_tile(tx, ty, true);

                        // A tile from the map's own sheet: its mask, if the map
                        // ships one, is four lines per item.
                        if let (Some(lines), true) = (&sheet_mask, t.custom_sheet_index().is_some())
                        {
                            grid.and_tile(tx, ty, mask_from_lines(lines, t.item as usize * 4));
                        }

                        if let Some((section, item)) = t.classic_name() {
                            // The remaster's hand-written corrections come
                            // first, then the file's own mask.
                            if let Ok(text) = src::read_to_string(
                                dir.join(format!("masks/{section:03}_{item:02}.txt")),
                            ) {
                                let lines: Vec<String> =
                                    text.lines().map(String::from).collect();
                                grid.and_tile(tx, ty, mask_from_lines(&lines, 0));
                            }

                            let Some(tile) = sets.get(&section).and_then(|s| s.tile(item as usize))
                            else {
                                missing.push(format!("{section:02}_{item:02}"));
                                continue;
                            };
                            grid.and_tile(tx, ty, tile.mask);

                            // Only a packed map needs its pictures; every
                            // map needs its mask, which is above this line.
                            if !selected {
                                continue;
                            }

                            let Some(palette) = palette.as_ref() else {
                                let mut key = [0u8; NAME_LEN];
                                let n = names::tile(&mut key, section, item).expect("fixed width");
                                if entries.iter().any(|e| e.name == key[..n]) {
                                    continue;
                                }
                                match decode_png(&dir.join(format!("tiles/{section:02}_{item:02}.png"))) {
                                    Some(data) => entries.push(Entry::new(&key[..n], data)),
                                    None => missing.push(format!("{section:02}_{item:02}")),
                                }
                                continue;
                            };
                            let mut key = [0u8; NAME_LEN];
                            let n = names::map_tile(&mut key, map_id, section, item)
                                .expect("tile names are fixed width");
                            let name_bytes = key[..n].to_vec();
                            if entries.iter().any(|e| e.name == name_bytes) {
                                continue;
                            }
                            let img = tile.to_rgba(palette);
                            let mut data = Vec::with_capacity(4 + img.rgba.len());
                            data.extend_from_slice(&(img.width as u16).to_le_bytes());
                            data.extend_from_slice(&(img.height as u16).to_le_bytes());
                            data.extend_from_slice(&img.rgba);
                            entries.push(Entry::new(&name_bytes, data));
                        }
                    }
                }

                // The original blocks the outer ring so nothing walks off.
                grid.close_the_edges();
                let mut data = Vec::with_capacity(4 + grid.width * grid.height);
                data.extend_from_slice(&(grid.width as u16).to_le_bytes());
                data.extend_from_slice(&(grid.height as u16).to_le_bytes());
                for y in 0..grid.height {
                    for x in 0..grid.width {
                        data.push(u8::from(grid.cell(x, y)));
                    }
                }
                if selected {
                    println!(
                        "{name}: walk grid {}x{}, {} of {} cells open, checksum {:08x}",
                        grid.width,
                        grid.height,
                        grid.open_cells(),
                        grid.width * grid.height,
                        grid.checksum()
                    );
                }

                // Every map's grid is packed, selected or not: it is where
                // the generator drops things, and where they lie must not
                // depend on which maps have pictures.
                let mut key = [0u8; NAME_LEN];
                let n = names::walk(&mut key, name).expect("walk names are fixed width");
                entries.push(Entry::new(&key[..n], data));
            }
        }
        None => {
            // Without the archive: no collision, and the sections with no
            // rendered PNG stay as gaps.
            println!("no burntime.pak — falling back to rendered PNG tiles, and no collision");
            for (section, item) in &wanted_tiles {
                match decode_png(&dir.join(format!("tiles/{section:02}_{item:02}.png"))) {
                    Some(data) => {
                        let mut key = [0u8; NAME_LEN];
                        let n = names::tile(&mut key, *section, *item).expect("fixed width");
                        entries.push(Entry::new(&key[..n], data));
                    }
                    None => missing.push(format!("{section:02}_{item:02}")),
                }
            }
        }
    }

    // The room list for **every** location, not only the packed ones.
    //
    // One byte a door, in door order; the game derives the room's kind, its
    // capacity and its picture from it. This is what lets a game be laid out
    // the same way whatever artwork the pack carries: the item generator
    // needs to know that Muarab has four rooms and one of them is a spring,
    // and that fact does not depend on whether Muarab's map was packed.
    // Thirty eight lists, one byte a door, under two hundred bytes in all.
    match &classic {
        Some(pak) => {
            let mut doors = 0;
            for id in 0..=37u32 {
                let raw_name = format!("burn_gfx/mat_{id:03}.raw");
                let Some(classic_map) = pak.get(&raw_name).and_then(|r| burngfx::matmap::parse(r).ok())
                else {
                    continue;
                };
                let ids: Vec<u8> = classic_map.doors.iter().map(|d| d.room_id).collect();
                doors += ids.len();

                // Only the packed maps need their interiors decoded.
                let name = format!("MAT_{id:03}");
                if maps.contains(&name) {
                    wanted_rooms.extend(classic_map.doors.iter().filter_map(|d| d.interior()));
                }

                let mut key = [0u8; NAME_LEN];
                let n = names::rooms(&mut key, &name)
                    .unwrap_or_else(|| panic!("{name}: rooms name too long for the pack"));
                entries.push(Entry::new(&key[..n], ids));
            }
            // New Village has no `mat_038.raw`: its rooms' kinds are its
            // `.txt`'s (`type = water` a spring, room id 4; every other a
            // normal room, id 0), one per door of its map, as
            // `Location.CreateRooms` makes them from the entrances.
            let nv = Remaster::new(&dir).join("maps/MAT_038.txt");
            let text = src::read_to_string(&nv).ok().or_else(|| src::read_to_string(dir.join("maps/MAT_038.txt")).ok());
            let doors_038 = parsed_maps.iter().find(|(n, _)| n == "MAT_038").map(|(_, m)| m.entrances.len());
            if let (Some(text), Some(n)) = (text, doors_038) {
                let settings = MapSettings::parse(&text);
                let ids: Vec<u8> = (0..n).map(|i| if settings.is_water_source(i) { 4 } else { 0 }).collect();
                doors += ids.len();
                let mut key = [0u8; NAME_LEN];
                let len = names::rooms(&mut key, "MAT_038").expect("short");
                entries.push(Entry::new(&key[..len], ids));
            }
            println!("rooms: {doors} doors across the whole world");
        }
        None => println!("no burntime.pak — the world has no room list"),
    }

    // The room interiors. A `.PAC` carries its own colour table, and five of
    // the nine use it for eight slots the map palettes do not have, so each is
    // decoded with its own rather than with the map's.
    match &classic {
        Some(pak) => {
            for &n in &wanted_rooms {
                match room_image(pak, n) {
                    Some(data) => {
                        let mut key = [0u8; NAME_LEN];
                        let len = names::room_image(&mut key, n).expect("room names are short");
                        entries.push(Entry::new(&key[..len], data));
                    }
                    None => println!("no burn_gfx/raum_{n}.pac — that room will have no interior"),
                }
            }
            println!("room interiors: {:?}", wanted_rooms);
        }
        None => println!("no burntime.pak — rooms will have no interiors"),
    }

    // The pictures behind the pub and the restaurant. Always all four: they
    // are small, and which one a door shows is decided by its room id, not by
    // which maps are packed.
    if let Some(pak) = &classic {
        // `sta.pac` is the start screen's own picture, and the menu is drawn
        // over it before any map is loaded.
        // **The start screen's picture, localised.** `sta.pac` is the German
        // release's and says "SPIELER I" and "SPIELER II"; v0.9.1 draws the
        // language's own captions (`@newburn?43/44`) only in its NewGfx look,
        // over a picture without words. So `scene/sta_en` is `sta.pac` with
        // the two captions' letters — palette index 227 inside the two
        // boxes, nothing else — filled with the stone around them, and the
        // game draws the captions on it. The archive is not touched.
        if let Some(bytes) = pak.get("burn_gfx/sta.pac") {
            if let (Ok(pac), true) = (burngfx::pac::decode(bytes), true) {
                if let Some((palette, _)) = burngfx::pac::colour_table(bytes, &pac) {
                    let w = burngfx::pac::WIDTH as usize;
                    let h = burngfx::pac::HEIGHT as usize;
                    let mut rgb: Vec<[u8; 3]> = pac.indices.iter().map(|&i| {
                        let c = palette.color(i);
                        [c.r, c.g, c.b]
                    }).collect();
                    let mut hole: Vec<bool> = vec![false; w * h];
                    let letter = |x: usize, y: usize| pac.indices[y * w + x] == 227;
                    for (x0, x1) in [(28usize, 104usize), (212, 300)] {
                        for y in 8..24 {
                            for x in x0..x1 {
                                // The letters, and their light drop shadow: a
                                // pale grey (palette 248–250) with a letter
                                // one pixel up, left or up-left of it.
                                let pale = matches!(pac.indices[y * w + x], 248..=250);
                                let beside_letter = letter(x - 1, y) || letter(x, y - 1) || letter(x - 1, y - 1);
                                if letter(x, y) || (pale && beside_letter) {
                                    hole[y * w + x] = true;
                                }
                            }
                        }
                    }
                    let holes = hole.iter().filter(|&&h| h).count();
                    // Fill from the edges inwards, each pixel taking one of its
                    // already-known neighbours — chosen by position, so the
                    // stone keeps its grain instead of smearing to an average.
                    loop {
                        let mut filled = Vec::new();
                        for y in 1..h - 1 {
                            for x in 1..w - 1 {
                                if !hole[y * w + x] {
                                    continue;
                                }
                                let known: Vec<usize> = [(0i32, -1i32), (-1, 0), (1, 0), (0, 1), (-1, -1), (1, 1), (1, -1), (-1, 1)]
                                    .iter()
                                    .map(|(dx, dy)| ((y as i32 + dy) as usize) * w + (x as i32 + dx) as usize)
                                    .filter(|&j| !hole[j])
                                    .collect();
                                if !known.is_empty() {
                                    filled.push((y * w + x, rgb[known[(x * 7 + y * 13) % known.len()]]));
                                }
                            }
                        }
                        if filled.is_empty() {
                            break;
                        }
                        for (i, c) in filled {
                            rgb[i] = c;
                            hole[i] = false;
                        }
                    }
                    let mut out = Vec::with_capacity(4 + w * h * 4);
                    out.extend_from_slice(&(w as u16).to_le_bytes());
                    out.extend_from_slice(&(h as u16).to_le_bytes());
                    for c in &rgb {
                        out.extend_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                    entries.push(Entry::new(b"scene/sta_en", out));
                    println!("sta_en: {holes} caption pixels filled");
                }
            }
        }
        for stem in burngfx::diner::BACKGROUNDS.iter().copied().chain(["sta", "hint2", "arzt", "info", "blz", "opti", "film_00", "film_01", "film_02", "film_03", "film_04", "film_05", "film_06", "film_08", "film_09", "film_10"]) {
            match pac_image(pak, &format!("burn_gfx/{stem}.pac")) {
                Some(data) => {
                    let mut key = [0u8; NAME_LEN];
                    let len = names::scene_image(&mut key, stem).expect("scene names are short");
                    entries.push(Entry::new(&key[..len], data));
                }
                None => println!("no burn_gfx/{stem}.pac — that scene will have no picture"),
            }
        }
    }

    // **The interface.** The original's sheets the screens draw from, frame
    // by frame, with the default colour table — `GetRawColorTable` has no
    // case for any of them — and the remaster's own pictures and fonts, from
    // `resources/game/classic` of the v0.9.1 tag, copied into
    // `testdata/classic` (see its README).
    if let (Some(pak), Some(palette)) = (&classic, classic.as_ref().and_then(item_palette)) {
        let sheets: &[(&str, &str, core::ops::Range<usize>)] = &[
            // The cursor (0), the HUD and the command menu.
            ("munt", "burn_gfx/munt.raw", 0..27),
            // Flags (0–15), player markers (16–19), the map's mode cursors
            // (20–27) and the roads (28–111).
            ("syst", "burn_gfx/syst.raw", 0..112),
            // The inventory: the trader's exchange frame (1), the right-hand
            // page (2) and the selection frame (3).
            ("inv", "burn_gfx/inv.raw", 0..4),
            // The start screen's colour radios (8, 9) and face covers (10–16).
            ("sta", "burn_gfx/sta.ani", 8..17),
        ];
        for (stem, path, frames) in sheets {
            let Some(raw) = pak.get(path) else {
                println!("no {path} — the interface will miss it");
                continue;
            };
            let sprites = burngfx::SpriteFile::parse(raw);
            let mut packed = 0;
            for frame in frames.clone() {
                let Some(img) = sprites.decode_frame(frame) else { continue };
                let rgba = img.to_rgba(&palette);
                let mut data = Vec::with_capacity(4 + rgba.rgba.len());
                data.extend_from_slice(&(rgba.width as u16).to_le_bytes());
                data.extend_from_slice(&(rgba.height as u16).to_le_bytes());
                data.extend_from_slice(&rgba.rgba);
                let mut key = [0u8; NAME_LEN];
                let n = names::frame(&mut key, stem, frame as u8).expect("frame names are short");
                entries.push(Entry::new(&key[..n], data));
                packed += 1;
            }
            println!("{stem}: {packed} frame(s)");
        }
        // **The faces**, all thirty `ges_NN.ani`. Frame 0 is the face; the
        // other thirty-nine are what changes over it, drawn progressively,
        // so each is packed cut to what it covers, with where it goes in
        // `fo/ges_NN` — two bytes a frame. Whole, they would be 18 MB.
        let mut faces = 0;
        for face in 0..30u8 {
            let stem = format!("ges_{face:02}");
            let Some(raw) = pak.get(&format!("burn_gfx/{stem}.ani")) else { continue };
            let sprites = burngfx::SpriteFile::parse(raw);
            let mut offsets = Vec::new();
            for frame in 0..sprites.frame_count().min(40) {
                let Some(img) = sprites.decode_frame(frame) else { break };
                let rgba = img.to_rgba(&palette);
                let (w, h) = (rgba.width as usize, rgba.height as usize);
                let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
                for y in 0..h {
                    for x in 0..w {
                        if rgba.rgba[(y * w + x) * 4 + 3] != 0 {
                            x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1);
                        }
                    }
                }
                if frame == 0 || x1 <= x0 {
                    (x0, y0, x1, y1) = if frame == 0 { (0, 0, w, h) } else { (0, 0, 1, 1) };
                }
                let mut data = Vec::with_capacity(4 + (x1 - x0) * (y1 - y0) * 4);
                data.extend_from_slice(&((x1 - x0) as u16).to_le_bytes());
                data.extend_from_slice(&((y1 - y0) as u16).to_le_bytes());
                for y in y0..y1 {
                    data.extend_from_slice(&rgba.rgba[(y * w + x0) * 4..(y * w + x1) * 4]);
                }
                let mut key = [0u8; NAME_LEN];
                let n = names::frame(&mut key, &stem, frame as u8).expect("frame names are short");
                entries.push(Entry::new(&key[..n], data));
                offsets.extend_from_slice(&[x0 as u8, y0 as u8]);
            }
            entries.push(Entry::new(format!("fo/{stem}").as_bytes(), offsets));
            faces += 1;
        }
        println!("faces: {faces} of 30");
    }
    // **The services' animations**, as `SpriteLoaderAni` renders a `?p`
    // resource: each frame laid over the one before, so every packed frame is
    // whole. Colours are the scene picture's own (`GetRawColorTable`: arzt,
    // bar, pub1, koch). `an/<track>/<n>`.
    if let Some(pak) = &classic {
        // The palette is the named `.pac`'s, or map 0's (`""`), as
        // `GetRawColorTable` has it: `opt.ani` and `opta.raw` take `opti.pac`'s,
        // `inf.ani` has no case and takes the default.
        let tracks: &[(&str, &str, &str, core::ops::Range<usize>, bool)] = &[
            ("inf_a", "burn_gfx/inf.ani", "", 0..3, true),
            ("inf_b", "burn_gfx/inf.ani", "", 4..7, false),
            ("inf_rad", "burn_gfx/inf.ani", "", 3..4, false),
            ("inf_gas", "burn_gfx/inf.ani", "", 7..8, false),
            ("opt", "burn_gfx/opt.ani", "burn_gfx/opti.pac", 0..64, true),
            ("opta0", "burn_gfx/opta.raw", "burn_gfx/opti.pac", 0..1, false),
            ("opta1", "burn_gfx/opta.raw", "burn_gfx/opti.pac", 1..2, false),
            ("arzt", "burn_gfx/arzt.ani", "burn_gfx/arzt.pac", 0..6, true),
            ("bar", "burn_gfx/bar.ani", "burn_gfx/bar.pac", 0..34, true),
            ("pub1a", "burn_gfx/pub1.ani", "burn_gfx/pub1.pac", 0..12, true),
            ("pub1b", "burn_gfx/pub1.ani", "burn_gfx/pub1.pac", 12..14, false),
            ("pub1c", "burn_gfx/pub1.ani", "burn_gfx/pub1.pac", 14..65, true),
            ("koch", "burn_gfx/koch.ani", "burn_gfx/koch.pac", 0..12, true),
            // `DeathScene` and `VictoryScene`: `film_00.ani?0-10?p` and so
            // on, over `film_00.pac` / `film_01.pac`, in their own colours.
            ("film00a", "burn_gfx/film_00.ani", "burn_gfx/film_00.pac", 0..11, true),
            ("film00b", "burn_gfx/film_00.ani", "burn_gfx/film_00.pac", 11..22, true),
            ("film00c", "burn_gfx/film_00.ani", "burn_gfx/film_00.pac", 22..33, true),
            ("film00bird", "burn_gfx/film_00.ani", "burn_gfx/film_00.pac", 33..73, true),
            ("film01a", "burn_gfx/film_01.ani", "burn_gfx/film_01.pac", 0..22, true),
            ("film01b", "burn_gfx/film_01.ani", "burn_gfx/film_01.pac", 22..24, true),
            ("film01c", "burn_gfx/film_01.ani", "burn_gfx/film_01.pac", 24..28, true),
            // The rooms' films (`ImageScene`, `ChurchScene`): the church's
            // `film_08.ani??p`, every frame; the traders' and the pub's.
            ("film08", "burn_gfx/film_08.ani", "burn_gfx/film_08.pac", 0..200, true),
            ("film05a", "burn_gfx/film_05.ani", "burn_gfx/film_05.pac", 0..18, true),
            ("film05b", "burn_gfx/film_05.ani", "burn_gfx/film_05.pac", 18..20, false),
            ("film05c", "burn_gfx/film_05.ani", "burn_gfx/film_05.pac", 20..22, false),
            ("film06a", "burn_gfx/film_06.ani", "burn_gfx/film_06.pac", 0..6, false),
            ("film06b", "burn_gfx/film_06.ani", "burn_gfx/film_06.pac", 6..16, false),
            ("film10", "burn_gfx/film_10.ani", "burn_gfx/film_10.pac", 0..200, false),
        ];
        // Drawn over their first frame, as `Animation.Progressive` (true
        // unless a scene says otherwise) draws a plain `.ani` range: the
        // pub's two and the trader's `film_10`.
        let over_first: &[&str] = &["film06a", "film06b", "film10"];
        let mut made = 0;
        for (track, ani, pac_path, frames, progressive) in tracks {
            let Some(raw) = pak.get(ani) else { continue };
            let palette = if pac_path.is_empty() {
                let Some(p) = item_palette(pak) else { continue };
                p
            } else {
                let Some(pac_bytes) = pak.get(pac_path) else { continue };
                let Ok(pac) = burngfx::pac::decode(pac_bytes) else { continue };
                let Some((p, _)) = burngfx::pac::colour_table(pac_bytes, &pac) else { continue };
                p
            };
            let sprites = burngfx::SpriteFile::parse(raw);
            let mut canvas: Option<(usize, usize, Vec<u8>)> = None;
            for (n, frame) in frames.clone().enumerate() {
                let Some(img) = sprites.decode_frame(frame) else { break };
                let mut rgba = img.to_rgba(&palette);
                // Index 0 is what an unwritten pixel holds, and transparent —
                // the scene's own table would make it opaque black, which is
                // right for the scene's picture and wrong for a frame over it.
                for (i, &index) in img.indices.iter().enumerate() {
                    if index == 0 {
                        rgba.rgba[i * 4 + 3] = 0;
                    }
                }
                let (w, h) = (rgba.width as usize, rgba.height as usize);
                if over_first.contains(track) {
                    if let Some((cw, ch, base)) = &canvas {
                        let mut buf = base.clone();
                        for y in 0..h.min(*ch) {
                            for x in 0..w.min(*cw) {
                                let s = (y * w + x) * 4;
                                if rgba.rgba[s + 3] != 0 {
                                    buf[(y * *cw + x) * 4..(y * *cw + x) * 4 + 4].copy_from_slice(&rgba.rgba[s..s + 4]);
                                }
                            }
                        }
                        let mut data = Vec::with_capacity(4 + buf.len());
                        data.extend_from_slice(&(*cw as u16).to_le_bytes());
                        data.extend_from_slice(&(*ch as u16).to_le_bytes());
                        data.extend_from_slice(&buf);
                        entries.push(Entry::new(format!("an/{track}/{n}").as_bytes(), data));
                        made += 1;
                        continue;
                    }
                }
                let composed = match (&mut canvas, progressive) {
                    (Some((cw, ch, buf)), true) => {
                        for y in 0..h.min(*ch) {
                            for x in 0..w.min(*cw) {
                                let s = (y * w + x) * 4;
                                if rgba.rgba[s + 3] != 0 {
                                    buf[(y * *cw + x) * 4..(y * *cw + x) * 4 + 4].copy_from_slice(&rgba.rgba[s..s + 4]);
                                }
                            }
                        }
                        (*cw, *ch, buf.clone())
                    }
                    _ => {
                        canvas = Some((w, h, rgba.rgba.clone()));
                        (w, h, rgba.rgba.clone())
                    }
                };
                let mut data = Vec::with_capacity(4 + composed.2.len());
                data.extend_from_slice(&(composed.0 as u16).to_le_bytes());
                data.extend_from_slice(&(composed.1 as u16).to_le_bytes());
                data.extend_from_slice(&composed.2);
                entries.push(Entry::new(format!("an/{track}/{n}").as_bytes(), data));
                made += 1;
            }
        }
        println!("service animations: {made} frames");
    }
    // **The intro** (`IntroScene`, before the start screen on every start):
    // `intro.txt` as it is, then what it names — each page's 1993 map, its
    // tiles in the colours that map's tiles take, and the frames of the
    // `introN.ani` ranges in theirs (`burngfx::intro`). The maps are in the
    // archive but for `mat_090.raw`, which the remaster ships beside it
    // (`resources/game/classic/MAT_090.RAW`).
    match (&classic, src::read_to_string(dir.join("v1.2/intro.txt"))) {
        (Some(pak), Ok(text)) => match burngfx::intro::parse(&text) {
            Some(script) => {
                let palette = |map: u8| pak.get(&format!("burn_gfx/mat_{map:03}.raw")).and_then(Palette::from_mat_raw);
                let mut tiles = 0;
                let mut maps: Vec<u8> = script.pages.iter().map(|p| p.map).collect();
                maps.sort();
                maps.dedup();
                for map in maps {
                    let raw = pak
                        .get(&format!("burn_gfx/mat_{map:03}.raw"))
                        .map(|b| b.to_vec())
                        .or_else(|| src::read(dir.join(format!("MAT_{map:03}.RAW"))).ok());
                    let Some((w, h, cells)) = raw.as_deref().and_then(|b| burngfx::matmap::tiles(b).ok()) else {
                        println!("intro: no mat_{map:03}.raw — that page will be black");
                        continue;
                    };
                    let colours = burngfx::intro::palette_of_map(map);
                    let Some(pal) = palette(colours) else {
                        println!("intro: no mat_{colours:03}.raw for map {map}'s colours");
                        continue;
                    };
                    let mut data = Vec::with_capacity(4 + cells.len() * 2);
                    data.extend_from_slice(&(w as u16).to_le_bytes());
                    data.extend_from_slice(&(h as u16).to_le_bytes());
                    let mut sets: std::collections::BTreeMap<u8, Option<TileSet>> = std::collections::BTreeMap::new();
                    for &(section, item) in &cells {
                        data.extend_from_slice(&[section, item]);
                        let set = sets.entry(section).or_insert_with(|| {
                            TileSet::parse(pak.get(&format!("burn_gfx/zei_{section:03}.raw"))?).ok()
                        });
                        let Some(tile) = set.as_ref().and_then(|s| s.tile(item as usize)) else {
                            missing.push(format!("{section:02}_{item:02}"));
                            continue;
                        };
                        // `ClassicMapProcessor` asks for each tile as a sprite,
                        // so NewGfx replaces the intro's tiles too.
                        intro_tiles.insert((section, item));
                        let mut key = [0u8; NAME_LEN];
                        let n = names::map_tile(&mut key, colours, section, item).expect("tile names are fixed width");
                        if entries.iter().any(|e| e.name == key[..n]) {
                            continue;
                        }
                        let img = tile.to_rgba(&pal);
                        let mut px = Vec::with_capacity(4 + img.rgba.len());
                        px.extend_from_slice(&(img.width as u16).to_le_bytes());
                        px.extend_from_slice(&(img.height as u16).to_le_bytes());
                        px.extend_from_slice(&img.rgba);
                        entries.push(Entry::new(&key[..n], px));
                        tiles += 1;
                    }
                    println!("intro: map {map}, {w}x{h} tiles in map {colours}'s colours");
                    entries.push(Entry::new(burngfx::intro::map_key(map).as_bytes(), data));
                }
                let mut frames = 0;
                let mut wanted: Vec<(String, u8)> = Vec::new();
                for a in &script.animations {
                    for f in a.first..=a.last {
                        if !wanted.contains(&(a.stem.clone(), f)) {
                            wanted.push((a.stem.clone(), f));
                        }
                    }
                }
                for (stem, frame) in wanted {
                    let colours = burngfx::intro::palette_of_ani(&stem);
                    let decoded = pak.get(&format!("burn_gfx/{stem}.ani")).and_then(|raw| {
                        let img = burngfx::SpriteFile::parse(raw).decode_frame(frame as usize)?;
                        Some(img.to_rgba(&palette(colours)?))
                    });
                    let Some(img) = decoded else {
                        println!("intro: no frame {frame} of {stem}.ani");
                        continue;
                    };
                    let mut px = Vec::with_capacity(4 + img.rgba.len());
                    px.extend_from_slice(&(img.width as u16).to_le_bytes());
                    px.extend_from_slice(&(img.height as u16).to_le_bytes());
                    px.extend_from_slice(&img.rgba);
                    entries.push(Entry::new(burngfx::intro::frame_key(&stem, frame).as_bytes(), px));
                    frames += 1;
                }
                entries.push(Entry::new(b"v12/intro", text.into_bytes()));
                println!("intro: {} pages, {tiles} tiles, {frames} animation frames", script.pages.len());
            }
            None => println!("intro: v1.2/intro.txt does not read as the intro — the game starts at the start screen"),
        },
        (None, _) => println!("no burntime.pak — no intro"),
        (_, Err(_)) => println!("intro: no v1.2/intro.txt — the game starts at the start screen"),
    }
    let remaster = dir.join("classic");
    let mut ui = 0;
    for (key, path) in names::UI_IMAGES {
        match decode_png(&remaster.join(path)) {
            Some(data) => {
                entries.push(Entry::new(key.as_bytes(), data));
                ui += 1;
            }
            None => println!("no classic/{path} — the start screen will miss it"),
        }
    }
    for (key, path) in names::FONTS {
        let Ok(text) = src::read(remaster.join(path)) else {
            println!("no classic/{path}");
            continue;
        };
        let parsed = burngfx::font::Font::parse(&String::from_utf8_lossy(&text));
        let Some(font) = parsed else {
            println!("classic/{path} does not parse as a font");
            continue;
        };
        let strip_path = remaster.join(&font.image);
        let Some(strip) = decode_png(&strip_path) else {
            println!("no {} for {path}", strip_path.display());
            continue;
        };
        entries.push(Entry::new(key.as_bytes(), text));
        entries.push(Entry::new(format!("{key}.strip").as_bytes(), strip));
        ui += 1;
    }
    for (key, path) in [("text/newburn.txt", "lang/en/newburn.txt"), ("text/conversion", "conversion_table.txt"),
                        ("rules/system", "settings.txt")] {
        if let Ok(bytes) = src::read(remaster.join(path)) {
            entries.push(Entry::new(key.as_bytes(), bytes));
            ui += 1;
        }
    }
    // What people say: `lang/en/MEN_NNN.TXT`, keyed `men/NNN`.
    let mut men = 0;
    if let Ok(dir) = std::fs::read_dir(remaster.join("lang/en")) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(n) = name.strip_prefix("MEN_").and_then(|r| r.strip_suffix(".TXT")) else { continue };
            if let Ok(bytes) = src::read(entry.path()) {
                entries.push(Entry::new(format!("men/{n}").as_bytes(), bytes));
                men += 1;
            }
        }
    }
    println!("conversations: {men} MEN files");
    println!("interface: {ui} of {} pictures, fonts and tables",
             names::UI_IMAGES.len() + names::FONTS.len() + 3);

    // The item pictures. `gst.raw` is the same format as the character
    // sprites, and reads with the same decoder — the one checked frame by
    // frame against the remaster's own renders. Its colours are map 0's:
    // `GetRawColorTable` has no case for gst.raw, so it gets the default,
    // and the default is `map0`.
    match &classic {
        Some(pak) => match (pak.get("burn_gfx/gst.raw"), item_palette(pak)) {
            (Some(raw), Some(palette)) => {
                let sprites = burngfx::SpriteFile::parse(raw);
                let mut packed = 0;
                for frame in 0..sprites.frame_count() {
                    let Some(img) = sprites.decode_frame(frame) else { continue };
                    let rgba = img.to_rgba(&palette);
                    let mut data = Vec::with_capacity(4 + rgba.rgba.len());
                    data.extend_from_slice(&(rgba.width as u16).to_le_bytes());
                    data.extend_from_slice(&(rgba.height as u16).to_le_bytes());
                    data.extend_from_slice(&rgba.rgba);

                    let mut key = [0u8; NAME_LEN];
                    let n = names::item_sprite(&mut key, frame as u8)
                        .expect("item names are short");
                    entries.push(Entry::new(&key[..n], data));
                    packed += 1;
                }
                println!("items: {packed} of {} frames from gst.raw", sprites.frame_count());
            }
            _ => println!("no gst.raw or no map 0 palette — items will have no pictures"),
        },
        None => println!("no burntime.pak — items will have no pictures"),
    }

    // The characters themselves. `syssze.raw` is the same format and the
    // same default palette as the items, and it is the one file that turns
    // the coloured posts on the map into people: sixteen frames per body,
    // starting at a record's `SpriteId` — idle, then two frames each for
    // right, left, down and up, then four of dying. The last six frames of
    // the file belong to no body: they are the spark drawn on both
    // fighters when a blow lands.
    match &classic {
        Some(pak) => match (pak.get("burn_gfx/syssze.raw"), item_palette(pak)) {
            (Some(raw), Some(palette)) => {
                let sprites = burngfx::SpriteFile::parse(raw);
                let mut packed = 0;
                for frame in 0..sprites.frame_count().min(256) {
                    let Some(img) = sprites.decode_frame(frame) else { continue };
                    let rgba = img.to_rgba(&palette);
                    let mut data = Vec::with_capacity(4 + rgba.rgba.len());
                    data.extend_from_slice(&(rgba.width as u16).to_le_bytes());
                    data.extend_from_slice(&(rgba.height as u16).to_le_bytes());
                    data.extend_from_slice(&rgba.rgba);

                    let mut key = [0u8; NAME_LEN];
                    let n = names::body_sprite(&mut key, frame as u8)
                        .expect("body names are short");
                    entries.push(Entry::new(&key[..n], data));
                    packed += 1;
                }
                println!("bodies: {packed} of {} frames from syssze.raw", sprites.frame_count());
            }
            _ => println!("no syssze.raw or no map 0 palette — people will have no bodies"),
        },
        None => println!("no burntime.pak — people will have no bodies"),
    }

    // The font: metrics as text, pixels as RGBA.
    let font_txt = src::read(dir.join("font.txt")).expect("font.txt");
    entries.push(Entry::new(b"font.txt", font_txt));
    let strip = decode_png(&dir.join("font.png")).expect("font.png");
    entries.push(Entry::new(b"font.strip", strip));

    // The classic save, straight out of the remaster's archive. It carries the
    // real entry points and the road graph, which nothing else has.
    match classic.as_ref().and_then(|pak| pak.get("burn_gfx/gam.dat")) {
        Some(gamdat) => {
            let parsed = burngfx::gamdat::parse(gamdat).expect("gam.dat should parse");
            println!("gam.dat: {} locations, day {}", parsed.locations.len(), parsed.day);
            entries.push(Entry::new(b"classic/gam.dat", gamdat.to_vec()));
        }
        // Optional: without it the game falls back to the middle of the map
        // and says so.
        None => println!("no gam.dat — entry points will fall back to map centres"),
    }

    // The rules, on both axes.
    //
    // A mode picks the item table; a release picks the settings file — and
    // v0.9.1 has one settings file for both modes where v1.1 has
    // two. Packing them under keys that name both is what stops a game being
    // assembled from one release's code and another's numbers, which is
    // exactly what happened here once.
    for mode in [burngfx::game::Mode::Original, burngfx::game::Mode::Extended] {
        match src::read(dir.join(mode.items_file())) {
            Ok(bytes) => {
                println!("items ({}): {} ({} bytes)", mode.name(), mode.items_file(), bytes.len());
                let mut key = [0u8; NAME_LEN];
                let n = names::rules_items(&mut key, mode.name()).expect("mode names are short");
                entries.push(Entry::new(&key[..n], bytes));
            }
            Err(_) => println!("items ({}): no {}", mode.name(), mode.items_file()),
        }
    }

    for engine in [burngfx::game::Engine::V091, burngfx::game::Engine::Current] {
        let mut packed: Vec<&str> = Vec::new();
        for mode in [burngfx::game::Mode::Original, burngfx::game::Mode::Extended] {
            let file = engine.settings_file(mode);
            // v0.9.1 names the same file for both modes; pack it once and
            // key it for each, so the game asks the same question either way.
            let Ok(bytes) = src::read(dir.join(file)) else {
                println!("settings ({} {}): no {file}", engine.name(), mode.name());
                continue;
            };
            let mut key = [0u8; NAME_LEN];
            let n = names::rules_settings(&mut key, engine.name(), mode.name())
                .expect("rule names are short");
            entries.push(Entry::new(&key[..n], bytes));
            packed.push(file);
        }
        if !packed.is_empty() {
            println!("settings ({}): {packed:?}", engine.name());
        }
    }

    // **v1.2's presets**, from `testdata/v1.2/rules/<dir>/`, taken at the tag
    // (`testdata/v1.2/SOURCE` says which commit). Each distinct file once,
    // checked the way the game will read it: a `game.txt` that needs a
    // fallback, or an item table the game would refuse, is reported here
    // rather than discovered when a game starts. A preset missing any of its
    // four files is named, and the game refuses to start it.
    {
        let v12 = Remaster::new(&dir);
        match src::read_to_string(dir.join("v1.2/SOURCE")) {
            Ok(source) => {
                println!("v1.2 presets from {}", source.trim());
                entries.push(Entry::new(b"v12/source", source.into_bytes()));
            }
            Err(_) => println!("v1.2 presets: no SOURCE — say which tag the files were taken from"),
        }
        // v1.3's files over v1.2's (`v1.3/`):
        // what the game plays is v1.3, and the pack says so.
        match src::read_to_string(dir.join("v1.3/SOURCE")) {
            Ok(source) => {
                println!("v1.3 over v1.2 from {}", source.trim());
                entries.push(Entry::new(b"v13/source", source.into_bytes()));
            }
            Err(_) => println!("v1.3: no v1.3/SOURCE — the pack holds v1.2's rules, which this version does not play"),
        }
        let mut packed: BTreeSet<(&str, &str)> = BTreeSet::new();
        for set in burngfx::rules::RuleSet::ALL {
            let mut complete = true;
            for (d, kind) in set.pack_entries() {
                if packed.contains(&(d, kind)) {
                    continue;
                }
                let path = v12.join(format!("rules/{d}/{kind}.txt"));
                let Ok(text) = src::read_to_string(&path) else {
                    println!("v1.2 {}: no {}", set.name(), path.display());
                    complete = false;
                    continue;
                };
                match kind {
                    "game" => {
                        let book = burngfx::rules::RuleBook::parse(&text);
                        if !book.warnings.is_empty() {
                            println!("v1.2 {}: game.txt needs fallbacks: {:?}", set.name(), book.warnings);
                        }
                    }
                    "items" => {
                        if let Err(e) = burngfx::items::ItemTypes::parse_v12(&text) {
                            println!("v1.2 {}: items.txt refused: {e:?}", set.name());
                            complete = false;
                            continue;
                        }
                    }
                    _ => {}
                }
                let mut key = [0u8; NAME_LEN];
                let n = names::v12(&mut key, d, kind).expect("v1.2 names are short");
                entries.push(Entry::new(&key[..n], text.into_bytes()));
                packed.insert((d, kind));
            }
            println!("v1.2 {}: {}", set.name(), if complete { "complete" } else { "INCOMPLETE" });
        }
        // v1.2's own lines that the 1993 text has no place for: the last
        // rival's offer (`LastRivalDialog`, `last_rival.txt`).
        match src::read(v12.join("lang/en/last_rival.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/last_rival", text)),
            Err(_) => println!("v1.2: no lang/en/last_rival.txt — the last rival's offer will be unworded"),
        }
        // v1.2's own `newburn.txt`, for the lines v0.9.1's has not got (the
        // report's "Okay", `newburn?45`).
        for (key, path) in names::V12_UI_IMAGES {
            match decode_png(&v12.join(path)) {
                Some(data) => entries.push(Entry::new(key.as_bytes(), data)),
                None => println!("no v1.2/{path} — the game will draw without it"),
            }
        }
        for (key, path) in names::V13_FLAGS {
            match decode_png(&dir.join("v1.3").join(path)) {
                Some(data) => entries.push(Entry::new(key.as_bytes(), data)),
                None => println!("no v1.3/{path} — the start screen has no picture for that colour"),
            }
        }
        match src::read(v12.join("lang/en/newburn.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/newburn", text)),
            Err(_) => println!("v1.2: no lang/en/newburn.txt"),
        }
        // The start screen's automatic names (`MenuScene._playerNames`).
        match src::read(v12.join("lang/en/names.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/names", text)),
            Err(_) => println!("v1.2: no lang/en/names.txt — the start screen's automatic name will be Max"),
        }
        // The Modern opponent's policy (`AiPolicy`, `ai.txt`): one file for
        // every preset, refused here as the game would refuse it.
        match src::read_to_string(v12.join("rules/classic/ai.txt")) {
            Ok(text) => match burngfx::modern_ai::policy::AiConfig::parse(&text) {
                Ok(_) => entries.push(Entry::new(b"v12/ai", text.into_bytes())),
                Err(e) => println!("v1.2: ai.txt refused: {e:?} — the Modern opponent cannot play"),
            },
            Err(_) => println!("v1.2: no rules/classic/ai.txt — the Modern opponent cannot play"),
        }
        // v1.2's own `BURN.TXT` and conversations: its item names capitalised,
        // some lines reworded; a v1.2 game reads these (`v12/burn`,
        // `v12/men/NNN`) and v0.9.1's where it has none.
        match src::read(v12.join("lang/en/BURN.TXT")) {
            Ok(text) => entries.push(Entry::new(b"v12/burn", text)),
            Err(_) => println!("v1.2: no lang/en/BURN.TXT — a v1.2 game will read v0.9.1's"),
        }
        let mut men12 = 0;
        {
            for name in v12.names("lang/en") {
                let Some(n) = name.strip_prefix("MEN_").and_then(|r| r.strip_suffix(".TXT")) else { continue };
                if let Ok(bytes) = src::read(v12.join(&format!("lang/en/{name}"))) {
                    entries.push(Entry::new(format!("v12/men/{n}").as_bytes(), bytes));
                    men12 += 1;
                }
            }
        }
        println!("v1.2 conversations: {men12} MEN files");
        // The items' tooltips (`ItemGridTooltip`).
        match src::read(v12.join("lang/en/tooltip.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/tooltip", text)),
            Err(_) => println!("v1.2: no lang/en/tooltip.txt — the items will have no tooltips"),
        }
        // The city's report (`MapScene.ShowCityReport`).
        match src::read(v12.join("lang/en/cityinfo.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/cityinfo", text)),
            Err(_) => println!("v1.2: no lang/en/cityinfo.txt — the city's report will be unworded"),
        }
        // v1.2's recipes (`GameCreation.cs:114`): the radio a `main_items`
        // recipe, the mine detector gated by `requires_function`. A v1.2
        // game reads these; a legacy game keeps v0.9.1's `rules/constructions`.
        match src::read(v12.join("construction.txt")) {
            Ok(bytes) => {
                let table = burngfx::construction::Constructions::parse(&String::from_utf8_lossy(&bytes));
                println!("v1.2 constructions: {}", table.len());
                entries.push(Entry::new(b"v12/constructions", bytes));
            }
            Err(_) => println!("v1.2: no construction.txt — a v1.2 game will build by v0.9.1's recipes"),
        }
        // v1.2's `settings.txt` (`[system]` with `music_volume`,
        // `[keyboard]`, `[gamepad]`, its `[debug]` lists): a v1.2 game
        // reads it, the legacy game v0.9.1's `rules/system`.
        match src::read(v12.join("settings.txt")) {
            Ok(bytes) => entries.push(Entry::new(b"v12/system", bytes)),
            Err(_) => println!("v1.2: no settings.txt — a v1.2 game will read v0.9.1's"),
        }
        for (key, path) in names::V12_TEXTS {
            match src::read(v12.join(path)) {
                Ok(text) => entries.push(Entry::new(key.as_bytes(), text)),
                Err(_) => println!("v1.2: no {path} — {key} will be missing"),
            }
        }
        {
            let (key, path) = names::V12_SMALL_FONT;
            let font = src::read(v12.join(path)).ok().and_then(|text| {
                let parsed = burngfx::font::Font::parse(&String::from_utf8_lossy(&text))?;
                Some((text, decode_png(&v12.join(&parsed.image))?))
            });
            match font {
                Some((text, strip)) => {
                    entries.push(Entry::new(key.as_bytes(), text));
                    entries.push(Entry::new(format!("{key}.strip").as_bytes(), strip));
                }
                None => println!("v1.2: no {path} or its image — the manual and Options will use the main font"),
            }
        }
        // v1.2's input prompts: the glyph sheets and their words.
        for (key, path) in names::V12_PROMPT_GLYPHS {
            match decode_png(&v12.join(path)) {
                Some(data) => entries.push(Entry::new(key.as_bytes(), data)),
                None => println!("v1.2: no {path} — the prompts will have no glyphs"),
            }
        }
        for (key, path) in names::V12_PROMPT_TEXTS {
            match src::read(v12.join(path)) {
                Ok(text) => entries.push(Entry::new(key.as_bytes(), text)),
                Err(_) => println!("v1.2: no {path} — {key} will be missing"),
            }
        }
        // And the two-way radio's report (`MapScene.ShowRadioReport`).
        match src::read(v12.join("lang/en/radio.txt")) {
            Ok(text) => entries.push(Entry::new(b"v12/radio", text)),
            Err(_) => println!("v1.2: no lang/en/radio.txt — the radio's report will be unworded"),
        }
        // **German** (`OptionsSettingsPage`'s language line): v1.2's
        // `classic_de` package from the player's own v1.2 copy
        // (`resources/game/classic_de/lang/de`), the same files as
        // `lang/en` in German. Nothing here when the copy has none: the
        // language line then changes nothing.
        let de = names::V12_GERMAN_DIR;
        let mut german = 0;
        for (key, file) in names::V12_GERMAN {
            if let Ok(text) = src::read(v12.join(&format!("{de}/{file}"))) {
                entries.push(Entry::new(key.as_bytes(), text));
                german += 1;
            }
        }
        {
            for name in v12.names(de) {
                // `-MEN_040.TXT` is not one of them: only `MEN_NNN.TXT`.
                let Some(n) = name.strip_prefix("MEN_").and_then(|r| r.strip_suffix(".TXT")) else { continue };
                if let Ok(bytes) = src::read(v12.join(&format!("{de}/{name}"))) {
                    entries.push(Entry::new(format!("v12/de/men/{n}").as_bytes(), bytes));
                    german += 1;
                }
            }
        }
        println!("v1.2 German: {german} files from {}", names::V12_GERMAN_DIR);
    }

    // What a camp can make. One file for both modes, and the same in both
    // releases — the axes do not reach it.
    match src::read(dir.join("production.txt")) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let productions = burngfx::production::Productions::parse(&text);
            println!(
                "productions: {} ({:?})",
                productions.len(),
                productions.iter().map(|p| p.produce.as_str()).collect::<Vec<_>>()
            );
            entries.push(Entry::new(b"rules/productions", bytes));
        }
        Err(_) => println!("no production.txt — camps will make nothing"),
    }

    // What can be built out of what. One file for both modes; the two tags
    // differ in it by two lines, and whichever file is here decides.
    match src::read(dir.join("construction.txt")) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let constructions = burngfx::construction::Constructions::parse(&text);
            let unreachable = constructions.iter().filter(|r| r.main_items.is_empty()).count();
            println!(
                "constructions: {} ({:?}){}",
                constructions.len(),
                constructions.iter().map(|r| r.result.as_str()).collect::<Vec<_>>(),
                if unreachable > 0 {
                    format!(", {unreachable} with no main item and so unreachable")
                } else {
                    String::new()
                }
            );
            entries.push(Entry::new(b"rules/constructions", bytes));
        }
        Err(_) => println!("no construction.txt — nothing can be built"),
    }

    // Who restocks from what. Keyed by a trader's `NameId`, which is how
    // `LoadTrader` looks it up.
    match src::read(dir.join("trader.txt")) {
        Ok(bytes) => {
            let lists = burngfx::trader::StockLists::parse(&String::from_utf8_lossy(&bytes));
            println!("traders: {} stock list(s)", lists.len());
            entries.push(Entry::new(b"rules/traders", bytes));
        }
        Err(_) => println!("no trader.txt — traders will have nothing to restock from"),
    }

    // What things are worth in a barter.
    //
    // **Not from `items.txt`**: `ItemTypesData` builds a classic item's
    // `TradeValue` as `ConstValues.GetValue(burngfx) / 4`, from a table in
    // the remaster's *source* indexed by the DOS sprite id. So the packer
    // reads that file and emits the numbers as data, and the provenance
    // goes in the pack beside them rather than into folklore.
    match src::read_to_string(dir.join("ConstValues.cs")) {
        Ok(text) => {
            let prices = prices_from_const_values(&text);
            let parsed = burngfx::trade::Prices::parse(&prices);
            let n = prices.lines().filter(|l| l.contains('=')).count();
            println!("prices: {n} from ConstValues.cs{}",
                     if parsed.is_empty() { " (all zero — check the file)" } else { "" });
            entries.push(Entry::new(b"rules/prices", prices.into_bytes()));
        }
        Err(_) => println!("no ConstValues.cs — nothing will have a trade value"),
    }

    // String tables, so a room can name itself.
    for table in ["BURN.TXT", "newlocations.txt"] {
        if let Ok(bytes) = src::read(dir.join("lang").join(table)) {
            entries.push(Entry::new(format!("text/{table}").as_bytes(), bytes));
        }
    }

    // **NewGfx**, only when asked: it multiplies the pack's size, and the
    // game reads a pack into one fixed buffer.
    if with_newgfx {
        let mut sheets: std::collections::BTreeMap<String, BTreeSet<usize>> = std::collections::BTreeMap::new();
        for (name, parsed) in parsed_maps.iter().filter(|(n, _)| maps.contains(n)) {
            let cells: BTreeSet<usize> = parsed.tiles.iter().filter_map(|t| t.custom_sheet_index()).collect();
            if !cells.is_empty() {
                sheets.insert(name.clone(), cells);
            }
        }
        let tiles: BTreeSet<(u8, u8)> = wanted_tiles.union(&intro_tiles).copied().collect();
        if let Some(s) = newgfx::pack(&dir, &tiles, &sheets, &wanted_rooms, &mut entries) {
            println!("newgfx: {} entries, {:.1} MiB of pixels; {} ids stay classic, as in v1.2",
                     s.entries, s.pixels_bytes as f64 / (1024.0 * 1024.0), s.classic);
            println!("newgfx: classic: {}", s.classic_keys.join(" "));
        }
    }

    let pack = build(&entries);
    std::fs::write(&out_path, &pack).expect("write pack");
    let mut sources = out_path.clone().into_os_string();
    sources.push(".sources");
    let mut listing = format!("# burntime.pack {}  {}\n", sha256::hex(&sha256::digest(&pack)), pack.len());
    listing.push_str(&src::listing());
    std::fs::write(&sources, listing).expect("write the list of sources");

    let bytes: usize = entries.iter().map(|e| e.data.len()).sum();
    println!(
        "\n{} entries, {} tiles, {:.1} MiB -> {}",
        entries.len(),
        wanted_tiles.len() - missing.len(),
        bytes as f64 / (1024.0 * 1024.0),
        out_path.display()
    );
    if !missing.is_empty() {
        println!(
            "  {} tiles have no artwork and will draw as gaps: {:?}",
            missing.len(),
            &missing[..missing.len().min(6)]
        );
    }
    assert_eq!(TILE, 32, "the packed tiles are 32x32");
}

/// The palette item pictures are drawn with: map 0's.
///
/// `GetRawColorTable` lists the files that have a table of their own and
/// gst.raw is not among them, so it falls to `DefaultColorTable` — which is
/// `colorTables["map0"]`, the table inside `mat_000.raw`.
fn item_palette(pak: &ClassicPak) -> Option<Palette> {
    Palette::from_mat_raw(pak.get("burn_gfx/mat_000.raw")?)
}

/// One `raum_N.pac`, decoded to the packed image format.
fn room_image(pak: &ClassicPak, n: u8) -> Option<Vec<u8>> {
    pac_image(pak, &format!("burn_gfx/raum_{n}.pac"))
}

/// Any full-screen `.pac`, decoded to the packed image format.
///
/// The picture is palette indices and the table is in the same file, so the
/// colours are the picture's own — including the eight slots a room is allowed
/// to replace. One whose table is missing is skipped rather than drawn with a
/// map's palette, which would be wrong in exactly those slots.
fn pac_image(pak: &ClassicPak, path: &str) -> Option<Vec<u8>> {
    let bytes = pak.get(path)?;
    let pac = burngfx::pac::decode(bytes).ok()?;
    let (palette, _) = burngfx::pac::colour_table(bytes, &pac)?;

    let mut out = Vec::with_capacity(4 + pac.indices.len() * 4);
    out.extend_from_slice(&(burngfx::pac::WIDTH as u16).to_le_bytes());
    out.extend_from_slice(&(burngfx::pac::HEIGHT as u16).to_le_bytes());
    for &i in &pac.indices {
        let c = palette.color(i);
        out.extend_from_slice(&[c.r, c.g, c.b, c.a]);
    }
    Some(out)
}

/// Four lines of four characters starting at `from`, where '1' blocks.
/// That is how both the map sheet masks and the hand-written corrections are
/// written; a short or missing block leaves the tile open.
fn mask_from_lines(lines: &[String], from: usize) -> burngfx::zei::WalkMask {
    let mut bits = 0u16;
    for y in 0..4 {
        let Some(line) = lines.get(from + y) else { continue };
        for (x, c) in line.chars().take(4).enumerate() {
            if c == '1' {
                bits |= 0x8000 >> (y * 4 + x);
            }
        }
    }
    burngfx::zei::WalkMask(bits)
}

/// `MAT_005` is map 5. The map id decides which palette its tiles are drawn
/// with — `OriginalLocationCreator` passes it alongside the .burnmap.
fn map_id_of(name: &str) -> u8 {
    name.trim_start_matches("MAT_").trim_start_matches('0').parse().unwrap_or(0)
}

/// The archive, its table in name order (a stable sort: of two entries with
/// one name the first added is found, as before), so a lookup can search.
fn build(entries: &[Entry]) -> Vec<u8> {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let entries = &sorted[..];
    let header = 16;
    let table = entries.len() * (NAME_LEN + 8);
    let mut out = Vec::new();

    out.extend_from_slice(burngfx::pack::MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());

    let mut offset = header + table;
    for e in entries {
        let mut name = [0u8; NAME_LEN];
        name[..e.name.len()].copy_from_slice(&e.name);
        out.extend_from_slice(&name);
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
        offset += e.data.len();
    }
    for e in entries {
        out.extend_from_slice(&e.data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use burngfx::matmap::RoomType;

    /// The demo profile is a claim about what it reaches, and this is the
    /// claim: an ordinary interior, a water source, and a room with no
    /// `raum_` picture at all. The third is the one that is easy to lose —
    /// Muarab has no such door, so a pack without Acid Town cannot show that
    /// entering a doctor falls back to the panel instead of leaving the
    /// previous room on screen.
    #[test]
    fn the_demo_profile_reaches_all_three_kinds_of_room() {
        let Ok(bytes) = std::fs::read(testdata().join("burntime.pak")) else { return };
        let pak = ClassicPak::open(&bytes).unwrap();

        let (mut plain, mut water, mut no_picture) = (0, 0, 0);
        for name in profile("demo").unwrap() {
            let id = map_id_of(&name);
            let Some(raw) = pak.get(&format!("burn_gfx/mat_{id:03}.raw")) else { continue };
            for d in &burngfx::matmap::parse(raw).unwrap().doors {
                match (d.kind, d.interior()) {
                    (RoomType::WaterSource, Some(_)) => water += 1,
                    (_, Some(_)) => plain += 1,
                    (_, None) => no_picture += 1,
                }
            }
        }

        assert!(plain > 0, "no ordinary room with a picture");
        assert!(water > 0, "no water source");
        assert!(no_picture > 0, "no room without a picture — the panel path is unreachable");
    }

    /// Every profile carries every location's room list, so that the item
    /// generator lays out the same world whatever artwork was packed. Which
    /// places have pictures is a packaging decision; the world is not.
    #[test]
    fn every_profile_knows_about_every_room_in_the_world() {
        let Ok(bytes) = std::fs::read(testdata().join("burntime.pak")) else { return };
        let pak = ClassicPak::open(&bytes).unwrap();

        let mut expected = 0;
        for id in 0..=37u32 {
            let Some(raw) = pak.get(&format!("burn_gfx/mat_{id:03}.raw")) else { continue };
            expected += burngfx::matmap::parse(raw).unwrap().doors.len();
        }
        assert!(expected > 180, "only {expected} doors in the archive");

        // The pack the tree currently holds, whichever profile built it.
        let Ok(packed) = std::fs::read(testdata().join("burntime.pack")) else { return };
        let pack = burngfx::pack::Pack::open(&packed).unwrap();

        let mut found = 0;
        for id in 0..=37u32 {
            let name = format!("MAT_{id:03}");
            let mut key = [0u8; NAME_LEN];
            let n = names::rooms(&mut key, &name).unwrap();
            let key = std::str::from_utf8(&key[..n]).unwrap();
            if pak.get(&format!("burn_gfx/mat_{id:03}.raw")).is_some() {
                found += pack.get(key).unwrap_or_else(|| panic!("{name} has no room list")).len();
            }
        }
        assert_eq!(found, expected, "the pack knows fewer rooms than the archive has");
    }

    /// `minimal` is what the default pack is, and the AROS build has to be
    /// able to walk it: the world, and somewhere to walk into.
    #[test]
    fn the_minimal_profile_is_the_world_and_one_town() {
        assert_eq!(profile("minimal").unwrap(), ["MAT_000", "MAT_005"]);
        let demo = profile("demo").unwrap();
        for map in profile("minimal").unwrap() {
            assert!(demo.contains(&map), "demo dropped {map}");
        }
        assert!(demo.contains(&String::from("MAT_001")), "the start has no map");
        assert_eq!(profile("nothing-of-the-sort"), None);
    }
}

#[cfg(test)]
mod remaster_overlay {
    use super::Remaster;

    /// A file v1.3 has is read from `v1.3/`, any other from `v1.2/`; a
    /// directory never counts as v1.3's file; the names are the union.
    #[test]
    fn v13_files_over_v12() {
        let dir = std::env::temp_dir().join(format!("bt-remaster-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in [("v1.2/rules/extended/items.txt", "old"), ("v1.2/rules/extended/game.txt", "old"),
                             ("v1.3/rules/extended/items.txt", "new"), ("v1.2/lang/en/-MEN_040.TXT", "x"),
                             ("v1.2/lang/en/MEN_001.TXT", "a"), ("v1.3/lang/en/MEN_054.TXT", "b")] {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let r = Remaster::new(&dir);
        assert_eq!(std::fs::read_to_string(r.join("rules/extended/items.txt")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(r.join("rules/extended/game.txt")).unwrap(), "old");
        assert_eq!(r.join("rules"), dir.join("v1.2/rules"), "a directory is not a file of v1.3's");
        assert_eq!(r.names("lang/en"), ["-MEN_040.TXT", "MEN_001.TXT", "MEN_054.TXT"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
