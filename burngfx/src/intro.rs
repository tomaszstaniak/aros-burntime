//! The intro, `Scenes/IntroScene.cs`: what the program shows on every start
//! before the start screen (`Application.Start`, `Application.cs:112-121`),
//! as state and arithmetic. The program draws what this says.
//!
//! `intro.txt` (`resources/game/classic/`, the same file in v0.9.1 and
//! v1.2) lists seven pages — a 1993 map each, how long it stays and how fast
//! it scrolls — and fourteen animations, each a range of an `introN.ani`
//! shown at a place in the scene from an absolute time. A page stays for its
//! `time` (`timeout`, a `FadingHelper` at `1 / time`), then the scene blend
//! (`BlendOverlay`, here at 0.9 a second) goes to black, the next page is put
//! in and the blend comes back; after the last, the start screen. Any click,
//! and in v1.2 Primary or Back (v0.9.1: any of its virtual keys but F9),
//! goes there at once (`IntroScene.cs:135-150`).
//!
//! Everything here runs on the caller's seconds, in `f32` as upstream's
//! `elapsed` is, so the same steps give the same pictures.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// `view.SetViewport(new Vector2(16, 0), new Vector2(288, 160))`.
pub const VIEW: (i32, i32, i32, i32) = (16, 0, 288, 160);
/// The map's tiles, 32 square (`MapData.TileSize`).
pub const TILE: i32 = 32;
/// `OnRender`'s black, over the map and the animations: left of the view,
/// right of it, and the strip under it (`IntroScene.cs:232-234`).
pub const MASKS: [(i32, i32, i32, i32); 3] = [(-48, 0, 64, 160), (304, 0, 64, 160), (0, 160, 320, 40)];
/// `BlendOverlay.Speed = 0.9f` while the intro is up (`IntroScene.cs:120`).
pub const BLEND_SPEED: f32 = 0.9;
/// The music is muted for the first two seconds (`delayedMusicStart`,
/// `IntroScene.cs:119-124`, `:160-167`).
pub const MUTED_FOR: f32 = 2.0;
/// `SpriteAnimation.Speed`'s default: five frames a second.
pub const FRAMES_PER_SECOND: f32 = 5.0;
/// `SceneManager.SetScene` waits 500 ms past the blend's black before the
/// next scene (`BlendOverlayBase.FadeOut(wait: true)`).
pub const LEAVE_HOLD: f32 = 0.5;
/// An animation is put up from half a second before its start
/// (`animations[i].Start >= time && animations[i].Start < time + 0.5f`).
pub const EARLY: f32 = 0.5;

/// `IntroPage`.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    /// `classicmap@mat_NNN.raw??N`.
    pub map: u8,
    pub time: f32,
    /// Pixels a second, leftwards.
    pub scrolling: f32,
}

/// `IntroAnimation`: `file = introN.ani?first-last` at `pos`.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub pos: (i32, i32),
    pub start: f32,
    pub time: f32,
    /// The file without `.ani`, e.g. `intro1`.
    pub stem: String,
    pub first: u8,
    pub last: u8,
}

impl Animation {
    pub fn frames(&self) -> u8 {
        self.last - self.first + 1
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub pages: Vec<Page>,
    pub animations: Vec<Animation>,
}

/// Read `intro.txt` as the constructor does: `[intro]` says how many pages
/// and animations, `[pageN]` and `[aniN]` each. `None` when a page's map or
/// an animation's file cannot be what the scene asks for.
pub fn parse(text: &str) -> Option<Script> {
    let sections = crate::rules::sections(text);
    let section = |name: &str| sections.iter().find(|s| s.name.eq_ignore_ascii_case(name));
    let head = section("intro")?;
    let mut script = Script::default();
    for i in 0..head.int("pages").max(0) {
        let s = section(&page_name("page", i))?;
        let map = u8::try_from(s.int("map")).ok()?;
        script.pages.push(Page { map, time: s.float("time"), scrolling: s.float("scrolling") });
    }
    for i in 0..head.int("animations").max(0) {
        let s = section(&page_name("ani", i))?;
        // `GetVector2`: `75x60`.
        let (x, y) = s.string("pos").split_once('x')?;
        let pos = (x.trim().parse().ok()?, y.trim().parse().ok()?);
        let (stem, first, last) = ani_range(s.string("file"))?;
        script.animations.push(Animation { pos, start: s.float("start"), time: s.float("time"), stem, first, last });
    }
    if script.pages.is_empty() {
        return None;
    }
    Some(script)
}

fn page_name(prefix: &str, i: i32) -> String {
    let mut s = String::from(prefix);
    s.push_str(&i.to_string());
    s
}

/// `intro1.ani?0-2` as `SpriteLoaderAni` reads its `ResourceID`: the file,
/// then the first and the last frame (one frame when there is no `-`).
fn ani_range(file: &str) -> Option<(String, u8, u8)> {
    let (name, range) = file.split_once('?')?;
    let stem = name.strip_suffix(".ani").or_else(|| name.strip_suffix(".ANI"))?;
    let range = range.split('?').next()?;
    let (first, last) = match range.split_once('-') {
        Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
        None => {
            let n = range.trim().parse().ok()?;
            (n, n)
        }
    };
    if last < first {
        return None;
    }
    Some((stem.to_ascii_lowercase(), first, last))
}

/// Whose colours a map's tiles take: `Tile.Render` asks
/// `GetMapColorTable(mapId)`, which has `map0` to `map37` and the intro's
/// `map80` to `map84` and `map89` (`BurnGfxData.InitializeMaps`), and gives
/// the default, `map0`, for any other — the intro's `mat_090` among them.
pub fn palette_of_map(map: u8) -> u8 {
    match map {
        0..=37 | 80..=84 | 89 => map,
        _ => 0,
    }
}

/// Whose colours an intro animation takes (`GetRawColorTable`): `intro2.ani`
/// map 82's, `intro3.ani` and `intro4.ani` map 83's, `intro1.ani` the
/// default, map 0's.
pub fn palette_of_ani(stem: &str) -> u8 {
    match stem {
        "intro2" => 82,
        "intro3" | "intro4" => 83,
        _ => 0,
    }
}

/// The packed map of a page: `u16` width and height in tiles, then a
/// section and an item byte a tile, row by row.
pub fn map_key(map: u8) -> String {
    page_name("intro/map/", map as i32)
}

/// One frame of an intro animation, decoded to RGBA in its own colours:
/// `an/intro1/7` is frame 7 of `intro1.ani`.
pub fn frame_key(stem: &str, frame: u8) -> String {
    let mut s = String::from("an/");
    s.push_str(stem);
    s.push('/');
    s.push_str(&frame.to_string());
    s
}

/// A page's map as packed: size in tiles and `(section, item)` a tile.
pub struct PageMap<'a> {
    pub width: usize,
    pub height: usize,
    tiles: &'a [u8],
}

impl<'a> PageMap<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<PageMap<'a>> {
        let width = u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]) as usize;
        let height = u16::from_le_bytes([*bytes.get(2)?, *bytes.get(3)?]) as usize;
        let tiles = bytes.get(4..4 + width * height * 2)?;
        Some(PageMap { width, height, tiles })
    }

    pub fn tile(&self, x: usize, y: usize) -> (u8, u8) {
        let at = (y * self.width + x) * 2;
        (self.tiles[at], self.tiles[at + 1])
    }

    /// `MapView.OnRender`'s offset for the scene's scroll: `ScrollPosition`
    /// as set (`(int)(-scroll + 0.5f)`, nought down), then
    /// `ConstrainPosition` — a map narrower than the view centred in it,
    /// a wider one kept from showing past its edges.
    pub fn offset(&self, scroll_x: i32) -> (i32, i32) {
        let (w, h) = ((self.width as i32) * TILE, (self.height as i32) * TILE);
        let x = if w <= VIEW.2 { (VIEW.2 - w) / 2 } else { scroll_x.clamp(VIEW.2 - w, 0) };
        let y = if h <= VIEW.3 { (VIEW.3 - h) / 2 } else { 0 };
        (x, y)
    }

    /// The tiles `MapView.OnRender` draws for that offset:
    /// `Rect(-offset / TileSize, Size / TileSize + 2)` within the map, as
    /// `(x0, y0, x1, y1)`, the ends exclusive. Nothing clips them to the
    /// view; the masks cover what spills.
    pub fn visible(&self, offset: (i32, i32)) -> (usize, usize, usize, usize) {
        let (x0, y0) = (-offset.0 / TILE, -offset.1 / TILE);
        let (x1, y1) = (x0 + VIEW.2 / TILE + 2, y0 + VIEW.3 / TILE + 2);
        let clip = |a: i32, n: usize| a.clamp(0, n as i32) as usize;
        (clip(x0, self.width), clip(y0, self.height), clip(x1, self.width), clip(y1, self.height))
    }
}

/// `Burntime.Platform.FadingHelper`, in `f32` as upstream steps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub speed: f32,
    pub state: f32,
    pub to: f32,
}

impl Fade {
    pub fn is_out(&self) -> bool {
        self.state == 0.0
    }

    pub fn update(&mut self, elapsed: f32) {
        if self.to < self.state {
            self.state -= elapsed * self.speed;
            if self.state < self.to {
                self.state = self.to;
            }
        } else if self.to > self.state {
            self.state += elapsed * self.speed;
            if self.state > self.to {
                self.state = self.to;
            }
        }
    }
}

/// What a frame shows: the page, the map's scroll, the animation up (its
/// index and the frame of its file) and the blend's black, 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub page: usize,
    pub scroll_x: i32,
    pub animation: Option<(usize, u8)>,
    pub black: f32,
}

/// The scene, from `OnActivateScene` to the start screen.
#[derive(Clone, Debug)]
pub struct Intro {
    pub script: Script,
    index: usize,
    timeout: Fade,
    /// `BlendOverlay`'s fade: 1 is black.
    blend: Fade,
    scroll: f32,
    scroll_x: i32,
    time: f32,
    ani_time: f32,
    active: Option<usize>,
    /// `image.Background`: the animation up, and its frame position
    /// (`SpriteAnimation.frame`, a fresh clone's at nought).
    shown: Option<(usize, f32)>,
    delayed_music: f32,
    muted: bool,
    /// Gone to the start screen: what is left of `SetScene`'s wait.
    leaving: Option<f32>,
    done: bool,
}

impl Intro {
    /// `OnActivateScene`, entered the way `SetScene` enters every scene:
    /// the blend is black (`FadeOut(wait: true)` before activation) and is
    /// told to clear (`FadeIn` after), at the intro's 0.9 a second. The
    /// music is muted (`IsMuted = true`, `delayedMusicStart = 2`).
    pub fn new(script: Script) -> Intro {
        let first = script.pages.first().map_or(1.0, |p| p.time);
        Intro {
            script,
            index: 0,
            timeout: Fade { speed: 1.0 / first, state: 1.0, to: 0.0 },
            blend: Fade { speed: BLEND_SPEED, state: 1.0, to: 0.0 },
            scroll: 0.0,
            scroll_x: 0,
            time: 0.0,
            ani_time: 0.0,
            active: None,
            shown: None,
            delayed_music: MUTED_FOR,
            muted: true,
            leaving: None,
            done: false,
        }
    }

    /// Whether the music is muted now.
    pub fn muted(&self) -> bool {
        self.muted
    }

    /// The start screen is due.
    pub fn done(&self) -> bool {
        self.done
    }

    /// The scene was left, by a click or a key or by its end, and is
    /// fading to the start screen.
    pub fn leaving(&self) -> bool {
        self.leaving.is_some()
    }

    /// The page shown now.
    pub fn page(&self) -> usize {
        self.index
    }

    /// The scene's seconds so far (`time`).
    pub fn time(&self) -> f32 {
        self.time
    }

    /// `NextScene`: `SetScene("MenuScene")`. Its `FadeOut(wait: true)`
    /// blocks the scene's thread for `500 + (1 - state) * 1000 / Speed` ms
    /// while the blend goes on to black at the intro's speed; nothing of the
    /// scene moves meanwhile but its animation, which the renderer steps.
    /// `OnInactivateScene` unmutes the music.
    pub fn skip(&mut self) {
        if self.leaving.is_none() && !self.done {
            self.blend.to = 1.0;
            self.leaving = Some(LEAVE_HOLD + (1.0 - self.blend.state) / self.blend.speed);
        }
    }

    /// One frame of `elapsed` seconds: the scene's `OnUpdate`, then what the
    /// renderer steps (the blend, the animation's frame).
    pub fn step(&mut self, elapsed: f32) {
        if self.done {
            return;
        }
        if let Some(left) = self.leaving.as_mut() {
            *left -= elapsed;
            let over = *left <= 0.0;
            self.render_step(elapsed);
            if over {
                self.done = true;
                self.muted = false;
            }
            return;
        }

        if self.delayed_music >= 0.0 {
            self.delayed_music -= elapsed;
            if self.delayed_music <= 0.0 {
                self.muted = false;
            }
        }

        self.timeout.update(elapsed);
        self.scroll += elapsed * self.script.pages[self.index].scrolling;
        self.scroll_x = (-self.scroll + 0.5) as i32;

        if self.timeout.is_out() {
            self.blend.to = 1.0;
        }
        if self.blend.state == 1.0 && self.timeout.is_out() {
            if self.index >= self.script.pages.len() - 1 {
                self.skip();
            } else {
                self.index += 1;
                let page = &self.script.pages[self.index];
                self.timeout = Fade { speed: 1.0 / page.time, state: 1.0, to: 0.0 };
                self.scroll = 0.0;
                self.blend.to = 0.0;
            }
        }

        self.time += elapsed;
        if self.ani_time > 0.0 {
            self.ani_time -= elapsed;
            if self.ani_time <= 0.0 {
                self.shown = None;
            }
        }
        for (i, a) in self.script.animations.iter().enumerate() {
            if a.start >= self.time && a.start < self.time + EARLY && self.active != Some(i) {
                self.active = Some(i);
                self.shown = Some((i, 0.0));
                self.ani_time = a.time - (self.time - a.start);
            }
        }

        self.render_step(elapsed);
    }

    /// The renderer's part of a frame: `BlendOverlay.Update`, and
    /// `Image.OnRender`'s `background.Update(Target.Elapsed)` — an endless
    /// `SpriteAnimation` at five frames a second.
    fn render_step(&mut self, elapsed: f32) {
        self.blend.update(elapsed);
        if let Some((i, frame)) = self.shown.as_mut() {
            let count = self.script.animations[*i].frames() as f32;
            *frame += elapsed * FRAMES_PER_SECOND;
            while *frame >= count {
                *frame -= count;
            }
        }
    }

    /// What to draw now.
    pub fn frame(&self) -> Frame {
        Frame {
            page: self.index,
            scroll_x: self.scroll_x,
            animation: self.shown.map(|(i, f)| (i, self.script.animations[i].first + f as u8)),
            black: self.blend.state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `resources/game/classic/intro.txt` at v1.2 (27412d2), the same as at
    /// v0.9.1: remaster data (MIT), small enough to quote whole.
    const INTRO_TXT: &str = "[intro]\npages=7\nanimations=14\n\n\
[page0]\nmap=89\ntime=5\nscrolling=0\n\n[page1]\nmap=81\ntime=8\nscrolling=0\n\n\
[page2]\nmap=80\ntime=6\nscrolling=30\n\n[page3]\nmap=82\ntime=13\nscrolling=0\n\n\
[page4]\nmap=90\ntime=4\nscrolling=0\n\n[page5]\nmap=83\ntime=12\nscrolling=30\n\n\
[page6]\nmap=84\ntime=80\nscrolling=0\n\n\
[ani0]\npos=75x60\nfile=intro1.ani?0-2\nstart=9\ntime=3\n\n\
[ani1]\npos=75x60\nfile=intro1.ani?3-6\nstart=12\ntime=0.25\n\n\
[ani2]\npos=65x60\nfile=intro1.ani?7-9\nstart=17\ntime=3\n\n\
[ani3]\npos=65x60\nfile=intro1.ani?10-15\nstart=20\ntime=0.3\n\n\
[ani4]\npos=65x30\nfile=intro2.ani?0-2\nstart=24\ntime=4\n\n\
[ani5]\npos=42x20\nfile=intro2.ani?3-3\nstart=28\ntime=8.5\n\n\
[ani6]\npos=70x60\nfile=intro3.ani?0-2\nstart=44\ntime=3\n\n\
[ani7]\npos=70x60\nfile=intro3.ani?3-6\nstart=47\ntime=0.25\n\n\
[ani8]\npos=65x60\nfile=intro3.ani?7-9\nstart=50\ntime=3\n\n\
[ani9]\npos=65x60\nfile=intro3.ani?10-13\nstart=53\ntime=0.25\n\n\
[ani10]\npos=35x60\nfile=intro4.ani?0-2\nstart=59\ntime=3\n\n\
[ani11]\npos=35x60\nfile=intro4.ani?3-6\nstart=62\ntime=0.25\n\n\
[ani12]\npos=85x60\nfile=intro4.ani?7-9\nstart=66\ntime=3\n\n\
[ani13]\npos=85x60\nfile=intro4.ani?10-13\nstart=69\ntime=0.25\n";

    const DT: f32 = 1.0 / 60.0;

    fn run_until(intro: &mut Intro, mut done: impl FnMut(&Intro) -> bool) -> f32 {
        let mut t = 0.0f32;
        while !done(intro) {
            intro.step(DT);
            t += DT;
            assert!(t < 200.0, "never happened");
        }
        t
    }

    #[test]
    fn the_file_is_seven_pages_and_fourteen_animations() {
        let s = parse(INTRO_TXT).unwrap();
        let maps: Vec<u8> = s.pages.iter().map(|p| p.map).collect();
        assert_eq!(maps, [89, 81, 80, 82, 90, 83, 84]);
        let times: Vec<f32> = s.pages.iter().map(|p| p.time).collect();
        assert_eq!(times, [5.0, 8.0, 6.0, 13.0, 4.0, 12.0, 80.0]);
        let scrolls: Vec<f32> = s.pages.iter().map(|p| p.scrolling).collect();
        assert_eq!(scrolls, [0.0, 0.0, 30.0, 0.0, 0.0, 30.0, 0.0]);
        assert_eq!(s.animations.len(), 14);
        assert_eq!(s.animations[3], Animation { pos: (65, 60), start: 20.0, time: 0.3, stem: "intro1".into(), first: 10, last: 15 });
        assert_eq!(s.animations[5].frames(), 1);
        assert_eq!(s.animations[13].stem, "intro4");
    }

    /// `GetMapColorTable` knows maps 0–37, 80–84 and 89; the rest are map 0's
    /// (`BurnGfxData.cs:32-38`, `:113-124`). `GetRawColorTable` gives
    /// `intro2.ani` map 82's colours and `intro3`/`intro4` map 83's
    /// (`BurnGfxData.cs:65-70`).
    #[test]
    fn colours_follow_the_tables_upstream_registers() {
        assert_eq!(palette_of_map(89), 89);
        assert_eq!(palette_of_map(90), 0);
        assert_eq!(palette_of_ani("intro1"), 0);
        assert_eq!(palette_of_ani("intro2"), 82);
        assert_eq!(palette_of_ani("intro4"), 83);
    }

    /// `OnUpdate` (`IntroScene.cs:158-224`): a page stays `time` seconds,
    /// counted from when it is put in; then the blend goes black at 0.9 a
    /// second (1.11 s) and the next is put in. The first comes out of black
    /// too. So page `n` begins after the earlier pages' times and as many
    /// blends: page 1 at 5 + 1/0.9.
    #[test]
    fn the_pages_follow_their_times_and_the_blend() {
        let mut intro = Intro::new(parse(INTRO_TXT).unwrap());
        assert_eq!(intro.frame().black, 1.0, "the scene comes in out of black");
        let mut begun = 0.0f32;
        for (n, page) in intro.script.pages.clone().iter().enumerate().skip(1) {
            let t = run_until(&mut intro, |i| i.page() == n);
            let expected = intro.script.pages[n - 1].time + 1.0 / BLEND_SPEED;
            assert!((t - expected).abs() <= 2.0 * DT, "page {n} ({}) after {t}, expected {expected}", page.map);
            begun += t;
        }
        // 48 s of the first six and six blends: the last page from about
        // 54.7 s, and the start screen 80 s, a blend and 500 ms after.
        assert!((begun - (48.0 + 6.0 / BLEND_SPEED)).abs() < 0.2, "{begun}");
        let t = run_until(&mut intro, |i| i.done());
        assert!((t - (80.0 + 1.0 / BLEND_SPEED + LEAVE_HOLD)).abs() < 0.1, "{t}");
        assert!(!intro.muted());
    }

    /// The music is muted for its first two seconds (`IntroScene.cs:160-167`).
    #[test]
    fn the_music_waits_two_seconds() {
        let mut intro = Intro::new(parse(INTRO_TXT).unwrap());
        assert!(intro.muted());
        let t = run_until(&mut intro, |i| !i.muted());
        assert!((t - MUTED_FOR).abs() <= DT, "{t}");
    }

    /// An animation goes up from half a second before its `start` and comes
    /// down `time` after it; the next one, due as one ends, replaces it
    /// half a second early (`IntroScene.cs:199-223`). Its frames run at five
    /// a second within its range, over and over.
    #[test]
    fn animations_go_up_early_and_down_on_time() {
        let mut intro = Intro::new(parse(INTRO_TXT).unwrap());
        let up = run_until(&mut intro, |i| i.frame().animation.is_some());
        assert!((up - 8.5).abs() <= DT, "ani0 up at {up}");
        assert_eq!(intro.frame().animation, Some((0, 0)));
        assert_eq!(intro.page(), 1, "on map 81");
        // A fifth of a second later, the next frame.
        for _ in 0..12 {
            intro.step(DT);
        }
        assert_eq!(intro.frame().animation, Some((0, 1)));
        let t = up + 12.0 * DT + run_until(&mut intro, |i| i.frame().animation.map(|a| a.0) == Some(1));
        assert!((t - 11.5).abs() <= 2.0 * DT, "ani1 replaces ani0 at {t}");
        let t = t + run_until(&mut intro, |i| i.frame().animation.is_none());
        assert!((t - 12.25).abs() <= 2.0 * DT, "ani1 down at {t}");
        // ani5 is one frame for 8.5 s: frame 3 all along.
        run_until(&mut intro, |i| i.frame().animation.map(|a| a.0) == Some(5));
        for _ in 0..300 {
            intro.step(DT);
            assert_eq!(intro.frame().animation, Some((5, 3)));
        }
    }

    /// The scrolling pages move left at 30 pixels a second from where the
    /// page began (`scroll = 0` on each page), until the map's edge.
    #[test]
    fn a_scrolling_page_moves_left() {
        let mut intro = Intro::new(parse(INTRO_TXT).unwrap());
        run_until(&mut intro, |i| i.page() == 2);
        assert_eq!(intro.frame().scroll_x, 0);
        for _ in 0..60 {
            intro.step(DT);
        }
        assert!((intro.frame().scroll_x + 30).abs() <= 1, "{}", intro.frame().scroll_x);
        let tiles = [0u8; 4 + 20 * 5 * 2];
        let mut bytes = tiles;
        bytes[0] = 20;
        bytes[2] = 5;
        let map = PageMap::parse(&bytes).unwrap();
        // 640 wide in a 288 view: from 0 to -352 at most; 160 high, so 0.
        assert_eq!(map.offset(-30), (-30, 0));
        assert_eq!(map.offset(-1000), (-352, 0));
        assert_eq!(map.visible((-30, 0)), (0, 0, 11, 5));
        assert_eq!(map.visible((-352, 0)), (11, 0, 20, 5));
    }

    /// Any click (and Primary or Back) goes to the start screen: the scene
    /// freezes, the blend goes on to black at 0.9 a second, 500 ms more,
    /// and the start screen (`SetScene`'s `FadeOut(wait: true)`).
    #[test]
    fn a_skip_fades_out_and_ends() {
        let mut intro = Intro::new(parse(INTRO_TXT).unwrap());
        run_until(&mut intro, |i| i.time() >= 3.0);
        let black = intro.frame().black;
        assert!(black < 0.01, "{black}");
        let time = intro.time();
        intro.skip();
        assert!(intro.leaving());
        let t = run_until(&mut intro, |i| i.done());
        assert!((t - (LEAVE_HOLD + (1.0 - black) / BLEND_SPEED)).abs() <= 2.0 * DT, "{t}");
        assert_eq!(intro.frame().black, 1.0);
        assert_eq!(intro.time(), time, "the scene stood still");
        assert!(!intro.muted());
    }

    #[test]
    fn a_file_without_pages_is_refused() {
        assert!(parse("[intro]\npages=0\n").is_none());
        assert!(parse("[intro]\npages=1\n").is_none());
        assert!(parse("[intro]\npages=1\n[page0]\nmap=81\ntime=1\n[ani0]\n").is_some());
    }
}
