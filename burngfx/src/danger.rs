//! What gas and radiation do to somebody standing in them.
//!
//! Ported from v0.9.1's `Logic/Interaction/Danger.cs`,
//! `Logic/Interaction/DangerProtection.cs` and `Character.GetDangerRate` /
//! `Character.Update`. **Every one of those files is byte-identical at
//! `v1.1`**, and so are the `protection=` lines of both item tables, so this
//! rule has no version axis.
//!
//! Three things decide the damage, and only three:
//!
//! * **How hard the danger hits.** `Danger.HealthDecrease` is hard-coded —
//!   `1.35` a second for radiation, `0.5` for gas. The number `gam.dat`
//!   carries per location is not a strength: it is the figure INFO prints.
//! * **What the character carries.** `UseBestProtection` looks through their
//!   own pack — not the group's, not the room's — and takes the highest rate
//!   for *this* danger. Carrying it is enough; nothing has to be put on.
//! * **Time.** It is spent per second of the world running, not per turn, so
//!   it only runs where people move. In this port that is the one scene
//!   [`crate::scenes`] says moves them, which is where v0.9.1 runs it too.
//!
//! Full protection is not "less damage": `rate` reaches zero and the whole
//! branch is skipped. A paper helmet's 20 leaves four fifths of it.
//!
//! **Kept in whole numbers, on purpose.** v0.9.1's health is a float and
//! survives a save as one. This port's is an integer, so what has been spent
//! but is not yet a whole point is carried in [`Exposure`] — and carried
//! exactly: every quantity here is a whole number of [`UNIT`]s, so twenty
//! seconds of radiation is 27 points and not 26.99, and the carry can be
//! written into a save and read back without a remainder going missing.

use crate::gamdat::Danger;
use crate::mind::SECOND;

/// `Danger.HealthDecrease`, in thousandths of a point a second.
pub fn health_decrease_milli(danger: Danger) -> u32 {
    match danger {
        Danger::None => 0,
        Danger::Gas => 500,
        Danger::Radiation => 1350,
    }
}

/// `Danger.HealthDecrease` as the reference writes it.
pub fn health_decrease(danger: Danger) -> f32 {
    health_decrease_milli(danger) as f32 / 1000.0
}

/// `Character.GetDangerRate`, in percent: what is left after the best thing
/// they carry. `best_protection` is the file's own whole percent — `??100`,
/// `??20` — and above a hundred it would heal, so it stops at zero.
pub fn rate_percent(danger: Danger, best_protection: u32) -> u32 {
    if danger == Danger::None {
        return 0;
    }
    100u32.saturating_sub(best_protection)
}

/// One point of health, in the units [`Exposure`] counts in: a thousandth of
/// a point, per percent, per world step.
pub const UNIT: u32 = 1000 * 100 * SECOND;

/// What one world step costs, in [`UNIT`]s.
pub fn per_step(danger: Danger, best_protection: u32) -> u32 {
    per_step_at(health_decrease_milli(danger), danger, best_protection)
}

/// [`per_step`] at a given rate, in thousandths of a point a second: v1.2's
/// `HazardDamage(type)`, read from the preset's `game.txt`.
pub fn per_step_at(milli: u32, danger: Danger, best_protection: u32) -> u32 {
    milli * rate_percent(danger, best_protection)
}

/// Damage over a stretch of seconds, as the reference computes it:
/// `HealthDecrease * elapsed * rate`. For describing, not for keeping.
pub fn damage(danger: Danger, best_protection: u32, seconds: f32) -> f32 {
    health_decrease(danger) * seconds * rate_percent(danger, best_protection) as f32 / 100.0
}

/// **`Character.Health`'s getter**, `(int)health` (`Character.cs:196-200`),
/// in a v1.2 game: the whole number kept, or nought for one lifted to 1
/// (see [`Exposure::lifted`]).
pub fn whole(health: i32, e: &Exposure) -> i32 {
    if e.lifted {
        0
    } else {
        health
    }
}

/// **A write through `Character.Health`'s setter** in a v1.2 game —
/// `Health -= 25`, `Health += 2`, a blow, `Health = result`: the value
/// written is a whole number, so the float's fraction is gone. Call it just
/// before the write; the health is then the getter's.
pub fn spend(health: &mut i32, e: &mut Exposure) {
    if e.lifted {
        *health = 0;
    }
    e.carried = 0;
    e.lifted = false;
}

/// What a danger has spent and not yet made a whole point of.
///
/// **Two games, two meanings.** A legacy game keeps its integer health and
/// `carried` is the damage taken past the last whole point paid, as v0.9.1's
/// port always kept it ([`Exposure::step`]). **A v1.2 game holds v1.2's float
/// health exactly** (RULE-41): the health number is the getter, `(int)h`,
/// and `carried` is `h` above it, in [`UNIT`]s — reads are never a point
/// high, a write through the setter drops the fraction ([`spend`]), and
/// leaving the danger leaves it as it is ([`Exposure::wear`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exposure {
    /// In [`UNIT`]s, always less than one: a legacy game's damage past the
    /// last whole point, a v1.2 game's float above its whole number.
    pub carried: u32,
    /// v1.2 only: alive with less than one point (`0 < health < 1`,
    /// `IsDead` is `health <= 0`, `Character.cs:351-354`), whose getter is
    /// nought. The port's number says 1 so that every "dead at nought or
    /// less" stays true to `IsDead`; [`whole`] reads it as v1.2 does.
    pub lifted: bool,
}

/// One v1.2 save token for a carry: the fraction, plus a whole [`UNIT`] when
/// lifted.
pub fn v12_token(e: &Exposure) -> u32 {
    e.carried + if e.lifted { UNIT } else { 0 }
}

/// [`v12_token`] read back.
pub fn from_v12_token(t: u32) -> Exposure {
    if t >= UNIT {
        Exposure { carried: t - UNIT, lifted: true }
    } else {
        Exposure { carried: t, lifted: false }
    }
}

impl Exposure {
    /// Add `steps` world steps of it and return the whole points it costs.
    pub fn step(&mut self, danger: Danger, best_protection: u32, steps: u32) -> i32 {
        self.step_at(health_decrease_milli(danger), danger, best_protection, steps)
    }

    /// [`Exposure::step`] at a given rate (see [`per_step_at`]).
    pub fn step_at(&mut self, milli: u32, danger: Danger, best_protection: u32, steps: u32) -> i32 {
        self.carried += per_step_at(milli, danger, best_protection) * steps;
        let whole = self.carried / UNIT;
        self.carried %= UNIT;
        whole as i32
    }

    /// Nothing is being taken (a legacy game): drop the fraction rather
    /// than bank it for the next place that has a danger. A v1.2 game does
    /// not call it: its float keeps its fraction.
    pub fn clear(&mut self) {
        self.carried = 0;
    }

    /// **`ApplyContinuousHazard`** in a v1.2 game: `health -= rate *
    /// elapsed` on the float (`Character.cs:1013`), held as the getter and
    /// the fraction above it. At nought or less he is dead; the getter
    /// truncates toward nought, so −0.3 reads 0.
    pub fn wear(&mut self, health: &mut i32, milli: u32, danger: Danger, best_protection: u32, steps: u32) {
        let unit = UNIT as i64;
        let h = whole(*health, self) as i64 * unit + self.carried as i64
            - per_step_at(milli, danger, best_protection) as i64 * steps as i64;
        self.lifted = false;
        if h <= 0 {
            *health = (h / unit) as i32;
            self.carried = 0;
        } else {
            self.carried = (h % unit) as u32;
            *health = (h / unit) as i32;
            if *health == 0 {
                *health = 1;
                self.lifted = true;
            }
        }
    }
}
