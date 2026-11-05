//! Targets put aside: `TerritorialTargetDeferrals`. At tag `v1.2`,
//! `AI/PlayerBehavior/Expansion/TerritorialTargetDeferrals.cs`.
//!
//! Upstream keeps these out of the save; here they are saved (see
//! `docs/audit-modern-ai.md`, "Differences"). A blocker is read off the
//! reason's sentence exactly as upstream reads it ([`classify`]), so the
//! caller passes the sentence as written with names ([`Reason::text`]).

use super::reason::{Names, Reason};
use super::state::{Blocker, Deferral, ModernAi};

/// `RetryDelay`.
pub const RETRY_DELAY: i32 = 20;
/// `MaximumEntries`.
pub const MAXIMUM_ENTRIES: usize = 16;

/// `TryClassify`, on the sentence, in upstream's order: "waiting for route
/// supplies"; "waiting for travel reserves" or "projected two-person route";
/// "waiting for" and "attackers"; "safe combat readiness". Anything else
/// defers nothing.
pub fn classify(reason: &str) -> Option<Blocker> {
    if reason.contains("waiting for route supplies") {
        Some(Blocker::RouteSupplies)
    } else if reason.contains("waiting for travel reserves") || reason.contains("projected two-person route") {
        Some(Blocker::SettlementSupplies)
    } else if reason.contains("waiting for") && reason.contains("attackers") {
        Some(Blocker::Attackers)
    } else if reason.contains("safe combat readiness") {
        Some(Blocker::CombatReadiness)
    } else {
        None
    }
}

/// [`classify`] of a reason as written with `names`.
pub fn blocker_of(reason: &Reason, names: &dyn Names) -> Option<Blocker> {
    classify(&reason.text(names))
}

/// What progress against a blocker is now (`Progress`): the lowest of food
/// and water with what is carried; the group's size; ten times the
/// attackers' strength, rounded.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub supplies: i32,
    pub group: i32,
    pub strength: f32,
}

impl Progress {
    fn of(&self, b: Blocker) -> i32 {
        match b {
            Blocker::RouteSupplies | Blocker::SettlementSupplies => self.supplies,
            Blocker::Attackers => self.group,
            Blocker::CombatReadiness => libm_round(self.strength * 10.0),
            Blocker::None => 0,
        }
    }
}

/// `MathF.Round`: to even on a half.
fn libm_round(x: f32) -> i32 {
    let f = x - (x as i32) as f32;
    let i = x as i32;
    let half = 0.5f32;
    if f.abs() > half || f.abs() == half && i % 2 != 0 {
        if x >= 0.0 { i + 1 } else { i - 1 }
    } else {
        i
    }
}

fn defer_until(ai: &mut ModernAi, target: usize, owner: Option<usize>, blocker: Blocker, retry_day: i32, now: &Progress) {
    let entry = Deferral { target, owner, blocker, retry_day, progress: now.of(blocker), sequence: ai.next_deferral };
    ai.next_deferral += 1;
    match ai.deferrals.iter_mut().find(|d| d.target == target) {
        Some(d) => *d = entry,
        None => ai.deferrals.push(entry),
    }
    if ai.deferrals.len() > MAXIMUM_ENTRIES {
        if let Some(i) = ai.deferrals.iter().enumerate().min_by_key(|(_, d)| d.sequence).map(|(i, _)| i) {
            ai.deferrals.remove(i);
        }
    }
}

/// `TryDefer`: put aside for twenty days when the reason's sentence names
/// a blocker. The day to try again.
pub fn try_defer(ai: &mut ModernAi, day: i32, target: usize, owner: Option<usize>, reason: &str, now: &Progress) -> Option<i32> {
    let blocker = classify(reason)?;
    let retry = day + RETRY_DELAY;
    defer_until(ai, target, owner, blocker, retry, now);
    Some(retry)
}

/// `DeferForTurns`.
pub fn defer_for_turns(ai: &mut ModernAi, day: i32, target: usize, owner: Option<usize>, turns: i32, now: &Progress) {
    defer_until(ai, target, owner, Blocker::None, day + turns, now);
}

/// `IsDeferred`: still put aside unless its owner changed, its day came, or
/// lasting progress was made (supplies come and go, so they do not count);
/// a target no longer put aside is forgotten.
pub fn is_deferred(ai: &mut ModernAi, day: i32, target: usize, owner_now: Option<usize>, now: &Progress) -> bool {
    let Some(i) = ai.deferrals.iter().position(|d| d.target == target) else { return false };
    let d = &ai.deferrals[i];
    let changed = owner_now != d.owner;
    let reached = day >= d.retry_day;
    let progressed = !matches!(d.blocker, Blocker::RouteSupplies | Blocker::SettlementSupplies) && now.of(d.blocker) > d.progress;
    if !changed && !reached && !progressed {
        return true;
    }
    ai.deferrals.remove(i);
    false
}
