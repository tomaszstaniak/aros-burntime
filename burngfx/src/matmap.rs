//! `mat_NNN.raw` — the 1993 map file, read for its doors.
//!
//! This project already reads two other things out of these files: the palette
//! at offset 16 ([`crate::Palette::from_mat_raw`]), and, indirectly, the tile
//! layout by way of the remaster's converted `.burnmap`. What it did *not*
//! read, and should have, is the door table at the end — because that is where
//! a room's kind comes from.
//!
//! The remaster's `BurnmapProcessor` is explicit about this: it fills each
//! entrance from the classic file first ("take data from original Burntime as
//! default") and lets the companion `.txt` override the *title* only. A
//! `.burnmap` carries nothing but the rectangle. Reading room types out of the
//! `.txt`, as this project did at first, therefore had it backwards — and cost
//! more than tidiness, because only `MAT_000`, `MAT_005` and `MAT_038` ship a
//! `.txt` and all 38 maps ship a raw.
//!
//! ```text
//! u16    width * 2      (bytes per tile row; the tile count is half)
//! u16    height
//! u32    file size      (written twice, as two u16 — both are the length)
//! u16    door table offset
//! 6      unknown
//! 768    colour table   (256 RGB triples, 6-bit — the palette)
//! 256    unknown
//! w*h    u16 tiles      (low byte section, high byte item)
//! doors  20 bytes each, until one begins with x == 0
//! ```
//!
//! A door record is `u16 id, u16 x, u16 y, u16 w, u16 h`, then ten bytes of
//! which only the third is understood: the room id.
//!
//! Ported from `Burntime.BurnGfx/Map.cs`, `Door.cs` and
//! `ResourceProcessor/MapProcessor.cs`.

use alloc::vec::Vec;

use crate::map::Entrance;

const HEADER_LEN: usize = 16;
const PALETTE_LEN: usize = 256 * 3;
const UNKNOWN_LEN: usize = 256;
/// Where the tile grid begins, and so what a door offset must clear.
pub const TILES_AT: usize = HEADER_LEN + PALETTE_LEN + UNKNOWN_LEN;
const DOOR_LEN: usize = 20;

#[derive(Debug, PartialEq, Eq)]
pub enum MatError {
    /// Ran out of bytes before the tile grid.
    Truncated,
    /// A width or height that cannot be a map.
    ImplausibleSize(usize, usize),
}

/// What kind of room lies behind a door.
///
/// `MapProcessor` derives this from the room id with a ladder of magic
/// numbers, reproduced here exactly. They are not ranges with meaning — 13 and
/// 17 are both traders, 14 and 23 are both pubs — so the shape of the original
/// is kept rather than tidied into something that would be easier to get
/// subtly wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoomType {
    #[default]
    Normal,
    WaterSource,
    /// Reached by rope: enterable from much further away, and only carrying
    /// one. See [`RoomType::entry_reach`].
    Rope,
    Scene,
    Trader,
    Doctor,
    Pub,
    Restaurant,
    Church,
}

impl RoomType {
    pub fn from_room_id(id: u8) -> RoomType {
        match id {
            0..=3 => RoomType::Normal,
            4..=8 => RoomType::WaterSource,
            9 => RoomType::Rope,
            13 | 17 => RoomType::Trader,
            14 | 23 => RoomType::Pub,
            16 => RoomType::Church,
            20 => RoomType::Doctor,
            22 | 24 => RoomType::Restaurant,
            _ => RoomType::Scene,
        }
    }

    /// How close you must be to the door to go in, in map pixels.
    ///
    /// `OriginalLocationCreator` sets 15 for everything except a rope, which
    /// gets 75 — you throw it from a distance. The rope also needs the item,
    /// which this cannot answer.
    pub fn entry_reach(&self) -> i32 {
        match self {
            RoomType::Rope => 75,
            _ => 15,
        }
    }

    /// Can anything be left on the floor here, and how much?
    ///
    /// Only the three room kinds you can walk around in hold items; a trader
    /// or a pub is a scene, and its floor is not a container.
    pub fn item_capacity(&self) -> usize {
        match self {
            RoomType::WaterSource => 8,
            RoomType::Normal | RoomType::Rope => 32,
            _ => 0,
        }
    }
}

/// One door in the classic map.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Door {
    /// The original's own numbering, counted from one.
    pub id: u16,
    pub area: Entrance,
    /// The byte everything else here is derived from.
    pub room_id: u8,
    pub kind: RoomType,
}

impl Door {
    /// The line of `BURN.TXT` that names this room, as `(database, index)`.
    ///
    /// `MapProcessor` builds `"burn?" + (660 + RoomID)`, which is why Nob
    /// Hill's ordinary rooms are `burn?661` and its spring is `burn?665`.
    pub fn title_index(&self) -> u32 {
        660 + self.room_id as u32
    }

    /// Which `raum_N.pac` is painted behind it. The original calls this the
    /// entrance's `Background`, and it is the room id unchanged.
    pub fn background(&self) -> u8 {
        self.room_id
    }

    /// The room interior to draw on going in, as a `raum_N.pac` number.
    ///
    /// **Not the same as [`Self::background`]**, and the difference is not a
    /// detail: only the rooms you walk around in are drawn from a `raum_` file
    /// at all. `LocationScene` sends a trader, pub, doctor, church or
    /// restaurant to a scene of its own instead, and passes the room id on as
    /// `InventoryBackground` — which is why a pub is background 14 and a
    /// restaurant 22, numbers no `raum_` file has. There are nine such files
    /// and the ids that reach them are exactly 0 to 8.
    ///
    /// A rope room is the one that does not map to itself: `InventoryBackground
    /// = 3` regardless, so every rope shows the same cave.
    pub fn interior(&self) -> Option<u8> {
        interior_of(self.room_id)
    }
}

/// [`Door::interior`] for a bare room id, which is all the archive carries.
pub fn interior_of(room_id: u8) -> Option<u8> {
    match RoomType::from_room_id(room_id) {
        RoomType::Normal | RoomType::WaterSource => Some(room_id),
        RoomType::Rope => Some(3),
        _ => None,
    }
}

#[derive(Debug)]
pub struct ClassicMap {
    /// Size in tiles.
    pub width: usize,
    pub height: usize,
    pub doors: Vec<Door>,
}

/// Read the doors, and only the doors.
///
/// The tiles are skipped rather than decoded: this project already has them,
/// better, from the `.burnmap`, which carries the remaster's corrections and
/// its extra tile sets. What the `.burnmap` does not carry is any of this.
pub fn parse(bytes: &[u8]) -> Result<ClassicMap, MatError> {
    if bytes.len() < TILES_AT {
        return Err(MatError::Truncated);
    }

    // The first field is bytes per tile row, so a tile row is half of it.
    let width = u16le(bytes, 0) as usize / 2;
    let height = u16le(bytes, 2) as usize;
    let stated_offset = u16le(bytes, 8) as usize;

    // 39x30 is the largest shipped map; anything an order of magnitude past
    // that is a file we are misreading, not an ambitious one.
    if width == 0 || height == 0 || width > 512 || height > 512 {
        return Err(MatError::ImplausibleSize(width, height));
    }

    let tiles_len = width.checked_mul(height).and_then(|n| n.checked_mul(2)).ok_or(MatError::Truncated)?;
    let after_tiles = TILES_AT.checked_add(tiles_len).ok_or(MatError::Truncated)?;
    if after_tiles > bytes.len() {
        return Err(MatError::Truncated);
    }

    // The header states where the door table begins, and walking past the
    // tiles arrives at the same place in every shipped file. Upstream trusts
    // the walk and ignores the field; this trusts the walk too, and uses the
    // field only when the walk would run off the end — a file whose tile
    // dimensions we misread would otherwise silently yield no doors.
    let start = if after_tiles < bytes.len() {
        after_tiles
    } else if (TILES_AT..bytes.len()).contains(&stated_offset) {
        stated_offset
    } else {
        after_tiles
    };

    let mut doors = Vec::new();
    let mut at = start;
    while at + DOOR_LEN <= bytes.len() {
        let r = &bytes[at..at + DOOR_LEN];
        let x = u16le(r, 2);
        // `Door.Read` returns null on x == 0, and the caller stops. Padding
        // after the last door reads as zeroes, so this is the terminator.
        if x == 0 {
            break;
        }
        let room_id = r[12];
        doors.push(Door {
            id: u16le(r, 0),
            area: Entrance {
                left: x as i32,
                top: u16le(r, 4) as i32,
                width: u16le(r, 6) as i32,
                height: u16le(r, 8) as i32,
            },
            room_id,
            kind: RoomType::from_room_id(room_id),
        });
        at += DOOR_LEN;
    }

    Ok(ClassicMap { width, height, doors })
}

/// The tiles, for a map no `.burnmap` was made of — the intro's
/// (`classicmap@mat_089.raw`, `MapProcessor`): width and height in tiles,
/// then `(section, item)` a tile, row by row.
pub fn tiles(bytes: &[u8]) -> Result<(usize, usize, Vec<(u8, u8)>), MatError> {
    let map = parse(bytes)?;
    let (w, h) = (map.width, map.height);
    let raw = &bytes[TILES_AT..TILES_AT + w * h * 2];
    Ok((w, h, raw.chunks(2).map(|t| (t[0], t[1])).collect()))
}

/// Where the door table would begin for a map of this size.
pub fn doors_at(width: usize, height: usize) -> usize {
    TILES_AT + width * height * 2
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
