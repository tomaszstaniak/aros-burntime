//! `ZEI_*.RAW` — the map tiles, and where you are allowed to walk.
//!
//! Each file holds 63 tiles of 1040 bytes. A tile is 32x32 palette indices
//! plus a sixteen-bit mask saying which quarters of it can be walked on, which
//! makes it the only source for two things this project has been without: the
//! artwork for sections 14 and 15, which no rendered PNG covers, and collision
//! for every classic tile.
//!
//! ```text
//! tile, 1040 bytes:
//!   2      unknown
//!   u16    walk mask
//!   12     unknown
//!   1024   palette indices, 32x32
//! ```
//!
//! Ported from `SpriteProcessor/Tile.cs` and
//! `ResourceProcessor/TileMaskProcessor.cs`.

use alloc::vec;
use alloc::vec::Vec;

use crate::{IndexedImage, Palette};

pub const TILES_PER_FILE: usize = 63;
pub const TILE_SIZE: usize = 32;
const RECORD_LEN: usize = 16 + TILE_SIZE * TILE_SIZE;
/// The mask splits a tile into 4x4 cells, so one mask cell is 8x8 pixels.
pub const MASK_GRID: usize = 4;
pub const MASK_CELL: usize = TILE_SIZE / MASK_GRID;

#[derive(Debug, PartialEq, Eq)]
pub enum ZeiError {
    /// Not 63 whole tiles.
    WrongSize(usize),
}

/// Which quarters of a tile can be walked on.
///
/// Sixteen cells in a 4x4 grid, row-major. The file stores a set bit for
/// *blocked*, most significant bit first — `TileMaskProcessor` reads the bits
/// from the bottom and writes them backwards, which comes to the same thing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WalkMask(pub u16);

impl WalkMask {
    /// Can you stand on cell (x, y) of the 4x4 grid?
    pub fn walkable(&self, x: usize, y: usize) -> bool {
        if x >= MASK_GRID || y >= MASK_GRID {
            return false;
        }
        let cell = y * MASK_GRID + x;
        // Cell 0 is the most significant bit.
        self.0 & (0x8000 >> cell) == 0
    }

    /// Nothing blocked at all — most of a desert map.
    pub fn is_open(&self) -> bool {
        self.0 == 0
    }

    /// Blocked everywhere.
    pub fn is_solid(&self) -> bool {
        self.0 == 0xFFFF
    }
}

/// `Debug` prints the shape, not 63 kilobytes of pixels.
impl core::fmt::Debug for TileSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TileSet {{ tiles: {} }}", self.tiles.len())
    }
}

pub struct TileSet {
    tiles: Vec<Tile>,
}

pub struct Tile {
    pub mask: WalkMask,
    /// 32x32 palette indices, already de-interleaved.
    pub indices: Vec<u8>,
}

impl Tile {
    pub fn to_rgba(&self, palette: &Palette) -> crate::Image {
        IndexedImage {
            width: TILE_SIZE as u32,
            height: TILE_SIZE as u32,
            indices: self.indices.clone(),
        }
        .to_rgba(palette)
    }
}

impl TileSet {
    pub fn parse(bytes: &[u8]) -> Result<TileSet, ZeiError> {
        if bytes.len() != TILES_PER_FILE * RECORD_LEN {
            return Err(ZeiError::WrongSize(bytes.len()));
        }

        let mut tiles = Vec::with_capacity(TILES_PER_FILE);
        for i in 0..TILES_PER_FILE {
            let r = &bytes[i * RECORD_LEN..(i + 1) * RECORD_LEN];
            // 2 unknown bytes, then the mask, then 12 more unknown.
            let mask = WalkMask(u16::from_le_bytes([r[2], r[3]]));
            let data = &r[16..];

            // The pixels are stored in four passes, the same VGA mode-X idea
            // the sprites use — but raw, with no run-length encoding. Pass `p`
            // holds every fourth pixel starting at `p`.
            let mut indices = vec![0u8; TILE_SIZE * TILE_SIZE];
            for p in 0..4 {
                for k in 0..(TILE_SIZE * TILE_SIZE / 4) {
                    indices[4 * k + p] = data[p * (TILE_SIZE * TILE_SIZE / 4) + k];
                }
            }
            tiles.push(Tile { mask, indices });
        }
        Ok(TileSet { tiles })
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn tile(&self, index: usize) -> Option<&Tile> {
        self.tiles.get(index)
    }
}

/// The walkability of a whole map, at four cells per tile in each direction.
///
/// One cell covers 8x8 map pixels. The original also blocks the outermost cell
/// of the map so nothing can walk off the edge.
#[derive(Clone)]
pub struct WalkGrid {
    pub width: usize,
    pub height: usize,
    cells: Vec<bool>,
}

impl WalkGrid {
    pub fn new(tiles_wide: usize, tiles_high: usize) -> WalkGrid {
        let (width, height) = (tiles_wide * MASK_GRID, tiles_high * MASK_GRID);
        WalkGrid { width, height, cells: vec![false; width * height] }
    }

    /// Open (or close) every cell of one tile.
    ///
    /// `BurnmapProcessor` starts each non-empty tile fully walkable and then
    /// narrows it, so this is the first step rather than a convenience.
    pub fn fill_tile(&mut self, tx: usize, ty: usize, walkable: bool) {
        for y in 0..MASK_GRID {
            for x in 0..MASK_GRID {
                let (cx, cy) = (tx * MASK_GRID + x, ty * MASK_GRID + y);
                if cx < self.width && cy < self.height {
                    self.cells[cy * self.width + cx] = walkable;
                }
            }
        }
    }

    /// Narrow one tile by a mask, keeping whatever was already blocked.
    pub fn and_tile(&mut self, tx: usize, ty: usize, mask: WalkMask) {
        for y in 0..MASK_GRID {
            for x in 0..MASK_GRID {
                let (cx, cy) = (tx * MASK_GRID + x, ty * MASK_GRID + y);
                if cx < self.width && cy < self.height {
                    self.cells[cy * self.width + cx] &= mask.walkable(x, y);
                }
            }
        }
    }

    pub fn set_tile(&mut self, tx: usize, ty: usize, mask: WalkMask) {
        for y in 0..MASK_GRID {
            for x in 0..MASK_GRID {
                let (cx, cy) = (tx * MASK_GRID + x, ty * MASK_GRID + y);
                if cx < self.width && cy < self.height {
                    self.cells[cy * self.width + cx] = mask.walkable(x, y);
                }
            }
        }
    }

    /// Block the outermost ring, as `BurnmapProcessor` does.
    pub fn close_the_edges(&mut self) {
        for x in 0..self.width {
            self.cells[x] = false;
            let last = (self.height - 1) * self.width + x;
            self.cells[last] = false;
        }
        for y in 0..self.height {
            self.cells[y * self.width] = false;
            self.cells[y * self.width + self.width - 1] = false;
        }
    }

    /// Read a grid the packer wrote: `u16` width, `u16` height, then one byte
    /// per cell. Building it on the host means AROS never touches a tile mask.
    pub fn from_packed(bytes: &[u8]) -> Option<WalkGrid> {
        if bytes.len() < 4 {
            return None;
        }
        let width = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let height = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
        let cells = &bytes[4..];
        if cells.len() < width.checked_mul(height)? {
            return None;
        }
        Some(WalkGrid {
            width,
            height,
            cells: cells[..width * height].iter().map(|&b| b != 0).collect(),
        })
    }

    pub fn cell(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.cells[y * self.width + x]
    }

    /// Can the character stand at this map pixel?
    pub fn walkable_at(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        self.cell(x as usize / MASK_CELL, y as usize / MASK_CELL)
    }

    /// A checksum over every cell, so a grid built on the host and a grid read
    /// on AROS can be compared rather than assumed equal. Matching open counts
    /// only says the same number of cells are open, not the same cells.
    ///
    /// FNV-1a, for no reason beyond being short and having no dependencies.
    pub fn checksum(&self) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        for &open in &self.cells {
            hash ^= u32::from(open);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash
    }

    /// How many cells are walkable — a quick sanity number for a whole map.
    pub fn open_cells(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }
}
