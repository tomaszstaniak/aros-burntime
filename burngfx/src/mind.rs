//! What everybody who is not the player does with their time.
//!
//! Two minds, ported from v0.9.1: `SimpleMind`, which every person and
//! trader has, and `CreatureMind`, which dogs and mutants have. They share a
//! wandering step and differ in everything else — a creature watches the
//! player, closes in, bites, and backs off.
//!
//! **Time here is counted in frames, not seconds.** This game's clock is
//! the frame everywhere — the played character covers
//! [`crate::game::WALK_SPEED`] pixels per frame rather than per second — and
//! a mind driven by a wall clock in a game driven by frames would make a
//! resumed run differ from an uninterrupted one on nothing but how fast the
//! two machines drew. So the original's seconds are converted once, by
//! [`SECOND`], and everything below is whole frames.
//!
//! The randomness: **in a v1.2 game every mind draws from the game's one
//! generator** ([`Draws::Game`]), as upstream's draw from its one `Random`,
//! the process's, which no save carries. In a legacy game each character
//! draws from a stream of their own, indexed by a counter they carry
//! ([`Draws::Own`]), so the state that has to survive a save is one number
//! rather than a generator.

use crate::game::Point;

/// Where a mind's numbers come from.
pub enum Draws<'r> {
    /// A legacy game: the character's own stream, from the world's seed,
    /// their record and the count in [`Mind::draws`].
    Own(u64),
    /// A v1.2 game: the game's generator, `Random.Next()` for each draw.
    Game(&'r mut crate::spawn::Rng),
}

impl Draws<'_> {
    pub fn is_game(&self) -> bool {
        matches!(self, Draws::Game(_))
    }
}

/// How many frames stand for one of the original's seconds.
///
/// `Path.Speed = 35` against our two pixels a frame: eighteen frames cover
/// about what a character covered in a second there. It is one number in
/// one place so that "three to seven seconds" below stays readable.
pub const SECOND: u32 = 18;

/// `Path.Speed`, in pixels per frame, for everybody who is not the player.
/// The same two the played character walks at: the legacy game's one
/// speed. A v1.2 game moves each kind at v1.2's own speed
/// (`Character.cs:75-81,972-1009`; compliance RULE-30).
pub const STEP: i32 = crate::game::WALK_SPEED;

/// How far a creature sees the played character: `(sel.Position -
/// Owner.Position).Length < 200`.
pub const SIGHT: i32 = 200;

/// How long a creature will chase before giving up: `tryToAttack > 3`.
pub const CHASE: u32 = 3 * SECOND;

/// `CREATURE_ATTACK_COOLDOWN`: one creature bite a second in the whole game
/// (`States/ClassicGame.cs:19-22`), in steps.
pub const CREATURE_COOLDOWN: u32 = SECOND;

/// What a v1.2 creature sees of the character it is charging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    pub at: Point,
    pub dead: bool,
    /// Carries something with `creature_deterrent` (`HasItemFunction`).
    pub deterred: bool,
}

/// A character's state of mind, and all of it that a save has to carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mind {
    /// `takeSomeRest`, in frames. Standing about, deciding nothing.
    pub rest: u32,
    /// `first`: the step just taken was the first of a new walk, so the
    /// next pause is the short one.
    pub first: bool,
    /// `waitAfterAttack`, in frames, and **signed on purpose**: upstream
    /// tests `>= 0` and lets it run negative, which is how "ready again"
    /// is spelled there.
    pub wait_after_attack: i32,
    /// `tryToAttack`, in frames: how long this creature has been closing in.
    pub chasing: u32,
    /// `attack != null` — a creature has chosen the played character and is
    /// walking at them. Upstream holds the character; the only character it
    /// is ever set to is the one the player is playing, so a flag says the
    /// same thing.
    pub charging: bool,
    /// Whom a v1.2 creature is charging (`attack`), while [`Mind::charging`]:
    /// the tag its caller gave the target — see [`Mind::prowl_v12`]. A
    /// legacy creature only ever charges the played character and leaves
    /// this at nought.
    pub target: u32,
    /// How many numbers this character has drawn from a stream of their own
    /// (a legacy game). The stream is a function of the world's seed, the
    /// character's record and this count, so a save carries a counter
    /// instead of a generator. A v1.2 game's minds draw from the game's
    /// generator and leave it alone.
    pub draws: u32,
}

impl Mind {
    /// The next number in this character's own stream.
    ///
    /// **The counter has to be stirred, not just xored in.** The first
    /// version seeded a generator with `seed ^ record ^ counter << 32` and
    /// took one number from it, and two draws one apart came out with the
    /// same parity — so a wanderer's two signs always agreed and everybody
    /// in the world walked north-east or south-west and nowhere else. A
    /// test that asked for all four quadrants is what caught it.
    ///
    /// This is the finalizer from SplitMix64, which is built for exactly
    /// this: turn a counter into a number with no visible relation to its
    /// neighbours.
    fn draw(&mut self, from: &mut Draws, record: usize) -> u64 {
        let seed = match from {
            Draws::Own(seed) => *seed,
            // `Random.Next()`: every `% n` below is upstream's own.
            Draws::Game(rng) => return rng.next_int() as u64,
        };
        let mut z = seed
            ^ (record as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (self.draws as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        self.draws = self.draws.wrapping_add(1);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Somewhere nearby, at random: `25 + rnd % 75` in each axis, each with
    /// a sign of its own.
    ///
    /// `SimpleMind` then **reverses both axes** when the spot is off the
    /// walkable mask, which is how a person turns round at the edge of the
    /// map rather than grinding into it. `CreatureMind` has no such test —
    /// the same eight lines without the check.
    fn somewhere_nearby(
        &mut self,
        d: &mut Draws,
        record: usize,
        from: Point,
        check_mask: bool,
        walkable: &impl Fn(Point) -> bool,
    ) -> Point {
        let span = |n: u64| (n % 75) as i32 + 25;
        let sign = |n: u64| (n % 2) as i32 * 2 - 1;
        let (x, y) = if d.is_game() {
            // Upstream's order: both spans, then both signs.
            let (sx, sy) = (span(self.draw(d, record)), span(self.draw(d, record)));
            (sx * sign(self.draw(d, record)), sy * sign(self.draw(d, record)))
        } else {
            let x = span(self.draw(d, record)) * sign(self.draw(d, record));
            (x, span(self.draw(d, record)) * sign(self.draw(d, record)))
        };
        let mut go = Point::new(x, y);
        if check_mask && !walkable(Point::new(from.x + go.x, from.y + go.y)) {
            go = Point::new(-go.x, -go.y);
        }
        Point::new(from.x + go.x, from.y + go.y)
    }

    /// The tail both minds share: having arrived, rest a little or pick
    /// somewhere else to be.
    ///
    /// `run_away` is the creature's: after a bite it neither rests nor
    /// rolls for a rest — it simply goes somewhere.
    #[allow(clippy::too_many_arguments)]
    fn decide(
        &mut self,
        d: &mut Draws,
        record: usize,
        at: Point,
        move_to: &mut Point,
        run_away: bool,
        check_mask: bool,
        walkable: &impl Fn(Point) -> bool,
    ) {
        if at != *move_to && !run_away {
            return;
        }
        if self.first && !run_away {
            self.rest = SECOND / 2;
            self.first = false;
        } else if !run_away && self.draw(d, record).is_multiple_of(9) {
            self.rest = 2 * SECOND;
        } else {
            *move_to = self.somewhere_nearby(d, record, at, check_mask, walkable);
            self.first = true;
        }
    }

    /// `SimpleMind.Process`: wander, and nothing else. A person with no
    /// employer walks about their location for ever, and that is the whole
    /// of their behaviour.
    pub fn wander(
        &mut self,
        d: &mut Draws,
        record: usize,
        at: Point,
        move_to: &mut Point,
        walkable: &impl Fn(Point) -> bool,
    ) {
        if self.rest > 0 {
            self.rest -= 1;
            return;
        }
        self.decide(d, record, at, move_to, false, true, walkable);
    }

    /// `CreatureMind.Process`: watch, close in, bite, back off, wander.
    ///
    /// `player` is where the played character is standing, or `None` when
    /// they are somewhere else entirely — upstream asks `player.Location ==
    /// Owner.Location` before it looks at the distance at all.
    ///
    /// Returns true on the frame it bites. **The blow itself is the
    /// caller's**: the rules of a fight need both packs and both kits, and
    /// a mind knows neither.
    pub fn prowl(
        &mut self,
        d: &mut Draws,
        record: usize,
        at: Point,
        move_to: &mut Point,
        player: Option<Point>,
        walkable: &impl Fn(Point) -> bool,
    ) -> bool {
        if self.rest > 0 {
            self.rest -= 1;
            return false;
        }

        let mut run_away = false;
        let mut struck = false;
        if self.charging {
            match player.filter(|p| crate::combat::in_reach(*p, at, crate::combat::CREATURE_REACH))
            {
                // Close enough: bite and forget the target. v1.2's mind
                // then **returns** (`AttackWithoutRetaliation(attack);
                // attack = null; return;`, `AI/CreatureMind.cs:56-58`):
                // no wait, no rest, no wander that frame — the flight is
                // the blow's own (`FleeFrom`). A legacy game's goes
                // somewhere else this same frame.
                Some(_) => {
                    self.charging = false;
                    if d.is_game() {
                        return true;
                    }
                    struck = true;
                    run_away = true;
                }
                None => {
                    self.chasing += 1;
                    if self.chasing > CHASE {
                        self.charging = false;
                    }
                }
            }
        }

        if self.wait_after_attack >= 0 {
            self.wait_after_attack -= 1;
        } else if let Some(p) = player {
            let (dx, dy) = ((p.x - at.x) as i64, (p.y - at.y) as i64);
            if dx * dx + dy * dy < (SIGHT as i64) * (SIGHT as i64) {
                // One time in three it comes for you.
                if self.draw(d, record).is_multiple_of(3) {
                    self.chasing = 0;
                    self.charging = true;
                    *move_to = p;
                }
                // And either way it waits three to seven seconds before
                // thinking about it again.
                self.wait_after_attack =
                    (3 * SECOND + (self.draw(d, record) % 5) as u32 * SECOND) as i32;
            }
        }

        self.decide(d, record, at, move_to, run_away, false, walkable);
        struck
    }

    /// **v1.2's `CreatureMind.Process`** (`AI/CreatureMind.cs:21-128`), for
    /// a frame in which the creature is neither approaching, held nor
    /// fleeing in a fight — v1.2 does not ask the mind then at all
    /// (`Logic/Character/Character.cs:1018-1023`), so the caller does not
    /// either.
    ///
    /// * **The target is held** (`attack`), by its tag in
    ///   [`Mind::target`], and `held` is what the caller sees of it now:
    ///   where it stands, whether it is dead, whether it carries a
    ///   deterrent. A deterrent drops it on the spot; a dead one is never
    ///   bitten and is dropped when the chase times out.
    /// * **In a city a creature attacks nothing** (`canAttack =
    ///   !Owner.Location.IsCity`): a held target is dropped and the watch
    ///   is not kept.
    /// * **A bite needs the game-wide cooldown** (`TryBeginCreatureAttack`,
    ///   `States/ClassicGame.cs:106-119`): at nought it bites, sets the
    ///   cooldown to a second and **returns** — no wait, no rest, no wander
    ///   that frame. Above nought it neither bites nor counts the chase:
    ///   it keeps its target and tries again next frame.
    /// * `chosen` is `SelectAttackTarget`'s pick from where the creature
    ///   stands — the selected character unless he carries a deterrent,
    ///   else the nearest of the party who does not — with its tag.
    ///
    /// Returns the tag of whom it bit. The blow and the flight are the
    /// caller's (`AttackWithoutRetaliation`).
    #[allow(clippy::too_many_arguments)]
    pub fn prowl_v12(
        &mut self,
        d: &mut Draws,
        record: usize,
        at: Point,
        move_to: &mut Point,
        city: bool,
        held: Option<Held>,
        chosen: Option<(Point, u32)>,
        cooldown: &mut u32,
        walkable: &impl Fn(Point) -> bool,
    ) -> Option<u32> {
        if self.rest > 0 {
            self.rest -= 1;
            return None;
        }
        if self.charging && held.is_some_and(|h| h.deterred) {
            self.charging = false;
            *move_to = at;
        }
        if city && self.charging {
            self.charging = false;
            *move_to = at;
        }
        if !city && self.charging {
            match held {
                Some(h) if !h.dead && crate::combat::in_reach(h.at, at, crate::combat::CREATURE_REACH) => {
                    if *cooldown == 0 {
                        *cooldown = CREATURE_COOLDOWN;
                        self.charging = false;
                        return Some(self.target);
                    }
                }
                _ => {
                    self.chasing += 1;
                    if self.chasing > CHASE {
                        self.charging = false;
                    }
                }
            }
        }
        if !city {
            if self.wait_after_attack >= 0 {
                self.wait_after_attack -= 1;
            } else if let Some((p, tag)) = chosen {
                let (dx, dy) = ((p.x - at.x) as i64, (p.y - at.y) as i64);
                if dx * dx + dy * dy < (SIGHT as i64) * (SIGHT as i64) {
                    if self.draw(d, record).is_multiple_of(3) {
                        self.chasing = 0;
                        self.charging = true;
                        self.target = tag;
                        *move_to = p;
                    }
                    self.wait_after_attack =
                        (3 * SECOND + (self.draw(d, record) % 5) as u32 * SECOND) as i32;
                }
            }
        }
        self.decide(d, record, at, move_to, false, false, walkable);
        None
    }

    /// `RequestToTalk`: stop where you are and wait four seconds. What
    /// somebody does when the player walks up to talk.
    pub fn asked_to_talk(&mut self, at: Point, move_to: &mut Point) {
        *move_to = at;
        self.rest = 4 * SECOND;
    }
}

/// How far a companion lets the leader get before walking after them:
/// `if (distance > 150)` in `FellowerMind`.
pub const FOLLOW_FAR: i32 = 150;
/// Past this they walk after the leader too, at a shorter leash:
/// `else if (distance > 30)`.
pub const FOLLOW_NEAR: i32 = 30;
/// Closer than this they step aside rather than stand on the leader:
/// `else if (distance < 15)`.
pub const FOLLOW_CROWD: i32 = 15;
/// How far aside: `Sin(r) * 14`, `Cos(r) * 14`.
pub const FOLLOW_MARGIN: i32 = 14;

fn length(a: Point, b: Point) -> f32 {
    let (dx, dy) = ((a.x - b.x) as f32, (a.y - b.y) as f32);
    // `core` has no square root; this is Newton's method, which converges
    // in a handful of steps for the distances a map has.
    let sq = dx * dx + dy * dy;
    if sq == 0.0 {
        return 0.0;
    }
    let mut x = if sq > 1.0 { sq / 2.0 } else { 1.0 };
    for _ in 0..16 {
        x = 0.5 * (x + sq / x);
    }
    x
}

/// `FellowerMind.Process`: what a companion does with a frame.
///
/// Three bands, measured from the companion to the leader, each only
/// re-aiming when the companion's own destination has drifted far enough
/// from the leader to be worth it:
///
/// | distance to leader | re-aim when destination is | to |
/// |---|---|---|
/// | over 150 | over 140 from the leader | the leader |
/// | 31 to 150 | over 20 from the leader | the leader |
/// | under 15 | under 10 from the leader | 14 aside, at a random angle |
///
/// **The first two branches draw a random angle and throw it away** — the
/// margin is computed and then `// +margin` is commented out of the line
/// that uses it. The draw is kept here too. That keeps the *order of
/// operations* the original's: a draw happens in the same branches, so the
/// counter moves at the same decisions. It does **not** make the numbers the
/// original's — our stream is not `System.Random` and was never meant to be.
///
/// Returns nothing to do in single mode, which this game does not have yet:
/// the whole group is always selected.
///
/// `d` is where the numbers come from: in a v1.2 game the game's generator,
/// and `draws` is left alone. (A companion's own stream is keyed by his
/// identity in his party, which a person's record or the other party's
/// companion can share; the game's generator has no such keys.)
pub fn follow(d: &mut Draws, id: usize, draws: &mut u32, at: Point, move_to: &mut Point, leader: Point) {
    let mut mind = Mind { draws: *draws, ..Mind::default() };
    let distance = length(leader, at);
    let to_destination = length(leader, *move_to);
    let mut angle = |mind: &mut Mind| {
        // `(Random.Next() % 360) * PI / 180`.
        let degrees = (mind.draw(d, id) % 360) as f32;
        degrees * core::f32::consts::PI / 180.0
    };

    if distance > FOLLOW_FAR as f32 {
        if to_destination > 140.0 {
            let _unused = angle(&mut mind);
            *move_to = leader;
        }
    } else if distance > FOLLOW_NEAR as f32 {
        if to_destination > 20.0 {
            let _unused = angle(&mut mind);
            *move_to = leader;
        }
    } else if distance < FOLLOW_CROWD as f32 && to_destination < 10.0 {
        let r = angle(&mut mind);
        let (sin, cos) = sin_cos(r);
        *move_to = Point::new(
            leader.x + (sin * FOLLOW_MARGIN as f32) as i32,
            leader.y + (cos * FOLLOW_MARGIN as f32) as i32,
        );
    }
    *draws = mind.draws;
}

/// `core` has no trigonometry. A Taylor series to the ninth power is exact
/// to well under a pixel at a radius of fourteen, which is all it is for.
fn sin_cos(r: f32) -> (f32, f32) {
    // Fold into [-PI, PI] so the series converges quickly.
    let two_pi = 2.0 * core::f32::consts::PI;
    let mut x = r % two_pi;
    if x > core::f32::consts::PI {
        x -= two_pi;
    }
    let x2 = x * x;
    let sin = x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
    let cos = 1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0 * (1.0 - x2 / 90.0))));
    (sin, cos)
}

// -- v1.2's follower ----------------------------------------------------------

/// `CatchUpDistance`: beyond it a follower walks a fifth faster than the
/// leader, and below it (down to 15) he leaves his slot alone.
pub const CATCH_UP: i32 = 40;

/// How the leader is moving, as v1.2's `FellowerMind` asks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lead {
    /// Walking by a click (`ComplexPath`) to this point, or standing
    /// (`None`: `MoveTo == Position`).
    Clicked(Option<Point>),
    /// Walking by the keys (`ManualPath`) this way — `(0, 0)` while the keys
    /// are let go.
    Keys((i32, i32)),
}

/// **v1.2's `FellowerMind`** (`AI/FellowerMind.cs`): a companion's own
/// state while he follows. v1.2 keeps none of it in a save — every field is
/// `[NonSerialized]` (`:20-37`) — and neither does the port: after a load it
/// is all nought, and the radius is drawn again the first time it is needed
/// (`burngfx::save::Save::forget_unserialized`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fellower {
    /// `formationRadius`, drawn `Next(14, 21)` the first time it is needed;
    /// nought until then.
    pub radius: i32,
    /// `lastLeaderDestination`, while `hasLeaderDestination`.
    pub leader_dest: Option<Point>,
    /// `followStartDelay`, in 1/18 000 s (a millisecond is 18).
    pub delay: u32,
    /// `manualFollowActive`.
    pub manual: bool,
    /// `manualFollowStartDelay`, as `delay`.
    pub manual_delay: u32,
    /// `manualFollowStopElapsed`, in steps.
    pub manual_stop: u32,
    /// `lastManualFollowDirection`, while `hasManualFollowDirection`.
    pub last_keys: Option<(i32, i32)>,
    /// What this step's [`Fellower::follow`] did to his `Path` — not state
    /// of v1.2's, the answer of one call.
    pub retarget: Retarget,
}

/// How `FellowerMind` told the path where to go this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Retarget {
    /// It did not.
    #[default]
    None,
    /// `Owner.Path.MoveTo = …`.
    MoveTo,
    /// `complexPath.UpdateMovingTarget(followTarget, directionChanged)`
    /// (`AI/FellowerMind.cs:160-170`).
    Moving { force: bool },
}

/// A step's length in the units of [`Fellower::delay`].
const STEP_UNITS: u32 = 1000;

/// `ManualFollowLookAhead`.
const LOOK_AHEAD: f64 = 24.0;

fn sin_cos64(r: f64) -> (f64, f64) {
    let pi = core::f64::consts::PI;
    let two_pi = 2.0 * pi;
    let mut x = r % two_pi;
    if x > pi {
        x -= two_pi;
    } else if x < -pi {
        x += two_pi;
    }
    let (mut sin, mut cos) = (0.0, 0.0);
    let (mut term_s, mut term_c) = (x, 1.0);
    for k in 0..12 {
        sin += term_s;
        cos += term_c;
        let n = (2 * k + 2) as f64;
        term_s *= -x * x / (n * (n + 1.0));
        term_c *= -x * x / ((n - 1.0) * n);
    }
    (sin, cos)
}

fn length64(a: Point, b: Point) -> f64 {
    length(a, b) as f64
}

/// `GetFollowTarget(center, radius)`: the follower's slot, his index of
/// `count` spread evenly round the leader and turned by 45 degrees, the
/// offset cut toward nought (`(int)`).
pub fn slot(center: Point, radius: i32, index: usize, count: usize) -> Point {
    let angle = core::f64::consts::PI / 4.0
        + 2.0 * core::f64::consts::PI * index as f64 / count.max(1) as f64;
    let (sin, cos) = sin_cos64(angle);
    Point::new(center.x + (sin * radius as f64) as i32, center.y + (cos * radius as f64) as i32)
}

impl Fellower {
    fn radius(&mut self, rng: &mut crate::spawn::Rng) -> i32 {
        if self.radius == 0 {
            self.radius = rng.range(14, 21);
        }
        self.radius
    }

    /// `GetFollowStartDelay`: `0.025 · (index + 1) + Next(0, 31) / 1000` s.
    fn start_delay(rng: &mut crate::spawn::Rng, index: usize) -> u32 {
        (25 * (index as u32 + 1) + rng.range(0, 31) as u32) * 18
    }

    /// One frame of `FellowerMind.Process`, for the follower `index` of
    /// `count` (the group in order, the leader not counted), standing at
    /// `at` and walking to `move_to`. The caller does not call it in single
    /// mode, nor while a fight moves him.
    ///
    /// Returns whether he walks at the catch-up speed this frame — the
    /// leader's times 1.2 (`CatchUpSpeedMultiplier`) — rather than his own.
    #[allow(clippy::too_many_arguments)]
    pub fn follow(
        &mut self,
        rng: &mut crate::spawn::Rng,
        index: usize,
        count: usize,
        at: Point,
        move_to: &mut Point,
        leader: Point,
        lead: Lead,
    ) -> bool {
        let distance = length64(leader, at);
        let boost = distance > CATCH_UP as f64;
        self.retarget = Retarget::None;

        if let Lead::Keys(dir) = lead {
            if dir != (0, 0) {
                self.leader_dest = None;
                self.delay = 0;
                self.manual_stop = 0;
                if !self.manual {
                    if distance <= CATCH_UP as f64 {
                        if *move_to != at {
                            *move_to = at;
                            self.retarget = Retarget::MoveTo;
                        }
                        return boost;
                    }
                    self.manual = true;
                    self.manual_delay = Self::start_delay(rng, index);
                    self.last_keys = None;
                }
                if self.manual_delay > 0 {
                    self.manual_delay = self.manual_delay.saturating_sub(STEP_UNITS);
                    return boost;
                }
                // `manualPath.Direction` is the keys' direction, normalised.
                let (dx, dy) = (dir.0 as f64, dir.1 as f64);
                let l = if dx != 0.0 && dy != 0.0 { core::f64::consts::SQRT_2 } else { 1.0 };
                let round = |v: f64| if v < 0.0 { -((-v + 0.5) as i32) } else { (v + 0.5) as i32 };
                let ahead = Point::new(
                    leader.x + round(dx / l * LOOK_AHEAD),
                    leader.y + round(dy / l * LOOK_AHEAD),
                );
                *move_to = slot(ahead, self.radius(rng), index, count);
                let changed = self.last_keys != Some(dir);
                self.last_keys = Some(dir);
                self.retarget = Retarget::Moving { force: changed };
                return true;
            }
        }

        if self.manual {
            if matches!(lead, Lead::Keys(_)) {
                self.manual_stop += 1;
                if self.manual_stop < crate::mind::SECOND {
                    return true;
                }
            }
            self.manual = false;
            self.manual_delay = 0;
            self.manual_stop = 0;
            self.last_keys = None;
            let target = slot(leader, self.radius(rng), index, count);
            if length64(target, *move_to) > 1.0 {
                *move_to = target;
                self.retarget = Retarget::MoveTo;
            }
            return boost;
        }
        self.manual_delay = 0;
        self.manual_stop = 0;

        if let Lead::Clicked(Some(dest)) = lead {
            if distance <= FOLLOW_FAR as f64 {
                if self.leader_dest != Some(dest) {
                    self.leader_dest = Some(dest);
                    self.delay = Self::start_delay(rng, index);
                }
                if self.delay > 0 {
                    self.delay = self.delay.saturating_sub(STEP_UNITS);
                    return boost;
                }
                let target = slot(dest, self.radius(rng), index, count);
                if length64(target, *move_to) > 1.0 {
                    *move_to = target;
                    self.retarget = Retarget::MoveTo;
                }
                return boost;
            }
        }

        self.leader_dest = None;
        self.delay = 0;

        if distance > FOLLOW_FAR as f64 {
            let target = slot(leader, 28, index, count);
            if length64(target, *move_to) > 120.0 {
                *move_to = target;
                self.retarget = Retarget::MoveTo;
            }
        } else if distance > CATCH_UP as f64 {
            let target = slot(leader, self.radius(rng), index, count);
            if length64(target, *move_to) > 20.0 {
                *move_to = target;
                self.retarget = Retarget::MoveTo;
            }
        } else if distance < FOLLOW_CROWD as f64 {
            let target = slot(leader, self.radius(rng), index, count);
            if length64(target, *move_to) > 1.0 {
                *move_to = target;
                self.retarget = Retarget::MoveTo;
            }
        }
        boost
    }
}
