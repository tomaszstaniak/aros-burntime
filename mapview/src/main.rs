//! Walk down through Burntime's world in a window.
//!
//!     cargo run -p mapview        # start on the world map
//!     cargo run -p mapview -- 4   # and set off for location 4, Nob Hill
//!
//! Click to walk. Arriving on a green rectangle acts on it: on the desert that
//! means entering the location, inside a location it means the room you are
//! standing in. Clicking a door from across the map does nothing until the
//! character gets there. Space toggles the rectangles, escape goes back up a
//! level before it quits.
//!
//! Host-only, and deliberately so: it reads PNG tiles from disk, which is the
//! one thing the `no_std` side cannot do. The map parsing is the same
//! `burngfx` code that builds for AROS.

use burngfx::font::Font;
use burngfx::game::{self, GameState, LocationData, Place, Point, WALK_SPEED};
use burngfx::map::{self, Map};
use burngfx::text::{MapSettings, TextDb, TextRef};
use sdlmini::{scancode, Event, Screen};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const TILE: usize = 32;
const SCALE: i32 = 2;

/// Width, height and RGBA pixels of a loaded PNG.
type Image = (usize, usize, Vec<u8>);

/// A drawing target: ARGB pixels plus the size needed to index them.
struct Canvas<'a> {
    px: &'a mut [u32],
    w: usize,
    h: usize,
}

impl Canvas<'_> {
    fn plot(&mut self, x: i32, y: i32, colour: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = colour;
        }
    }
}

fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata")
}

fn load_png(path: &Path) -> Option<Image> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(file);
    // Map sheets are palette PNGs; expand so every source is RGB or RGBA.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return None,
    };
    Some((info.width as usize, info.height as usize, rgba))
}


/// Copy one 32x32 cell out of a source image onto the canvas.
fn put_tile(
    canvas: &mut [u32],
    w: usize,
    src: &[u8],
    src_w: usize,
    from: (usize, usize),
    to: (usize, usize),
) {
    for y in 0..TILE {
        for x in 0..TILE {
            let s = ((from.1 + y) * src_w + from.0 + x) * 4;
            if s + 4 > src.len() {
                continue;
            }
            let px = &src[s..s + 4];
            if px[3] == 0 {
                continue;
            }
            canvas[(to.1 + y) * w + to.0 + x] =
                0xFF00_0000 | (px[0] as u32) << 16 | (px[1] as u32) << 8 | px[2] as u32;
        }
    }
}

/// Draw a tile we have no artwork for, in a way nobody mistakes for scenery.
///
/// The remaster ships its classic tile set as rendered PNGs, but not all of
/// it: sections 14 and 15 have no images at all and 28 and 29 are partial.
/// Those live in the player's own `zei_<section>.raw`. Leaving the cell black
/// reads as a hole in the world, so mark it instead.
fn draw_missing(canvas: &mut [u32], w: usize, at_x: usize, at_y: usize) {
    const DARK: u32 = 0xFF2A_1E2E;
    const MARK: u32 = 0xFFA0_3878;
    for y in 0..TILE {
        for x in 0..TILE {
            let diagonal = x == y || x + y == TILE - 1;
            let border = x == 0 || y == 0 || x == TILE - 1 || y == TILE - 1;
            canvas[(at_y + y) * w + at_x + x] = if diagonal || border { MARK } else { DARK };
        }
    }
}

/// Compose a whole map into an ARGB canvas the size of the map.
fn compose(name: &str, m: &Map, tiles: &mut HashMap<(u8, u8), Option<Image>>) -> (usize, usize, Vec<u32>) {
    let dir = testdata();
    let (w, h) = m.pixel_size();
    let (w, h) = (w as usize, h as usize);
    let mut canvas = vec![0xFF10_1820u32; w * h];

    let sheet = load_png(&dir.join(format!("maps/{name}_tiles.png")));

    for ty in 0..m.height {
        for tx in 0..m.width {
            let tile = m.tile(tx, ty);
            let (px, py) = (tx * TILE, ty * TILE);

            if let Some((section, item)) = tile.classic_name() {
                let entry = tiles
                    .entry((section, item))
                    .or_insert_with(|| load_png(&dir.join(format!("tiles/{section:02}_{item:02}.png"))));
                match entry {
                    Some((tw, _, rgba)) => put_tile(&mut canvas, w, rgba, *tw, (0, 0), (px, py)),
                    // No artwork for this one; see draw_missing.
                    None => draw_missing(&mut canvas, w, px, py),
                }
            } else if let Some(index) = tile.custom_sheet_index() {
                if let Some((sw, _, rgba)) = &sheet {
                    let cols = sw / TILE;
                    let from = ((index % cols) * TILE, (index / cols) * TILE);
                    put_tile(&mut canvas, w, rgba, *sw, from, (px, py));
                }
            }
        }
    }
    (w, h, canvas)
}

/// Outline every entrance, so the doorways the game reads from the file are
/// visible on the picture it draws.
fn draw_entrances(canvas: &mut Canvas, m: &Map) {
    const MARK: u32 = 0xFF00_FF66;
    for e in &m.entrances {
        for x in e.left..e.left + e.width {
            canvas.plot(x, e.top, MARK);
            canvas.plot(x, e.top + e.height - 1, MARK);
        }
        for y in e.top..e.top + e.height {
            canvas.plot(e.left, y, MARK);
            canvas.plot(e.left + e.width - 1, y, MARK);
        }
    }
}

/// The game's own bitmap font: metrics from font.txt, pixels from font.png.
struct Text {
    font: Font,
    strip: Image,
}

impl Text {
    fn load() -> Option<Text> {
        let dir = testdata();
        let font = Font::parse(&std::fs::read_to_string(dir.join("font.txt")).ok()?)?;
        let strip = load_png(&dir.join(&font.image))?;
        Some(Text { font, strip })
    }

    /// Draw a string, recolouring every non-transparent pixel. The font strip
    /// is a single colour, so tinting is how the game varies it.
    fn draw(&self, canvas: &mut Canvas, s: &str, at_x: i32, at_y: i32, colour: u32) {
        let (sw, _, pixels) = &self.strip;
        for g in self.font.layout(s) {
            for y in 0..self.font.height as usize {
                for x in 0..g.width as usize {
                    let src = (y * sw + g.src_x as usize + x) * 4;
                    if src + 4 > pixels.len() || pixels[src + 3] == 0 {
                        continue;
                    }
                    canvas.plot(at_x + g.dst_x + x as i32, at_y + y as i32, colour);
                }
            }
        }
    }

    fn width(&self, s: &str) -> i32 {
        self.font.measure(s) as i32
    }
}

/// What a map's .txt and the string tables say about one entrance.
struct Location {
    index: usize,
    name: String,
    kind: String,
    reference: String,
}

fn describe(settings: &MapSettings, index: usize) -> Location {
    let room = settings.room(index);
    let reference = room.map(|r| r.title.clone()).unwrap_or_default();
    let kind = room
        .map(|r| r.kind.clone())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| String::from("normal"));

    // `burn?4` means line 4 of BURN.TXT. The originals are upper case; the
    // remaster's own tables are lower, so try both spellings.
    let name = TextRef::parse(&reference)
        .and_then(|r| {
            let dir = testdata().join("lang");
            let upper = dir.join(r.file_name());
            let lower = dir.join(format!("{}.txt", r.database));
            let text = std::fs::read_to_string(&upper)
                .or_else(|_| std::fs::read_to_string(&lower))
                .ok()?;
            TextDb::parse(&text).get(r.index).map(String::from)
        })
        .unwrap_or_else(|| String::from("(no name)"));

    Location { index, name, kind, reference }
}

/// The panel that opens over the map when a location is entered.
fn draw_location(canvas: &mut Canvas, text: &Text, loc: &Location) {
    const PANEL: u32 = 0xFF1A_1410;
    const BORDER: u32 = 0xFFC8_9048;
    const TITLE: u32 = 0xFFF0_D8A0;
    const LABEL: u32 = 0xFF98_8058;

    let (pw, ph) = (220.min(canvas.w as i32 - 8), 74);
    let (px, py) = ((canvas.w as i32 - pw) / 2, (canvas.h as i32 - ph) / 2);

    for y in py..py + ph {
        for x in px..px + pw {
            let edge = x == px || x == px + pw - 1 || y == py || y == py + ph - 1;
            canvas.plot(x, y, if edge { BORDER } else { PANEL });
        }
    }

    let cx = px + pw / 2;
    text.draw(canvas, &loc.name, cx - text.width(&loc.name) / 2, py + 10, TITLE);

    let lines = [
        format!("entrance {}", loc.index),
        format!("type {}", loc.kind),
        format!("title {}", loc.reference),
    ];
    for (i, line) in lines.iter().enumerate() {
        text.draw(canvas, line, px + 10, py + 28 + i as i32 * 13, LABEL);
    }
}

/// Draw the character. A stand-in: the original's sprites are in game data we
/// do not have, so this is deliberately a marker rather than a bad guess at a
/// person. `at` is where the feet are.
fn draw_character(canvas: &mut Canvas, at: Point) {
    const BODY: u32 = 0xFF20_C8F0;
    const EDGE: u32 = 0xFF08_3040;
    for y in -13i32..=0 {
        for x in -3i32..=3 {
            let head = y < -9 && x.abs() <= 2;
            let body = y >= -9 && x.abs() <= (if y > -3 { 3 } else { 2 });
            if head || body {
                let edge = x.abs() == 3 || y == -13 || y == 0;
                canvas.plot(at.x + x, at.y + y, if edge { EDGE } else { BODY });
            }
        }
    }
}

/// One loaded map: the picture, the grid it came from and its room settings.
struct Scene {
    name: String,
    map: Map,
    settings: MapSettings,
    base: Vec<u32>,
    w: usize,
    h: usize,
}

impl Scene {
    fn bounds(&self) -> (u32, u32) {
        (self.w as u32, self.h as u32)
    }

    fn location_data(&self) -> LocationData {
        LocationData { entry_point: self.settings.entry_point }
    }
}

fn load_scene(name: &str, tiles: &mut HashMap<(u8, u8), Option<Image>>) -> Option<Scene> {
    let bytes = std::fs::read(testdata().join(format!("maps/{name}.burnmap"))).ok()?;
    let map = map::parse(&bytes).ok()?;
    let settings = std::fs::read_to_string(testdata().join(format!("maps/{name}.txt")))
        .map(|t| MapSettings::parse(&t))
        .unwrap_or_default();
    let (w, h, base) = compose(name, &map, tiles);
    Some(Scene { name: String::from(name), map, settings, base, w, h })
}

/// Burntime is three levels deep — the desert, a settlement's own map, and the
/// rooms inside it. Entrance N of `MAT_000` is location N, whose map is
/// `MAT_<N+1>`; the game builds them in exactly that order, see
/// OriginalLocationCreator.
const WORLD: &str = "MAT_000";

fn location_map_for(entrance: usize) -> String {
    format!("MAT_{:03}", entrance + 1)
}

fn main() {
    // `mapview [N]` walks to world entrance N on start. Not a shortcut: it
    // sets the same walk target a click would, so everything after it is the
    // ordinary path. Useful for demonstrating without a mouse.
    let walk_to_entrance: Option<usize> = std::env::args().nth(1).and_then(|a| a.parse().ok());

    let mut tiles: HashMap<(u8, u8), Option<Image>> = HashMap::new();
    let text = Text::load();
    if text.is_none() {
        eprintln!("no font in burngfx/testdata — location panels will be blank");
    }

    let Some(mut scene) = load_scene(WORLD, &mut tiles) else {
        eprintln!("no {WORLD}.burnmap in burngfx/testdata/maps — see its README.md");
        return;
    };

    let mut state = GameState::new(game::map_centre(&scene.map));
    let mut walk_to: Option<Point> = None;
    let mut open_room: Option<Location> = None;
    let mut show_entrances = true;
    let mut screen: Option<Screen> = None;
    let mut size = (0usize, 0usize);

    if let Some(i) = walk_to_entrance {
        if let Some(e) = scene.map.entrances.get(i) {
            walk_to = Some(Point::new(e.left + e.width / 2, e.top + e.height / 2));
        }
    }

    println!("{}: the world, {} locations", scene.name, scene.map.entrances.len());
    println!("  click to walk; arriving on a rectangle acts on it. space: rectangles, esc: back");

    loop {
        if size != (scene.w, scene.h) {
            // Drop the old window before opening the new one: Screen's Drop
            // calls SDL_Quit, and assigning would run it *after* SDL_Init.
            drop(screen.take());
            screen = Some(
                Screen::open(b"Burntime\0", scene.w, scene.h, scene.w as i32 * SCALE, scene.h as i32 * SCALE)
                    .unwrap_or_else(|step| panic!("{step}: {}", sdlmini::error())),
            );
            size = (scene.w, scene.h);
        }
        let screen = screen.as_mut().unwrap();

        // Changing scene changes the map size, and the window and its texture
        // are rebuilt at the top of the loop — so go round again before
        // presenting rather than pushing a differently sized frame at the old
        // texture. sdlmini's debug assertion catches that, which is how this
        // was found.
        let mut scene_changed = false;

        while let Some(event) = screen.poll() {
            match event {
                Event::Quit => return,
                Event::MouseDown { x, y, .. } => {
                    // The window is SCALE times the map and nothing sets a
                    // logical size, so mouse coordinates are window pixels.
                    walk_to = Some(game::clamp(Point::new(x / SCALE, y / SCALE), scene.bounds()));
                    open_room = None;
                }
                Event::KeyDown(key) => match key {
                    scancode::ESCAPE if open_room.is_some() => open_room = None,
                    scancode::ESCAPE | scancode::Q => {
                        if !state.is_inside() {
                            return;
                        }
                        state.leave();
                        scene = load_scene(WORLD, &mut tiles).expect("world map");
                        walk_to = None;
                        open_room = None;
                        scene_changed = true;
                        println!("{}: back on the world at {:?}", scene.name, state.position);
                    }
                    scancode::SPACE => show_entrances = !show_entrances,
                    _ => {}
                },
                _ => {}
            }
        }

        // Walk, and act on whatever we arrive at. One rule for both levels:
        // a rectangle does something when the character is standing in it, so
        // clicking a door from far away is only a decision to walk there.
        if let Some(target) = walk_to {
            state.position = game::step_towards(state.position, target, WALK_SPEED, scene.bounds());
            if state.position == target {
                walk_to = None;
                if let Some(i) = game::entrance_under(&scene.map, state.position) {
                    match state.place {
                        Place::World => {
                            let name = location_map_for(i);
                            match load_scene(&name, &mut tiles) {
                                Some(next) => {
                                    let spawn = game::spawn_point(&next.location_data(), &next.map);
                                    state.enter(i, spawn);
                                    let world_name = describe(&scene.settings, i).name;
                                    scene = next;
                                    scene_changed = true;
                                    println!(
                                        "-> {world_name} ({}), {} rooms, arrive at {:?}{}",
                                        scene.name,
                                        scene.map.entrances.len(),
                                        spawn,
                                        if scene.settings.entry_point.is_some() {
                                            ""
                                        } else {
                                            " (map centre: no entry point without GAMDAT)"
                                        }
                                    );
                                }
                                None => eprintln!("  {name}: no map for location {i}"),
                            }
                        }
                        Place::Location(_) => {
                            let room = describe(&scene.settings, i);
                            println!("   room {i}: {} ({})", room.name, room.reference);
                            open_room = Some(room);
                        }
                    }
                }
            }
        }

        if scene_changed {
            continue;
        }

        let mut canvas = scene.base.clone();
        {
            let mut target = Canvas { px: &mut canvas, w: scene.w, h: scene.h };
            if show_entrances {
                draw_entrances(&mut target, &scene.map);
            }
            draw_character(&mut target, state.position);
            if let (Some(room), Some(text)) = (&open_room, &text) {
                draw_location(&mut target, text, room);
            }
        }
        screen.present(&canvas);

        // Roughly 60 frames a second; the walk is the only thing animating.
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
