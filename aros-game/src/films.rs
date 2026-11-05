//! The films: v0.9.1's `DeathScene` and `VictoryScene` between turns, and the
//! rooms' films — `ChurchScene`, and `ImageScene` behind a scene door and in
//! front of two traders and a pub. A picture, its animations, a song, and
//! for some `burn` lines scrolled one at a time along the bottom with the
//! player's name in `|A`. A click or a key goes back to the scene before
//! (`WaitScene`, the location), or on to the trader or the pub.
//!
//! v1.2 keeps the same films in `scenes/*.txt` (`ImageScene`'s request);
//! where it changed one — the black frame around the picture, `film_09`'s
//! song, the church's subtitle colour, the trader's sound played once — the
//! game says which it is (`v12`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::gfx::{Align, Face, Target};
use crate::panels::{self, Anim};
use burngfx::pack::{Pack, PackedImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Death,
    Victory,
    /// `ChurchScene` (v1.2 `scenes/church.txt`).
    Church,
    /// `ImageScene` on `film_NN.pac`: 2, 3, 4 and 9 behind scene doors,
    /// 5 and 10 before a trader, 6 before a pub.
    Room(u8),
}

impl Kind {
    /// What the log calls it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Death => "DeathScene",
            Kind::Victory => "VictoryScene",
            Kind::Church => "ChurchScene",
            Kind::Room(2) => "ImageScene film_02",
            Kind::Room(3) => "ImageScene film_03",
            Kind::Room(4) => "ImageScene film_04",
            Kind::Room(5) => "ImageScene film_05",
            Kind::Room(6) => "ImageScene film_06",
            Kind::Room(9) => "ImageScene film_09",
            Kind::Room(10) => "ImageScene film_10",
            Kind::Room(_) => "ImageScene",
        }
    }

    /// The film a door shows, by its room id (`entrance.Background`), as
    /// `LocationScene` decides it: a trader `0x0D` or `0x11`, the pub `14`,
    /// the church, and the scene doors `0x0A`–`0x0C` and `0x12`. (`0x10`
    /// under a scene door is the church's id, so the church takes it.)
    pub fn of_door(room_id: u8) -> Option<Kind> {
        match room_id {
            0x0D => Some(Kind::Room(10)),
            0x11 => Some(Kind::Room(5)),
            14 => Some(Kind::Room(6)),
            16 => Some(Kind::Church),
            0x0A..=0x0C => Some(Kind::Room(room_id - 8)),
            0x12 => Some(Kind::Room(9)),
            _ => None,
        }
    }
}

/// One animation of a film: `Endless`, or played once after `Delay`
/// seconds and held on its last frame; `ReverseAnimation` from the last.
struct Track<'a> {
    anim: Anim<'a>,
    once_after_ms: Option<u32>,
    reverse: bool,
}

pub struct Film<'a> {
    pub kind: Kind,
    /// The player the news is about.
    pub name: String,
    since: u32,
    picture: Option<PackedImage<'a>>,
    tracks: Vec<Track<'a>>,
    v12: bool,
}

/// `SpriteAnimation.Speed`'s default: five frames a second.
const SPEED_TENTHS: u32 = 50;

fn track<'a>(pack: &Pack<'a>, name: &str, at: (i32, i32), tenths: u32, pause_ms: u32, reverse: bool) -> Track<'a> {
    Track { anim: panels::anim(pack, name, at, tenths, pause_ms), once_after_ms: None, reverse }
}

/// A film's NewGfx description, as the packer keeps it (`ng/scenes/<stem>`,
/// the pictures under `ng/fs/<stem>`), read as `GuiImage.Reload` reads it:
/// each `[animationN]`'s frames, `position`, `speed`, `interval_margin`,
/// `reverse`.
fn newgfx_film<'a>(pack: &Pack<'a>, scene: &str) -> Option<(PackedImage<'a>, Vec<Track<'a>>)> {
    let stem = scene.strip_prefix("scene/")?;
    let mut key = String::from("ng/scenes/");
    key.push_str(stem);
    let config = burngfx::config::Config::parse(core::str::from_utf8(pack.get(&key)?).ok()?)?;
    let mut fs = String::from("fs/");
    fs.push_str(stem);
    let picture = crate::start_view::newgfx_image(pack, &fs)?;
    let count: usize = config.section("image")?.get("animations").and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    let mut tracks = Vec::new();
    for a in 0..count {
        let Some(sec) = config.section(&alloc::format!("animation{a}")) else { continue };
        let frames: Vec<PackedImage<'a>> = (0..80)
            .map_while(|f| crate::start_view::newgfx_image(pack, &alloc::format!("{fs}/{a}/{f}")))
            .collect();
        let pos = sec.get("position").and_then(|p| {
            let (x, y) = p.trim().split_once('x')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        }).unwrap_or((0, 0));
        // `speed` in frames a second, to tenths (6.5); `IntervalMargin` in
        // whole seconds.
        let tenths = sec.get("speed").and_then(burngfx::ratio::Ratio::parse)
            .map_or(SPEED_TENTHS, |r| (r * burngfx::ratio::Ratio::int(10)).round_even() as u32);
        let pause = sec.get("interval_margin").and_then(|m| m.trim().parse::<u32>().ok()).unwrap_or(0) * 1000;
        let reverse = sec.get("reverse").is_some_and(|r| matches!(r.trim(), "true" | "yes" | "1" | "on"));
        let anim = panels::Anim { frames, at: pos, fps: tenths / 10, fps_tenths: tenths, pause_ms: pause };
        tracks.push(Track { anim, once_after_ms: None, reverse });
    }
    Some((picture, tracks))
}

impl<'a> Film<'a> {
    pub fn open(pack: &Pack<'a>, kind: Kind, name: String, now: u32, v12: bool) -> Film<'a> {
        let (stem, tracks): (&str, Vec<Track<'a>>) = match kind {
            Kind::Death => (
                "scene/film_00",
                alloc::vec![
                    track(pack, "film00a", (16, 68), SPEED_TENTHS, 0, false),
                    track(pack, "film00b", (176, 86), SPEED_TENTHS, 0, false),
                    track(pack, "film00c", (264, 77), SPEED_TENTHS, 0, false),
                    // The bird: `Endless = false`, `Delay = 8`.
                    Track { anim: panels::anim(pack, "film00bird", (166, 20), SPEED_TENTHS, 0), once_after_ms: Some(8000), reverse: false },
                ],
            ),
            Kind::Victory => (
                "scene/film_01",
                alloc::vec![
                    track(pack, "film01a", (170, 43), SPEED_TENTHS, 0, false),
                    track(pack, "film01b", (33, 23), SPEED_TENTHS, 0, false),
                    track(pack, "film01c", (126, 121), SPEED_TENTHS, 0, false),
                ],
            ),
            // `film_08.ani??p` at 200,117, 4.5 a second, a second between.
            Kind::Church => ("scene/film_08", alloc::vec![track(pack, "film08", (200, 117), 45, 1000, false)]),
            Kind::Room(5) => (
                "scene/film_05",
                alloc::vec![
                    track(pack, "film05a", (98, 120), 65, 4000, false),
                    track(pack, "film05b", (77, 89), 65, 5000, true),
                    track(pack, "film05c", (106, 59), 65, 0, false),
                ],
            ),
            Kind::Room(6) => (
                "scene/film_06",
                alloc::vec![
                    track(pack, "film06a", (49, 46), 65, 6000, false),
                    track(pack, "film06b", (179, 96), 65, 0, false),
                ],
            ),
            Kind::Room(10) => ("scene/film_10", alloc::vec![track(pack, "film10", (125, 92), 50, 3000, false)]),
            Kind::Room(2) => ("scene/film_02", Vec::new()),
            Kind::Room(3) => ("scene/film_03", Vec::new()),
            Kind::Room(4) => ("scene/film_04", Vec::new()),
            Kind::Room(_) => ("scene/film_09", Vec::new()),
        };
        // NewGfx: `classic_newgfx/scenes/film_NN.txt` instead, its own
        // picture and animations at its `sprite_scale`.
        if let Some((picture, tracks)) = crate::gfx::newgfx().then(|| newgfx_film(pack, stem)).flatten() {
            return Film { kind, name, since: now, picture: Some(picture), tracks, v12 };
        }
        Film {
            kind,
            name,
            since: now,
            picture: crate::start_view::image(pack, stem),
            tracks,
            v12,
        }
    }

    /// What the pack is missing for it, to be said once.
    pub fn missing(&self) -> Option<&'static str> {
        if !(self.picture.is_none() || self.tracks.iter().any(|t| t.anim.frames.is_empty())) {
            return None;
        }
        Some(match self.kind {
            Kind::Death => "film_00.pac / film_00.ani",
            Kind::Victory => "film_01.pac / film_01.ani",
            Kind::Church => "film_08.pac / film_08.ani",
            Kind::Room(5) => "film_05.pac / film_05.ani",
            Kind::Room(6) => "film_06.pac / film_06.ani",
            Kind::Room(10) => "film_10.pac / film_10.ani",
            Kind::Room(_) => "the film's .pac",
        })
    }

    /// `Music` of the scene: a song's name, or a file under `sounds/`; none
    /// for v1.2's `film_08` (`song=`), which no door reaches.
    pub fn song(&self) -> Option<&'static str> {
        Some(match self.kind {
            Kind::Death => "death",
            Kind::Victory => "victory",
            Kind::Church => "church",
            Kind::Room(2) => "ruin",
            Kind::Room(3) => "death",
            Kind::Room(4) => "building",
            Kind::Room(5) | Kind::Room(10) => "sounds/trader.ogg",
            Kind::Room(6) => "pub",
            // v0.9.1 gave the ruined filling station the death song; v1.2
            // its own.
            Kind::Room(9) => {
                if self.v12 {
                    "texaco"
                } else {
                    "death"
                }
            }
            Kind::Room(_) => return None,
        })
    }

    /// v1.2's `loop=false` for the trader's sound; v0.9.1 played every
    /// scene's music round (`Scene.Music`, `Play`).
    pub fn looped(&self) -> bool {
        !(self.v12 && matches!(self.kind, Kind::Room(5) | Kind::Room(10)))
    }

    /// v1.2's `frame=yes`: a black border 32 wide round the game's whole
    /// view, over the picture's edges. The church, death and victory have
    /// none.
    pub fn framed(&self) -> bool {
        self.v12 && matches!(self.kind, Kind::Room(_))
    }

    /// Which `burn` line is showing, if any: `txtoffset + (int)txtline`,
    /// `txtline += elapsed * rate` until `txtlines`.
    pub fn line(&self, now: u32) -> Option<u32> {
        let ms = now.wrapping_sub(self.since);
        let (offset, lines, ms_per_line) = match self.kind {
            // `txtline += elapsed * 0.05f`, nine lines from 600.
            Kind::Death => (600, 9, 20_000),
            // `elapsed * 0.2f`, fifty lines from 610.
            Kind::Victory => (610, 50, 5_000),
            // `elapsed * 0.25f`, nine lines from 590.
            Kind::Church => (590, 9, 4_000),
            Kind::Room(_) => return None,
        };
        let n = ms / ms_per_line;
        (n < lines).then_some(offset + n)
    }

    /// Drawn in its own 320 × 200, which the caller has centred.
    pub fn draw(&self, t: &mut Target, font: Option<&Face>, line: Option<&str>, now: u32) {
        let ms = now.wrapping_sub(self.since);
        // `Size = 320 × 200`: the picture centred on it (NewGfx's are 360
        // wide), the animations placed from its corner (`GuiImage.Draw`).
        let mut corner = (0, 0);
        if let Some(p) = &self.picture {
            corner = ((320 - p.game_w()) / 2, (200 - p.game_h()) / 2);
            t.blit(p, corner.0, corner.1);
        }
        for track in &self.tracks {
            let a = &track.anim;
            let n = a.frames.len();
            if n == 0 {
                continue;
            }
            let frame = match track.once_after_ms {
                None if track.reverse => n - 1 - a.frame_at(ms),
                None => a.frame_at(ms),
                Some(delay) if ms < delay => 0,
                Some(delay) => (((ms - delay) * SPEED_TENTHS / 10_000) as usize).min(n - 1),
            };
            if let Some(img) = a.frames.get(frame) {
                t.blit(img, corner.0 + a.at.0, corner.1 + a.at.1);
            }
        }
        if let (Some(f), Some(text)) = (font, line) {
            let text = text.replace("|A", &self.name);
            // `new PixelColor(72, 72, 76)`, centred at (160, 185); v1.2's
            // church writes in `ClassicColors.LightGray` (212, 212, 212).
            let colour = if self.v12 && self.kind == Kind::Church { 0x00D4_D4D4 } else { 0x0048_484C };
            f.draw(t, &text, 160, 200 - 15, Align::Centre, Align::Start, Some(colour));
        }
    }
}
