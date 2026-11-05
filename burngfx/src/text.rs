//! The text side of a map: room settings, and the string tables they point at.
//!
//! A map's companion `.txt` is an INI-ish file with one `[roomN]` section per
//! entrance. Its `title` is not a name but a reference of the form
//! `<database>?<index>` — `burn?4` means line 4 of `lang/<lang>/BURN.TXT`.
//! Resolving one is what turns entrance 4 into "Nob Hill".

use crate::game::Point;
use alloc::string::String;
use alloc::vec::Vec;

/// One `[roomN]` section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    /// `normal`, `water`, or empty when the map does not say.
    pub kind: String,
    /// A `db?index` reference, or empty.
    pub title: String,
    /// A `raum_N.pac` background, or empty.
    pub image: String,
}

impl Room {
    /// The `.txt`'s own answer to which interior to draw, when it gives one.
    ///
    /// `BurnmapProcessor` matches the nine known file names and falls back to
    /// `raum_1.pac` for anything else it is handed, so an unreadable name is
    /// a picture rather than a hole. `None` here means the file said nothing,
    /// which is different: then the classic map's room id decides.
    pub fn interior(&self) -> Option<u8> {
        if self.image.is_empty() {
            return None;
        }
        Some(match self.image.as_str() {
            "raum_0.pac" => 0,
            "raum_2.pac" => 2,
            "raum_3.pac" => 3,
            "raum_4.pac" => 4,
            "raum_5.pac" => 5,
            "raum_6.pac" => 6,
            "raum_7.pac" => 7,
            "raum_8.pac" => 8,
            _ => 1,
        })
    }
}

/// A map's `.txt`, as far as we need it.
#[derive(Debug, Default)]
pub struct MapSettings {
    /// Rooms in index order. May be longer than the map's entrance list —
    /// MAT_000 declares 39 and the binary carries 38.
    pub rooms: Vec<Room>,
    /// Where the character arrives, written as `entry_point = 252x229`.
    ///
    /// Almost always absent: the classic entry points live in `GAMDAT`, and of
    /// the shipped maps only `MAT_038` carries one. `None` means "we do not
    /// know", never "the middle of the map".
    pub entry_point: Option<Point>,
    /// `water_refresh` — how much the location's spring yields per turn. The
    /// original calls this the source's `Water`, and starts the reserve at the
    /// same number.
    pub water_refresh: Option<i32>,
    /// `water_capacity` — the most the reserve is allowed to hold.
    pub water_capacity: Option<i32>,
}

impl MapSettings {
    pub fn room(&self, index: usize) -> Option<&Room> {
        self.rooms.get(index)
    }

    /// Does this room draw from the location's spring? `type = water`.
    pub fn is_water_source(&self, index: usize) -> bool {
        self.room(index).is_some_and(|r| r.kind == "water")
    }

    /// Parse the room sections. Unknown keys and sections are ignored rather
    /// than rejected: the file carries more than this reads.
    pub fn parse(text: &str) -> MapSettings {
        let mut rooms: Vec<Room> = Vec::new();
        let mut entry_point = None;
        let mut water_refresh = None;
        let mut water_capacity = None;
        let mut current: Option<usize> = None;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.strip_prefix("room").and_then(|n| n.parse::<usize>().ok());
                if let Some(index) = current {
                    if rooms.len() <= index {
                        rooms.resize(index + 1, Room::default());
                    }
                }
                continue;
            }

            let Some((key, value)) = line.split_once('=') else { continue };
            let Some(index) = current else {
                // Keys before the first [roomN] belong to the map itself.
                let value = value.trim();
                match key.trim() {
                    "entry_point" => entry_point = parse_point(value),
                    "water_refresh" => water_refresh = value.parse().ok(),
                    "water_capacity" => water_capacity = value.parse().ok(),
                    _ => {}
                }
                continue;
            };
            let value = String::from(value.trim());
            match key.trim() {
                "type" => rooms[index].kind = value,
                "title" => rooms[index].title = value,
                "image" => rooms[index].image = value,
                _ => {}
            }
        }
        MapSettings { rooms, entry_point, water_refresh, water_capacity }
    }
}

/// `252x229` — the way the map files write a position.
fn parse_point(s: &str) -> Option<Point> {
    let (x, y) = s.split_once('x')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// A `db?index` reference, split apart.
pub struct TextRef<'a> {
    pub database: &'a str,
    pub index: usize,
}

impl<'a> TextRef<'a> {
    pub fn parse(s: &'a str) -> Option<TextRef<'a>> {
        let (database, index) = s.split_once('?')?;
        if database.is_empty() {
            return None;
        }
        Some(TextRef { database, index: index.parse().ok()? })
    }

    /// The file this reference names, e.g. `burn?4` -> `BURN.TXT`.
    ///
    /// The originals are upper case and the remaster's own additions are
    /// lower; callers that care should try both.
    pub fn file_name(&self) -> String {
        let mut name = String::new();
        for c in self.database.chars() {
            name.push(c.to_ascii_uppercase());
        }
        name.push_str(".TXT");
        name
    }
}

/// One of the game's string tables: one string per line.
pub struct TextDb {
    lines: Vec<String>,
}

impl TextDb {
    /// `}` terminates a string — the original's own tables carry it on every
    /// line — and trailing whitespace is dropped, as the C# reader does.
    pub fn parse(text: &str) -> TextDb {
        TextDb {
            lines: text
                .lines()
                .map(|l| {
                    let l = l.split('}').next().unwrap_or(l);
                    String::from(l.trim_end())
                })
                .collect(),
        }
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
