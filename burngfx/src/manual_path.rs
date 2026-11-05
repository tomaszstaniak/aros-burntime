//! v1.2's direct walking (`PathFinding/ManualPath.cs`): the selected
//! character walks the way the arrows are held, sliding along a wall it
//! meets at an angle, and taking a short detour out of a small pocket.
//!
//! Positions are map pixels; the walk grid is sampled at a cell's centre as
//! `ManualPath.IsWalkable` does (`(p + res/2 − 1) / res`). The caller gives
//! the distance of a step: v1.2's `speed × elapsed`, here a frame's walk.

use alloc::collections::VecDeque;
use alloc::vec;

use crate::game::Point;
use crate::zei::{WalkGrid, MASK_CELL};

const RES: i32 = MASK_CELL as i32;

/// `(x, y)` in map pixels, with the fraction `ManualPath` keeps.
pub type Vecf = (f32, f32);

#[derive(Clone, Debug, Default)]
pub struct ManualPath {
    /// `PathState.precisePosition`, while it still matches the raster.
    precise: Option<Vecf>,
    /// The held direction, of length one (or nought).
    direction: Vecf,
    slide_side: i32,
    detour: VecDeque<Vecf>,
    /// Where a detour was looked for and none found.
    failed_detour: Option<Point>,
}

fn len(v: Vecf) -> f32 {
    crate::input::sqrt32(v.0 * v.0 + v.1 * v.1)
}

/// `(Vector2)position`: toward nought, as C#'s `(int)` cast.
fn raster(p: Vecf) -> Point {
    Point::new(p.0 as i32, p.1 as i32)
}

fn mask_cell(p: Vecf) -> (i32, i32) {
    let r = raster(p);
    ((r.x + (RES / 2 - 1)) / RES, (r.y + (RES / 2 - 1)) / RES)
}

fn cell_centre(c: (i32, i32)) -> Vecf {
    ((c.0 * RES + RES / 2) as f32, (c.1 * RES + RES / 2) as f32)
}

fn walkable(grid: &WalkGrid, p: Vecf) -> bool {
    if p.0 < 0.0 || p.1 < 0.0 {
        return false;
    }
    let (cx, cy) = mask_cell(p);
    cx >= 0 && cy >= 0 && grid.cell(cx as usize, cy as usize)
}

fn rotate90(v: Vecf, side: i32) -> Vecf {
    if side > 0 { (-v.1, v.0) } else { (v.1, -v.0) }
}

impl ManualPath {
    /// `Direction`'s setter: normalised; a new direction forgets the slide
    /// and any detour.
    pub fn set_direction(&mut self, d: (i32, i32)) {
        let mut v = (d.0 as f32, d.1 as f32);
        let l = len(v);
        if l > 0.0 {
            v = (v.0 / l, v.1 / l);
        }
        if v != self.direction {
            self.slide_side = 0;
            self.detour.clear();
            self.failed_detour = None;
        }
        self.direction = v;
    }

    pub fn is_moving(&self) -> bool {
        self.direction != (0.0, 0.0)
    }

    /// `PathState.GetRenderPosition`: the precise position while it rounds
    /// to `at`, the raster otherwise (`None`).
    pub fn precise_at(&self, at: Point) -> Option<Vecf> {
        self.precise.filter(|&p| raster(p) == at)
    }

    /// `Process`: walk `distance` from `position` the held way. Where it ends,
    /// on the raster.
    pub fn step(&mut self, grid: &WalkGrid, position: Point, distance: f32) -> Point {
        // `BeginMovement`.
        let mut at = match self.precise {
            Some(p) if raster(p) == position => p,
            _ => (position.x as f32, position.y as f32),
        };
        let mut left = distance;
        while left > 0.0 && self.direction != (0.0, 0.0) {
            let step_len = left.min(1.0);
            if !self.detour.is_empty() {
                if !self.move_along_detour(grid, &mut at, step_len) {
                    break;
                }
                left -= step_len;
                continue;
            }
            let step = (self.direction.0 * step_len, self.direction.1 * step_len);
            let full = (at.0 + step.0, at.1 + step.1);
            if walkable(grid, full) {
                at = full;
                self.slide_side = 0;
                self.failed_detour = None;
            } else {
                let mut moved = false;
                let horizontal = (at.0 + step.0, at.1);
                if step.0 != 0.0 && walkable(grid, horizontal) {
                    at = horizontal;
                    moved = true;
                }
                let vertical = (at.0, at.1 + step.1);
                if step.1 != 0.0 && walkable(grid, vertical) {
                    at = vertical;
                    moved = true;
                }
                if !moved {
                    let side = self.slide_side(grid, at, step_len);
                    if side == 0 {
                        if !self.begin_detour(grid, at) || !self.move_along_detour(grid, &mut at, step_len) {
                            break;
                        }
                        left -= step_len;
                        continue;
                    }
                    let d = rotate90(self.direction, side);
                    let slid = (at.0 + d.0 * step_len, at.1 + d.1 * step_len);
                    if !walkable(grid, slid) {
                        self.slide_side = 0;
                        break;
                    }
                    at = slid;
                }
            }
            left -= step_len;
        }
        // `CommitMovement`.
        self.precise = Some(at);
        raster(at)
    }

    fn begin_detour(&mut self, grid: &WalkGrid, start: Vecf) -> bool {
        const RADIUS: i32 = 4;
        const LONGEST: i32 = RADIUS * 3;
        let raster_start = raster(start);
        if self.failed_detour == Some(raster_start) {
            return false;
        }
        let (w, h) = (grid.width as i32, grid.height as i32);
        let s = mask_cell(start);
        if s.0 < 0 || s.1 < 0 || s.0 >= w || s.1 >= h {
            return false;
        }
        let count = (w * h) as usize;
        let mut previous = vec![-2i32; count];
        let mut depth = vec![0i32; count];
        let start_index = s.0 + s.1 * w;
        previous[start_index as usize] = -1;
        let mut open: VecDeque<i32> = VecDeque::new();
        open.push_back(start_index);
        let mut goal = -1;
        while let Some(current) = open.pop_front() {
            if goal != -1 {
                break;
            }
            if depth[current as usize] >= LONGEST {
                continue;
            }
            let c = (current % w, current / w);
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (c.0 + ox, c.1 + oy);
                if (n.0 - s.0).abs() > RADIUS || (n.1 - s.1).abs() > RADIUS || n.0 < 0 || n.1 < 0
                    || !grid.cell(n.0 as usize, n.1 as usize)
                {
                    continue;
                }
                let ni = n.0 + n.1 * w;
                if ni < 0 || ni as usize >= count || previous[ni as usize] != -2 {
                    continue;
                }
                previous[ni as usize] = current;
                depth[ni as usize] = depth[current as usize] + 1;
                if self.is_detour_exit(grid, start, n) {
                    goal = ni;
                    break;
                }
                open.push_back(ni);
            }
        }
        if goal == -1 {
            self.failed_detour = Some(raster_start);
            return false;
        }
        let mut reversed = alloc::vec::Vec::new();
        let mut i = goal;
        while i != start_index {
            reversed.push(cell_centre((i % w, i / w)));
            i = previous[i as usize];
        }
        self.detour = reversed.into_iter().rev().collect();
        self.slide_side = 0;
        self.failed_detour = None;
        !self.detour.is_empty()
    }

    fn is_detour_exit(&self, grid: &WalkGrid, start: Vecf, cell: (i32, i32)) -> bool {
        let p = cell_centre(cell);
        let forward = (p.0 - start.0) * self.direction.0 + (p.1 - start.1) * self.direction.1;
        if forward < 0.0 {
            return false;
        }
        let r = RES as f32;
        walkable(grid, (p.0 + self.direction.0 * r, p.1 + self.direction.1 * r))
            && walkable(grid, (p.0 + self.direction.0 * r * 2.0, p.1 + self.direction.1 * r * 2.0))
    }

    fn move_along_detour(&mut self, grid: &WalkGrid, at: &mut Vecf, step_len: f32) -> bool {
        while let Some(&next) = self.detour.front() {
            let diff = (next.0 - at.0, next.1 - at.1);
            let d = len(diff);
            if d < 0.01 {
                self.detour.pop_front();
                continue;
            }
            let unit = (diff.0 / d, diff.1 / d);
            let m = d.min(step_len);
            let to = (at.0 + unit.0 * m, at.1 + unit.1 * m);
            if !walkable(grid, to) {
                self.detour.clear();
                return false;
            }
            *at = to;
            if d <= step_len {
                self.detour.pop_front();
            }
            return true;
        }
        false
    }

    fn slide_side(&mut self, grid: &WalkGrid, start: Vecf, step_len: f32) -> i32 {
        if self.slide_side != 0 {
            let d = rotate90(self.direction, self.slide_side);
            if walkable(grid, (start.0 + d.0 * step_len, start.1 + d.1 * step_len)) {
                return self.slide_side;
            }
            self.slide_side = 0;
        }
        let positive = self.clearance(grid, start, 1);
        let negative = self.clearance(grid, start, -1);
        if positive == i32::MAX && negative == i32::MAX {
            return 0;
        }
        self.slide_side = if positive <= negative { 1 } else { -1 };
        self.slide_side
    }

    fn clearance(&self, grid: &WalkGrid, start: Vecf, side: i32) -> i32 {
        let d = rotate90(self.direction, side);
        let mut probe = start;
        for distance in 1..=RES * 4 {
            probe = (probe.0 + d.0, probe.1 + d.1);
            if !walkable(grid, probe) {
                return i32::MAX;
            }
            if walkable(grid, (probe.0 + self.direction.0, probe.1 + self.direction.1)) {
                return distance;
            }
        }
        i32::MAX
    }
}

/// v1.3's `ManualMovementInputGate` (`LocationScene.cs` at `v1.3`): when a
/// click starts an automatic walk while a movement key is held, the held key
/// does not take the character over until every movement key has been let
/// go. v1.2 had no gate: the held key won at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputGate {
    waiting_for_neutral: bool,
}

impl InputGate {
    /// `ProtectAutomaticMovement`: at the click, whether a movement key is down.
    pub fn protect(&mut self, movement_down: bool) {
        self.waiting_for_neutral = movement_down;
    }

    /// `Suppress`: true while held movement must be ignored; the first frame
    /// with no movement key down opens the gate again.
    pub fn suppress(&mut self, movement_down: bool) -> bool {
        if !self.waiting_for_neutral {
            return false;
        }
        if movement_down {
            return true;
        }
        self.waiting_for_neutral = false;
        false
    }

    /// `Reset`: a scene entered or left.
    pub fn reset(&mut self) {
        self.waiting_for_neutral = false;
    }
}

#[cfg(test)]
mod v13_gate {
    use super::InputGate;

    #[test]
    fn a_held_key_waits_for_release_after_a_click() {
        let mut g = InputGate::default();
        assert!(!g.suppress(true), "no click: keys walk");
        g.protect(true);
        assert!(g.suppress(true));
        assert!(g.suppress(true), "still held");
        assert!(!g.suppress(false), "let go: open");
        assert!(!g.suppress(true), "pressed again: walks");
        g.protect(false);
        assert!(!g.suppress(true), "a click with no key held protects nothing");
        g.protect(true);
        g.reset();
        assert!(!g.suppress(true));
    }
}
