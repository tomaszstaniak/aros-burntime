//! `ngbench PACK OUT [FRAMES] [S1,…,S8]`: what v1.2's NewGfx look would
//! cost to draw here, measured, written to OUT.
//!
//! Design step A0 (the NewGfx study, §2.4): before the look is built into the
//! game, draw what it draws — through the same `gfx::Target` and the NewGfx
//! blit the game would use — and time it on the machine. Four scenarios,
//! FRAMES frames each (600 by default), the view panning one game pixel a
//! frame and back so the tiles' phase changes:
//!
//! * **S1** the classic location as the game draws it today: a canvas at one
//!   pixel per game pixel (1993 tiles, twelve bodies), blown up with
//!   `blit_pixels`, the HUD and three lines of the plain font over it;
//! * **S2** the NewGfx location: the 60 × 72 tiles one to one, twelve bodies
//!   and their shadows, the HUD's daytime picture and a face, three lines of
//!   `font-highres` and one of `font-small-highres` (½, interpolated);
//! * **S3** a NewGfx full screen: a `room*_large` interior cropped to the
//!   game area, the inventory's three panels and thirty item cells;
//! * **S4** S2 in a 1280 × 960 window, where SDL scales the 640 × 480
//!   backbuffer ×2;
//! * **S5** S2 and **S6** S3 drawn the way the game draws them now: each
//!   picture through the pre-scaled cache (`Target::draw_cached`), the
//!   same pixels as S2 and S3, copied one to one once made.
//! * **S7** S5 and **S8** S6 in **fullscreen at the desktop's size**, the
//!   viewport v1.2 recomputes there (UI-42): the game's window, switched
//!   as F11 switches it, and the backbuffer and game area made from the
//!   size it then has — on AROS One the Workbench screen's, not the
//!   640 × 480 the other scenarios keep. What a NewGfx frame costs at the
//!   size a player who goes fullscreen gets.
//! * Any of them as `S5@1920x1080`: in a window of that size.
//!
//! Per scenario: draw, present and frame time, median, p95 and maximum, in
//! microseconds by `gettimeofday` on AROS (SDL's counter does not move
//! there, `docs/measurements/2026-09-19-frame-cost.md`), and frames a
//! second. Also: the renderer SDL gave, the pack's load time, and on AROS
//! `AvailMem` before and after the load and after the first frame. The
//! pack comes from `packer --newgfx`; entries it lacks are counted, not
//! fatal. The program ends by itself; a key or closing the window ends it
//! early.

#![cfg_attr(target_os = "aros", no_std)]
#![cfg_attr(target_os = "aros", no_main)]

extern crate alloc;

#[path = "../../aros-game/src/gfx.rs"]
#[allow(dead_code)]
mod gfx;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use burngfx::font::{Font, ScaledFont};
use burngfx::pack::{Pack, PackedImage, ScaledImage};
use burngfx::ratio::{Pair, Ratio};
use gfx::scaled::{Filter, ScaledFace};
use gfx::{Align, Face, Target};

// -- the platform ------------------------------------------------------------

#[cfg(target_os = "aros")]
mod os {
    use alloc::string::String;
    use alloc::vec::Vec;

    extern "C" {
        fn ngbench_avail_any() -> u64;
        fn ngbench_avail_largest() -> u64;
        fn ngbench_file_size(path: *const u8) -> i64;
    }

    pub fn args() -> Vec<String> {
        aros::args::all().skip(1).map(String::from).collect()
    }

    pub fn say(s: &str) {
        aros::println!("{}", s);
    }

    /// `AvailMem(MEMF_ANY)` and `AvailMem(MEMF_ANY | MEMF_LARGEST)`.
    pub fn avail() -> Option<(u64, u64)> {
        unsafe { Some((ngbench_avail_any(), ngbench_avail_largest())) }
    }

    pub fn read(path: &str) -> Result<Vec<u8>, String> {
        let mut c = Vec::from(path.as_bytes());
        c.push(0);
        let n = unsafe { ngbench_file_size(c.as_ptr()) };
        if n < 0 {
            return Err(String::from("cannot open it"));
        }
        let mut buf = alloc::vec![0u8; n as usize];
        match aros::fs::read_into(path, &mut buf) {
            Ok(used) => {
                buf.truncate(used);
                Ok(buf)
            }
            Err(e) => Err(alloc::format!("{e}")),
        }
    }

    pub fn write(path: &str, text: &str) -> Result<(), String> {
        aros::fs::write_new(path, text.as_bytes()).map_err(|e| alloc::format!("{e}"))
    }

    pub const OS: &str = "aros";
}

#[cfg(not(target_os = "aros"))]
mod os {
    pub fn args() -> Vec<String> {
        std::env::args().skip(1).collect()
    }

    pub fn say(s: &str) {
        println!("{s}");
    }

    /// No `AvailMem` on the host.
    pub fn avail() -> Option<(u64, u64)> {
        None
    }

    pub fn read(path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }

    pub fn write(path: &str, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    pub const OS: &str = std::env::consts::OS;
}

// -- the measurements --------------------------------------------------------

#[derive(Default)]
struct Times {
    draw: Vec<u64>,
    present: Vec<u64>,
    frame: Vec<u64>,
    wall: u64,
}

fn stats(v: &[u64]) -> (u64, u64, u64) {
    if v.is_empty() {
        return (0, 0, 0);
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    let at = |q: usize| s[(s.len() - 1) * q / 100];
    (at(50), at(95), s[s.len() - 1])
}

fn mem_line(label: &str) -> String {
    match os::avail() {
        Some((any, largest)) => format!("mem {label}: avail {any} largest {largest}\n"),
        None => format!("mem {label}: n/a on {}\n", os::OS),
    }
}

/// The view's x on frame `f`: 0, 1, …, `range`, …, 1, 0, …
fn pan(f: usize, range: i32) -> i32 {
    if range <= 0 {
        return 0;
    }
    let p = (f as i32) % (2 * range);
    if p <= range { p } else { 2 * range - p }
}

// -- what is drawn -----------------------------------------------------------

/// A packed picture, NewGfx or classic, the part of it drawn, and its
/// scale.
#[derive(Clone, Copy)]
struct Pic<'a> {
    img: (u32, u32, &'a [u8]),
    src: (u32, u32, u32, u32),
    scale: Pair,
}

fn pic(w: u32, h: u32, rgba: &[u8], scale: Pair) -> Pic<'_> {
    Pic { img: (w, h, rgba), src: (0, 0, w, h), scale }
}

impl<'a> Pic<'a> {
    fn image(&self) -> PackedImage<'a> {
        PackedImage { width: self.img.0, height: self.img.1, rgba: self.img.2, scale: Pair::ONE }
    }
}

struct Assets<'a> {
    pack: &'a Pack<'a>,
    missing: Vec<String>,
}

impl<'a> Assets<'a> {
    fn classic(&mut self, key: &str) -> Option<Pic<'a>> {
        match self.pack.get(key).and_then(PackedImage::parse) {
            Some(i) => Some(pic(i.width, i.height, i.rgba, Pair::ONE)),
            None => {
                self.note(key);
                None
            }
        }
    }

    /// `ng/<key>` if packed, else the classic picture — the game's fallback.
    fn newgfx(&mut self, key: &str) -> Option<Pic<'a>> {
        let ng = format!("ng/{key}");
        match self.pack.get(&ng).and_then(ScaledImage::parse) {
            Some(s) => Some(pic(s.image.width, s.image.height, s.image.rgba, s.scale)),
            None => {
                self.note(&ng);
                self.classic(key)
            }
        }
    }

    fn note(&mut self, key: &str) {
        if !self.missing.iter().any(|m| m == key) {
            self.missing.push(String::from(key));
        }
    }
}

const MAP: &str = "MAT_005";
const MAP_ID: u8 = 5;
const TILE: i32 = 32;

/// The location: its tiles' pictures, classic and NewGfx, row by row.
struct Location<'a> {
    w: usize,
    h: usize,
    classic: Vec<Option<Pic<'a>>>,
    newgfx: Vec<Option<Pic<'a>>>,
}

fn location<'a>(a: &mut Assets<'a>) -> Option<Location<'a>> {
    let bytes = a.pack.get(&format!("map/{MAP}.burnmap"))?;
    let map = burngfx::map::parse(bytes).ok()?;
    let sheet = a.pack.get(&format!("sheet/{MAP}")).and_then(PackedImage::parse);
    let mut classic = Vec::new();
    let mut newgfx = Vec::new();
    for ty in 0..map.height {
        for tx in 0..map.width {
            let t = map.tile(tx, ty);
            if let Some((s, i)) = t.classic_name() {
                classic.push(a.classic(&format!("tile/{MAP_ID}/{s:02}_{i:02}")));
                let ng = format!("ng/tile/{s:02}_{i:02}");
                newgfx.push(match a.pack.get(&ng).and_then(ScaledImage::parse) {
                    Some(si) => Some(pic(si.image.width, si.image.height, si.image.rgba, si.scale)),
                    None => {
                        a.note(&ng);
                        classic.last().copied().flatten()
                    }
                });
            } else if let Some(cell) = t.custom_sheet_index() {
                // The map's own sheet, 32 × 32 cells.
                let c = sheet.as_ref().and_then(|s| {
                    let cols = (s.width / 32).max(1) as usize;
                    let (x, y) = ((cell % cols) as u32 * 32, (cell / cols) as u32 * 32);
                    (y + 32 <= s.height).then(|| Pic { src: (x, y, 32, 32), ..pic(s.width, s.height, s.rgba, Pair::ONE) })
                });
                classic.push(c);
                let ng = format!("ng/sheet/{MAP}/{cell}");
                newgfx.push(match a.pack.get(&ng).and_then(ScaledImage::parse) {
                    Some(si) => Some(pic(si.image.width, si.image.height, si.image.rgba, si.scale)),
                    None => {
                        a.note(&ng);
                        c
                    }
                });
            } else {
                classic.push(None);
                newgfx.push(None);
            }
        }
    }
    Some(Location { w: map.width, h: map.height, classic, newgfx })
}

struct Scene<'a> {
    loc: Option<Location<'a>>,
    bodies_classic: Vec<Pic<'a>>,
    bodies_ng: Vec<Pic<'a>>,
    shadow: Option<Pic<'a>>,
    daytime_classic: Option<Pic<'a>>,
    daytime_ng: Option<Pic<'a>>,
    face_classic: Option<Pic<'a>>,
    face_ng: Option<Pic<'a>>,
    plain: Option<Face<'a>>,
    highres: Option<ScaledFace<'a>>,
    small: Option<ScaledFace<'a>>,
    room: Option<Pic<'a>>,
    panels: Vec<Pic<'a>>,
    items: Vec<Pic<'a>>,
}

fn scaled_face<'a>(a: &mut Assets<'a>, key: &str) -> Option<ScaledFace<'a>> {
    let text = a.pack.get(&format!("ng/{key}"));
    let strip = a.pack.get(&format!("ng/{key}.strip")).and_then(PackedImage::parse);
    match (text, strip) {
        (Some(t), Some(s)) => {
            let font = ScaledFont::parse(core::str::from_utf8(t).ok()?, s.height as i64)?;
            Some(ScaledFace { font, atlas: s })
        }
        _ => {
            a.note(&format!("ng/{key}"));
            None
        }
    }
}

fn scene<'a>(a: &mut Assets<'a>) -> Scene<'a> {
    let loc = location(a);
    if loc.is_none() {
        a.note(&format!("map/{MAP}.burnmap"));
    }
    let ids = [0u32, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176];
    let bodies_classic = ids.iter().filter_map(|n| a.classic(&format!("body/{n}"))).collect();
    let bodies_ng = ids.iter().filter_map(|n| a.newgfx(&format!("body/{n}"))).collect();
    let plain = match (a.pack.get("font/plain"), a.pack.get("font/plain.strip").and_then(PackedImage::parse)) {
        (Some(t), Some(strip)) => core::str::from_utf8(t).ok().and_then(Font::parse).map(|font| Face { font, strip, ng: None }),
        _ => None,
    };
    if plain.is_none() {
        a.note("font/plain");
    }
    let room = (0..9).find_map(|n| {
        let k = format!("ng/room/{n}");
        a.pack.get(&k).and_then(ScaledImage::parse)
            .map(|si| pic(si.image.width, si.image.height, si.image.rgba, si.scale))
    });
    if room.is_none() {
        a.note("ng/room/*");
    }
    let panels = ["ui/inventory_left", "f/inv/1", "f/inv/2"].iter().filter_map(|k| a.newgfx(k)).collect();
    let items = (0..30).filter_map(|n| a.newgfx(&format!("item/{n}"))).collect();
    Scene {
        loc,
        bodies_classic,
        bodies_ng,
        shadow: a.newgfx("n/char_shadow"),
        daytime_classic: a.classic("f/munt/22"),
        daytime_ng: a.newgfx("f/munt/22"),
        face_classic: a.classic("f/ges_00/0"),
        face_ng: a.newgfx("f/ges_00"),
        plain,
        highres: scaled_face(a, "font/plain"),
        small: scaled_face(a, "font/small"),
        room,
        panels,
        items,
    }
}

/// Copy a picture into a canvas at one pixel per game pixel, transparent
/// where its alpha is 0 — the old renderer's way.
fn canvas_put(canvas: &mut [u32], cw: usize, ch: usize, p: &Pic, x: i32, y: i32) {
    let (iw, _, rgba) = p.img;
    let (ox, oy, w, h) = p.src;
    for sy in 0..h as i32 {
        let dy = y + sy;
        if dy < 0 || dy >= ch as i32 {
            continue;
        }
        for sx in 0..w as i32 {
            let dx = x + sx;
            if dx < 0 || dx >= cw as i32 {
                continue;
            }
            let at = (((oy + sy as u32) * iw + ox + sx as u32) * 4) as usize;
            if rgba[at + 3] == 0 {
                continue;
            }
            canvas[dy as usize * cw + dx as usize] =
                0xFF00_0000 | (rgba[at] as u32) << 16 | (rgba[at + 1] as u32) << 8 | rgba[at + 2] as u32;
        }
    }
}

const LINES: [&str; 3] = ["Nob Hill, day 12", "Water 4  Food 7  Health 87", "Joe drinks from the spring."];
const PROMPT: &str = "ENTER  Select   ESC  Back   SPACE  Inventory";

fn body_spot(i: usize) -> (i32, i32) {
    (40 + (i as i32 % 6) * 45, 50 + (i as i32 / 6) * 70)
}

/// S1: the classic location as today.
fn draw_s1(t: &mut Target, s: &Scene, canvas: &mut Vec<u32>, cam: i32) {
    let (gw, gh) = t.game_size();
    let (cw, ch) = (gw as usize, gh as usize);
    canvas.clear();
    canvas.resize(cw * ch, 0xFF00_0000);
    if let Some(loc) = &s.loc {
        for ty in 0..loc.h {
            for tx in 0..loc.w {
                let (x, y) = (tx as i32 * TILE - cam, ty as i32 * TILE);
                if x <= -TILE || x >= gw || y >= gh {
                    continue;
                }
                if let Some(p) = &loc.classic[ty * loc.w + tx] {
                    canvas_put(canvas, cw, ch, p, x, y);
                }
            }
        }
    }
    for (i, b) in s.bodies_classic.iter().enumerate() {
        let (x, y) = body_spot(i);
        canvas_put(canvas, cw, ch, b, x, y);
    }
    t.origin = (0, 0);
    t.blit_pixels(canvas, cw, ch, 0, 0);
    if let Some(p) = &s.daytime_classic {
        t.blit(&p.image(), gw - p.img.0 as i32 - 4, 4);
    }
    if let Some(p) = &s.face_classic {
        t.blit(&p.image(), 4, gh - p.img.1 as i32 - 4);
    }
    if let Some(f) = &s.plain {
        for (i, line) in LINES.iter().enumerate() {
            f.draw(t, line, 80, gh - 40 + i as i32 * 11, Align::Start, Align::Start, Some(0xFFE0A0));
        }
    }
}

/// Through the cache (S5, S6) or scaled at every draw (S2, S3, S4).
static CACHED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn blit_pic(t: &mut Target, p: &Pic, x: Ratio, y: Ratio) {
    if CACHED.load(core::sync::atomic::Ordering::Relaxed) {
        t.draw_cached(&p.image(), p.src, p.scale, x, y);
    } else {
        t.blit_scaled(&p.image(), p.src, p.scale, x, y, None, Filter::Point);
    }
}

/// S2 and S4: the NewGfx location.
fn draw_s2(t: &mut Target, s: &Scene, cam: i32) {
    let (gw, gh) = t.game_size();
    t.origin = (0, 0);
    t.clear();
    if let Some(loc) = &s.loc {
        for ty in 0..loc.h {
            for tx in 0..loc.w {
                let (x, y) = (tx as i32 * TILE - cam, ty as i32 * TILE);
                if x <= -TILE || x >= gw || y >= gh {
                    continue;
                }
                if let Some(p) = &loc.newgfx[ty * loc.w + tx] {
                    blit_pic(t, p, Ratio::int(x as i64), Ratio::int(y as i64));
                }
            }
        }
    }
    for (i, b) in s.bodies_ng.iter().enumerate() {
        let (x, y) = body_spot(i);
        if let Some(sh) = &s.shadow {
            blit_pic(t, sh, Ratio::int(x as i64), Ratio::int(y as i64 + 26));
        }
        blit_pic(t, b, Ratio::int(x as i64), Ratio::int(y as i64));
    }
    if let Some(p) = &s.daytime_ng {
        let w = (Ratio::int(p.img.0 as i64) / p.scale.x).ceil();
        blit_pic(t, p, Ratio::int(gw as i64 - w - 4), Ratio::int(4));
    }
    if let Some(p) = &s.face_ng {
        let h = (Ratio::int(p.img.1 as i64) / p.scale.y).ceil();
        blit_pic(t, p, Ratio::int(4), Ratio::int(gh as i64 - h - 4));
    }
    if let Some(f) = &s.highres {
        for (i, line) in LINES.iter().enumerate() {
            f.draw(t, line, 80, gh - 44 + i as i32 * 11, Align::Start, Align::Start, Some(0xFFE0A0));
        }
    }
    if let Some(f) = &s.small {
        f.draw(t, PROMPT, gw / 2, gh - 8, Align::Centre, Align::Start, Some(0xFFFFFF));
    }
}

/// S3: a NewGfx full screen.
fn draw_s3(t: &mut Target, s: &Scene) {
    let (gw, gh) = t.game_size();
    t.origin = (0, 0);
    t.clear();
    if let Some(p) = &s.room {
        // Centred on the game area and cut by it, as `MenuScene` places
        // `start.png` (`MenuScene.cs:368-372`).
        let w = Ratio::int(p.img.0 as i64) / p.scale.x;
        let h = Ratio::int(p.img.1 as i64) / p.scale.y;
        blit_pic(t, p, (Ratio::int(gw as i64) - w) / Ratio::int(2), (Ratio::int(gh as i64) - h) / Ratio::int(2));
    }
    let mut x = Ratio::int(8);
    for p in &s.panels {
        blit_pic(t, p, x, Ratio::int(30));
        x = x + Ratio::int(p.img.0 as i64) / p.scale.x;
    }
    for (i, p) in s.items.iter().enumerate() {
        let (cx, cy) = (16 + (i as i64 % 6) * 34, 40 + (i as i64 / 6) * 34);
        blit_pic(t, p, Ratio::int(cx), Ratio::int(cy));
    }
}

// -- the run -----------------------------------------------------------------

fn quit_requested(screen: &mut sdlmini::Screen) -> bool {
    let mut quit = false;
    while let Some(ev) = screen.poll() {
        if matches!(ev, sdlmini::Event::Quit | sdlmini::Event::KeyDown(_)) {
            quit = true;
        }
    }
    quit
}

/// Where a scenario draws: a window of a size, or fullscreen at the
/// desktop's.
#[derive(Clone, Copy)]
enum Window {
    Sized(usize, usize),
    Fullscreen,
}

fn run_scenario(name: &str, draws: &str, window: Window, frames: usize, s: &Scene, out: &mut String,
                first_frame_mem: &mut bool) -> bool {
    let opened = match window {
        Window::Sized(w, h) => {
            let t = Target::new(w, h);
            sdlmini::Screen::open(b"ngbench\0", t.w, t.h, w as i32, h as i32).map(|sc| (sc, t))
        }
        // As the game does it: its window, F11's switch, then the target
        // made from the size the window has (`apply_window`).
        Window::Fullscreen => sdlmini::Screen::open_game(b"ngbench\0", None, None, None).map(|mut sc| {
            if !sc.set_fullscreen(true) {
                out.push_str(&format!("{name}: fullscreen refused ({})\n", sdlmini::error().as_str()));
            }
            // The switch's own events, and time for a system that makes it
            // later than it says.
            for _ in 0..30 {
                while sc.poll().is_some() {}
                sdlmini::delay(10);
            }
            let (w, h) = sc.client_size();
            let t = Target::new(w.max(1) as usize, h.max(1) as usize);
            if !sc.set_buffer_size(t.w, t.h) {
                out.push_str(&format!("{name}: no {}x{} texture ({})\n", t.w, t.h, sdlmini::error().as_str()));
            }
            let desktop = sc.desktop_size().map_or(String::from("unknown"), |(w, h)| format!("{w}x{h}"));
            out.push_str(&format!("{name}: fullscreen {}, client {w}x{h}, desktop {desktop}\n",
                                  if sc.is_fullscreen() { "on" } else { "off" }));
            (sc, t)
        }),
    };
    let (mut screen, mut t) = match opened {
        Ok(opened) => opened,
        Err(e) => {
            out.push_str(&format!("{name}: no window ({e}: {})\n", sdlmini::error().as_str()));
            return true;
        }
    };
    let window = screen.client_size();
    let (gw, gh) = t.game_size();
    out.push_str(&format!("{name}: window {}x{} backbuffer {}x{} factor {} game {gw}x{gh} renderer {}\n",
                          window.0, window.1, t.w, t.h, t.factor_text(), screen.renderer_name()));
    let range = s.loc.as_ref().map_or(0, |l| (l.w as i32 * TILE - gw).clamp(0, 300));
    let mut canvas = Vec::new();
    let mut times = Times::default();
    CACHED.store(matches!(draws, "S5" | "S6"), core::sync::atomic::Ordering::Relaxed);
    let start = sdlmini::micros();
    let mut quit = false;
    for f in 0..frames {
        let t0 = sdlmini::micros();
        let cam = pan(f, range);
        match draws {
            "S1" => draw_s1(&mut t, s, &mut canvas, cam),
            "S3" | "S6" => draw_s3(&mut t, s),
            _ => draw_s2(&mut t, s, cam),
        }
        let t1 = sdlmini::micros();
        screen.present(&t.px);
        let t2 = sdlmini::micros();
        if !*first_frame_mem {
            *first_frame_mem = true;
            out.push_str(&mem_line("after the first frame"));
        }
        if quit_requested(&mut screen) {
            quit = true;
            break;
        }
        times.draw.push(t1.saturating_sub(t0));
        times.present.push(t2.saturating_sub(t1));
        times.frame.push(sdlmini::micros().saturating_sub(t0));
    }
    times.wall = sdlmini::micros().saturating_sub(start);
    let n = times.frame.len() as u64;
    let line = |label: &str, v: &[u64]| {
        let (p50, p95, max) = stats(v);
        format!("  {label:<8} p50 {p50:>7} us  p95 {p95:>7} us  max {max:>7} us\n")
    };
    out.push_str(&format!("{name}: {n} frames in {} ms, {}.{} frames/s{}\n", times.wall / 1000,
                          n * 1_000_000 / times.wall.max(1), (n * 10_000_000 / times.wall.max(1)) % 10,
                          if quit { " (stopped by a key or the window)" } else { "" }));
    out.push_str(&line("draw", &times.draw));
    out.push_str(&line("present", &times.present));
    out.push_str(&line("frame", &times.frame));
    if matches!(draws, "S5" | "S6") {
        out.push_str(&format!("  cache    {} hits, {} made, {} bytes kept\n", t.cache.hits, t.cache.misses, t.cache.bytes()));
    }
    quit
}

/// A scenario's name as what it draws and where: S1–S6 in their windows,
/// S7 and S8 (S5 and S6) in fullscreen, `Sn@WxH` in a window of that size.
fn scenario(name: &str) -> Option<(&str, Window)> {
    let (draws, size) = match name.split_once('@') {
        Some((d, size)) => (d, Some(size)),
        None => (name, None),
    };
    let (draws, default) = match draws {
        "S4" => ("S4", Window::Sized(1280, 960)),
        "S7" => ("S5", Window::Fullscreen),
        "S8" => ("S6", Window::Fullscreen),
        "S1" | "S2" | "S3" | "S5" | "S6" => (draws, Window::Sized(640, 480)),
        _ => return None,
    };
    let window = match size {
        None => default,
        Some(size) => {
            let (w, h) = size.split_once('x')?;
            let (w, h) = (w.parse::<usize>().ok()?, h.parse::<usize>().ok()?);
            if w == 0 || h == 0 {
                return None;
            }
            Window::Sized(w, h)
        }
    };
    Some((draws, window))
}

fn run() {
    let args = os::args();
    if args.len() < 2 {
        os::say("usage: ngbench PACK OUT [FRAMES] [S1,S2,S3,S4,S5,S6,S7,S8 or Sn@WxH]");
        return;
    }
    let (pack_path, out_path) = (&args[0], &args[1]);
    let frames: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(600);
    let which: Vec<String> = args.get(3).map_or_else(
        || ["S1", "S2", "S3", "S4", "S5", "S6"].iter().map(|s| String::from(*s)).collect(),
        |a| a.split(',').map(String::from).collect());

    let mut out = format!("ngbench {} ({}), {} frames a scenario\npack {pack_path}\n",
                          env!("NGBENCH_COMMIT"), os::OS, frames);
    out.push_str(&mem_line("before the pack"));
    let t0 = sdlmini::micros();
    let bytes = match os::read(pack_path) {
        Ok(b) => b,
        Err(e) => {
            out.push_str(&format!("cannot read the pack: {e}\n"));
            let _ = os::write(out_path, &out);
            os::say(&out);
            return;
        }
    };
    let t1 = sdlmini::micros();
    let bytes: &'static [u8] = alloc::boxed::Box::leak(bytes.into_boxed_slice());
    let pack = match Pack::open(bytes) {
        Ok(p) => p,
        Err(e) => {
            out.push_str(&format!("not a pack: {e:?}\n"));
            let _ = os::write(out_path, &out);
            os::say(&out);
            return;
        }
    };
    let t2 = sdlmini::micros();
    let ng = (0..pack.len()).filter(|&i| pack.name(i).starts_with(b"ng/")).count();
    out.push_str(&format!("pack: {} bytes, {} entries ({ng} NewGfx), read {} ms, opened {} ms\n",
                          bytes.len(), pack.len(), (t1 - t0) / 1000, (t2 - t1) / 1000));
    out.push_str(&mem_line("after the pack"));

    let mut assets = Assets { pack: &pack, missing: Vec::new() };
    let s = scene(&mut assets);
    if !assets.missing.is_empty() {
        out.push_str(&format!("missing (drawn without, or classic): {}\n", assets.missing.join(" ")));
    }
    let mut first = false;
    for name in &which {
        let Some((draws, window)) = scenario(name) else {
            out.push_str(&format!("{name}: not a scenario\n"));
            continue;
        };
        // Written as it goes: a run that dies in a scenario keeps the rest.
        let _ = os::write(out_path, &out);
        if run_scenario(name, draws, window, frames, &s, &mut out, &mut first) {
            break;
        }
    }
    out.push_str("done\n");
    if let Err(e) = os::write(out_path, &out) {
        os::say(&format!("cannot write {out_path}: {e}"));
    }
    os::say(&out);
}

#[cfg(target_os = "aros")]
aros::aros_main!(run);

#[cfg(not(target_os = "aros"))]
fn main() {
    run();
}
