//! v1.3's `LocationPopulationCreator` (`314768a`, "populate New Village";
//! `Logic/Generation/LocationPopulationCreator.cs` at `v1.3`): a map's
//! `.txt` may name residents (`[residentN]`) and items for the ground and
//! the rooms, applied **only when a new game is created**, after the
//! initial items and before the players' starts. Only `MAT_038` (New
//! Village) has any: three dogs, a technician and a mercenary, meat on the
//! ground, a bottle and wire in the rooms.

use alloc::string::String;
use alloc::vec::Vec;

use crate::game::Point;
use crate::gamdat::{CharClass, CharacterRecord, Gamdat, Placed};

/// A name in `newburn.txt` rather than `burn.txt`: `name_id` from here up
/// is `newburn?(name_id - NEWBURN_NAME)`. `gam.dat`'s names are far below.
pub const NEWBURN_NAME: u16 = 0x8000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resident {
    pub class: CharClass,
    /// `name = newburn?93`: the database and line; none takes the
    /// template's.
    pub name: Option<(String, u16)>,
    /// `dialog`: the `MEN_NNN.TXT` he speaks from (0 when absent).
    pub dialog: u8,
    pub position: Point,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Population {
    pub residents: Vec<Resident>,
    /// `initial_ground_items`, `initial_room_items`: item ids, in order.
    pub ground: Vec<String>,
    pub rooms: Vec<String>,
}

impl Population {
    /// The root's two item lists and every section whose name starts with
    /// `resident` whose `class` is a dog, mercenary, technician or doctor
    /// (`Enum.TryParse`, anything else skipped).
    pub fn parse(text: &str) -> Population {
        let mut out = Population::default();
        let mut section: Option<String> = None;
        let mut cur: Option<(Option<CharClass>, Option<(String, u16)>, u8, Point)> = None;
        let flush = |cur: &mut Option<(Option<CharClass>, Option<(String, u16)>, u8, Point)>, out: &mut Population| {
            if let Some((Some(class), name, dialog, position)) = cur.take() {
                out.residents.push(Resident { class, name, dialog, position });
            }
        };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix('[') {
                flush(&mut cur, &mut out);
                let name = String::from(rest.trim_end_matches(']').trim());
                cur = name.starts_with("resident").then(|| (None, None, 0, Point::new(0, 0)));
                section = Some(name);
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let words = || v.split_whitespace().map(String::from).collect::<Vec<_>>();
            match (&section, cur.as_mut()) {
                (None, _) if k == "initial_ground_items" => out.ground = words(),
                (None, _) if k == "initial_room_items" => out.rooms = words(),
                (Some(_), Some(r)) => match k {
                    "class" => {
                        r.0 = match v {
                            "Dog" => Some(CharClass::Dog),
                            "Mercenary" => Some(CharClass::Mercenary),
                            "Technician" => Some(CharClass::Technician),
                            "Doctor" => Some(CharClass::Doctor),
                            _ => None,
                        }
                    }
                    "name" => {
                        r.1 = v.split_once('?').and_then(|(db, n)| Some((String::from(db), n.trim().parse().ok()?)))
                    }
                    "dialog" => r.2 = v.parse().unwrap_or(0),
                    "position" => {
                        if let Some((x, y)) = v.split_once('x') {
                            r.3 = Point::new(x.trim().parse().unwrap_or(0), y.trim().parse().unwrap_or(0));
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        flush(&mut cur, &mut out);
        out
    }
}

/// `GetNearestWalkablePosition` (`PathState.cs` at `v1.3`): the position
/// itself when it is inside the mask and walkable, else the centre of the
/// walkable cell nearest to it, the first in row order on a tie; the
/// position unchanged when nothing is walkable.
pub fn nearest_walkable(grid: &crate::zei::WalkGrid, at: Point) -> Point {
    let cell = crate::zei::MASK_CELL as i32;
    let (w, h) = (grid.width as i32, grid.height as i32);
    if at.x >= 0 && at.y >= 0 && at.x < w * cell && at.y < h * cell && grid.cell((at.x / cell) as usize, (at.y / cell) as usize) {
        return at;
    }
    let mut best: Option<(i64, Point)> = None;
    for y in 0..h {
        for x in 0..w {
            if !grid.cell(x as usize, y as usize) {
                continue;
            }
            let c = Point::new(x * cell + cell / 2, y * cell + cell / 2);
            let (dx, dy) = ((c.x - at.x) as i64, (c.y - at.y) as i64);
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, c));
            }
        }
    }
    best.map_or(at, |(_, p)| p)
}

/// The residents as records after `gam.dat`'s, in order, standing at
/// `location`. Each borrows its template's face, experience, food, water and
/// health (`template.Info`): a dog the first record named 178, any other the
/// first of its class placed somewhere; one with no template is skipped
/// (upstream logs a warning). A dog keeps the template's body; the others
/// get their class's first body (`GetBodyId(SetBodyId, 0)`). The name is the
/// section's or the template's; the dialog the section's. They accept the
/// three staples (`HireItems`: snake, meat, water bottle), as a record with
/// no hire item does. A dog's health is set by the class when the people
/// are built, as for every dog.
pub fn resident_records(g: &Gamdat, location: usize, pop: &Population, walk: Option<&crate::zei::WalkGrid>) -> Vec<CharacterRecord> {
    let mut out = Vec::new();
    for r in &pop.residents {
        let template = g.characters.iter().find(|c| match r.class {
            CharClass::Dog => c.name_id == 178,
            k => c.class == k && matches!(c.placed, Placed::At(_)),
        });
        let Some(t) = template else { continue };
        let name_id = match &r.name {
            Some((db, n)) if db == "newburn" => NEWBURN_NAME + n,
            Some((_, n)) => *n,
            None => t.name_id,
        };
        let sprite_id = match r.class {
            CharClass::Dog => t.sprite_id,
            k => crate::game::respawn_body(k, 0).and_then(|b| u8::try_from(b).ok()).unwrap_or(t.sprite_id),
        };
        out.push(CharacterRecord {
            placed: Placed::At(location),
            class: r.class,
            name_id,
            face_id: t.face_id,
            text_id: r.dialog,
            sprite_id,
            at: walk.map_or(r.position, |w| nearest_walkable(w, r.position)),
            water: t.water,
            food: t.food,
            health: t.health,
            experience: t.experience,
            hire_item_id: 0,
            employer: 0,
        });
    }
    out
}
