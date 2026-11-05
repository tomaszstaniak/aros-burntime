//! The opponent's choices: `AiDecision`, `AiAction`, `AiActionResult`, and
//! `AiTurnController.AddTravelCandidate`. At tag `v1.2`,
//! `AI/PlayerBehavior/Core/`.
//!
//! A candidate is an action, a score and what it is about; the turn takes
//! the best. Executing one belongs to the controller.

use alloc::vec::Vec;

use super::camps::Sites;
use super::reason::Reason;
use super::group::Items;
use super::route::{self, Route};
use super::supplies::{self, GroupSupplies};

/// `AiAction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Recruit,
    ReleaseFollower,
    DismissFollower,
    StationFollower,
    StationTradeFollower,
    RecallFollower,
    MobilizeFrontierFollower,
    CancelAttackPlan,
    ImproveCamp,
    ClaimNeutral,
    AttackHostile,
    EmergencyEscape,
    Travel,
    Wait,
}

/// `AiActionResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    ContinuePlanning,
    EndTurn,
    StateUnchanged,
}

/// `AiDecision`.
#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub action: Action,
    pub score: f32,
    pub target: Option<usize>,
    pub next_step: Option<usize>,
    pub reason: Reason,
    /// The person to hire, by their record.
    pub recruit: Option<usize>,
    pub commit_journey: bool,
}

impl Decision {
    pub fn new(action: Action, score: f32, target: Option<usize>, reason: Reason) -> Decision {
        Decision { action, score, target, next_step: None, reason, recruit: None, commit_journey: false }
    }
}

/// What a travel candidate needs of the group and where it stands.
pub struct Traveller<'a> {
    pub sites: &'a Sites,
    pub items: Items<'a>,
    pub player: usize,
    pub previous: Option<usize>,
    pub here: usize,
    pub group: &'a GroupSupplies,
}

/// How much risk a journey may take (`AddTravelCandidate`'s two flags).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Risk {
    pub survivable_recovery: bool,
    pub fatal_recovery: bool,
}

/// `AddTravelCandidate`: a journey toward `target`, scored less a point a
/// day, when the group is supplied for it — or, as allowed, survives it or
/// is no worse off going than staying thirsty.
#[allow(clippy::too_many_arguments)]
pub fn add_travel(
    t: &Traveller,
    candidates: &mut Vec<Decision>,
    target: Option<usize>,
    score: f32,
    reason: Reason,
    risk: Risk,
    known: Option<Route>,
    commit_journey: bool,
) {
    let Some(target) = target else { return };
    let Some(route) = known.or_else(|| route::find(t.sites, t.player, t.previous, t.here, target)) else { return };
    let supplied = supplies::has_territorial_route_supplies(t.group, t.sites, t.items, t.player, t.previous, t.here, &route, false);
    let escape = risk.survivable_recovery && supplies::dehydration_travel_no_worse_than_waiting(t.group, &route, local_water(t, route.days));
    if !supplied && !risk.fatal_recovery && (!risk.survivable_recovery || !supplies::can_survive_recovery_route(t.group, &route)) && !escape {
        return;
    }
    candidates.push(Decision {
        action: Action::Travel,
        score: score - route.days as f32,
        target: Some(target),
        next_step: Some(route.next_step),
        reason,
        recruit: None,
        commit_journey,
    });
}

/// What staying put would add to the group's water over `days`: stored,
/// the spring's reserve, and its daily surplus after the residents.
pub fn local_water(t: &Traveller, days: i32) -> i32 {
    let Some(s) = t.sites.get(t.here) else { return 0 };
    let residents = s.owner.map_or(0, |o| s.living_guards(o));
    s.stored_water(t.items) + s.source_reserve + (s.water - residents).max(0) * days
}
