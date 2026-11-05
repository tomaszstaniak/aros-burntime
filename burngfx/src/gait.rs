//! How fast somebody walks in a v1.2 game, and the fraction of a pixel they
//! have walked and not yet shown.
//!
//! v1.2 gives every character a speed in pixels a second
//! (`Logic/Character/Character.cs:75-81,972-985`) and walks a float position
//! (`PathFinding/PathState.cs:16-21`, `precisePosition`) of which the map
//! shows the rounded pixel. A legacy game walks two pixels a step for
//! everybody ([`crate::game::WALK_SPEED`]) and does not come here.
//!
//! **The fraction is kept in fixed point**, [`SUB`] to the pixel, so the host
//! and AROS walk the same pixels: a step's walk is the speed over
//! [`crate::mind::SECOND`] steps, and what is left over is carried in a
//! [`Stride`] to the next step. The world steps 18 times a second
//! (`burngfx::clock`), so a speed of 50 is 2 7/9 pixels a step.

use crate::game::Point;

/// Parts of a pixel the fraction is counted in.
pub const SUB: i64 = 1024;

/// `CONTROLLED_WALK_SPEED`: the active player's group, and whoever the
/// player has selected or controls. In thousandths of a pixel a second.
pub const CONTROLLED: u32 = 50_000;
/// `WALK_SPEED`: whoever has no `SimpleMind` — a creature with its
/// `CreatureMind`, another player's follower — and a `SimpleMind` dog.
pub const WALK: u32 = 35_000;
/// `WALK_SPEED * 0.66f`: everybody else with a `SimpleMind` — free people,
/// traders, campers. (`35f * 0.66f` is 23.099998 in `float`.)
pub const WANDER: u32 = 23_100;
/// `WALK_SPEED / 2`: a mutant with a `SimpleMind`.
pub const SLOW_MUTANT: u32 = 17_500;
/// `FLEE_SPEED`.
pub const FLEE: u32 = 70_000;

/// One step's walk at `speed` (thousandths of a pixel a second), in
/// [`SUB`]s of a pixel, rounded to the nearest: within a thousandth of a
/// pixel a second of the speed.
pub fn per_step(speed: u32) -> i64 {
    let whole = speed as i64 * SUB;
    let div = 1000 * crate::mind::SECOND as i64;
    (whole + div / 2) / div
}

/// The fraction of a pixel somebody is past the pixel they stand on —
/// `precisePosition` less its rounding.
///
/// **It belongs to one pixel.** v1.2 forgets the fraction when the raster
/// position is changed under it (`BeginMovement`: a precise position that
/// does not round to where the character stands is replaced by it), so the
/// pixel it was taken at is kept with it: a character put somewhere else
/// starts that walk from the whole pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stride {
    /// The pixel this fraction is a fraction of.
    pub at: Point,
    /// `precisePosition − at`, in [`SUB`]s of a pixel.
    pub fx: i32,
    pub fy: i32,
}

impl Stride {
    /// The fraction past `at`, or none if it belongs to another pixel —
    /// what a save writes.
    pub fn carry(&self, at: Point) -> (i32, i32) {
        if self.at == at {
            (self.fx, self.fy)
        } else {
            (0, 0)
        }
    }

    /// A fraction read back from a save, for somebody standing at `at`.
    pub fn carried(at: Point, (fx, fy): (i32, i32)) -> Stride {
        if (fx, fy) == (0, 0) {
            // The whole pixel: the same walk as no fraction at all.
            return Stride::default();
        }
        Stride { at, fx: fx.clamp(-(SUB as i32) / 2, SUB as i32 / 2), fy: fy.clamp(-(SUB as i32) / 2, SUB as i32 / 2) }
    }
}

/// Everything about how one of the location's people moves in a v1.2 game
/// that is not where they are and where they are going.
///
/// v1.2 keeps all of it but `controlled` out of a save (`[NonSerialized]`,
/// `Character.cs:52-73`, `PathState.cs:17-22`), and so does the port since
/// save version 25: after a load only `controlled` is what it was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bearing {
    pub stride: Stride,
    /// `proximityPauseRemaining`, in steps: standing still for the selected
    /// character who came within talking distance.
    pub pause: u32,
    /// `wasInTalkingDistance`.
    pub near: bool,
    /// Has a `PlayerControlledMind`: a camper the player selected, walked by
    /// the player's clicks until the next day gives him back his
    /// `SimpleMind` (`ResetStationedMind`, `Character.cs:893-904`).
    pub controlled: bool,
    /// Has a `SimpleMind` for good: a creature some boss once took on, let
    /// go (`Dismiss`, `Character.cs:449`) or left in a camp (`JoinCamp`,
    /// `:377`). Only `GameCreation` gives a `CreatureMind`
    /// (`Logic/Generation/GameCreation.cs:452`), so such a dog or mutant
    /// wanders and never prowls again. Saved, as v1.2 saves the mind; set
    /// for creatures only, the one kind it changes.
    pub tame: bool,
}

impl Bearing {
    /// Nothing worth writing down for somebody standing at `at`.
    pub fn is_rest(&self, at: Point) -> bool {
        self.stride.carry(at) == (0, 0) && self.pause == 0 && !self.near && !self.controlled && !self.tame
    }
}

/// The game-wide part of v1.2 movement: the fraction of a pixel the playing
/// human's leader has walked, and the creatures' bite cooldown. Both
/// `[NonSerialized]` in v1.2 (`PathState.cs:17-18`,
/// `States/ClassicGame.cs:21-22`): a legacy save still writes it, a v1.2
/// save does not (version 25), and a v1.2 load starts both at nought.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    /// [`Stride::carry`] of the leader at his saved position.
    pub leader: (i32, i32),
    /// Steps until a creature may bite again — see
    /// [`crate::mind::CREATURE_COOLDOWN`].
    pub cooldown: u32,
}

/// One character's flight from another (`FleeFrom`: `fleeTimeRemaining`,
/// `fleeDestination`, `fleeFromCharacter`, `Character.cs:62-67`), as a save
/// writes it: who by tag — 0 the leader, 1 + id a companion, a million and
/// up somebody standing in the place, by index — where to, and steps left.
/// `[NonSerialized]` in v1.2, so a v1.2 load stops a flight under way, and a
/// v1.2 save (version 25) writes none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    pub who: u32,
    pub from: u32,
    pub to: Point,
    pub left: u32,
}

/// One of the group's `ComplexPath`s as v1.2 serializes it: `move` and
/// `adjustedMove` (`PathFinding/ComplexPath.cs:418-421`; only the search,
/// `path`, is `[NonSerialized]`, `:452-453`). `who` is tagged as in
/// [`Flight`]; `adjusted` is a `float` pair, written as its bits. A save
/// writes only those whose `adjustedMove` is not `move` — the rest a load
/// rebuilds from the `move_to` it already keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathRecord {
    pub who: u32,
    pub move_to: Point,
    pub adjusted: (f32, f32),
}

/// What a walk came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walked {
    /// Still on the way.
    Going,
    /// Reached the point aimed at.
    Arrived,
    /// The way was not walkable, and the walk was given up where it stood.
    Blocked,
}

/// `sqrt` of a non-negative integer, rounded down.
pub(crate) fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = (n as f64).sqrt_approx() as i64;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// A first guess for [`isqrt`] that needs nothing `core` lacks: Newton's
/// method from above converges in a few rounds for map distances.
trait SqrtApprox {
    fn sqrt_approx(self) -> f64;
}

impl SqrtApprox for f64 {
    fn sqrt_approx(self) -> f64 {
        if self <= 0.0 {
            return 0.0;
        }
        let mut x = if self > 1.0 { self / 2.0 } else { 1.0 };
        for _ in 0..64 {
            let next = 0.5 * (x + self / x);
            if next >= x {
                break;
            }
            x = next;
        }
        x
    }
}

impl Stride {
    /// Where somebody standing at `at` precisely is.
    pub fn precise(&self, at: Point) -> (i64, i64) {
        let (x, y) = (at.x as i64 * SUB, at.y as i64 * SUB);
        if self.at == at {
            (x + self.fx as i64, y + self.fy as i64)
        } else {
            (x, y)
        }
    }

    /// `Path.Stop`: the precise position is the pixel.
    pub fn stop(&mut self, at: Point) {
        *self = Stride { at, fx: 0, fy: 0 };
    }

    /// Take `p` as the precise position: the pixel it rounds to, and the
    /// fraction past it. `(int)(x + 0.5f)`.
    pub(crate) fn settle(&mut self, p: (i64, i64)) -> Point {
        let round = |v: i64| (v + SUB / 2).div_euclid(SUB);
        let at = Point::new(round(p.0) as i32, round(p.1) as i32);
        self.at = at;
        self.fx = (p.0 - at.x as i64 * SUB) as i32;
        self.fy = (p.1 - at.y as i64 * SUB) as i32;
        at
    }
}

/// From `p` toward `to` (both precise), at most `budget`: the new point and
/// how much of the budget it took.
fn toward(p: (i64, i64), to: (i64, i64), budget: i64) -> ((i64, i64), i64) {
    let (dx, dy) = (to.0 - p.0, to.1 - p.1);
    let len = isqrt(dx * dx + dy * dy);
    if len <= budget {
        (to, len)
    } else {
        ((p.0 + dx * budget / len, p.1 + dy * budget / len), budget)
    }
}

fn pixel(p: (i64, i64)) -> Point {
    let round = |v: i64| (v + SUB / 2).div_euclid(SUB) as i32;
    Point::new(round(p.0), round(p.1))
}

/// **`SimplePath.Process`**: one step straight at `move_to`, and **given up
/// when the way is blocked** — the step's end and a point half a mask cell
/// further on must both be walkable, or `move_to` becomes where they stand
/// (`PathFinding/SimplePath.cs:25-59`). Free people, traders, campers and
/// creatures walk this way; the mind picks somewhere else next time.
pub fn simple(
    at: Point,
    stride: &mut Stride,
    move_to: &mut Point,
    budget: i64,
    walkable: &impl Fn(Point) -> bool,
) -> (Point, Walked) {
    let p = stride.precise(at);
    let goal = (move_to.x as i64 * SUB, move_to.y as i64 * SUB);
    let (dx, dy) = (goal.0 - p.0, goal.1 - p.1);
    let len = isqrt(dx * dx + dy * dy);
    // `walkVector.Length < 0.1f`
    if len * 10 < SUB {
        stride.settle(p);
        return (at, Walked::Arrived);
    }
    let (walk, ahead) = if len > budget {
        let lead = budget + crate::zei::MASK_CELL as i64 / 2 * SUB;
        (
            (p.0 + dx * budget / len, p.1 + dy * budget / len),
            (p.0 + dx * lead / len, p.1 + dy * lead / len),
        )
    } else {
        (goal, goal)
    };
    if walkable(pixel(walk)) && walkable(pixel(ahead)) {
        let now = stride.settle(walk);
        (now, if walk == goal { Walked::Arrived } else { Walked::Going })
    } else {
        // `moveTo = this.position`: the precise position, rounded.
        let now = stride.settle(p);
        *move_to = now;
        (now, Walked::Blocked)
    }
}

/// **`ComplexPath.Process`**'s walk: along a route found around what is in
/// the way, carrying what is left of a step past a waypoint on to the next
/// (`PathFinding/ComplexPath.cs:591-610`).
///
/// `route` holds the waypoints still to reach, **the next one last**, and
/// loses each as it is reached; `goal` is walked to once it is empty. A
/// point on the way that `passable` refuses ends the walk where it stood —
/// the route said one thing and the grid another.
pub fn along(
    at: Point,
    stride: &mut Stride,
    route: &mut alloc::vec::Vec<Point>,
    goal: Point,
    budget: i64,
    passable: &impl Fn(Point) -> bool,
) -> (Point, Walked) {
    let mut p = stride.precise(at);
    let mut left = budget;
    loop {
        let aim = route.last().copied().unwrap_or(goal);
        let to = (aim.x as i64 * SUB, aim.y as i64 * SUB);
        let (next, used) = toward(p, to, left);
        if next != p && !passable(pixel(next)) {
            let now = stride.settle(p);
            return (now, Walked::Blocked);
        }
        p = next;
        left -= used;
        if p != to {
            return (stride.settle(p), Walked::Going);
        }
        if route.pop().is_none() {
            return (stride.settle(p), Walked::Arrived);
        }
        if left <= 0 {
            return (stride.settle(p), if route.is_empty() && aim == goal { Walked::Arrived } else { Walked::Going });
        }
    }
}

/// Straight at `to`, at most `budget`, with nothing in the way asked about:
/// the walk of a combat approach or a flight for somebody whose route is
/// already decided. Returns where they got to.
pub fn straight(at: Point, stride: &mut Stride, to: Point, budget: i64) -> Point {
    let p = stride.precise(at);
    let (next, _) = toward(p, (to.x as i64 * SUB, to.y as i64 * SUB), budget);
    stride.settle(next)
}

/// Distance in pixels between two precise points, for "within four pixels"
/// and the like.
pub fn within(a: Point, b: Point, pixels: i64) -> bool {
    let (dx, dy) = ((a.x - b.x) as i64, (a.y - b.y) as i64);
    dx * dx + dy * dy <= pixels * pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isqrt_is_exact() {
        for n in [0i64, 1, 2, 3, 4, 15, 16, 17, 1 << 40, (1 << 40) - 1, 123_456_789_012] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "{n}: {r}");
        }
    }
}
