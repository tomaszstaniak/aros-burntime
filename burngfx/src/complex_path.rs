//! v1.2's route round what is in the way (`PathFinding/ComplexPath.cs`): the
//! walk of the player's group, of whoever the player selects, and of a fight's
//! approach.
//!
//! **An incremental search, not a route found at once.** `Path` is an A* over
//! the walk mask's cells that is given a budget of node expansions — 50 a
//! second (`pathfinding_cycles`, `RES/game/classic/settings.txt:11`) — and
//! goes on from one frame to the next. Meanwhile the walker heads for the
//! cell nearest the goal found so far, and turns back along the cells he has
//! walked when a better branch appears (`undoToWayPoint`). Its quirks are
//! v1.2's route, and are kept:
//!
//! * eight neighbours, in `Rect(-1, -1, 3, 3)`'s order — row by row,
//!   left to right (`Platform/Math/Rect.cs:155-191`);
//! * every step costs `Vector2.Length`, which is an `int`: a diagonal costs
//!   one, as a straight step does (`Platform/Math/Vector2.cs:35-38`);
//! * the estimate is the Manhattan distance (`ComplexPath.cs:79-80,89-90`);
//! * the open list is sorted **before** the expanded node's neighbours are
//!   appended (`:334-337`), so the next node taken is the best of the older
//!   ones, and the sort is .NET's unstable introsort ([`dotnet_sort`]);
//! * a node scoring the cap or more — one and a half times the mask's longer
//!   side — is never listed (`:228,368`);
//! * a goal on a closed cell starts a second, reverse search from the goal
//!   over closed cells for the nearest open one (`:211-216,269-299`), and
//!   the route is searched afresh to that once it is found.
//!
//! Costs and estimates are whole numbers held in `float`s upstream; here they
//! are integers, which compare the same. Positions are the port's fixed
//! point ([`crate::gait`]); v1.2's are `float`s.
//!
//! The search is `[NonSerialized]` (`:452-453`) and rebuilt after a load
//! (`:461-465`); so is this one — nothing here is saved.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::gait::{Stride, Walked, SUB};
use crate::game::Point;
use crate::zei::{WalkGrid, MASK_CELL};

const RES: i32 = MASK_CELL as i32;

/// `pathfinding_cycles`: node expansions a second.
pub const CYCLES_PER_SECOND: f32 = 50.0;

/// `MovingTargetRepathDistance` (`ComplexPath.cs:423`): a moving target is
/// adopted once the walker is this close to the end of his route.
pub const MOVING_TARGET_REPATH: i64 = 8;

/// One world step, in seconds: what each step's search is given.
pub const STEP_SECONDS: f32 = 1.0 / crate::mind::SECOND as f32;

/// A `Vector2f`: positions and directions are `float`s, as upstream's are,
/// so the walk rounds to the pixels v1.2's does (checked step by step
/// against upstream's own code run by .NET, `tests/complex_path.rs`).
type Fx = (f32, f32);

fn fx(p: Point) -> Fx {
    (p.x as f32, p.y as f32)
}

/// `(Vector2)v`: `(int)(x + 0.5f)`.
fn raster(p: Fx) -> Point {
    Point::new((p.0 + 0.5) as i32, (p.1 + 0.5) as i32)
}

/// `ToGridPoint`: `((Vector2)position + (Resolution / 2 - 1)) / Resolution`
/// (`ComplexPath.cs:384-387`), the division `(int)(x / (float)res)`.
fn to_grid(p: Fx) -> (i32, i32) {
    let r = raster(p);
    ((r.x + RES / 2 - 1) / RES, (r.y + RES / 2 - 1) / RES)
}

/// `ToMapPoint`: a cell's centre (`:389-392`).
fn to_map(g: (i32, i32)) -> Point {
    Point::new(g.0 * RES + RES / 2, g.1 * RES + RES / 2)
}

/// `PathMask[Vector2]`: closed outside the mask.
fn open_cell(grid: &WalkGrid, g: (i32, i32)) -> bool {
    g.0 >= 0 && g.1 >= 0 && grid.cell(g.0 as usize, g.1 as usize)
}

/// `Vector2f.Length`: `(float)Math.Sqrt(x * x + y * y)`, the sum in
/// `float`.
fn length(d: Fx) -> f32 {
    crate::input::sqrt32(d.0 * d.0 + d.1 * d.1)
}

/// `Vector2f.Normalize`.
fn normalize(d: Fx) -> Fx {
    let l = length(d);
    if l == 0.0 {
        (0.0, 0.0)
    } else {
        (d.0 / l, d.1 / l)
    }
}

/// `Rect(-1, -1, 3, 3)` enumerated, the middle skipped.
const NEIGHBOURS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Clone, Copy, Debug)]
struct Node {
    pos: (i32, i32),
    parent: Option<usize>,
    cost: i32,
    /// `TotalCost`, cached as upstream caches it: re-parenting a node does
    /// not change what its children were given.
    total: i32,
    remaining: i32,
}

impl Node {
    fn scoring(&self) -> i32 {
        self.total + self.remaining
    }
}

fn manhattan(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

/// `ComplexPath.Path`: one search, and the waypoints it has given so far.
#[derive(Clone, Debug)]
struct Search {
    way_points: Vec<Point>,
    walked: Vec<Point>,
    undo_to: i32,
    finished: bool,
    goal: Fx,
    grid_goal: (i32, i32),
    nearest: Fx,
    nearest_node: usize,
    counter: f32,
    reverse: bool,
    score_cap: i32,
    reverse_path: Option<Box<Search>>,
    nodes: Vec<Node>,
    /// `openList`; `None` is `!IsPathSet`.
    open: Option<Vec<usize>>,
    closed: Vec<usize>,
}

impl Default for Search {
    fn default() -> Search {
        Search {
            way_points: Vec::new(),
            walked: Vec::new(),
            undo_to: -1,
            finished: false,
            goal: (0.0, 0.0),
            grid_goal: (0, 0),
            nearest: (0.0, 0.0),
            nearest_node: 0,
            counter: 0.0,
            reverse: false,
            score_cap: 0,
            reverse_path: None,
            nodes: Vec::new(),
            open: None,
            closed: Vec::new(),
        }
    }
}

impl Search {
    fn is_set(&self) -> bool {
        self.open.is_some()
    }

    fn is_empty(&self) -> bool {
        self.way_points.is_empty()
    }

    /// `Clear` (`:186-197`): the goal and the undo mark stay.
    fn clear(&mut self) {
        self.reverse_path = None;
        self.way_points.clear();
        self.walked.clear();
        self.finished = false;
        self.closed.clear();
        self.open = None;
        self.nodes.clear();
    }

    /// `NextWayPoint` (`:166-174`).
    fn next_way_point(&self) -> Point {
        match self.walked.last() {
            Some(&p) if self.undo_to != -1 => p,
            _ => self.way_points[0],
        }
    }

    /// `Next` (`:199-218`).
    fn next(&mut self) {
        if self.walked.len() as i32 == self.undo_to {
            self.undo_to = -1;
            return;
        }
        if self.undo_to != -1 {
            self.walked.pop();
            if self.walked.is_empty() {
                self.undo_to = -1;
            }
        } else {
            let p = self.way_points.remove(0);
            self.walked.push(p);
        }
    }

    fn start_node(&mut self, at: (i32, i32)) -> usize {
        self.nodes.push(Node { pos: at, parent: None, cost: 0, total: 0, remaining: manhattan(at, self.grid_goal) });
        self.nodes.len() - 1
    }

    /// `BeginProcess` (`:220-245`).
    fn begin(&mut self, grid: &WalkGrid, position: Fx, move_to: Fx, elapsed: f32, reverse: bool) {
        self.clear();
        self.open = Some(Vec::new());
        self.goal = move_to;
        self.nearest = position;
        self.grid_goal = to_grid(move_to);
        self.reverse = reverse;
        if !reverse && !open_cell(grid, self.grid_goal) {
            let mut back = Search::default();
            back.begin(grid, move_to, position, elapsed, true);
            self.reverse_path = Some(Box::new(back));
        }
        let node = self.start_node(to_grid(position));
        self.nearest_node = node;
        if let Some(open) = self.open.as_mut() {
            open.push(node);
        }
        self.counter = 0.0;
        self.score_cap = (grid.width.max(grid.height) as f32 * 1.5) as i32;
        self.process(grid, position, elapsed);
    }

    /// `FinishCalculation` (`:442-448`).
    fn finish(&mut self) {
        if let Some(open) = self.open.as_mut() {
            open.clear();
        }
        self.closed.clear();
        self.finished = true;
        self.goal = self.nearest;
    }

    /// `Process` (`:247-381`).
    fn process(&mut self, grid: &WalkGrid, current: Fx, elapsed: f32) {
        if !self.is_set() {
            return;
        }
        if let Some(back) = self.reverse_path.as_mut() {
            back.process(grid, current, elapsed);
            if back.finished {
                // The nearest open cell to the goal is the goal now, and the
                // search starts again from where he stands.
                self.goal = back.nearest;
                self.grid_goal = to_grid(self.goal);
                self.nodes.clear();
                self.closed.clear();
                let node = self.start_node(to_grid(current));
                self.nearest_node = node;
                self.nearest = current;
                self.open = Some(vec![node]);
                self.walked.clear();
                self.way_points.clear();
                self.undo_to = -1;
                self.reverse_path = None;
                if self.grid_goal == to_grid(current) {
                    self.finish();
                }
            }
        }

        // `CycleCounter.Process`.
        self.counter += elapsed * CYCLES_PER_SECOND;
        let cycles = libm_floor(self.counter);
        self.counter -= cycles as f32;

        let bound = (grid.width as i32 * RES, grid.height as i32 * RES);
        for _ in 0..cycles {
            let first = match self.open.as_mut() {
                Some(open) if !open.is_empty() => open.remove(0),
                _ => break,
            };
            self.closed.push(first);
            let f = self.nodes[first];
            let n = self.nodes[self.nearest_node];
            if f.remaining < n.remaining || self.reverse && (f.total < n.total && f.remaining == 0) {
                self.nearest_node = first;
                self.nearest = fx(to_map(f.pos));
                self.way_points.clear();
                let mut trace = Some(first);
                while let Some(t) = trace {
                    let node = self.nodes[t];
                    if node.parent.is_none() {
                        break;
                    }
                    self.way_points.insert(0, to_map(node.pos));
                    trace = node.parent;
                }
                // The last waypoint the new route shares with the walked one.
                let mut least_common: i32 = -1;
                for j in 0..self.way_points.len().min(self.walked.len()) {
                    if self.way_points[j] == self.walked[j] {
                        least_common = j as i32;
                    } else {
                        break;
                    }
                }
                for _ in 0..=least_common {
                    self.way_points.remove(0);
                }
                self.undo_to = least_common + 1;
                if self.undo_to >= self.walked.len() as i32 {
                    self.undo_to = -1;
                }
                if f.pos == self.grid_goal {
                    self.finish();
                    break;
                }
                // `:315-320`'s early end needs a finished reverse search,
                // which never outlives the top of this call.
            }

            let nodes = &self.nodes;
            if let Some(open) = self.open.as_mut() {
                if self.reverse {
                    dotnet_sort(open, |a, b| nodes[b].total.cmp(&nodes[a].total));
                } else {
                    dotnet_sort(open, |a, b| nodes[a].scoring().cmp(&nodes[b].scoring()));
                }
            }

            for d in NEIGHBOURS {
                let pos = (f.pos.0 + d.0, f.pos.1 + d.1);
                let mut neighbour =
                    Node { pos, parent: Some(first), cost: 1, total: f.total + 1, remaining: manhattan(pos, self.grid_goal) };
                // `bounding` is in map pixels, the node in cells.
                if pos.0 < 0 || pos.1 < 0 || pos.0 >= bound.0 || pos.1 >= bound.1 {
                    continue;
                }
                let ok = if self.reverse { !open_cell(grid, f.pos) } else { open_cell(grid, pos) };
                if !ok || self.closed.iter().any(|&c| self.nodes[c].pos == pos) {
                    continue;
                }
                let open = self.open.as_mut().expect("set");
                if let Some(&o) = open.iter().find(|&&o| self.nodes[o].pos == pos) {
                    if self.nodes[o].total > neighbour.total {
                        let node = &mut self.nodes[o];
                        node.parent = Some(first);
                        node.cost = neighbour.cost;
                        node.total = f.total + neighbour.cost;
                        if self.reverse {
                            node.remaining = if open_cell(grid, pos) { 0 } else { 1 };
                        }
                    }
                } else if neighbour.scoring() < self.score_cap {
                    if self.reverse {
                        neighbour.remaining = if open_cell(grid, pos) { 0 } else { 1 };
                    }
                    self.nodes.push(neighbour);
                    open.push(self.nodes.len() - 1);
                }
            }
        }

        if self.open.as_ref().is_some_and(|o| o.is_empty()) {
            self.finish();
        }
    }
}

/// `(int)Math.Floor(counter)` for the small positive counts here.
fn libm_floor(v: f32) -> i32 {
    let t = v as i32;
    if (t as f32) > v {
        t - 1
    } else {
        t
    }
}

/// **.NET's `List<T>.Sort(IComparer<T>)`**: `ArraySortHelper<T>`'s
/// introspective sort — insertion sort up to sixteen, median-of-three
/// quicksort, heapsort past the depth limit. It is not stable, and which of
/// two equal nodes comes first is part of v1.2's route.
pub fn dotnet_sort<T: Copy>(keys: &mut [T], cmp: impl Fn(T, T) -> Ordering) {
    if keys.len() > 1 {
        let depth = 2 * ((usize::BITS - 1 - keys.len().leading_zeros()) as i32 + 1);
        intro_sort(keys, depth, &cmp);
    }
}

fn intro_sort<T: Copy>(keys: &mut [T], mut depth: i32, cmp: &impl Fn(T, T) -> Ordering) {
    let mut size = keys.len();
    while size > 1 {
        if size <= 16 {
            match size {
                2 => swap_if_greater(keys, cmp, 0, 1),
                3 => {
                    swap_if_greater(keys, cmp, 0, 1);
                    swap_if_greater(keys, cmp, 0, 2);
                    swap_if_greater(keys, cmp, 1, 2);
                }
                _ => insertion_sort(&mut keys[..size], cmp),
            }
            return;
        }
        if depth == 0 {
            heap_sort(&mut keys[..size], cmp);
            return;
        }
        depth -= 1;
        let p = pick_pivot_and_partition(&mut keys[..size], cmp);
        intro_sort(&mut keys[p + 1..size], depth, cmp);
        size = p;
    }
}

fn swap_if_greater<T: Copy>(keys: &mut [T], cmp: &impl Fn(T, T) -> Ordering, i: usize, j: usize) {
    if cmp(keys[i], keys[j]) == Ordering::Greater {
        keys.swap(i, j);
    }
}

fn pick_pivot_and_partition<T: Copy>(keys: &mut [T], cmp: &impl Fn(T, T) -> Ordering) -> usize {
    let hi = keys.len() - 1;
    let middle = hi >> 1;
    swap_if_greater(keys, cmp, 0, middle);
    swap_if_greater(keys, cmp, 0, hi);
    swap_if_greater(keys, cmp, middle, hi);
    let pivot = keys[middle];
    keys.swap(middle, hi - 1);
    let (mut left, mut right) = (0usize, hi - 1);
    while left < right {
        loop {
            left += 1;
            if cmp(keys[left], pivot) != Ordering::Less {
                break;
            }
        }
        loop {
            right -= 1;
            if cmp(pivot, keys[right]) != Ordering::Less {
                break;
            }
        }
        if left >= right {
            break;
        }
        keys.swap(left, right);
    }
    if left != hi - 1 {
        keys.swap(left, hi - 1);
    }
    left
}

fn heap_sort<T: Copy>(keys: &mut [T], cmp: &impl Fn(T, T) -> Ordering) {
    let n = keys.len();
    let mut i = n >> 1;
    while i >= 1 {
        down_heap(keys, i, n, cmp);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        keys.swap(0, i - 1);
        down_heap(keys, 1, i - 1, cmp);
        i -= 1;
    }
}

fn down_heap<T: Copy>(keys: &mut [T], mut i: usize, n: usize, cmp: &impl Fn(T, T) -> Ordering) {
    let d = keys[i - 1];
    while i <= n >> 1 {
        let mut child = 2 * i;
        if child < n && cmp(keys[child - 1], keys[child]) == Ordering::Less {
            child += 1;
        }
        if cmp(d, keys[child - 1]) != Ordering::Less {
            break;
        }
        keys[i - 1] = keys[child - 1];
        i = child;
    }
    keys[i - 1] = d;
}

fn insertion_sort<T: Copy>(keys: &mut [T], cmp: &impl Fn(T, T) -> Ordering) {
    for i in 0..keys.len() - 1 {
        let t = keys[i + 1];
        let mut j = i as isize;
        while j >= 0 && cmp(t, keys[j as usize]) == Ordering::Less {
            keys[j as usize + 1] = keys[j as usize];
            j -= 1;
        }
        keys[(j + 1) as usize] = t;
    }
}

/// **`ComplexPath`**: one walker's destination and his search.
#[derive(Clone, Debug, Default)]
pub struct ComplexPath {
    /// `move`: where he was sent.
    move_to: Point,
    /// `adjustedMove`: what the search aims at — `move`, or the nearest
    /// place to it the search found.
    adjusted: Fx,
    /// `position`: where the last step left him.
    position: Fx,
    search: Search,
    /// `PathState.precisePosition`, while it rounds to where he stands.
    precise: Option<Fx>,
    /// `PathState.MovementDirection`: the last step's walk.
    direction: Fx,
}

impl ComplexPath {
    /// `Create<ComplexPath>()` and `MoveTo = Position`.
    pub fn standing(at: Point) -> ComplexPath {
        let mut p = ComplexPath { position: fx(at), ..ComplexPath::default() };
        p.set_move_to(at);
        p
    }

    /// `MoveTo`'s getter.
    pub fn move_to(&self) -> Point {
        self.move_to
    }

    /// `adjustedMove`, as a save writes it: v1.2 serializes it with `move`
    /// (only `path` is `[NonSerialized]`, `ComplexPath.cs:452-453`).
    pub fn adjusted(&self) -> (f32, f32) {
        self.adjusted
    }

    /// A path read back from a save: `move` and `adjustedMove` as they were,
    /// no search, and — `precisePosition` being `[NonSerialized]` — the whole
    /// pixel.
    pub fn loaded(at: Point, move_to: Point, adjusted: (f32, f32)) -> ComplexPath {
        ComplexPath { move_to, adjusted, position: fx(at), ..ComplexPath::default() }
    }

    /// `MoveTo`'s setter: a new destination, searched for on the next step.
    pub fn set_move_to(&mut self, to: Point) {
        self.move_to = to;
        self.adjusted = fx(to);
    }

    /// **`UpdateMovingTarget`** (`ComplexPath.cs:425-442`): follow a target
    /// that moves without throwing the route away. The new destination is
    /// adopted only when asked to (`force`), when no search is set, or when
    /// the search is done and its route used up or within
    /// [`MOVING_TARGET_REPATH`] pixels of its end.
    pub fn update_moving_target(&mut self, to: Point, force: bool) {
        self.move_to = to;
        let near_end = || {
            let d = (self.search.goal.0 - self.position.0, self.search.goal.1 - self.position.1);
            length(d) <= MOVING_TARGET_REPATH as f32
        };
        if force || !self.search.is_set() || self.search.finished && (self.search.is_empty() || near_end()) {
            self.adjusted = fx(to);
        }
    }

    /// `Stop` (`PathState.cs:36-43`, `ComplexPath.cs:467-472`).
    pub fn stop(&mut self, at: Point, stride: &mut Stride) {
        self.set_move_to(at);
        stride.stop(at);
        self.precise = Some(fx(at));
        self.direction = (0.0, 0.0);
        self.position = fx(at);
        self.search.clear();
    }

    /// Whether a search is under way or done: `IsPathSet`.
    pub fn searching(&self) -> bool {
        self.search.is_set()
    }

    /// Whether the search has finished: `HasFinishedCalculation`.
    pub fn finished(&self) -> bool {
        self.search.finished
    }

    /// The waypoints still ahead, the next first.
    pub fn way_points(&self) -> &[Point] {
        &self.search.way_points
    }

    /// `Path.Goal`: where the search aims — the destination, or after it
    /// has finished the nearest place to it it found.
    pub fn goal(&self) -> Point {
        raster(self.search.goal)
    }

    /// **`Process`** (`ComplexPath.cs:474-556`): one step's search and walk
    /// at `speed` (thousandths of a pixel a second, as [`crate::gait`]).
    /// `Arrived` when he is at `MoveTo`, or when the search is done and
    /// nothing is left to walk; `Going` otherwise. `stride` is kept to the
    /// same position, for whatever else reads it.
    pub fn process(&mut self, grid: &WalkGrid, at: Point, stride: &mut Stride, speed: u32, elapsed: f32) -> (Point, Walked) {
        let speed = speed as f32 / 1000.0;
        let previous = self.direction;
        // `BeginMovement`.
        let mut p = match self.precise {
            Some(q) if raster(q) == at => q,
            _ => fx(at),
        };
        self.direction = (0.0, 0.0);
        self.position = p;
        let start = p;
        let goal = fx(self.move_to);
        let mut elapsed_speed = speed * elapsed;
        if length((goal.0 - p.0, goal.1 - p.1)) < 0.1 {
            self.search.clear();
            return (self.commit(p, start, stride), Walked::Arrived);
        }

        let updated = self.adjusted != self.search.goal || !self.search.is_set();
        if updated {
            self.search.begin(grid, p, self.adjusted, elapsed, false);
        } else if !self.search.finished {
            self.search.process(grid, p, elapsed);
        }
        self.adjusted = self.search.goal;

        // The waypoints. One reached is let go *without* stepping onto it:
        // the rest of the step heads for the next from where he is.
        while elapsed_speed > 0.0 && !self.search.is_empty() {
            let w = fx(self.search.next_way_point());
            let d = (w.0 - p.0, w.1 - p.1);
            let len = length(d);
            if len > elapsed_speed {
                let n = normalize(d);
                p = (p.0 + n.0 * elapsed_speed, p.1 + n.1 * elapsed_speed);
                elapsed_speed = 0.0;
            } else {
                elapsed_speed -= len;
                self.search.next();
            }
        }

        // `:531-551`: no waypoint while a new route is searched for — keep
        // going the way he went, a pixel at a time while it is walkable.
        if p == start && (updated || !self.search.finished) && previous != (0.0, 0.0) {
            let dir = normalize(previous);
            let mut remaining = speed * elapsed;
            while remaining > 0.0 {
                let step = if remaining < 1.0 { remaining } else { 1.0 };
                let next = (p.0 + dir.0 * step, p.1 + dir.1 * step);
                if !walkable(grid, next) {
                    break;
                }
                p = next;
                remaining -= step;
            }
        }

        let stood = p == start && self.search.finished && self.search.is_empty();
        let now = self.commit(p, start, stride);
        (now, if stood { Walked::Arrived } else { Walked::Going })
    }

    /// `CommitMovement`.
    fn commit(&mut self, p: Fx, start: Fx, stride: &mut Stride) -> Point {
        self.direction = (p.0 - start.0, p.1 - start.1);
        self.position = p;
        self.precise = Some(p);
        let now = raster(p);
        let sub = |v: f32, w: i32| ((v - w as f32) * SUB as f32) as i32;
        *stride = Stride { at: now, fx: sub(p.0, now.x), fy: sub(p.1, now.y) };
        now
    }
}

/// `ComplexPath.IsWalkable` (`:563-571`).
fn walkable(grid: &WalkGrid, p: Fx) -> bool {
    if p.0 < 0.0 || p.1 < 0.0 {
        return false;
    }
    open_cell(grid, to_grid(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sort_is_dotnet_s_on_ties() {
        // Seventeen keys and forty: past insertion sort, through the
        // partition, and (forty, by a depth limit of nought) the heap sort.
        let mut keys: Vec<(i32, usize)> = (0..17).map(|i| (if i % 2 == 0 { 1 } else { 0 }, i)).collect();
        dotnet_sort(&mut keys, |a, b| a.0.cmp(&b.0));
        assert!(keys.windows(2).all(|w| w[0].0 <= w[1].0));
        let mut sorted: Vec<usize> = keys.iter().map(|k| k.1).collect();
        sorted.sort();
        assert_eq!(sorted, (0..17).collect::<Vec<_>>());
        let mut many: Vec<i32> = (0..40).map(|i| (i * 7919) % 23).collect();
        heap_sort(&mut many, &|a: i32, b: i32| a.cmp(&b));
        assert!(many.windows(2).all(|w| w[0] <= w[1]));
    }

    /// Up to sixteen keys it is an insertion sort, which keeps equals in
    /// the order they came: what the hand-worked routes below rely on.
    #[test]
    fn a_short_list_keeps_equals_in_order() {
        let mut keys: Vec<(i32, usize)> = [(2, 0), (1, 1), (2, 2), (1, 3), (0, 4), (1, 5)].to_vec();
        dotnet_sort(&mut keys, |a, b| a.0.cmp(&b.0));
        assert_eq!(keys, [(0, 4), (1, 1), (1, 3), (1, 5), (2, 0), (2, 2)]);
    }
}
