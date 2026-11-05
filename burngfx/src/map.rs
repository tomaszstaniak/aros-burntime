//! The `.burnmap` map format.
//!
//! This is the remaster's own format rather than a 1993 one: Jakob Harder's
//! map editor writes it, and his repository ships one per original map. It is
//! a .NET `BinaryWriter` stream, so strings carry a 7-bit-encoded length and
//! integers are little-endian.
//!
//! Reading it is worth doing before the original `MAT_xxx.RAW` parser, for two
//! reasons: it needs no data we do not have, and once both exist the `.burnmap`
//! files become an oracle — decode a `.RAW` and the result must agree with the
//! map the editor produced from that same file.
//!
//! Ported from `Burntime.Remaster/ResourceProcessor/BurnmapProcessor.cs`.

use alloc::string::String;
use alloc::vec::Vec;

/// Which tile sheet a tile's graphics come from.
///
/// The file names its tile sets; two names are meaningful. `"classic"` means
/// the original 1993 artwork, addressed as `zei_<section>.raw` entry `item`.
/// `"_"` means artwork the map carries itself, in a `<map>_tiles.png` sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileSet {
    /// Original artwork: `zei_<section>.raw`, entry `item`.
    Classic,
    /// The map's own sheet, at a flat index into `<map>_tiles.png`.
    Custom,
    /// A set this decoder does not recognise, kept as its index.
    Other(u8),
}

/// One cell of the map grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    /// Entry within the section. Zero is a legitimate entry.
    pub item: u8,
    /// Which `zei_NNN.raw` file, or which band of the custom sheet. Zero means
    /// the cell is empty and the rest of the tile is meaningless.
    pub section: u8,
    pub set: TileSet,
}

impl Tile {
    pub const EMPTY: Tile = Tile { item: 0, section: 0, set: TileSet::Other(0) };

    pub fn is_empty(&self) -> bool {
        self.section == 0
    }

    /// Where this tile lives in the map's own PNG sheet.
    ///
    /// The sheet is a flat run of 32x32 cells and the section acts as a band
    /// of 63 — the original computes `(section - 1) * 63 + item`. Only
    /// meaningful for [`TileSet::Custom`].
    pub fn custom_sheet_index(&self) -> Option<usize> {
        match self.set {
            TileSet::Custom if !self.is_empty() => {
                Some((self.section as usize - 1) * 63 + self.item as usize)
            }
            _ => None,
        }
    }

    /// The file name the remaster's rendered originals use for this tile,
    /// e.g. `02_48` for section 2, item 48.
    pub fn classic_name(&self) -> Option<(u8, u8)> {
        match self.set {
            TileSet::Classic if !self.is_empty() => Some((self.section, self.item)),
            _ => None,
        }
    }
}

/// A doorway on the map: the rectangle you walk into to enter a location.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Entrance {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// A parsed `.burnmap`.
#[derive(Debug)]
pub struct Map {
    /// Size in tiles.
    pub width: usize,
    pub height: usize,
    /// Edge of one tile, in pixels. 32 in every shipped map.
    pub tile_size: u32,
    /// Tile set names as the file lists them, in index order.
    pub set_names: Vec<String>,
    /// `width * height` tiles, row-major.
    pub tiles: Vec<Tile>,
    pub entrances: Vec<Entrance>,
}

impl Map {
    pub fn tile(&self, x: usize, y: usize) -> Tile {
        self.tiles[y * self.width + x]
    }

    /// Size in pixels.
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.width as u32 * self.tile_size, self.height as u32 * self.tile_size)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    /// Ran out of bytes partway through.
    Truncated,
    /// The leading string was not "Burntime Map".
    NotAMap,
    /// Only "0.2" is understood; the original refuses anything else too.
    UnsupportedVersion,
    /// Dimensions that cannot be right.
    ImplausibleSize,
}

pub fn parse(bytes: &[u8]) -> Result<Map, MapError> {
    let mut r = Reader { b: bytes, at: 0 };

    if r.string()? != "Burntime Map" {
        return Err(MapError::NotAMap);
    }
    if r.string()? != "0.2" {
        return Err(MapError::UnsupportedVersion);
    }

    let width = r.i32()?;
    let height = r.i32()?;
    let tile_size = r.i32()?;
    if !(1..=1024).contains(&width) || !(1..=1024).contains(&height) || !(1..=256).contains(&tile_size)
    {
        return Err(MapError::ImplausibleSize);
    }
    let (width, height) = (width as usize, height as usize);

    let set_count = r.i32()?;
    if !(0..=64).contains(&set_count) {
        return Err(MapError::ImplausibleSize);
    }
    let mut set_names = Vec::new();
    for _ in 0..set_count {
        set_names.push(String::from(r.string()?));
    }
    let classic = set_names.iter().position(|n| n == "classic");
    let custom = set_names.iter().position(|n| n == "_");

    let mut tiles = Vec::with_capacity(width * height);
    for _ in 0..width * height {
        // Written in this order, which is not the order they read best in.
        let item = r.u8()?;
        let section = r.u8()?;
        let set = r.u8()?;

        if section == 0 {
            tiles.push(Tile::EMPTY);
            continue;
        }
        let set = if Some(set as usize) == classic {
            TileSet::Classic
        } else if Some(set as usize) == custom {
            TileSet::Custom
        } else {
            TileSet::Other(set)
        };
        tiles.push(Tile { item, section, set });
    }

    // A single byte, so a map cannot have more than 255 entrances. Note this
    // count is the authority, not the `rooms = N` line in the companion .txt:
    // MAT_000 declares 39 rooms there and carries 38 rectangles here, and the
    // original only looks up settings for entrances that exist.
    let entrance_count = r.u8()? as usize;
    let mut entrances = Vec::with_capacity(entrance_count);
    for _ in 0..entrance_count {
        entrances.push(Entrance {
            left: r.i32()?,
            top: r.i32()?,
            width: r.i32()?,
            height: r.i32()?,
        });
    }

    // Four zero bytes follow in every shipped map: a ways count that version
    // 0.2 writes and the original's reader leaves commented out. Ignored here
    // for the same reason — nothing has ever put a non-zero value there.

    Ok(Map { width, height, tile_size: tile_size as u32, set_names, tiles, entrances })
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, MapError> {
        let v = *self.b.get(self.at).ok_or(MapError::Truncated)?;
        self.at += 1;
        Ok(v)
    }

    fn i32(&mut self) -> Result<i32, MapError> {
        let end = self.at + 4;
        let s = self.b.get(self.at..end).ok_or(MapError::Truncated)?;
        self.at = end;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    /// A .NET `BinaryWriter` string: a 7-bit-encoded byte length, then UTF-8.
    fn string(&mut self) -> Result<&'a str, MapError> {
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            len |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err(MapError::Truncated);
            }
        }
        let end = self.at + len;
        let s = self.b.get(self.at..end).ok_or(MapError::Truncated)?;
        self.at = end;
        core::str::from_utf8(s).map_err(|_| MapError::Truncated)
    }
}
