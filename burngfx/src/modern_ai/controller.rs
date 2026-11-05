//! The Modern opponent's turn: `ClassicAiState.Turn` and
//! `AiTurnController.RunTurn` — the reserve's caps, the dog, local items,
//! the last chance, the turn's preparation, and at most ten strategic
//! actions, each chosen on the world as the last one left it. At tag
//! `v1.2`, `AI/PlayerBehavior/Core/`.
//!
//! **One generator a turn**, drawn from `(seed, day, seat)`; **one call a
//! turn**: the turn begins and ends inside [`turn`], so a save never falls
//! inside it. The turn's needs and camp assessments are cached on the seat
//! for the turn, refreshed where upstream refreshes them.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::spawn::Rng;

use super::attack::{self, Field};
use super::camps::CampCache;
use super::decision::{add_travel, Action, Decision, Outcome, Risk, Traveller};
use super::expansion::Theatre;
use super::group::{self, Items};
use super::hiring::{self, Planner};
use super::policy::AiPolicy;
use super::reason::{Names, Reason};
use super::recovery::{self, Trip};
use super::route::{self, RouteMap};
use super::seat::{Seat, Snap, Who};
use super::upkeep;

/// `maximumStrategicActions`.
pub const MAXIMUM_STRATEGIC_ACTIONS: usize = 10;

/// What a turn did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Each chosen decision and its outcome, in order.
    pub actions: Vec<(Decision, Outcome)>,
    /// The last chance acted and ended the turn.
    pub last_chance: Option<super::resolve::Result>,
    /// The ten actions ran out with planning still going on.
    pub cut_short: bool,
}

/// What the controller is told about the game for this turn.
pub struct Game<'n> {
    pub seed: u64,
    pub extended: bool,
    pub names: &'n dyn Names,
    /// Milliseconds from some fixed point, for upstream's slow-turn reports
    /// (`Stopwatch`); none, and those two reports are not made.
    pub clock: Option<fn() -> u64>,
}

/// `AiTelemetry.Report(state.Player, …)`.
fn tell(seat: &Seat, message: impl FnOnce() -> alloc::string::String) {
    seat.ai.telemetry.report(message);
}

fn title(l: Option<usize>) -> alloc::string::String {
    l.map_or_else(|| alloc::string::String::from("none"), super::telemetry::place)
}

fn report_line(seat: &Seat, line: alloc::string::String) {
    seat.ai.telemetry.report(|| line);
}

fn now(game: &Game) -> Option<u64> {
    game.clock.map(|c| c())
}

fn home(seat: &Seat) -> bool {
    seat.camps.get(seat.here()).and_then(|c| c.owner) == Some(seat.me())
}

fn in_city(seat: &Seat) -> bool {
    seat.gam.locations.get(seat.here()).is_some_and(|l| l.is_city)
}

fn travelling_or_dead(seat: &Seat) -> bool {
    seat.rival.journey.is_some() || seat.rival.dead || seat.rival.needs.health <= 0
}

fn doctor_at(seat: &Seat, l: usize) -> bool {
    seat.room_ids.get(l).is_some_and(|ids| ids.iter().any(|&id| crate::matmap::RoomType::from_room_id(id) == crate::matmap::RoomType::Doctor))
}

/// `GetProgressBenchmark` and `ProgressRelativeLimitOrUnrestricted`: the
/// strongest living human's camps (or, with none, another machine's) plus
/// `camp_max_advance`; no limit for a sole survivor.
fn neutral_expansion_allowed(seat: &Seat, snap: &Snap) -> bool {
    let camps_of = |p: usize| snap.v.sites.sites.iter().filter(|s| s.owner == Some(p)).count() as i32;
    let mut candidates: Vec<usize> = seat.humans.iter().map(|h| h.0).collect();
    if candidates.is_empty() {
        candidates = seat.others.iter().filter(|r| !r.dead && r.modern.is_some() || !r.dead && r.amiga.is_some()).map(|r| r.player).collect();
    }
    if candidates.is_empty() {
        return true;
    }
    let strongest = candidates.iter().map(|&p| camps_of(p)).max().unwrap_or(0);
    (camps_of(seat.me()) as i64) < strongest as i64 + seat.ai.max_advance as i64
}

/// `DecisionContext.CriticalSupplies`: `NeedsRecovery`, or a city with no
/// supplied way to an own provisioning camp.
fn critical_supplies(seat: &Seat, snap: &Snap, policy: &AiPolicy) -> bool {
    let t = seat.tables();
    let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
    recovery::needs_recovery(seat.rival, seat.types, policy.recovery_water_days) || recovery::needs_city_recovery_staging(&w, seat.rival, seat.rival.previous)
}

/// The turn: the reserve's caps, a dog for meat, local items, then
/// [`run_turn`]. `rng` is the game's one generator (`Platform.Math.Random`),
/// which every draw of the turn takes its number from, in upstream's order.
pub fn turn(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng) -> Report {
    let discarded = upkeep::enforce_reserve_caps(seat.ai);
    if discarded > 0 {
        tell(seat, || alloc::format!("discarded {discarded} excess shared reserve items above the per-type cap"));
    }
    if seat.can_collect_local_loot() {
        upkeep::hunt_one_dog_for_meat(seat, policy, game.extended);
    }
    upkeep::manage_local_items(seat, policy, game.extended);
    run_turn(seat, policy, game, rng)
}

/// `AiTurnController.RunTurn`.
pub fn run_turn(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng) -> Report {
    let started = now(game);
    let mut report = Report::default();
    let extended = game.extended;
    // `AiTurnContext.Begin`: the needs, now; the camps, not yet captured.
    seat.camp_cache = None;
    seat.trade_city_cache = None;
    let needs = seat.fresh_needs(policy, extended);
    seat.needs_cache = Some(Rc::new(RefCell::new(Some(needs))));
    seat.ai.forget_expired(seat.day);
    {
        let snap = seat.snap(policy, extended);
        let t = seat.tables();
        let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
        let f = field(seat, &w, &snap);
        let mut ai = seat.ai.clone();
        attack::update_knowledge(&mut ai, &f);
        drop(f);
        *seat.ai = ai;
    }
    if let Some(r) = super::resolve::last_chance(seat, policy, extended, rng) {
        report.last_chance = Some(r);
        finish(seat);
        return report;
    }
    prepare_turn(seat, policy, game, rng);
    capture_camps(seat, policy, extended);
    let local = now(game);
    let mut timings: Vec<alloc::string::String> = Vec::new();
    for action in 0..MAXIMUM_STRATEGIC_ACTIONS {
        let begun = now(game);
        let decision = choose(seat, policy, game, rng);
        let chosen = now(game);
        let outcome = execute(seat, policy, game, &decision, rng);
        if let (Some(b), Some(c), Some(e)) = (begun, chosen, now(game)) {
            timings.push(alloc::format!("{:?} choose {} ms, execute {} ms", decision.action, c - b, e - c));
        }
        report.actions.push((decision, outcome));
        if outcome != Outcome::ContinuePlanning || travelling_or_dead(seat) {
            break;
        }
        if action == MAXIMUM_STRATEGIC_ACTIONS - 1 {
            report.cut_short = true;
            report_line(seat, alloc::format!("stopped local strategy after {MAXIMUM_STRATEGIC_ACTIONS} actions to avoid an infinite loop"));
        }
    }
    if let (Some(s0), Some(l), Some(e)) = (started, local, now(game)) {
        if e - s0 >= 1000 {
            tell(seat, || alloc::format!("slow AI turn {} ms: local opportunities {} ms; {}", e - s0, l - s0, timings.join("; ")));
        }
    }
    finish(seat);
    report
}

/// The turn is over: its caches go.
fn finish(seat: &mut Seat) {
    seat.camp_cache = None;
    seat.needs_cache = None;
    seat.trade_city_cache = None;
}

/// `AiTurnContext.BestTradeCity`: the turn's first answer, kept. The
/// caller stores a fresh answer in `trade_city_cache`.
pub fn cached_trade_city(seat: &Seat, cx: &super::trading::Context) -> Option<usize> {
    match seat.trade_city_cache {
        Some(city) => city,
        None => super::trade_plan::best_trade_city(cx, seat.ai, seat.rival.previous),
    }
}

/// `AiTurnContext.RefreshNeeds`.
fn refresh_needs(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let needs = seat.fresh_needs(policy, extended);
    if let Some(c) = seat.needs_cache.as_ref() {
        *c.borrow_mut() = Some(needs);
    }
}

/// `CampAssessments.Refresh`: the own camps and their threat, taken now.
fn capture_camps(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    seat.camp_cache = None;
    let snap = seat.snap(policy, extended);
    let me = seat.me();
    let camps: Vec<(usize, bool)> = (0..snap.v.sites.sites.len()).filter(|&l| snap.v.sites.sites[l].owner == Some(me) && !snap.v.sites.sites[l].city).map(|l| (l, snap.v.sites.sites[l].threatened)).collect();
    seat.camp_cache = Some(Rc::new(RefCell::new(CampCache { camps, ..CampCache::default() })));
}

fn field<'a>(seat: &'a Seat, w: &'a super::needs::World<'a>, snap: &'a Snap) -> Field<'a> {
    Field {
        w,
        rules: seat.rules(),
        party: &snap.v.party,
        group: &snap.v.group,
        previous: seat.rival.previous,
        humans: &snap.v.humans,
        defenders_here: &snap.v.defenders_here,
        travelling: seat.rival.journey.is_some(),
    }
}

// --- preparation (`PrepareTurn`) ------------------------------------------

/// `PrepareTurn`.
fn prepare_turn(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng) {
    let extended = game.extended;
    let me = seat.me();
    // `MaintainCampProductionPolicy`.
    let camps: Vec<usize> = (0..seat.gam.locations.len()).filter(|&l| seat.camps.get(l).and_then(|c| c.owner) == Some(me) && !seat.gam.locations[l].is_city).collect();
    for &l in &camps {
        seat.auto_select_production(l);
    }
    immediate_recovery(seat, policy, extended);
    {
        let snap = seat.snap(policy, extended);
        let items = seat.items();
        super::support::grant_slump_support(items, &snap.v.sites, seat.rival, seat.ai, me, seat.day, policy);
    }
    upkeep::refill_construction_reserve(seat);
    upkeep::construct_portable_upgrade(seat, policy, extended);
    upkeep::maintain_group_equipment(seat, policy, extended, true);
    upkeep::maintain_camp_network(seat, policy, extended);
    upkeep::maintain_equipment(seat, policy, extended);
    try_claim_current_as_local_opportunity(seat, policy, extended, rng);
    if home(seat) {
        upkeep::maintain_current_camp(seat, policy, extended);
        let settlement = seat.ai.target_was_neutral && seat.ai.target.is_some();
        let attack_plan = seat.ai.target.is_some_and(|t| seat.camps.get(t).and_then(|c| c.owner).is_some_and(|o| o != me) && !seat.gam.locations[t].is_city);
        let blocked = !in_city(seat) && (settlement || attack_plan);
        if !blocked {
            let snap = seat.snap(policy, extended);
            let city = upkeep::with_context(seat, &snap, policy, |cx| cached_trade_city(seat, cx));
            seat.trade_city_cache = Some(city);
            let visit = upkeep::with_context(seat, &snap, policy, |cx| super::trade_plan::should_visit_trader(cx, seat.ai, seat.rival.previous, city));
            if visit {
                let here = seat.here();
                upkeep::fill_city_caravan(seat, policy, extended, here);
            }
        }
    }
    let traded = trade_locally(seat, policy, extended);
    use_doctor(seat, policy, extended);
    if traded {
        upkeep::maintain_group_equipment(seat, policy, extended, false);
        upkeep::maintain_equipment(seat, policy, extended);
        upkeep::maintain_leader_roles(seat, policy, extended);
    }
}

/// `ProvideCityWaterMinimum` / `ProvideCityFoodMinimum`'s lines.
fn report_city_minimum(seat: &Seat, items: group::Items, (food, water, refilled): (i32, i32, Vec<usize>)) {
    if food > 0 || water > 0 {
        tell(seat, || alloc::format!("received city minimum of {food} food and {water} water"));
    }
    if !refilled.is_empty() {
        let ids: Vec<alloc::string::String> = refilled.iter().map(|&k| alloc::string::String::from(items.types.name_of(k).unwrap_or(""))).collect();
        tell(seat, || alloc::format!("used city water service to refill {}", ids.join(", ")));
    }
}

/// `ExecuteImmediateRecovery`.
fn immediate_recovery(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let items = seat.items();
    let owned = (0..seat.gam.locations.len()).filter(|&l| seat.camps.get(l).and_then(|c| c.owner) == Some(seat.me())).count();
    let city = in_city(seat);
    let given = group::city_minimum(seat.rival, items, city, owned, false, true, true);
    report_city_minimum(seat, items, given);
    // `UseLocalWaterSource`.
    let here = seat.here();
    let rival = &mut *seat.rival;
    if let Some(spring) = seat.springs.at(here) {
        group::use_local_water_source(rival, items, spring);
    }
    // `RemoveAdviceItems`: the group's packs, and the camps' rooms and guards.
    for m in 0..group::size(seat.rival) {
        group::remove_advice(group::pack_mut(seat.rival, m), seat.types);
    }
    let me = seat.me();
    let camps: Vec<usize> = (0..seat.gam.locations.len()).filter(|&l| seat.camps.get(l).and_then(|c| c.owner) == Some(me)).collect();
    for b in seat.rooms.bags_in(&camps) {
        group::remove_advice(b, seat.types);
    }
    for (_, r) in seat.camps.residents_in(&camps) {
        group::remove_advice(&mut r.pack, seat.types);
    }
    if home(seat) {
        upkeep::provision_from_camp_surplus(seat, policy, extended, here);
    }
    let reserved = seat.snap(policy, extended).needs.planned_settlement_payment;
    let loot_rooms = seat.can_collect_local_loot() && home(seat);
    if loot_rooms {
        let mut bags: Vec<crate::game::ItemBag> = (0..seat.room_count(here)).map(|r| seat.rooms.get(here, r).cloned().unwrap_or_else(|| crate::game::ItemBag::new(seat.room_capacity(here, r)))).collect();
        if group::consume_available_supplies(seat.rival, items, reserved, Some(&mut bags)) {
            tell(seat, || "consumed carried or stored supplies before seeking paid recovery".into());
        }
        for (r, b) in bags.into_iter().enumerate() {
            if seat.rooms.get(here, r).is_some() || !b.is_empty() {
                seat.rooms.set(here, r, b);
            }
        }
    } else if group::consume_available_supplies(seat.rival, items, reserved, None) {
        tell(seat, || "consumed carried or stored supplies before seeking paid recovery".into());
    }
    let given = group::city_minimum(seat.rival, items, city, owned, true, false, false);
    report_city_minimum(seat, items, given);
    upkeep::maintain_leader_roles(seat, policy, extended);
    use_doctor(seat, policy, extended);
}

/// `RecoveryServices.UseDoctor` here.
fn use_doctor(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let here = seat.here();
    let has = doctor_at(seat, here);
    if !has {
        return;
    }
    let snap = seat.snap(policy, extended);
    let (sellable, priority): (Vec<usize>, Vec<(usize, i32)>) = upkeep::with_context(seat, &snap, policy, |cx| {
        let items = cx.w.items;
        let kinds: Vec<usize> = (0..items.types.len()).collect();
        let sellable = kinds.iter().copied().filter(|&k| super::trading::can_sell_kind(cx, k, None)).collect();
        let priority = kinds.iter().map(|&k| (k, super::trading::sale_priority(items, k))).collect();
        (sellable, priority)
    });
    let doctor = group::Doctor { healing_factor: seat.book.rules.healing_factor, health_cap: seat.book.rules.health_cap };
    let items = seat.items();
    let done = group::use_doctor(seat.rival, items, true, doctor, |k| sellable.contains(&k), |k| priority.iter().find(|p| p.0 == k).map_or(3, |p| p.1));
    for t in done {
        let paid: Vec<alloc::string::String> = t.paid.iter().map(|&(_, k)| alloc::string::String::from(items.types.name_of(k).unwrap_or(""))).collect();
        // `(int)payment.Sum(TradeValue)`: the sum, truncated.
        let value = t.paid.iter().map(|&(_, k)| items.value(k)).sum::<f32>() as i32;
        let factor = super::reason::format_fixed(doctor.healing_factor, 2);
        let patient = if t.patient == 0 { seat.who_name(Who::Leader) } else { seat.who_name(Who::Follower(t.patient - 1)) };
        tell(seat, || alloc::format!("paid doctor with {} (trade value {value}, healing factor x{factor}) for {} health for {patient}", paid.join(", "), t.healed));
    }
}

/// `TradeLocally`: unless the settler's payment must be kept (and no doctor
/// is to be paid), every trader here, the most promising first, with the
/// construction reserve and portable building after each. Whether a trade
/// was made.
fn trade_locally(seat: &mut Seat, policy: &AiPolicy, extended: bool) -> bool {
    let snap = seat.snap(policy, extended);
    let reserve_settler_pay = should_reserve_settler_payment(seat, &snap, policy);
    let needs_doctor = group::needs_doctor_payment(seat.rival, doctor_at(seat, seat.here()));
    if reserve_settler_pay && !needs_doctor {
        return false;
    }
    let traders: Vec<usize> = upkeep::with_context(seat, &snap, policy, |cx| super::trade_plan::encountered_traders(cx).iter().map(|t| t.index).collect());
    let mut done = false;
    for t in traders {
        if upkeep::trade_with_trader(seat, policy, extended, t) {
            done = true;
            refresh_needs(seat, policy, extended);
        }
        upkeep::refill_construction_reserve(seat);
        upkeep::construct_portable_upgrade(seat, policy, extended);
    }
    done
}

/// `ShouldReserveSettlerPayment`: in a city, alone, with a settlement
/// plan and somebody here who could be paid for.
fn should_reserve_settler_payment(seat: &Seat, snap: &Snap, policy: &AiPolicy) -> bool {
    if !in_city(seat) || !(seat.ai.target_was_neutral && seat.ai.target.is_some()) || group::size(seat.rival) != 1 {
        return false;
    }
    let purse = super::recruitment::Purse { items: seat.items(), r: seat.rival, leader_kit: &seat.ai.leader_kit, camp_rooms: &snap.v.camp_rooms, owned_camps: snap.v.sites.sites.iter().filter(|s| s.owner == Some(seat.me())).count() };
    let people = snap.v.people.get(seat.here()).map_or(&[][..], |p| p.as_slice());
    super::recruitment::hireable(people, policy, &seat.ai.demobilized).iter().any(|c| super::recruitment::recruitment_plan(&purse, c, true).funded)
}

/// `ExpansionPlanning.TryClaimCurrentAsLocalOpportunity`.
fn try_claim_current_as_local_opportunity(seat: &mut Seat, policy: &AiPolicy, extended: bool, rng: &mut Rng) -> bool {
    let snap = seat.snap(policy, extended);
    let here = seat.here();
    let me = seat.me();
    let ok = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let kit = seat.ai.leader_kit;
        let th = theatre(seat, &f, &snap, &kit);
        let mut ai = seat.ai.clone();
        let claimable = super::claims::can_claim(seat.rival, &ai, &w, &snap.v.group, here)
            && th.can_station_camp(&ai)
            && super::claims::has_travelling_hazard_protection(seat.rival, w.items, &snap.v.sites.sites[here])
            && th.suitable_current_claim(&mut ai);
        let knowledge = ai.deferrals.clone();
        let next = ai.next_deferral;
        (claimable, knowledge, next)
    };
    seat.ai.deferrals = ok.1;
    seat.ai.next_deferral = ok.2;
    if !ok.0 {
        return false;
    }
    let selected = seat.ai.target == Some(here);
    let first = !(0..seat.gam.locations.len()).any(|l| seat.camps.get(l).and_then(|c| c.owner) == Some(me));
    let secures = snap.v.sites.connects_owned_camps(here, me);
    let settlement = seat.ai.target_was_neutral && seat.ai.target.is_some();
    let onward = settlement && seat.ai.target.is_some_and(|t| t != here);
    if !selected && !first && !secures && !onward {
        return false;
    }
    let Some(settler) = upkeep::select_camp_npc(seat, policy, extended, rng) else { return false };
    let onward_target = if settlement && seat.ai.target != Some(here) { seat.ai.target } else { None };
    let name = seat.companion_name(settler);
    upkeep::create_camp(seat, policy, extended, settler);
    if let Some(t) = onward_target {
        attack::set_settlement_target(seat.ai, Some(t), &mut || true);
    }
    upkeep::provision_from_camp_surplus(seat, policy, extended, here);
    tell(seat, || match onward_target {
        None => alloc::format!("claimed {} as a local opportunity using {name}", super::telemetry::place(here)),
        Some(t) => alloc::format!("claimed waypoint {} using {name} while continuing toward {}", super::telemetry::place(here), super::telemetry::place(t)),
    });
    true
}

fn theatre<'a>(seat: &'a Seat, f: &'a Field<'a>, snap: &'a Snap, kit: &'a crate::combat::Kit) -> Theatre<'a> {
    Theatre {
        f,
        r: seat.rival,
        kit,
        people: &snap.v.people,
        camp_rooms: &snap.v.camp_rooms,
        rivals_at: &snap.v.rivals_at,
        dead_players: &snap.v.dead_players,
        travelling_to: seat.rival.journey.map(|j| j.to),
        neutral_expansion_allowed: neutral_expansion_allowed(seat, snap),
        critical_supplies: critical_supplies(seat, snap, f.w.policy),
    }
}

// --- choosing (`Choose`) ---------------------------------------------------

/// `Choose`: the emergency, the committed journey, or the normal plan.
fn choose(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng) -> Decision {
    let started = now(game);
    let extended = game.extended;
    // `CancelSettlementAtHostileWaypoint`.
    if seat.ai.target_was_neutral && seat.ai.target.is_some() {
        let here = seat.here();
        let hostile = seat.camps.get(here).and_then(|c| c.owner).is_some_and(|o| o != seat.me()) && !in_city(seat);
        if hostile {
            if let Some(t) = seat.ai.target {
                super::expansion::report_cancelled_settlement(seat.ai, t, here);
            }
            attack::clear_target(seat.ai);
        }
    }
    if let Some(d) = plan_emergency(seat, policy, game, rng) {
        return d;
    }
    if let Some(d) = continue_committed_journey(seat, policy, extended, game.names) {
        return d;
    }
    plan_normal(seat, policy, game, rng, started)
}

/// `ContinueCommittedJourney`.
fn continue_committed_journey(seat: &mut Seat, policy: &AiPolicy, extended: bool, names: &dyn Names) -> Option<Decision> {
    let j = seat.ai.journey.clone()?;
    let here = seat.here();
    if here == j.destination {
        tell(seat, || alloc::format!("completed committed journey to {}: {}", super::telemetry::place(j.destination), j.reason));
        seat.ai.journey = None;
        return None;
    }
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let s = &snap.v.sites.sites[j.destination];
    let size = group::size(seat.rival) as i32;
    if s.city || s.owner != Some(seat.me()) || !s.can_provision_food(items) || !s.can_provision_group_water(items, size) {
        tell(seat, || alloc::format!("cancelled committed journey to {}: destination is no longer a sustainable owned camp", super::telemetry::place(j.destination)));
        seat.ai.journey = None;
        return None;
    }
    let r = route::find(&snap.v.sites, seat.me(), seat.rival.previous, here, j.destination);
    let ok = r.as_ref().is_some_and(|r| seat.can_travel(here, r.next_step) && super::supplies::has_route_supplies(&snap.v.group, r, false, 0, 0));
    let Some(r) = r.filter(|_| ok) else {
        tell(seat, || alloc::format!("cancelled committed journey to {}: remaining route is unavailable or no longer supplied", super::telemetry::place(j.destination)));
        seat.ai.journey = None;
        return None;
    };
    let mut d = Decision::new(Action::Travel, 1500.0 - r.days as f32, Some(j.destination), Reason::Note(alloc::format!("continue committed journey to {}: {}", names.place(j.destination), j.reason)));
    d.next_step = Some(r.next_step);
    Some(d)
}

/// `CanFinishCommittedSettlementJourney`.
fn can_finish_settlement_journey(seat: &Seat, snap: &Snap) -> bool {
    let Some(t) = seat.ai.target.filter(|_| seat.ai.target_was_neutral) else { return false };
    if snap.v.sites.owner(t).is_some() || (0..group::size(seat.rival)).any(|m| group::needs(seat.rival, m).health <= 40) {
        return false;
    }
    route::find(&snap.v.sites, seat.me(), seat.rival.previous, seat.here(), t).is_some_and(|r| super::supplies::has_route_supplies(&snap.v.group, &r, false, 0, 0))
}

/// `PlanEmergency`.
fn plan_emergency(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng) -> Option<Decision> {
    let extended = game.extended;
    let snap = seat.snap(policy, extended);
    let mut candidates: Vec<Decision> = Vec::new();
    {
        let t = seat.tables();
        let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
        let f = field(seat, &w, &snap);
        let mut ai = seat.ai.clone();
        let responded = attack::try_add_immediate_response(&mut ai, &f, policy, &mut candidates);
        drop(f);
        *seat.ai = ai;
        if responded {
            return Some(select_and_report(seat, policy, game, &snap, candidates, rng));
        }
    }
    if !critical_supplies(seat, &snap, policy) || can_finish_settlement_journey(seat, &snap) {
        return None;
    }
    let t = seat.tables();
    let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
    let kit = seat.ai.leader_kit;
    {
        let planner = Planner::new(&w, &snap.needs, seat.rival, &kit, seat.ai, snap.barter, seat.rival.previous, &snap.v.people, &snap.v.camp_rooms, &snap.v.rivals_at);
        if planner.critical_route_recruit_continuation(&mut candidates) {
            drop(planner);
            return Some(select_and_report(seat, policy, game, &snap, candidates, rng));
        }
    }
    let doc = |l: usize| doctor_at(seat, l);
    let reachable = recovery::find_destination(&w, seat.rival, seat.rival.previous, true, Trip::Normal, &doc);
    let last = reachable.is_none() && recovery::waiting_will_be_fatal(&w, seat.rival);
    let destination = reachable.or_else(|| {
        if last {
            recovery::last_chance_destination(&w, seat.rival, seat.rival.previous)
        } else {
            recovery::find_destination(&w, seat.rival, seat.rival.previous, false, Trip::Normal, &doc)
        }
    });
    let traveller = Traveller { sites: &snap.v.sites, items: w.items, player: seat.me(), previous: seat.rival.previous, here: seat.here(), group: &snap.v.group };
    let reason = if last { Reason::Note("take the least-bad route toward emergency supplies instead of waiting to die".into()) } else { Reason::Note("seek real food, water, or medical services".into()) };
    add_travel(&traveller, &mut candidates, destination, 1100.0, reason, Risk { survivable_recovery: true, fatal_recovery: last }, None, false);
    if reachable.is_none() && group::size(seat.rival) > 1 && !recovery::can_recover_locally_for_travel(&w, seat.rival) {
        candidates.push(Decision::new(Action::ReleaseFollower, 1110.0, Some(seat.here()), Reason::Note("release followers when the enlarged group has no survivable recovery option".into())));
    }
    let (score, reason) = if destination.is_none() {
        (1090.0, "no affordable local recovery and no useful destination")
    } else {
        (900.0, "cannot yet reach a provisioned recovery destination safely")
    };
    candidates.push(Decision::new(Action::Wait, score, Some(seat.here()), Reason::Note(reason.into())));
    Some(select_and_report(seat, policy, game, &snap, candidates, rng))
}

/// `PlanNormalActions`.
fn plan_normal(seat: &mut Seat, policy: &AiPolicy, game: &Game, rng: &mut Rng, started: Option<u64>) -> Decision {
    let extended = game.extended;
    let snap = seat.snap(policy, extended);
    let t_context = now(game);
    let mut candidates: Vec<Decision> = Vec::new();
    let me = seat.me();
    let here = seat.here();
    // The territorial plan, on a copy of the opponent's state the plan may change.
    let kit = seat.ai.leader_kit;
    let recruit_here = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let planner = Planner::new(&w, &snap.needs, seat.rival, &kit, seat.ai, snap.barter, seat.rival.previous, &snap.v.people, &snap.v.camp_rooms, &snap.v.rivals_at);
        in_city(seat) && !seat.ai.survival_releases.contains(&here) && planner.can_recruit(policy.allow_generated_recruit_payment_in_cities) && planner.safe_local_recruit().is_some()
    };
    let mut ai = seat.ai.clone();
    let mut prepare_reststop = || true;
    let plan = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let th = theatre(seat, &f, &snap, &kit);
        let plan = th.create_plan(&mut ai, policy, recruit_here, rng, &mut prepare_reststop);
        th.add_immediate_claim(&plan, &mut candidates);
        plan
    };
    let t_territory = now(game);
    *seat.ai = ai;
    // A Reststop settlement needs its bible brought along (`PrepareReststopSettlement`).
    if plan.target == Some(super::camps::RESTSTOP) && seat.ai.target_was_neutral && !prepare_reststop_settlement(seat) {
        attack::clear_target(seat.ai);
    }
    let snap = seat.snap(policy, extended);
    let target = seat.ai.target;
    let preparing_attack = plan.preparing_attack && target.is_some();
    let items: Items = seat.items();
    let carried: i32 = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum();
    let needs_attack_water = preparing_attack && carried < super::needs::desired_water_container_capacity(group::size(seat.rival) as i32);
    let settlement = seat.ai.target_was_neutral && target.is_some();
    let blocked = !in_city(seat) && (settlement || preparing_attack);
    let best_city = if blocked && !needs_attack_water {
        None
    } else {
        let city = upkeep::with_context(seat, &snap, policy, |cx| cached_trade_city(seat, cx));
        seat.trade_city_cache = Some(city);
        city
    };
    let visit = needs_attack_water || !blocked && upkeep::with_context(seat, &snap, policy, |cx| super::trade_plan::should_visit_trader(cx, seat.ai, seat.rival.previous, best_city));
    let t_trade_plan = now(game);
    // Recruitment and reinforcement.
    let attack_size = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let mut ai = seat.ai.clone();
        let size = hiring::attack_size(&mut ai, &f, target, preparing_attack, policy);
        size
    };
    let needs = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let planner = Planner::new(&w, &snap.needs, seat.rival, &kit, seat.ai, snap.barter, seat.rival.previous, &snap.v.people, &snap.v.camp_rooms, &snap.v.rivals_at);
        let needs = planner.add_candidates(target, attack_size, visit, &mut candidates);
        let city = planner.attack_recruitment_city.get();
        (needs, city)
    };
    seat.ai.attack_recruitment_city = needs.1;
    let t_recruitment = now(game);
    {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let traveller = Traveller { sites: &snap.v.sites, items: w.items, player: me, previous: seat.rival.previous, here, group: &snap.v.group };
        super::reinforcement::add_candidates(&traveller, policy, group::size(seat.rival), attack_size.filter(|_| preparing_attack), needs.0, &mut candidates);
    }
    let t_reinforcement = now(game);
    // `NeedsCampImprovement`.
    if home(seat) && !upkeep::should_reserve_production_tool(seat, &snap) {
        let site = &snap.v.sites.sites[here];
        let products = super::claims::available_products(items, site);
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let has_trap = super::claims::reserve_has_trap(seat.ai, &w, &products);
        let produces = |k: usize| w.produce_of(k).is_some_and(|p| products.contains(&p));
        let installed = site.stored.iter().chain(site.guard_items.iter()).any(|&k| produces(k));
        if has_trap && !installed {
            let gain = super::reason::dotnet_max(0.0, snap.v.sites.marginal_camp(items, here, me));
            candidates.push(Decision::new(Action::ImproveCamp, 650.0 + gain * 180.0, Some(here), Reason::Note(alloc::format!("improve sustainable camp income by about {} value/day", super::reason::format_tenths(gain)))));
        }
    }
    // Trading.
    {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let th = theatre(seat, &f, &snap, &kit);
        let t0 = now(game);
        let needs_expansion_tool = th.needs_expansion_tool(seat.ai);
        let t_equipment = now(game);
        let improvement_return = super::production::best_empire_production_improvement(items, &snap.v.sites, me);
        let t_improvement = now(game);
        let inputs = super::trade_plan::TradeInputs {
            target,
            preparing_attack,
            visit_trader: visit,
            best_trade_city: best_city,
            needs_expansion_tool,
            improvement_return,
            snake_campaign: seat.ai.support.saving_for_snake_trap,
        };
        // Computed before the trade candidates here, after them upstream;
        // each phase is timed where it runs.
        let d0 = now(game);
        let delivery = delivery_candidate(seat, &snap, policy);
        let d1 = now(game);
        let cx = super::trading::Context { w: &w, needs: &snap.needs, r: seat.rival, leader_kit: &kit, ai: seat.ai, barter: snap.barter };
        let traveller = Traveller { sites: &snap.v.sites, items: w.items, player: me, previous: seat.rival.previous, here, group: &snap.v.group };
        let inner = super::trade_plan::add_candidates(&cx, seat.ai, &traveller, &inputs, delivery, &mut candidates);
        if let (Some(t0), Some(te), Some(ti), Some(d0), Some(d1), Some([tc, tt, tq])) = (t0, t_equipment, t_improvement, d0, d1, inner) {
            let (equipment, improvement, campaign, cont, trade, deliv) = (te - t0, ti - te, tc.saturating_sub(d1), tt - tc, tq - tt, d1 - d0);
            let total = equipment + improvement + campaign + cont + trade + deliv;
            if total >= 50 {
                tell(seat, || alloc::format!(
                    "Trading.AddCandidates took {total} ms: expansion equipment {equipment} ms, empire improvement {improvement} ms, campaign {campaign} ms, continue trading {cont} ms, trade candidate {trade} ms, delivery {deliv} ms"
                ));
            }
        }
    }
    let t_trade = now(game);
    // Expansion's own candidates.
    {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let mut ai = seat.ai.clone();
        let th = theatre(seat, &f, &snap, &kit);
        th.add_candidates(&mut ai, policy, &plan, &mut candidates);
        drop(th);
        *seat.ai = ai;
    }
    let t_expansion = now(game);
    // Look for recruits.
    if !preparing_attack && group::size(seat.rival) > 1 && (group::size(seat.rival) as i32) < policy.group_size && !hireable_here(seat, &snap) {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let planner = Planner::new(&w, &snap.needs, seat.rival, &kit, seat.ai, snap.barter, seat.rival.previous, &snap.v.people, &snap.v.camp_rooms, &snap.v.rivals_at);
        let prep = upkeep::with_context(seat, &snap, policy, |cx| super::trading::best_camp_for_city_preparation(cx, seat.rival.previous));
        let city = planner.nearest_recruitment_city(None);
        let traveller = Traveller { sites: &snap.v.sites, items: w.items, player: me, previous: seat.rival.previous, here, group: &snap.v.group };
        let reason = if prep.is_none() { "look for recruits" } else { "collect trade cargo before looking for recruits" };
        add_travel(&traveller, &mut candidates, prep.or(city), 560.0, Reason::Note(reason.into()), Risk::default(), None, false);
    }
    // The idle reason.
    let idle = {
        let t = seat.tables();
        let w = snap.world(&t, policy, me, seat.day, here);
        let f = field(seat, &w, &snap);
        let mut ai = seat.ai.clone();
        match target.filter(|_| preparing_attack) {
            Some(tgt) => attack::advance_blocking_reason(&mut ai, &f, tgt, policy),
            None => {
                let th = theatre(seat, &f, &snap, &kit);
                if th.needs_expansion_tool(seat.ai) {
                    Reason::Note("expansion blocked: no portable production tool and no affordable or collectible route".into())
                } else if !attack::has_group_weapon(&f) {
                    Reason::Note("expansion blocked: group has no weapon and no equipment route is available".into())
                } else {
                    Reason::Note("no eligible neutral or hostile frontier target with current supplies".into())
                }
            }
        }
    };
    candidates.push(Decision::new(Action::Wait, 0.0, if preparing_attack { target } else { None }, idle));
    if let (Some(s0), Some(c), Some(te), Some(tp), Some(rc), Some(rf), Some(tr), Some(ex), Some(e)) =
        (started, t_context, t_territory, t_trade_plan, t_recruitment, t_reinforcement, t_trade, t_expansion, now(game))
    {
        if e - s0 >= 500 {
            tell(seat, || alloc::format!(
                "slow decision planning {} ms: context {} ms, territory {} ms, recruitment {} ms, trade plan {} ms, reinforcement {} ms, trade {} ms, expansion {} ms",
                e - s0, c - s0, te - c, rc - tp, tp - te, rf - rc, tr - rf, ex - tr
            ));
        }
    }
    select_and_report(seat, policy, game, &snap, candidates, rng)
}

/// `HasHireableNpc` (`CanHireNpc`): somebody here who could be hired.
fn hireable_here(seat: &Seat, snap: &Snap) -> bool {
    snap.v.people.get(seat.here()).is_some_and(|p| p.iter().any(|c| c.available))
}

/// `PrepareReststopSettlement` on the world: a bible carried, or one from a
/// camp's rooms, then from a camp's guards (not Reststop's), onto a
/// carrier.
pub fn prepare_reststop_settlement(seat: &mut Seat) -> bool {
    let items = seat.items();
    let resting = |k: usize| super::needs::Needs::resting_sustenance(items, k);
    if group::carried(seat.rival).iter().any(|&(_, _, k)| resting(k)) {
        return true;
    }
    let me = seat.me();
    let camps: Vec<usize> = (0..seat.gam.locations.len()).filter(|&l| l != super::camps::RESTSTOP && seat.camps.get(l).and_then(|c| c.owner) == Some(me)).collect();
    /// Where the bible lies: a room, or a living guard's pack.
    enum At {
        Room(usize, usize, usize),
        Guard(Who, usize),
    }
    // Every camp's rooms first, then every camp's living guards (`Concat`).
    let in_rooms = camps.iter().find_map(|&l| {
        (0..seat.room_count(l)).find_map(|r| seat.rooms.get(l, r).and_then(|b| (0..b.len()).find(|&s| b.get(s).is_some_and(resting))).map(|s| At::Room(l, r, s)))
    });
    let found = in_rooms.or_else(|| {
        camps.iter().find_map(|&l| {
            seat.guards(l).into_iter().find_map(|g| seat.pack_of(g).and_then(|p| (0..p.len()).find(|&s| p.get(s).is_some_and(resting))).map(|s| At::Guard(g, s)))
        })
    });
    let Some(at) = found else { return false };
    let kind = match at {
        At::Room(l, r, s) => seat.rooms.get(l, r).and_then(|b| b.get(s)),
        At::Guard(g, s) => seat.pack_of(g).and_then(|p| p.get(s)),
    };
    let Some(kind) = kind else { return false };
    let Some(m) = super::trading::cargo_carrier(seat.rival, items, kind) else { return false };
    let rounds = match at {
        At::Room(l, r, s) => seat.rooms.get_mut(l, r).and_then(|b| {
            let rounds = b.rounds_left(s);
            b.remove(s).map(|_| rounds)
        }),
        At::Guard(g, s) => seat.with_pack(g, |p, kit| super::equipment::take(p, kit, s)).flatten().map(|x| x.1),
    };
    let Some(rounds) = rounds else { return false };
    if super::equipment::give(group::pack_mut(seat.rival, m), kind, rounds).is_none() {
        return false;
    }
    let id = alloc::string::String::from(items.types.name_of(kind).unwrap_or(""));
    tell(seat, || alloc::format!("allocated {id} from camp storage for the Reststop expedition"));
    true
}

/// `CampDelivery.AddDeliveryCandidate`'s choice, from the turn's
/// assessments: `(camp, gain, critical, route)`.
fn delivery_candidate(seat: &Seat, snap: &Snap, policy: &AiPolicy) -> Option<(usize, f32, bool, route::Route)> {
    let items = seat.items();
    let me = seat.me();
    let here = seat.here();
    let sites = &snap.v.sites;
    let size = group::size(seat.rival) as i32;
    let carries_pump = group::carried(seat.rival).iter().any(|&(_, _, k)| super::needs::is_pump(items, k));
    let portable: i32 = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum();
    let desired = super::needs::desired_water_container_capacity(size);
    let spare_container = group::carried(seat.rival).iter().any(|&(_, _, k)| items.is_water_container(k) && portable - items.water_capacity(k) >= desired);
    let complete = if snap.needs.has_complete_useful_recipe() {
        super::production::useful_construction_opportunities(items, sites, me, seat.ai)
            .into_iter()
            .filter(|o| o.materials.iter().all(|m| super::support::has_construction_component(items, seat.rival, seat.ai, m)))
            .collect()
    } else {
        Vec::new()
    };
    let reserve_tool = upkeep::should_reserve_production_tool(seat, snap);
    let tool = |k: usize| items.types.name_of(k).and_then(|n| seat.productions.worked_by(n)).is_some();
    let carries_tool = !reserve_tool && (group::carried(seat.rival).iter().any(|&(_, _, k)| tool(k)) || seat.ai.reserve.iter().any(|&(k, _)| tool(k)));
    let t = seat.tables();
    let w = snap.world(&t, policy, me, seat.day, here);
    let mut best: Option<(usize, f32, bool, route::Route)> = None;
    let camps: Vec<usize> = (0..sites.sites.len()).filter(|&l| sites.sites[l].owner == Some(me) && !sites.sites[l].city).collect();
    for l in camps {
        if l == here {
            continue;
        }
        let s = &sites.sites[l];
        let needs_pump = super::production::needs_pump(items, s);
        let bottleneck = s.travel_water_bottleneck(items, size);
        let upgrade = carries_tool && portable_best_production(seat, &w, sites, l);
        let usable = |o: &super::production::Opportunity| match o.result {
            "item_trap" => super::production::needs_production_result(items, sites, me, l, "item_meat"),
            "item_rat_trap" => super::production::needs_production_result(items, sites, me, l, "item_rats"),
            "item_hand_pump" | "item_industrial_pump" => needs_pump,
            "item_protective_suit" => s.danger != crate::gamdat::Danger::None,
            _ => false,
        };
        let construct_pump = needs_pump && complete.iter().any(|o: &super::production::Opportunity| matches!(o.result, "item_hand_pump" | "item_industrial_pump"));
        let non_water = complete.iter().filter(|o| !matches!(o.result, "item_hand_pump" | "item_industrial_pump")).any(usable);
        let water_upgrade = needs_pump && carries_pump || construct_pump || spare_container && s.needs_reusable_water_reserve(size);
        let critical_water = bottleneck && water_upgrade;
        let ordinary_water = !bottleneck && water_upgrade;
        let other = upgrade || non_water;
        if !(water_upgrade || other) {
            continue;
        }
        let Some(r) = route::find(sites, me, seat.rival.previous, here, l) else { continue };
        let critical = critical_water && super::supplies::has_route_supplies(&snap.v.group, &r, false, 0, 0);
        if !(critical || ordinary_water || other) {
            continue;
        }
        let gain = super::reason::dotnet_max(super::reason::dotnet_max(0.0, sites.marginal_camp(items, l, me)), sites.marginal_water(items, l, me, needs_pump));
        let better = best.as_ref().is_none_or(|b| (critical && !b.2) || critical == b.2 && (gain > b.1 || gain == b.1 && r.days < b.3.days));
        if better {
            best = Some((l, gain, critical, r));
        }
    }
    best
}

/// `CampManagement.HasPortableBestProduction`.
fn portable_best_production(seat: &Seat, w: &super::needs::World, sites: &super::camps::Sites, l: usize) -> bool {
    let items = seat.items();
    let site = &sites.sites[l];
    let mut best: Option<(usize, f32, i32)> = None;
    for i in 0..site.productions.len() {
        let pr = super::production::trade_priority(items, site, i);
        let food = items.types.food_of(site.productions[i].produce);
        if best.is_none_or(|b| pr > b.1 || pr == b.1 && food > b.2) {
            best = Some((i, pr, food));
        }
    }
    let Some((i, _, _)) = best else { return false };
    if site.productions[i].installed >= super::production::desired_tool_count(items, sites, w.player, l, i) {
        return false;
    }
    let id = site.productions[i].id;
    let works = |k: usize| items.types.name_of(k).and_then(|n| seat.productions.worked_by(n)) == Some(id);
    group::carried(seat.rival).iter().any(|&(_, _, k)| works(k)) || seat.ai.reserve.iter().any(|&(k, c)| c > 0 && works(k))
}

// --- choosing among the candidates (`SelectAndReport`) --------------------

/// `SelectAndReport`: the best score, ties broken by a random key
/// (`OrderByDescending(Score).ThenBy(_ => Random.Next()).First()`); a
/// theatre change after a failed city recruitment; the non-progress
/// watchdog; the stalled-wait cancellation.
fn select_and_report(seat: &mut Seat, policy: &AiPolicy, game: &Game, snap: &Snap, candidates: Vec<Decision>, rng: &mut Rng) -> Decision {
    let names = game.names;
    let mut selected = first_by_score(&candidates, rng)
        .cloned()
        .expect("SelectAndReport: every plan adds a Wait candidate");
    if let Some(anchor) = seat.ai.recruitment_failure_anchor.take() {
        if let Some(relocation) = theater_relocation(seat, snap, policy, Some(anchor), true, names, rng) {
            let text = alloc::format!("recruitment unavailable for {} at {}; {}", names.place(anchor), names.place(seat.here()), relocation.reason.text(names));
            tell(seat, || alloc::format!("recruitment failure for {} triggered an immediate strategic-theater change toward {}", super::telemetry::place(anchor), title(relocation.target)));
            selected = Decision { reason: Reason::Note(text), ..relocation };
        } else {
            tell(seat, || alloc::format!("recruitment failure for {} found no alternative owned frontier with an eligible expansion opportunity; continuing ordinary planning", super::telemetry::place(anchor)));
        }
    }
    let blocked = selected.clone();
    let (tier, stationary, strategic) = stationary_relocation_tier(seat, &selected, names);
    if tier > 0 {
        let relocation = if tier == 1 { exploratory_relocation(seat, snap, names) } else { purposeful_relocation(seat, snap, policy, &blocked, strategic, names, rng) };
        let tier_name = if tier == 1 { "adjacent-owned exploration" } else { "reachable logistics relocation" };
        let here = super::telemetry::place(seat.here());
        let underlying = blocked.reason.text(&super::telemetry::Tokens);
        match relocation {
            Some(r) => {
                tell(seat, || alloc::format!(
                    "non-progress watchdog selected {tier_name} after {stationary} stationary waits and {strategic} strategic waits at {here}; underlying decision: {underlying}; moving toward {}",
                    title(r.target.or(r.next_step))
                ));
                let text = alloc::format!("{}; non-progress fallback for: {}", r.reason.text(names), blocked.reason.text(names));
                selected = Decision { reason: Reason::Note(text), ..r };
                seat.ai.stall.no_progress_waits = 0;
                seat.ai.stall.no_progress_location = None;
            }
            None if tier == 2 && strategic >= 12 => {
                if let Some(target) = emergency_escape_target(seat, snap) {
                    tell(seat, || alloc::format!(
                        "non-progress watchdog escalated to emergency escape from {here} toward {} after {strategic} strategic waits; ordinary relocation and one-way escape checks found no sufficiently supplied route",
                        super::telemetry::place(target)
                    ));
                    let step = route::find(&snap.v.sites, seat.me(), seat.rival.previous, seat.here(), target).map(|r| r.next_step);
                    let text = alloc::format!(
                        "emergency watchdog bailout after {stationary} stationary waits and {strategic} strategic waits; normal relocation failed: {}",
                        blocked.reason.text(names)
                    );
                    let mut d = Decision::new(Action::EmergencyEscape, 2000.0, Some(target), Reason::Note(text));
                    d.next_step = step;
                    selected = d;
                } else {
                    tell(seat, || alloc::format!("non-progress watchdog could not find {tier_name} after {stationary} stationary waits and {strategic} strategic waits at {here}; underlying decision: {underlying}"));
                }
            }
            None => tell(seat, || alloc::format!("non-progress watchdog could not find {tier_name} after {stationary} stationary waits and {strategic} strategic waits at {here}; underlying decision: {underlying}")),
        }
    }
    let stalled_target = if seat.ai.target == selected.target { selected.target } else { None };
    let target = title(selected.target);
    if cancel_stalled_strategic_wait(seat, &selected, names) {
        let mut deferred = None;
        if let Some(t) = stalled_target {
            let snap = seat.snap(policy, game.extended);
            let tt = seat.tables();
            let w = snap.world(&tt, policy, seat.me(), seat.day, seat.here());
            let f = field(seat, &w, &snap);
            let now = attack::progress(&f);
            let owner = snap.v.sites.owner(t);
            let text = selected.reason.text(names);
            let mut ai = seat.ai.clone();
            deferred = super::deferrals::try_defer(&mut ai, seat.day, t, owner, &text, &now);
            drop(f);
            *seat.ai = ai;
        }
        let why = selected.reason.text(&super::telemetry::Tokens);
        tell(seat, || {
            let tail = deferred.map_or(alloc::string::String::new(), |d| alloc::format!("; deferred until day {d} or relevant readiness improves"));
            alloc::format!("abandoned strategic target {target}: waited 5 consecutive turns without progress ({why}){tail}")
        });
    }
    let why = selected.reason.text(&super::telemetry::Tokens);
    tell(seat, || alloc::format!("decision {:?}, target {target}, score {}: {why}", selected.action, super::reason::format_fixed(selected.score, 0)));
    selected
}

/// `StationaryRelocationTier`: waits counted here and anywhere; the first
/// tier on the third idle turn in one place, the second on every third of
/// six or more strategic waits.
fn stationary_relocation_tier(seat: &mut Seat, d: &Decision, names: &dyn Names) -> (i32, i32, i32) {
    let here = seat.rival.at;
    let s = &mut seat.ai.stall;
    let (stationary, strategic) = (s.no_progress_waits, s.strategic_no_progress_waits);
    if d.reason.text(names) == "post-capture pause" {
        s.no_progress_waits = 0;
        s.no_progress_location = None;
        return (0, stationary, strategic);
    }
    if d.action != Action::Wait {
        s.no_progress_waits = 0;
        s.no_progress_location = None;
        if d.action != Action::Travel {
            s.strategic_no_progress_waits = 0;
        }
        return (0, stationary, strategic);
    }
    if s.no_progress_location != Some(here) {
        s.no_progress_location = Some(here);
        s.no_progress_waits = 0;
    }
    s.no_progress_waits += 1;
    s.strategic_no_progress_waits += 1;
    let (stationary, strategic) = (s.no_progress_waits, s.strategic_no_progress_waits);
    if strategic >= 6 && strategic % 3 == 0 {
        return (2, stationary, strategic);
    }
    if stationary == 3 {
        return (1, stationary, strategic);
    }
    (0, stationary, strategic)
}

/// `CancelStalledStrategicWait`: five waits in a row on one target for one
/// reason drop the target.
fn cancel_stalled_strategic_wait(seat: &mut Seat, d: &Decision, names: &dyn Names) -> bool {
    let s = &mut seat.ai.stall;
    let reset = |s: &mut super::state::Stall| {
        s.stalled_target = None;
        s.stalled_reason = alloc::string::String::new();
        s.stalled_waits = 0;
    };
    let Some(target) = d.target.filter(|_| d.action == Action::Wait) else {
        reset(s);
        return false;
    };
    let text = d.reason.text(names);
    if s.stalled_target == Some(target) && s.stalled_reason == text {
        s.stalled_waits += 1;
    } else {
        s.stalled_target = Some(target);
        s.stalled_reason = text;
        s.stalled_waits = 1;
    }
    if s.stalled_waits < 5 {
        return false;
    }
    reset(s);
    attack::clear_target(seat.ai);
    true
}

/// `FindExploratoryRelocation(allowUnowned: false)`: a neighbouring own
/// camp, not the one last explored from, the fewest own neighbours first,
/// then the longer road, then the title.
fn exploratory_relocation(seat: &mut Seat, snap: &Snap, names: &dyn Names) -> Option<Decision> {
    let here = seat.here();
    let me = seat.me();
    let items = seat.items();
    let sites = &snap.v.sites;
    let reachable: Vec<(usize, i32)> = sites.sites[here]
        .roads
        .iter()
        .copied()
        .filter(|&(n, d)| d > 0 && sites.owner(n) == Some(me) && seat.can_travel(here, n))
        .filter(|&(n, d)| {
            let r = route::Route { next_step: n, days: d, path: alloc::vec![n], effective_cost: d };
            super::supplies::has_territorial_route_supplies(&snap.v.group, sites, items, me, seat.rival.previous, here, &r, false)
        })
        .collect();
    if reachable.is_empty() {
        return None;
    }
    let alternatives: Vec<(usize, i32)> = reachable.iter().copied().filter(|&(n, _)| Some(n) != seat.ai.stall.previous_exploratory_camp).collect();
    let pool = if alternatives.is_empty() { reachable } else { alternatives };
    let owned_neighbours = |l: usize| sites.sites[l].roads.iter().filter(|&&(n, _)| sites.owner(n) == Some(me)).count();
    let pick = pool.iter().copied().min_by(|a, b| {
        (sites.owner(b.0) == Some(me))
            .cmp(&(sites.owner(a.0) == Some(me)))
            .then(owned_neighbours(a.0).cmp(&owned_neighbours(b.0)))
            .then(b.1.cmp(&a.1))
            .then(names.place(a.0).cmp(&names.place(b.0)))
    })?;
    seat.ai.stall.previous_exploratory_camp = Some(here);
    let text = alloc::format!("explore the owned territory at {} after three idle turns", names.place(pick.0));
    let mut d = Decision::new(Action::Travel, 0.0, None, Reason::Note(text));
    d.next_step = Some(pick.0);
    Some(d)
}

/// `FindPurposefulRelocation`.
fn purposeful_relocation(seat: &mut Seat, snap: &Snap, policy: &AiPolicy, blocked: &Decision, strategic: i32, names: &dyn Names, rng: &mut Rng) -> Option<Decision> {
    let here = seat.here();
    let me = seat.me();
    let text = blocked.reason.text(names);
    let blocked_attack = seat.ai.target.filter(|&t| snap.v.sites.owner(t).is_some_and(|o| o != me) && text.starts_with("attack plan for "));
    let mut change = match blocked_attack {
        Some(t) => theater_relocation(seat, snap, policy, Some(t), false, names, rng),
        None => None,
    };
    if change.is_none() && strategic >= 12 && text.starts_with("no eligible neutral or hostile frontier target") {
        change = theater_relocation(seat, snap, policy, None, false, names, rng);
    }
    if change.is_some() {
        return change;
    }
    let t = seat.tables();
    let w = snap.world(&t, policy, me, seat.day, here);
    let doc = |l: usize| doctor_at(seat, l);
    if let Some(escape) = recovery::find_destination(&w, seat.rival, seat.rival.previous, true, Trip::Escape, &doc) {
        if let Some(r) = route::find(&snap.v.sites, me, seat.rival.previous, here, escape).filter(|r| seat.can_travel(here, r.next_step)) {
            let text = alloc::format!("take a one-way escape to sustainable owned camp {}; return-route and reserve-building requirements waived", names.place(escape));
            let mut d = Decision::new(Action::Travel, 0.0, Some(escape), Reason::Note(text));
            d.next_step = Some(r.next_step);
            d.commit_journey = true;
            return Some(d);
        }
    }
    let needs_recruit = text.contains("attackers");
    let items = seat.items();
    let reachable: Vec<(usize, route::Route)> = (0..snap.v.sites.sites.len())
        .filter(|&l| l != here && (snap.v.sites.is_city(l) || snap.v.sites.owner(l) == Some(me)))
        .filter_map(|l| route::find(&snap.v.sites, me, seat.rival.previous, here, l).map(|r| (l, r)))
        .filter(|(_, r)| seat.can_travel(here, r.next_step) && super::supplies::has_territorial_route_supplies(&snap.v.group, &snap.v.sites, items, me, seat.rival.previous, here, r, false))
        .collect();
    if reachable.is_empty() {
        return None;
    }
    let alternatives: Vec<&(usize, route::Route)> = reachable.iter().filter(|(l, _)| Some(*l) != seat.ai.stall.previous_exploratory_camp).collect();
    let pool: Vec<&(usize, route::Route)> = if alternatives.is_empty() { reachable.iter().collect() } else { alternatives };
    let city = |l: usize| snap.v.sites.is_city(l);
    let pick = pool.into_iter().min_by(|a, b| {
        (needs_recruit && city(b.0))
            .cmp(&(needs_recruit && city(a.0)))
            .then(city(b.0).cmp(&city(a.0)))
            .then(a.1.days.cmp(&b.1.days))
            .then(names.place(a.0).cmp(&names.place(b.0)))
    })?;
    seat.ai.stall.previous_exploratory_camp = Some(here);
    let mut d = Decision::new(Action::Travel, 0.0, Some(pick.0), Reason::Note(alloc::format!("seek useful logistics at {}", names.place(pick.0))));
    d.next_step = Some(pick.1.next_step);
    Some(d)
}

/// `LinkDistance`: roads crossed, by breadth.
fn link_distance(sites: &super::camps::Sites, from: usize, to: usize) -> Option<i32> {
    if from == to {
        return Some(0);
    }
    let mut visited = alloc::vec![false; sites.sites.len()];
    visited[from] = true;
    let mut q = alloc::collections::VecDeque::from([(from, 0)]);
    while let Some((at, d)) = q.pop_front() {
        for &(nb, len) in &sites.sites[at].roads {
            if len <= 0 || visited[nb] {
                continue;
            }
            visited[nb] = true;
            if nb == to {
                return Some(d + 1);
            }
            q.push_back((nb, d + 1));
        }
    }
    None
}

/// `FrontierOpportunityScore`.
fn frontier_opportunity_score(seat: &Seat, ai: &mut super::state::ModernAi, f: &Field, policy: &AiPolicy, l: usize) -> i32 {
    let sites = f.w.sites;
    let me = seat.me();
    let mut score = 0;
    for &(nb, d) in &sites.sites[l].roads {
        if d <= 0 || sites.is_city(nb) || sites.owner(nb) == Some(me) {
            continue;
        }
        if sites.owner(nb).is_none() {
            if super::claims::can_claim(seat.rival, ai, f.w, f.group, nb) && sites.sites[nb].food_production_potential(f.w.items) {
                score += if sites.sites[nb].acceptable_first_camp(f.w.items) { 5 } else { 2 };
            }
        } else if attack::is_target_allowed(ai, f, nb, policy) {
            let defenders = attack::estimate(ai, f, nb).expected_defenders;
            score += (8 - defenders * 2).max(2);
        }
    }
    score
}

/// `FindStrategicTheaterRelocation`: a far own frontier camp with
/// opportunities, drawn by lot; the blocked attack put aside.
/// `OrderByDescending(d => d.Score).ThenBy(_ => Random.Next()).First()` as
/// .NET computes it (`OrderedEnumerable.TryGetFirst` with a caching
/// comparer): **the random key is drawn lazily** — one for the first
/// candidate, one for each later candidate that scores better than the best
/// so far (it becomes the best), and one for each that ties it (it becomes
/// the best when its key is smaller). A candidate that scores worse draws
/// nothing. The pick is uniform among the top-score ties, and the number of
/// draws is upstream's.
fn first_by_score<'d>(candidates: &'d [Decision], rng: &mut Rng) -> Option<&'d Decision> {
    let mut best: Option<(usize, i32)> = None;
    for (i, d) in candidates.iter().enumerate() {
        best = Some(match best {
            None => (i, rng.next_int()),
            Some((b, key)) => match single_compare(d.score, candidates[b].score) {
                core::cmp::Ordering::Greater => (i, rng.next_int()),
                core::cmp::Ordering::Equal => {
                    let k = rng.next_int();
                    if k < key { (i, k) } else { (b, key) }
                }
                core::cmp::Ordering::Less => (b, key),
            },
        });
    }
    best.map(|(i, _)| &candidates[i])
}

/// `float.CompareTo`, which `Comparer<float>.Default` is: by value, the two
/// zeros equal, and NaN below everything but itself.
fn single_compare(a: f32, b: f32) -> core::cmp::Ordering {
    use core::cmp::Ordering::*;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Equal,
        (true, false) => Less,
        (false, true) => Greater,
        _ => a.partial_cmp(&b).unwrap_or(Equal),
    }
}

fn theater_relocation(seat: &mut Seat, snap: &Snap, policy: &AiPolicy, blocked: Option<usize>, small: bool, names: &dyn Names, rng: &mut Rng) -> Option<Decision> {
    let me = seat.me();
    let here = seat.here();
    let camps = snap.v.sites.sites.iter().filter(|s| s.owner == Some(me)).count() as i32;
    if camps < if blocked.is_none() || small { 2 } else { 6 } {
        return None;
    }
    let anchor = blocked.unwrap_or(here);
    let minimum = (2 + camps / 5).clamp(2, 6);
    let t = seat.tables();
    let w = snap.world(&t, policy, me, seat.day, here);
    let f = field(seat, &w, snap);
    let mut ai = seat.ai.clone();
    let mut candidates: Vec<(usize, route::Route, i32, i32)> = Vec::new();
    for l in 0..snap.v.sites.sites.len() {
        if l == here || snap.v.sites.is_city(l) || snap.v.sites.owner(l) != Some(me) {
            continue;
        }
        let r = route::find(&snap.v.sites, me, seat.rival.previous, here, l);
        let depth = link_distance(&snap.v.sites, anchor, l);
        let score = frontier_opportunity_score(seat, &mut ai, &f, policy, l);
        let (Some(r), Some(depth)) = (r, depth) else { continue };
        if score > 0 && seat.can_travel(here, r.next_step) {
            candidates.push((l, r, depth, score));
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let mut depth = minimum;
    let mut pool: Vec<&(usize, route::Route, i32, i32)> = candidates.iter().filter(|c| c.2 >= depth).collect();
    while depth > 1 && pool.len() < 3 {
        depth -= 1;
        pool = candidates.iter().filter(|c| c.2 >= depth).collect();
    }
    if pool.is_empty() {
        pool = candidates.iter().collect();
    }
    let n = pool.len();
    let pick = pool[rng.below(n as u32) as usize].clone();
    if let Some(b) = blocked {
        attack::defer_attack_plan(&mut ai, &f, b, policy);
    }
    drop(f);
    ai.stall.previous_exploratory_camp = Some(here);
    *seat.ai = ai;
    let from = match blocked {
        None => alloc::format!("unproductive {} region", names.place(here)),
        Some(b) => alloc::format!("blocked {} frontier", names.place(b)),
    };
    let text = alloc::format!(
        "change strategic theater from {from} to {} ({} links away, {} frontier opportunity score; uniformly selected from {n} frontiers at minimum depth {depth}; route-supply constraints waived)",
        names.place(pick.0),
        pick.2,
        pick.3
    );
    let mut d = Decision::new(Action::Travel, 0.0, Some(pick.0), Reason::Note(text));
    d.next_step = Some(pick.1.next_step);
    d.commit_journey = true;
    Some(d)
}

/// `FindEmergencyEscapeTarget`: the nearest own camp that feeds and waters.
fn emergency_escape_target(seat: &Seat, snap: &Snap) -> Option<usize> {
    let me = seat.me();
    let here = seat.here();
    let items = seat.items();
    (0..snap.v.sites.sites.len())
        .filter(|&l| {
            let s = &snap.v.sites.sites[l];
            l != here && !s.city && s.owner == Some(me) && s.can_provision_food(items) && s.water_surplus() > 0
        })
        .filter_map(|l| route::find(&snap.v.sites, me, seat.rival.previous, here, l).filter(|r| seat.can_travel(here, r.next_step)).map(|r| (l, r.days)))
        .min_by_key(|&(l, d)| (d, l))
        .map(|x| x.0)
}

// --- carrying out (`AiDecision.Execute`) -----------------------------------

/// `AiDecision.Execute`.
fn execute(seat: &mut Seat, policy: &AiPolicy, game: &Game, d: &Decision, rng: &mut Rng) -> Outcome {
    let extended = game.extended;
    let here = seat.here();
    match d.action {
        Action::Recruit => {
            let snap = seat.snap(policy, extended);
            let people_here = snap.v.people.get(here).cloned().unwrap_or_default();
            let t = seat.tables();
            let w = snap.world(&t, policy, seat.me(), seat.day, here);
            let mut kit = seat.ai.leader_kit;
            let ai = seat.ai.clone();
            let mut rooms = snap.v.camp_rooms.clone();
            let mut chosen: Option<usize> = None;
            let got = hiring::execute_recruit(seat.rival, &mut kit, &mut rooms, &w, &ai, &people_here, d.recruit, rng, |_, record| {
                chosen = Some(record);
                true
            });
            seat.ai.leader_kit = kit;
            let hired = match (got, chosen) {
                (hiring::Hired::Hired { .. }, Some(record)) => {
                    seat.write_camp_rooms(&rooms);
                    seat.hire(record).then_some(record)
                }
                _ => None,
            };
            let Some(record) = hired else {
                tell(seat, || "could not afford an available recruit".into());
                return Outcome::StateUnchanged;
            };
            let class = seat.class_of(Some(record));
            tell(seat, || alloc::format!("hired {} ({:?})", super::telemetry::person(record), class));
            after_hire(seat);
            upkeep::maintain_group_equipment(seat, policy, extended, false);
            if let Some(step) = d.next_step.filter(|&s| seat.can_travel(here, s)) {
                upkeep::prepare_water_for_departure(seat, policy, extended);
                seat.travel(step);
                tell(seat, || alloc::format!("departs toward {} with new settler via {}", super::telemetry::place(d.target.unwrap_or(step)), super::telemetry::place(step)));
                return Outcome::EndTurn;
            }
            Outcome::ContinuePlanning
        }
        Action::ReleaseFollower => release_followers(seat, policy, extended, rng),
        Action::DismissFollower => {
            let who = upkeep::dismiss_surplus_follower(seat, policy);
            reported(seat, who, |n, _| alloc::format!("dismissed surplus follower {n}; no strategic garrison needs another guard"))
        }
        Action::StationFollower => {
            let who = upkeep::station_surplus_follower(seat, policy, extended);
            reported(seat, who, |n, at| alloc::format!("stationed surplus follower {n} at {at}"))
        }
        Action::StationTradeFollower => {
            let who = upkeep::station_trade_follower(seat, policy, extended);
            reported(seat, who, |n, at| alloc::format!("left {n} at {at} while preparing a city caravan"))
        }
        Action::RecallFollower => {
            let who = upkeep::recall_camp_follower(seat, policy, extended);
            reported(seat, who, |n, at| alloc::format!("recalled {n} from {at}"))
        }
        Action::MobilizeFrontierFollower => {
            let who = upkeep::mobilize_camp_follower(seat);
            reported(seat, who, |n, at| alloc::format!("mobilized {n} from the frontier at {at}"))
        }
        Action::CancelAttackPlan => {
            let me = seat.me();
            let attack_plan = seat.ai.target.is_some_and(|t| seat.camps.get(t).and_then(|c| c.owner).is_some_and(|o| o != me) && !seat.gam.locations[t].is_city);
            let Some(target) = d.target.filter(|_| attack_plan) else { return Outcome::StateUnchanged };
            let snap = seat.snap(policy, extended);
            let t = seat.tables();
            let w = snap.world(&t, policy, me, seat.day, here);
            let f = field(seat, &w, &snap);
            let mut ai = seat.ai.clone();
            attack::defer_attack_plan(&mut ai, &f, target, policy);
            // `DeferAttacksForFailedCityRecruitment` in a city, `DeferAttackPlan` elsewhere.
            if in_city(seat) {
                ai.recruitment_failure_anchor = Some(target);
                ai.attack_recruitment = Some(super::state::AttackRecruitment { group_size: group::size(seat.rival) as i32, until_day: seat.day + 20 });
            }
            *seat.ai = ai;
            let why = d.reason.text(&super::telemetry::Tokens);
            tell(seat, || alloc::format!("abandoned attack plan for {}: {why}; returning to economic and territorial planning", super::telemetry::place(target)));
            Outcome::ContinuePlanning
        }
        Action::ImproveCamp => {
            if !upkeep::improve_camp(seat, policy, extended) {
                return Outcome::StateUnchanged;
            }
            tell(seat, || alloc::format!("improved production at {}", super::telemetry::place(here)));
            Outcome::ContinuePlanning
        }
        Action::ClaimNeutral => {
            let Some(settler) = upkeep::select_camp_npc(seat, policy, extended, rng) else { return Outcome::StateUnchanged };
            let name = seat.companion_name(settler);
            upkeep::create_camp(seat, policy, extended, settler);
            upkeep::provision_from_camp_surplus(seat, policy, extended, here);
            tell(seat, || alloc::format!("claimed {} using {name}", super::telemetry::place(here)));
            Outcome::ContinuePlanning
        }
        Action::AttackHostile => {
            super::resolve::resolve(seat, policy, extended, false, rng);
            if seat.rival.needs.health <= 0 { Outcome::EndTurn } else { Outcome::ContinuePlanning }
        }
        Action::EmergencyEscape => {
            let released: Vec<alloc::string::String> = (0..seat.rival.party.companions.len()).map(|i| seat.companion_name(i)).collect();
            while !seat.rival.party.companions.is_empty() {
                let last = seat.rival.party.companions.len() - 1;
                seat.dismiss_follower(last);
            }
            let (old_food, old_water) = (seat.rival.needs.food, seat.rival.needs.water);
            seat.rival.needs.food = 9;
            seat.rival.needs.water = 5;
            match d.target {
                Some(t) => seat.ai.target = Some(t),
                None => attack::clear_target(seat.ai),
            }
            // `ResetNonProgressWatchdog`: the stalled-wait record stays.
            let s = &mut seat.ai.stall;
            s.no_progress_waits = 0;
            s.no_progress_location = None;
            s.strategic_no_progress_waits = 0;
            s.previous_exploratory_camp = None;
            if let Some(t) = d.target {
                seat.ai.journey = Some(super::state::Commitment { destination: t, reason: "reach the sustainable camp selected by emergency escape".into() });
            }
            let leader = super::telemetry::person(seat.me());
            tell(seat, || {
                let list = if released.is_empty() { alloc::string::String::new() } else { alloc::format!(" ({})", released.join(", ")) };
                alloc::format!(
                    "emergency watchdog bailout released {} follower(s){list}, reset {leader} from F{old_food}/W{old_water} to F9/W5, and selected one-way escape toward {}",
                    released.len(),
                    d.target.map_or_else(|| alloc::string::String::from("no destination"), super::telemetry::place)
                )
            });
            match d.next_step.filter(|&s| seat.can_travel(here, s)) {
                Some(step) => {
                    upkeep::prepare_water_for_departure(seat, policy, extended);
                    seat.travel(step);
                    tell(seat, || alloc::format!("emergency watchdog escape travels toward {} via {}", super::telemetry::place(d.target.unwrap_or(step)), super::telemetry::place(step)));
                    Outcome::EndTurn
                }
                None => {
                    tell(seat, || "emergency watchdog bailout could not begin travel because the selected route is no longer permitted".into());
                    Outcome::StateUnchanged
                }
            }
        }
        Action::Travel => match d.next_step.filter(|&s| seat.can_travel(here, s)) {
            Some(step) => {
                if d.commit_journey {
                    if let Some(t) = d.target {
                        seat.ai.journey = Some(super::state::Commitment { destination: t, reason: d.reason.text(game.names) });
                    }
                }
                if let Some(t) = d.target {
                    let current_is_city = seat.ai.target.is_some_and(|x| seat.gam.locations[x].is_city);
                    // `StrategicTarget = Target`: the plan's kind stays as it was.
                    if seat.ai.target.is_none() || current_is_city {
                        seat.ai.target = Some(t);
                    }
                }
                upkeep::prepare_water_for_departure(seat, policy, extended);
                seat.travel(step);
                tell(seat, || alloc::format!("travels toward {} via {}", super::telemetry::place(d.target.unwrap_or(step)), super::telemetry::place(step)));
                Outcome::EndTurn
            }
            None => {
                if let Some(step) = d.next_step {
                    tell(seat, || alloc::format!("did not travel toward {}: route is no longer permitted", super::telemetry::place(step)));
                }
                Outcome::StateUnchanged
            }
        },
        Action::Wait => Outcome::EndTurn,
    }
}

/// A follower action's outcome, reported with who it moved and where.
fn reported(seat: &Seat, who: Option<alloc::string::String>, line: impl FnOnce(&str, &str) -> alloc::string::String) -> Outcome {
    match who {
        Some(n) => {
            let at = super::telemetry::place(seat.here());
            tell(seat, || line(&n, &at));
            Outcome::ContinuePlanning
        }
        None => Outcome::StateUnchanged,
    }
}

/// `HireNpc`'s end: `EquipWaterContainers` for the recruit, then, with a
/// weapon in the reserve (`HasWeapon`: knife, axe or pitchfork) and room,
/// `GetBestWeapon(allowProductionTool: true)` into his hands.
fn after_hire(seat: &mut Seat) {
    let items = seat.items();
    let Some(last) = seat.rival.party.companions.len().checked_sub(1) else { return };
    let pack = &seat.rival.party.companions[last].pack;
    if !pack.iter().any(|k| items.is_water_container(k)) && !pack.is_full() {
        if let Some(k) = upkeep::reserve_best_water_container(seat.ai, items) {
            seat.ai.reserve_take(k);
            super::equipment::give(&mut seat.rival.party.companions[last].pack, k, None);
        }
    }
    let has_weapon = upkeep::WEAPON_ORDER[2..].iter().any(|n| items.types.index_of(n).is_some_and(|k| seat.ai.reserve_count(k) > 0));
    if has_weapon && !seat.rival.party.companions[last].pack.is_full() {
        if let Some(k) = upkeep::best_pool_weapon(seat.ai, items, 0, true, seat.productions, None) {
            seat.ai.reserve_take(k);
            let c = &mut seat.rival.party.companions[last];
            c.kit.weapon = super::equipment::give(&mut c.pack, k, None);
        }
    }
}

/// `ReleaseFollower`: until a recovery place is reachable or the group can
/// recover here — an emergency camp where one can be made, a guard where
/// the camp wants one, else a follower let go — then on toward recovery.
fn release_followers(seat: &mut Seat, policy: &AiPolicy, extended: bool, rng: &mut Rng) -> Outcome {
    let initial = group::size(seat.rival);
    let here = seat.here();
    if !seat.ai.survival_releases.contains(&here) {
        seat.ai.survival_releases.push(here);
    }
    loop {
        let snap = seat.snap(policy, extended);
        let items = seat.items();
        let site = &snap.v.sites.sites[here];
        let at = super::telemetry::place(here);
        let owned = site.owner == Some(seat.me());
        if site.owner.is_none() && site.food_production_potential(items) && site.water > 1 {
            let Some(npc) = upkeep::select_camp_npc(seat, policy, extended, rng) else { break };
            let name = seat.companion_name(npc);
            upkeep::create_camp(seat, policy, extended, npc);
            tell(seat, || alloc::format!("created an emergency camp at {at} using {name} instead of releasing them"));
        } else if owned && super::reinforcement::wants_additional_guard(items, &snap.v.sites, seat.me(), policy, here, policy.critical_garrison_target) {
            if let Some(name) = upkeep::station_surplus_follower(seat, policy, extended) {
                tell(seat, || alloc::format!("stationed {name} at {at} instead of releasing them"));
            } else {
                let Some(name) = upkeep::release_follower_for_survival(seat) else { break };
                tell(seat, || alloc::format!("released {name} because the strategically wanted garrison at {at} cannot sustainably support them"));
            }
        } else {
            let Some(name) = upkeep::release_follower_for_survival(seat) else { break };
            tell(seat, || {
                if owned {
                    alloc::format!("released {name} because {at} does not need another strategic guard")
                } else {
                    alloc::format!("released {name} completely because the location cannot support a camp")
                }
            });
        }
        let snap = seat.snap(policy, extended);
        let t = seat.tables();
        let w = snap.world(&t, policy, seat.me(), seat.day, here);
        let doc = |l: usize| doctor_at(seat, l);
        let recovery = recovery::find_destination(&w, seat.rival, seat.rival.previous, true, Trip::Normal, &doc);
        if !(recovery.is_none() && group::size(seat.rival) > 1 && !recovery::can_recover_locally_for_travel(&w, seat.rival)) {
            break;
        }
    }
    let snap = seat.snap(policy, extended);
    let t = seat.tables();
    let w = snap.world(&t, policy, seat.me(), seat.day, here);
    let doc = |l: usize| doctor_at(seat, l);
    let mut recovery = recovery::find_destination(&w, seat.rival, seat.rival.previous, true, Trip::Normal, &doc);
    let last_chance = recovery.is_none() && recovery::waiting_will_be_fatal(&w, seat.rival);
    if last_chance {
        recovery = recovery::last_chance_destination(&w, seat.rival, seat.rival.previous);
    }
    let step = recovery.and_then(|r| route::find(&snap.v.sites, seat.me(), seat.rival.previous, here, r)).map(|r| r.next_step).filter(|&s| seat.can_travel(here, s));
    if let (Some(s), Some(r)) = (step, recovery) {
        upkeep::prepare_water_for_departure(seat, policy, extended);
        seat.travel(s);
        let to = super::telemetry::place(r);
        tell(seat, || {
            if last_chance {
                alloc::format!("takes the least-bad route toward {to} after reducing the group")
            } else {
                alloc::format!("departs toward {to} after reducing the group to a survivable size")
            }
        });
        return Outcome::EndTurn;
    }
    if group::size(seat.rival) != initial { Outcome::ContinuePlanning } else { Outcome::StateUnchanged }
}
