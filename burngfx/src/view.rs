//! A fixed window onto a map that is rarely the same size as it.
//!
//! Location maps range from 480x320 to at least 576x480, and the world map is
//! 480x384. Sizing the window to whichever map is largest makes every other
//! map sit in a window that does not suit it, and on AROS it means the biggest
//! town in the archive decides how much screen the game takes. So the window
//! is a fixed [`Viewport`] and the map moves behind it.
//!
//! Two rules, and they are the whole of the design:
//!
//! - A map **larger** than the viewport scrolls. The camera follows a point
//!   and is clamped so the edge of the map never comes into view.
//! - A map **smaller** than the viewport is centred, and what is left over is
//!   background rather than a wrapped or sheared copy of the map.
//!
//! Everything above this module works in map coordinates. [`Viewport::to_map`]
//! is the one place a screen coordinate becomes a map coordinate, so a click
//! is converted once and the rules never see a pixel of the window.

use alloc::vec::Vec;

use crate::game::Point;

/// The map's part of the window, in game pixels. It changes only when the
/// window does (v1.2's `SetMapRenderArea` on a resize); the window itself is
/// never reopened for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Viewport {
    pub w: usize,
    pub h: usize,
}

/// The top-left corner of the viewport, in map pixels.
///
/// Negative on an axis where the map is smaller than the viewport: the map
/// starts that far *into* the window, and the margin is background.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Camera {
    pub x: i32,
    pub y: i32,
}

impl Viewport {
    pub fn new(w: usize, h: usize) -> Viewport {
        Viewport { w, h }
    }

    /// Where the camera must sit to show `focus`, given a map this size.
    ///
    /// Centred on the focus, then clamped to the map. When the map is smaller
    /// than the viewport the clamp inverts and the result is a fixed, negative
    /// offset — the map centred in the window, wherever the focus is.
    pub fn follow(&self, map: (usize, usize), focus: Point) -> Camera {
        Camera {
            x: axis(self.w, map.0, focus.x),
            y: axis(self.h, map.1, focus.y),
        }
    }

    /// A screen point, in window pixels, as a point on the map.
    ///
    /// The result can be outside the map when the map is smaller than the
    /// window; callers that need a point on the map should clamp, which is
    /// what [`crate::game::clamp`] is for.
    pub fn to_map(&self, camera: Camera, screen: Point) -> Point {
        Point::new(screen.x + camera.x, screen.y + camera.y)
    }

    /// A point on the map, in window pixels. The inverse of [`Self::to_map`].
    pub fn to_screen(&self, camera: Camera, map: Point) -> Point {
        Point::new(map.x - camera.x, map.y - camera.y)
    }

    /// Is this map point inside the window at all?
    pub fn shows(&self, camera: Camera, map: Point) -> bool {
        let p = self.to_screen(camera, map);
        p.x >= 0 && p.y >= 0 && (p.x as usize) < self.w && (p.y as usize) < self.h
    }

    /// Copy the visible part of a map image into a window-sized buffer.
    ///
    /// Row by row, and clipped on every side. Copying the two as one flat run
    /// only works while they are the same width; when they are not, each row
    /// lands further along than the last and the picture arrives as diagonal
    /// streaks. That is not a hypothetical — it is what this used to do.
    ///
    /// `dst` is left untouched where the map does not reach, so whatever the
    /// caller filled it with shows through as the background.
    pub fn blit(&self, dst: &mut [u32], src: &[u32], src_w: usize, src_h: usize, camera: Camera) {
        if src_w == 0 || dst.len() < self.w * self.h || src.len() < src_w * src_h {
            return;
        }

        for row in 0..self.h {
            let sy = row as i32 + camera.y;
            if sy < 0 || sy as usize >= src_h {
                continue;
            }

            // The overlap of [camera.x, camera.x + w) with [0, src_w), in map
            // pixels, and where it lands in the window.
            let from_x = camera.x.max(0);
            let to_x = (camera.x + self.w as i32).min(src_w as i32);
            if to_x <= from_x {
                continue;
            }
            let count = (to_x - from_x) as usize;
            let dst_x = (from_x - camera.x) as usize;

            let s = sy as usize * src_w + from_x as usize;
            let d = row * self.w + dst_x;
            dst[d..d + count].copy_from_slice(&src[s..s + count]);
        }
    }

    /// A window-sized buffer filled with `background`, ready to blit into.
    pub fn canvas(&self, background: u32) -> Vec<u32> {
        alloc::vec![background; self.w * self.h]
    }
}

/// One axis of [`Viewport::follow`].
fn axis(window: usize, map: usize, focus: i32) -> i32 {
    let (window, map) = (window as i32, map as i32);
    if map <= window {
        // Centred, and negative: the map begins inside the window. Rounding
        // down keeps a one-pixel remainder on the right rather than the left,
        // which matters only in that it is the same every frame.
        (map - window) / 2
    } else {
        (focus - window / 2).clamp(0, map - window)
    }
}

/// A grid of item slots, and the arithmetic for hitting one with a click.
///
/// Both the room's floor and the character's pack are drawn as one of these,
/// so a click lands in exactly one place and the drawing and the hit test
/// cannot drift apart — they are the same rectangles.
///
/// The original's layout, in its own 320x200 screen: the character's panel at
/// (2, 5), the room's items at (160, 20) in four columns, or (160, 128) and
/// two rows at a water source, which holds eight rather than thirty two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemGrid {
    pub left: i32,
    pub top: i32,
    pub columns: usize,
    pub rows: usize,
    /// The side of one slot, in pixels. Item pictures are 32x32.
    pub cell: i32,
    /// The gap between slots, across.
    pub gap: i32,
    /// The gap between rows — `ItemGridWindow`'s spacing is two numbers, and
    /// the pack's is (4, 5).
    pub gap_y: i32,
    /// `DoubleLayered`: a second layer of `(columns-1) × (rows-1)` slots, each
    /// half a step down and right of a slot of the first, between four of them
    /// — how a room holds thirty two in a four by five grid.
    pub double: bool,
}

impl ItemGrid {
    /// A grid with one spacing both ways and no second layer.
    pub const fn plain(left: i32, top: i32, columns: usize, rows: usize, gap: i32) -> ItemGrid {
        ItemGrid { left, top, columns, rows, cell: 32, gap, gap_y: gap, double: false }
    }

    /// How many slots there are: the first layer, then the second.
    pub fn capacity(&self) -> usize {
        let extra = if self.double {
            self.columns.saturating_sub(1) * self.rows.saturating_sub(1)
        } else {
            0
        };
        self.columns * self.rows + extra
    }

    /// The top-left corner of slot `i`: the first layer in rows, then the
    /// second, `p·(32+spacing) + 16`.
    pub fn slot_at(&self, i: usize) -> Option<Point> {
        if i >= self.capacity() {
            return None;
        }
        let first = self.columns * self.rows;
        let (sx, sy) = (self.cell + self.gap, self.cell + self.gap_y);
        if i < first {
            let (col, row) = (i % self.columns, i / self.columns);
            return Some(Point::new(self.left + col as i32 * sx, self.top + row as i32 * sy));
        }
        let j = i - first;
        let per = self.columns - 1;
        let (col, row) = (j % per, j / per);
        Some(Point::new(self.left + col as i32 * sx + 16, self.top + row as i32 * sy + 16))
    }

    /// Which slot a point falls in, ignoring filled-ness: the first layer only.
    /// The gaps between slots are not part of any slot.
    pub fn hit(&self, p: Point) -> Option<usize> {
        let (sx, sy) = (self.cell + self.gap, self.cell + self.gap_y);
        let (dx, dy) = (p.x - self.left, p.y - self.top);
        if dx < 0 || dy < 0 {
            return None;
        }
        let (col, row) = (dx / sx, dy / sy);
        if col >= self.columns as i32 || row >= self.rows as i32 {
            return None;
        }
        if dx % sx >= self.cell || dy % sy >= self.cell {
            return None;
        }
        Some(row as usize * self.columns + col as usize)
    }

    /// The slot a click means: of the filled slots under the point, the one
    /// on top — the second layer lies over the first, as its windows were
    /// added after — or `None`. A click on an empty slot does nothing in
    /// `ItemGridWindow`.
    pub fn hit_filled(&self, p: Point, filled: impl Fn(usize) -> bool) -> Option<usize> {
        (0..self.capacity())
            .rev()
            .filter(|&i| filled(i))
            .find(|&i| {
                self.slot_at(i).is_some_and(|at| {
                    p.x >= at.x && p.y >= at.y && p.x < at.x + self.cell && p.y < at.y + self.cell
                })
            })
    }
}

/// v1.2's keyboard focus in an item grid (`ItemGridWindow`): only slots
/// holding something take it. Slot centres, for the arithmetic.
fn centre_of(grid: &ItemGrid, i: usize) -> Option<Point> {
    grid.slot_at(i).map(|p| Point::new(p.x + grid.cell / 2, p.y + grid.cell / 2))
}

/// `MoveFocus`: from the focused slot, the nearest filled one that way —
/// least sideways first, then least forward; in a double-layered grid left
/// and right keep to the layer and row, and up and down go to the nearer
/// layer first. With nothing focused, the first filled slot
/// (`FocusFirstItem`). `None` when nothing lies that way.
pub fn grid_move_focus(grid: &ItemGrid, filled: &[bool], current: Option<usize>, dir: (i32, i32)) -> Option<usize> {
    let ok = |i: usize| filled.get(i).copied().unwrap_or(false) && i < grid.capacity();
    let Some(cur) = current.filter(|&c| ok(c)) else {
        return (0..grid.capacity()).find(|&i| ok(i));
    };
    let here = centre_of(grid, cur)?;
    let split = grid.columns * grid.rows;
    let mut best: Option<(usize, i32)> = None;
    for i in (0..grid.capacity()).filter(|&i| ok(i) && i != cur) {
        let c = centre_of(grid, i)?;
        let (dx, dy) = (c.x - here.x, c.y - here.y);
        if grid.double && dir.0 != 0 && ((cur >= split) != (i >= split) || c.y != here.y) {
            continue;
        }
        let forward = dx * dir.0 + dy * dir.1;
        if forward <= 0 {
            continue;
        }
        let sideways = (dx * dir.1 - dy * dir.0).abs();
        let score = if grid.double && dir.0 == 0 { forward * 1000 + sideways } else { sideways * 1000 + forward };
        if best.is_none_or(|b| score < b.1) {
            best = Some((i, score));
        }
    }
    best.map(|b| b.0)
}

/// `FocusEdge`: entering this grid from `source` (a point in the same
/// coordinates as the grid), the filled slot nearest the edge it is entered
/// by, on the row nearest the source's.
pub fn grid_focus_edge(grid: &ItemGrid, filled: &[bool], source: Point, dir: (i32, i32)) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for i in (0..grid.capacity()).filter(|&i| filled.get(i).copied().unwrap_or(false)) {
        let c = centre_of(grid, i)?;
        let forward = (c.x - source.x) * dir.0 + (c.y - source.y) * dir.1;
        if forward <= 0 {
            continue;
        }
        let edge = if dir.0 > 0 { c.x } else { -c.x };
        let score = edge * 1000 + (c.y - source.y).abs();
        if best.is_none_or(|b| score < b.1) {
            best = Some((i, score));
        }
    }
    best.map(|b| b.0)
}

/// A slot's centre, where a keyed click on it lands.
pub fn grid_slot_centre(grid: &ItemGrid, i: usize) -> Option<Point> {
    centre_of(grid, i)
}
