//! `burn_gfx/gam.dat` — the original game's starting state.
//!
//! This is where the things the rest of this project has been standing in for
//! actually live: entry points, the road graph, and each location's water.
//!
//! Locations, players, characters and items are read; the layout comes from
//! `Burntime.BurnGfx/Save/SaveGame.cs` and `Save/Item.cs`, not from guessing.
//!
//! ```text
//! magic   4 bytes  "BURN"
//! header  18 bytes  char id offset, active player, day, difficulty, ...
//! 37 records of 50 bytes
//! (characters, items, players — not read)
//! ```
//!
//! **It describes 37 locations, and the game has 38.** The last one, New
//! Village, is the remaster's own addition and comes from `MAT_038.txt`
//! ([`add_remaster_location`], as `LocationCreator.Create` adds it upstream
//! since v1.2); the world joins the two.

use alloc::vec::Vec;

use crate::game::Point;
use crate::text::TextRef;

/// The original file always holds this many.
pub const LOCATION_COUNT: usize = 37;

const MAGIC: &[u8; 4] = b"BURN";
const HEADER_LEN: usize = 22;
const RECORD_LEN: usize = 50;
/// Four player slots follow the locations, after two bytes of padding.
pub const PLAYER_COUNT: usize = 4;
const PLAYER_LEN: usize = 62;
/// Padding between the player slots and the character table.
const AFTER_PLAYERS: usize = 0x190;
/// Every record in the file, players included. The first four *are* the
/// players; `LoadNPCs` walks all of them and lets the location test decide.
pub const CHARACTER_COUNT: usize = 309;
const CHARACTER_LEN: usize = 46;

#[derive(Debug, PartialEq, Eq)]
pub enum GamdatError {
    NotAGamdat,
    /// Too short to hold the location table.
    Truncated,
}

/// What kind of thing is killing you slowly here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Danger {
    None,
    Gas,
    /// The original writes 3 for radiation and anything else non-zero for gas.
    Radiation,
}

/// One of the 37 classic locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationRecord {
    /// Index into the `burn` string table — the location's name.
    pub title_id: u16,
    /// Where the character arrives, in map pixels.
    pub entry_point: Point,
    pub is_city: bool,

    /// What the spring holds right now. Note this is not the refresh rate:
    /// Nob Hill opens at 0 and yields 5 a turn.
    pub water_reserve: i32,
    pub water_capacity: i32,
    /// The original calls this `WaterSource`; it is the per-turn yield.
    pub water_refresh: i32,

    /// Which player holds it, as the save records. Zero in a new game: the
    /// original claims a location only when somebody is left camped in it.
    pub owner: u16,
    /// Which productions may be selected here, as indices, `None` for an
    /// empty slot. Four flag bytes in the file; `Production1 == 1` means
    /// production 0 is available, and so on.
    pub available_products: [Option<usize>; 4],
    /// What it is producing now. The file stores the index plus 51, and zero
    /// for none — `(Producing == 0) ? -1 : Producing - 51`.
    pub producing: Option<usize>,

    pub danger: Danger,
    pub danger_amount: u16,
    pub life_reduction: u16,

    /// Up to four roads out. Each is `(neighbour, travel days, way id)`.
    ///
    /// The file stores identifiers counted from one and uses zero for "no
    /// road", so these are `Option` rather than the `-1` the C# uses.
    pub roads: Vec<Road>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Road {
    /// The location this road leads to, as a zero-based index.
    pub neighbour: usize,
    /// Days of travel.
    pub days: u8,
    /// Which drawn path on the world map, zero-based, if any.
    pub way: Option<usize>,
}

impl LocationRecord {
    /// The line of `BURN.TXT` that names this location.
    ///
    /// **`title_id` is counted from one and the string table is not**, so the
    /// line is one less. Getting that wrong is quiet — every location takes
    /// its neighbour's name and the result still reads like a place in this
    /// game — so the arithmetic lives here, once, rather than at each call.
    /// `MAT_000.txt` names the same places by giving the world map's rooms
    /// `title = burn?<room index>`; the two agree, and a test says so.
    ///
    /// `None` for a record that names nothing.
    pub fn title_ref(&self) -> Option<TextRef<'static>> {
        if self.title_id == 0 {
            return None;
        }
        Some(TextRef { database: "burn", index: self.title_id as usize - 1 })
    }

    /// Does this location have a spring at all?
    pub fn has_water(&self) -> bool {
        self.water_capacity > 0 || self.water_refresh > 0
    }
}

/// One of the four players the 1993 save carries.
///
/// The interesting field is where they are. A new game has all four standing
/// in different towns, and the first is the one this project plays.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlayerRecord {
    /// The sprite of their flag. Four consecutive frames from here.
    pub flag_id: u16,
    /// Where they are, zero-based. While travelling this is still the place
    /// they set out from: `Player.Turn` only moves it on arrival.
    pub location: usize,
    /// Days left of a journey already under way, and where to. Zero in a new
    /// game, but a saved one can be caught mid-road.
    pub travel_days: u16,
    pub travel_to: Option<usize>,
    /// Where they set out from, which is the only place they may turn back to
    /// when the destination turns out to be somebody else's.
    pub from: Option<usize>,
}

/// What `LoadNPCs` decides a record is.
///
/// **The numbers are `CharType` and come straight from the file**, except
/// `Dog`, which no record actually stores. There are two rules for it in the
/// source and they disagree: `Save/Character.Type` says `CharType == 4 &&
/// FaceId == 0`, while `GameCreation.LoadNPCs` — the code that builds the
/// world — says a `Mutant` record whose `NameId` is 178. We follow
/// `LoadNPCs`. In the classic `gam.dat` the difference is two records (both
/// at location 36, `FaceId = 20`): 66 dogs by the rule that builds them, 64
/// by the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharClass {
    Mercenary,
    Technician,
    Doctor,
    Boss,
    Mutant,
    Trader,
    /// Never stored; see the note above.
    Dog,
    /// A CharType the enum does not cover.
    Unknown(u8),
}

impl CharClass {
    /// `LoadNPCs`, in its own order: trader first, then the mutant branch
    /// with its dog test, then everything else by its raw `CharType`.
    fn of(char_type: u8, name_id: u16) -> CharClass {
        match char_type {
            5 => CharClass::Trader,
            4 if name_id == 178 => CharClass::Dog,
            4 => CharClass::Mutant,
            0 => CharClass::Mercenary,
            1 => CharClass::Technician,
            2 => CharClass::Doctor,
            3 => CharClass::Boss,
            other => CharClass::Unknown(other),
        }
    }

    /// Can this class be offered work at all?
    ///
    /// `Dialog.GetGreetingConversation` decides in one `if`/`else if`, and
    /// the order matters: **a trader takes the first branch and is offered
    /// trade**, so the hire branch never sees one. The hire branch itself
    /// then excludes mutants and dogs. What is left is the four professions.
    pub fn can_be_hired(&self) -> bool {
        matches!(
            self,
            CharClass::Mercenary | CharClass::Technician | CharClass::Doctor | CharClass::Boss
        )
    }
}

/// Where a record puts its character, which is what `LoadNPCs` branches on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placed {
    /// `LocationId` names a real location: index, counted from zero.
    At(usize),
    /// `LocationId == 0`. **This is how a save stores a recruited
    /// companion**, not an unused slot: `SaveGame` reads such a record into
    /// `player[EmployerId].Group`, and `LoadNPCs` builds nothing for it
    /// because neither of its branches fires. A fresh `gam.dat` has none.
    InGroupOf,
    /// `LocationId` is out of range but not zero. `LoadNPCs` still builds a
    /// trader from it — one with no home area. Five records in the classic
    /// file are like this.
    HomelessTrader,
}

/// One of the 309 records in the character table.
#[derive(Clone, Debug)]
pub struct CharacterRecord {
    pub placed: Placed,
    pub class: CharClass,
    pub name_id: u16,
    pub face_id: u8,
    pub text_id: u8,
    pub sprite_id: u8,
    pub at: Point,
    pub water: u8,
    pub food: u8,
    pub health: u8,
    pub experience: u8,
    /// `HireItemId`, unresolved: an index into the item types, or zero.
    /// [`CharacterRecord::hire_item_ids`] turns it into the actual list.
    pub hire_item_id: u16,
    /// Whose employee this already is, counted from one. Zero is nobody.
    pub employer: u16,
}

impl CharacterRecord {
    /// What this character accepts as payment, as item-type indices.
    ///
    /// `LoadNPCs`: with no special item, the three staples. With one, the
    /// staples **as well**, but only if its id is below 4 — and then the
    /// named item on top either way. Indices, because the item table is what
    /// resolves them, and it differs between game modes.
    pub fn hire_item_ids(&self, snake: usize, meat: usize, water_bottle: usize) -> Vec<usize> {
        let mut items = Vec::new();
        if self.hire_item_id == 0 || self.hire_item_id < 4 {
            items.push(snake);
            items.push(meat);
            items.push(water_bottle);
        }
        if self.hire_item_id != 0 {
            items.push(self.hire_item_id as usize);
        }
        items
    }
}

#[derive(Debug)]
pub struct Gamdat {
    pub locations: Vec<LocationRecord>,
    /// The four players, in the save's own order.
    pub players: Vec<PlayerRecord>,
    /// Every character record, the four player slots included. Reading them
    /// all keeps the indices the file's own.
    pub characters: Vec<CharacterRecord>,
    /// The item table, where it is in the file.
    pub items: Vec<ItemRecord>,
    /// The day the game starts on.
    pub day: u16,
    pub difficulty: u16,
}

pub fn parse(bytes: &[u8]) -> Result<Gamdat, GamdatError> {
    if bytes.len() < 4 || &bytes[..4] != MAGIC {
        return Err(GamdatError::NotAGamdat);
    }
    let end = HEADER_LEN + LOCATION_COUNT * RECORD_LEN;
    if bytes.len() < end {
        return Err(GamdatError::Truncated);
    }

    // Header fields we have a use for. The rest is player and turn bookkeeping.
    let day = u16le(bytes, 12);
    let difficulty = u16le(bytes, 20);

    let mut locations = Vec::with_capacity(LOCATION_COUNT);
    for i in 0..LOCATION_COUNT {
        let at = HEADER_LEN + i * RECORD_LEN;
        let r = &bytes[at..at + RECORD_LEN];

        // Offsets from SaveGame.cs, in the order it reads them.
        let title_id = u16le(r, 0);
        let water_reserve = u16le(r, 12) as i32;
        let water_capacity = u16le(r, 14) as i32;
        let water_refresh = u16le(r, 16) as i32;
        let entry_point = Point::new(u16le(r, 18) as i32, u16le(r, 20) as i32);
        let is_city = u16le(r, 22) != 0;
        let owner = u16le(r, 2);
        let mut available_products = [None; 4];
        for (k, slot) in available_products.iter_mut().enumerate() {
            *slot = (r[24 + k] == 1).then_some(k);
        }
        // 51 is the first production; anything below it, zero included, is
        // none at all.
        let raw_producing = u16le(r, 28);
        let producing = (raw_producing >= 51).then(|| raw_producing as usize - 51);
        let danger = match r[45] {
            0 => Danger::None,
            3 => Danger::Radiation,
            _ => Danger::Gas,
        };
        let danger_amount = u16le(r, 46);
        let life_reduction = u16le(r, 48);

        // Four parallel arrays: neighbour, travel time, way. A neighbour of
        // zero ends the list — the ones after it are padding, not roads.
        let mut roads = Vec::new();
        for k in 0..4 {
            let neighbour = r[32 + k];
            if neighbour == 0 {
                continue;
            }
            let way = r[40 + k];
            roads.push(Road {
                neighbour: neighbour as usize - 1,
                days: r[36 + k],
                way: if way == 0 { None } else { Some(way as usize - 1) },
            });
        }

        locations.push(LocationRecord {
            title_id,
            entry_point,
            is_city,
            owner,
            available_products,
            producing,
            water_reserve,
            water_capacity,
            water_refresh,
            danger,
            danger_amount,
            life_reduction,
            roads,
        });
    }

    // The four player slots, after two bytes of padding. A file too short for
    // them is not an error: everything above this point is still readable,
    // and a caller that does not ask about players is unaffected.
    let mut players = Vec::new();
    let base = end + 2;
    for i in 0..PLAYER_COUNT {
        let at = base + i * PLAYER_LEN;
        let Some(r) = bytes.get(at..at + PLAYER_LEN) else { break };
        // Zero means "nowhere" in every one of these, and they are stored
        // counted from one.
        let index = |b: u8| (b != 0).then(|| b as usize - 1);
        let to = u16le(r, 4);
        players.push(PlayerRecord {
            flag_id: u16le(r, 0),
            location: index(r[46]).unwrap_or(0),
            travel_days: u16le(r, 2),
            travel_to: (to != 0).then(|| to as usize - 1),
            from: index(r[58]),
        });
    }

    // The character table. As with the players, a file too short for it
    // yields none rather than an error.
    let mut characters = Vec::new();
    let base = base + PLAYER_COUNT * PLAYER_LEN + AFTER_PLAYERS;
    for i in 0..CHARACTER_COUNT {
        let at = base + i * CHARACTER_LEN;
        let Some(r) = bytes.get(at..at + CHARACTER_LEN) else { break };
        let location_id = u16le(r, 0);
        let name_id = u16le(r, 6);
        characters.push(CharacterRecord {
            placed: match location_id {
                0 => Placed::InGroupOf,
                id if (id as usize) <= LOCATION_COUNT => Placed::At(id as usize - 1),
                _ => Placed::HomelessTrader,
            },
            class: CharClass::of(r[8], name_id),
            name_id,
            face_id: r[3],
            text_id: r[38],
            sprite_id: r[40],
            at: Point { x: u16le(r, 12) as i32, y: u16le(r, 14) as i32 },
            water: r[2],
            food: r[5],
            health: r[45],
            experience: r[44],
            hire_item_id: u16le(r, 36),
            employer: u16le(r, 10),
        });
    }

    // The item table: fourteen bytes after the characters, 1199 records of
    // twelve (`SaveGame.cs`). A file too short for it yields none.
    let mut items = Vec::new();
    let base = base + CHARACTER_COUNT * CHARACTER_LEN + AFTER_CHARACTERS;
    for i in 0..ITEM_COUNT {
        let at = base + i * ITEM_LEN;
        let Some(r) = bytes.get(at..at + ITEM_LEN) else { break };
        items.push(ItemRecord {
            sprite_id: r[0],
            value: r[1],
            title_id: u16le(r, 2),
            owner: u16le(r, 4),
            damage: u16le(r, 6),
            x_or_room: u16le(r, 8),
            y: u16le(r, 10),
        });
    }

    Ok(Gamdat { locations, players, characters, items, day, difficulty })
}

const AFTER_CHARACTERS: usize = 0xE;
/// `ItemCount`.
pub const ITEM_COUNT: usize = 1199;
const ITEM_LEN: usize = 12;

/// Who holds one of the file's items (`ItemOwnerType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemOwner {
    /// `0xffff`: not in the world.
    Pool,
    /// On the ground: at a location, at a position.
    Dropped { location: usize, x: u16, y: u16 },
    /// In a room of a location.
    Room { location: usize, room: usize },
    /// In a character's pack, by record index.
    Character(usize),
    Unknown,
}

/// One record of the item table (`ItemInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRecord {
    /// The type, as the classic tables' `burngfx`.
    pub sprite_id: u8,
    /// Food or water in it, for the kinds that hold any.
    pub value: u8,
    pub title_id: u16,
    pub owner: u16,
    pub damage: u16,
    pub x_or_room: u16,
    pub y: u16,
}

impl ItemRecord {
    /// `ItemInfo.CityId`: `(Owner - 0x29EC)` in steps of `0x32`, counted from
    /// one, and nought for anything outside 1..=36 — C#'s truncating
    /// arithmetic, negative values included.
    fn city_id(&self) -> i32 {
        let mut id = self.owner as i32 - 0x29EC;
        id = (id - id % 0x32) / 0x32 + 1;
        if !(0..=36).contains(&id) { 0 } else { id }
    }

    /// `Item.OwnerType` with its ids, in the source's order of tests.
    pub fn owner(&self) -> ItemOwner {
        if self.owner == 0xffff {
            return ItemOwner::Pool;
        }
        let city = self.city_id();
        if city != 0 {
            // `RoomId`: the low byte when the high one is nought, else the high.
            let room = if self.x_or_room >> 8 == 0 { self.x_or_room } else { self.x_or_room >> 8 };
            return ItemOwner::Room { location: city as usize - 1, room: room as usize };
        }
        if self.owner >> 8 >= 0x33 {
            // `(Owner - 0x33b0) / 46`, truncating: below the first record it
            // names no character.
            let id = (self.owner as i32 - 0x33b0) / 46;
            return if self.owner >= 0x33b0 { ItemOwner::Character(id as usize) } else { ItemOwner::Unknown };
        }
        if self.owner >> 8 == 0 {
            // `DroppedLocationId = Owner - 1`.
            return match (self.owner as usize).checked_sub(1) {
                Some(location) => ItemOwner::Dropped { location, x: self.x_or_room, y: self.y },
                None => ItemOwner::Unknown,
            };
        }
        ItemOwner::Unknown
    }
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}


/// The 38th location's index: New Village, `MAT_038` (`Location.Id` 37).
pub const NEW_VILLAGE: usize = LOCATION_COUNT;

/// The root keys of a remaster map's `.txt` that `LocationCreator.Create`
/// reads for a location `gam.dat` does not have (`maps/mat_038.txt`):
/// `water_refresh` (the reserve starts at it), `water_capacity`, `ways` and
/// `way_lengths` (zero-based neighbours, days), `available_products`,
/// `danger`, `city`, `entry_point`. Unknown keys and the sections are
/// ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemasterLocation {
    pub entry_point: Option<Point>,
    pub water_refresh: i32,
    pub water_capacity: i32,
    pub is_city: bool,
    pub danger: Danger,
    pub available_products: Vec<usize>,
    pub ways: Vec<(usize, u8)>,
}

impl Default for Danger {
    fn default() -> Danger {
        Danger::None
    }
}

impl RemasterLocation {
    pub fn parse(text: &str) -> RemasterLocation {
        let mut out = RemasterLocation::default();
        let (mut ways, mut lengths): (Vec<usize>, Vec<u8>) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                break; // the root section ends at the first section
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let ints = || v.split_whitespace().filter_map(|n| n.parse::<i64>().ok()).collect::<Vec<_>>();
            match k {
                "entry_point" => {
                    if let Some((x, y)) = v.split_once('x') {
                        if let (Ok(x), Ok(y)) = (x.trim().parse(), y.trim().parse()) {
                            out.entry_point = Some(Point::new(x, y));
                        }
                    }
                }
                "water_refresh" => out.water_refresh = v.parse().unwrap_or(0),
                "water_capacity" => out.water_capacity = v.parse().unwrap_or(0),
                "city" => out.is_city = v.eq_ignore_ascii_case("true"),
                // `resources.GetData(danger) as Danger`: `danger@gas??N`,
                // `danger@radiation??N`; empty is none.
                "danger" => {
                    out.danger = if v.contains("radiation") { Danger::Radiation } else if v.contains("gas") { Danger::Gas } else { Danger::None }
                }
                "available_products" => out.available_products = ints().into_iter().filter(|&n| n >= 0).map(|n| n as usize).collect(),
                "ways" => ways = ints().into_iter().filter(|&n| n >= 0).map(|n| n as usize).collect(),
                "way_lengths" => lengths = ints().into_iter().map(|n| n.clamp(0, 255) as u8).collect(),
                _ => {}
            }
        }
        out.ways = ways.iter().enumerate().map(|(i, &n)| (n, lengths.get(i).copied().unwrap_or(0))).collect();
        out
    }
}

/// `LocationCreator.Create` for New Village (`LocationCreator.cs:55-90` at
/// `v1.2`): a 38th location after the 37 of `gam.dat`, named `burn?37`
/// (`Location.Title`), its spring's reserve starting at its refresh, no
/// production chosen, nobody's; its roads from `ways`/`way_lengths`, and
/// the way back added to each neighbour that has none to it
/// (`updateBurngfxLocation`). There is no drawn path for these roads
/// (`Ways: Array.Empty`). Nothing when the world already has it, or has
/// not the 37 it is added after.
pub fn add_remaster_location(g: &mut Gamdat, settings: &RemasterLocation) -> bool {
    if g.locations.len() != NEW_VILLAGE {
        return false;
    }
    let mut products = [None; 4];
    for (slot, &p) in products.iter_mut().zip(settings.available_products.iter()) {
        *slot = Some(p);
    }
    let roads: Vec<Road> = settings.ways.iter()
        .filter(|&&(n, _)| n < NEW_VILLAGE)
        .map(|&(neighbour, days)| Road { neighbour, days, way: None })
        .collect();
    for r in &roads {
        let back = &mut g.locations[r.neighbour].roads;
        if !back.iter().any(|b| b.neighbour == NEW_VILLAGE) {
            back.push(Road { neighbour: NEW_VILLAGE, days: r.days, way: None });
        }
    }
    g.locations.push(LocationRecord {
        title_id: NEW_VILLAGE as u16 + 1,
        entry_point: settings.entry_point.unwrap_or(Point::new(0, 0)),
        is_city: settings.is_city,
        water_reserve: settings.water_refresh,
        water_capacity: settings.water_capacity,
        water_refresh: settings.water_refresh,
        owner: 0,
        available_products: products,
        producing: None,
        danger: settings.danger,
        danger_amount: 0,
        life_reduction: 0,
        roads,
    });
    true
}
