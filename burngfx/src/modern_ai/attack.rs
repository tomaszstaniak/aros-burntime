//! Planning an attack on another's camp: `AttackPlanning` with the attack
//! plan's state (`ClassicAiState.StartAttackPlan` and its kin) and
//! `CombatStrength`. At tag `v1.2`, `AI/PlayerBehavior/Combat/`.
//!
//! What the opponent believes of a camp's defence is its own knowledge
//! ([`super::intel`]); what it knows of its own group is the group.

use alloc::vec::Vec;

use super::camps::Site;
use super::decision::{Action, Decision};
use super::deferrals::{self, Progress};
use super::encounter::{Fighter, Rules};
use super::intel::{self, CampFacts, DefenseEstimate};
use super::needs::World;
use super::policy::AiPolicy;
use super::reason::{Reason, Waiting};
use super::route::{self, Route, RouteMap};
use super::state::{FailedAttack, Force, ModernAi};
use super::supplies::{self, GroupSupplies};

/// What attack planning reads.
pub struct Field<'a> {
    pub w: &'a World<'a>,
    /// The fight's rules where the group stands.
    pub rules: Rules<'a>,
    /// The group as fighters, the leader first.
    pub party: &'a [Fighter],
    pub group: &'a GroupSupplies,
    pub previous: Option<usize>,
    /// Players a human plays.
    pub humans: &'a [usize],
    /// The defenders where the group stands (`CombatStrength.Defenders`).
    pub defenders_here: &'a [Fighter],
    /// On the road (`Player.IsTraveling`).
    pub travelling: bool,
}

/// `Player.CanTravel(from, to)`: from where the group stands, not on the
/// road, down an open road, and out of somebody else's camp only the way it
/// came — the world's rule (`Seat::can_travel`), on the picture.
pub fn can_travel(f: &Field, from: usize, to: usize) -> bool {
    if f.travelling || from != f.w.here {
        return false;
    }
    if f.w.sites.owner(from).is_some_and(|o| o != f.w.player) && f.previous != Some(to) {
        return false;
    }
    f.w.sites.ways(from).iter().any(|&(n, d)| n == to && d > 0)
}

impl Field<'_> {
    fn sites(&self) -> &super::camps::Sites {
        self.w.sites
    }
    fn player(&self) -> usize {
        self.w.player
    }
    fn day(&self) -> i32 {
        self.w.day
    }
    fn route(&self, from: usize, to: usize) -> Option<Route> {
        route::find(self.sites(), self.player(), self.previous, from, to)
    }
}

/// `IsHostile`: somebody else's camp, not a city.
pub fn is_hostile(sites: &super::camps::Sites, l: usize, player: usize) -> bool {
    route::is_hostile(sites, l, player)
}

/// `CombatStrength.Fighter`: the detailed strength.
pub fn fighter_strength(rules: &Rules, f: &Fighter) -> f32 {
    rules.strength(f, true)
}

/// `CombatStrength.Attacker`: the living group's.
pub fn attacker_strength(f: &Field) -> f32 {
    f.party.iter().filter(|x| !x.dead()).map(|x| fighter_strength(&f.rules, x)).sum()
}

/// `AttackRetryMemory.Force.Capture`.
pub fn capture(rules: &Rules, fighters: &[Fighter], boss: Option<&Fighter>) -> Force {
    let living: Vec<&Fighter> = fighters.iter().filter(|x| !x.dead()).collect();
    Force {
        count: living.len() as i32,
        health: living.iter().map(|x| x.health).sum(),
        strength: living.iter().map(|x| fighter_strength(rules, x)).sum(),
        boss_health: boss.map_or(0, |b| b.health),
    }
}

/// `HasGroupWeapon`.
pub fn has_group_weapon(f: &Field) -> bool {
    let t = f.rules.types;
    f.party.iter().any(|x| {
        !x.dead() && crate::table_combat::find_best_weapon(&x.pack, None, t).and_then(|s| x.pack.get(s)).is_some_and(|k| t.damage_of(k) > 0)
    })
}

/// The camp as `DefenseIntelligence.Estimate` reads it.
fn camp_facts(f: &Field, camp: usize, owner: Option<usize>) -> CampFacts {
    let items = f.w.items;
    let site: &Site = &f.sites().sites[camp];
    let sustains = |g: i32| {
        site.productions.iter().any(|p| {
            let r = p.production.rate(p.max_tools().max(0) as usize, g as usize, items.types.food_of(p.produce));
            !r.camp_starving && r.food_per_day >= g
        })
    };
    let open = site.roads.iter().filter(|&&(_, d)| d > 0).count();
    CampFacts {
        capacity: intel::sustainable_capacity(site.water, sustains),
        owner_camps: owner.map_or(0, |o| f.sites().sites.iter().filter(|s| s.owner == Some(o)).count() as i32),
        strategic: open <= 2 || site.well_established_potential(items),
    }
}

/// `DefenseIntelligence.UpdateKnowledge` over the world.
pub fn update_knowledge(ai: &mut ModernAi, f: &Field) {
    let camps: Vec<(usize, Option<usize>)> = (0..f.sites().sites.len()).filter(|&l| !f.sites().is_city(l)).map(|l| (l, f.sites().owner(l))).collect();
    intel::update_knowledge(ai, f.day(), camps);
}

/// `DefenseIntelligence.Estimate`.
pub fn estimate(ai: &mut ModernAi, f: &Field, camp: usize) -> DefenseEstimate {
    update_knowledge(ai, f);
    let owner = intel::observation(ai, camp).and_then(|o| o.owner);
    intel::estimate(ai, f.day(), camp, camp_facts(f, camp, owner))
}

/// `AttackRetryMemory.CanRetry(player, target)`.
fn can_retry(ai: &ModernAi, f: &Field, target: usize) -> bool {
    let Some(owner) = f.sites().owner(target) else { return true };
    let attackers = capture(&f.rules, f.party, f.party.first());
    let observed = (f.w.here == target).then(|| capture(&f.rules, f.defenders_here, None));
    intel::can_retry(ai, target, owner, &attackers, observed.as_ref())
}

/// `IsTargetAllowed`: hostile, not a stalemate, and a human's camp only in
/// retaliation or when few enough defend it (a lone knife guard counting as
/// none on easy).
pub fn is_target_allowed(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> bool {
    if !is_hostile(f.sites(), target, f.player()) || !can_retry(ai, f, target) {
        return false;
    }
    let Some(owner) = f.sites().owner(target) else { return true };
    if !f.humans.contains(&owner) || ai.retaliating_against(owner, f.day()) {
        return true;
    }
    let mut expected = estimate(ai, f, target).expected_defenders;
    if policy.treat_lone_knife_guard_as_undefended && expected == 1 {
        expected = 0;
    }
    expected <= policy.max_human_camp_defenders_to_attack
}

/// `HasImprovedSinceFailedAttack`.
pub fn has_improved_since_failed_attack(ai: &ModernAi, day: i32, target: usize, group: i32, attackers: f32, defenders: f32) -> bool {
    match &ai.failed_attack {
        Some(fa) if fa.camp == target && day <= fa.until_day => {
            group > fa.group_size || attackers >= fa.attacker_strength * 1.10 || defenders <= fa.defender_strength * 0.85
        }
        _ => true,
    }
}

/// `IsSuitable`: followers, a weapon, an allowed target, improved since a
/// failure, and strength enough against what is expected.
pub fn is_suitable(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> bool {
    let followers = f.party.iter().skip(1).filter(|x| !x.dead()).count() as i32;
    if followers == 0 || !has_group_weapon(f) || !is_target_allowed(ai, f, target, policy) {
        return false;
    }
    let defenders = estimate(ai, f, target).estimated_strength;
    if defenders <= 0.0 {
        return true;
    }
    let attackers = attacker_strength(f);
    let leader = if f.party.first().is_some_and(|l| !l.dead()) { 1 } else { 0 };
    has_improved_since_failed_attack(ai, f.day(), target, followers + leader, attackers, defenders) && attackers / defenders >= policy.minimum_attack_ratio
}

/// `RequiredAttackGroupSize`: the expected defenders and one more (none
/// more on the detailed estimate), two to the policy's size — or the group
/// as it is, two or more, if it is already suitable.
pub fn required_attack_group_size(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> i32 {
    let expected = estimate(ai, f, target).expected_defenders;
    let advantage = if policy.use_detailed_combat_estimate { 0 } else { 1 };
    let desired = (expected + advantage).clamp(2, policy.attack_group_size.max(2));
    let available = f.party.len() as i32;
    if !policy.use_detailed_combat_estimate && available >= 2 && available < desired && is_suitable(ai, f, target, policy) {
        return available;
    }
    desired
}

/// `IsTerritorialFrontierTarget`: reached from the player's camps through
/// cities and free places that could not hold a camp anyway.
pub fn is_territorial_frontier_target(f: &Field, target: usize) -> bool {
    let sites = f.sites();
    let player = f.player();
    if !is_hostile(sites, target, player) {
        return false;
    }
    let n = sites.locations();
    let mut visited = alloc::vec![false; n];
    let mut queue = alloc::collections::VecDeque::new();
    for l in 0..n {
        if sites.owner(l) == Some(player) {
            visited[l] = true;
            queue.push_back(l);
        }
    }
    while let Some(at) = queue.pop_front() {
        for (nb, d) in sites.ways(at) {
            if d <= 0 || nb >= n {
                continue;
            }
            if nb == target {
                return true;
            }
            if visited[nb] {
                continue;
            }
            visited[nb] = true;
            if sites.is_city(nb) || sites.owner(nb).is_none() && !sites.sites[nb].can_sustain_camp(f.w.items) {
                queue.push_back(nb);
            }
        }
    }
    false
}

/// `CanAdvancePlan` and `AdvanceBlockingReason` together: `None` when the
/// plan may advance, else why not.
pub fn advance_blocked(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> Option<Waiting> {
    let size = required_attack_group_size(ai, f, target, policy);
    if (f.party.len() as i32) < size {
        return Some(Waiting::Attackers { size });
    }
    if !has_group_weapon(f) {
        return Some(Waiting::Weapon);
    }
    if !is_territorial_frontier_target(f, target) {
        return Some(Waiting::NotFrontier);
    }
    let Some(r) = f.route(f.w.here, target) else { return Some(Waiting::NoRoute) };
    let sites = f.sites();
    let next = r.next_step;
    let own = sites.owner(next) == Some(f.player());
    if own && !sites.sites[next].can_provision_group_water(f.w.items, f.party.len() as i32) {
        return Some(Waiting::CampWater);
    }
    let supplied = if next == target {
        supplies::has_route_supplies(f.group, &r, true, 0, 0)
    } else if own || sites.is_city(next) {
        f.route(f.w.here, next).is_some_and(|leg| supplies::has_route_supplies(f.group, &leg, false, 0, 0))
    } else {
        supplies::has_route_supplies(f.group, &r, true, 0, 0)
    };
    if !supplied {
        return Some(Waiting::RouteSupplies);
    }
    if (next == target || !(own || sites.is_city(next))) && !is_suitable(ai, f, target, policy) {
        return Some(Waiting::CombatReadiness);
    }
    None
}

/// `CanAdvancePlan`.
pub fn can_advance_plan(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> bool {
    advance_blocked(ai, f, target, policy).is_none()
}

/// `AdvanceBlockingReason`.
pub fn advance_blocking_reason(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) -> Reason {
    Reason::AttackWaiting { target, why: advance_blocked(ai, f, target, policy).unwrap_or(Waiting::LegalAdvance) }
}

/// What deferrals measure progress by, now.
pub fn progress(f: &Field) -> Progress {
    Progress {
        supplies: f.group.lowest_food_with_inventory().min(f.group.lowest_water_with_inventory()),
        group: f.party.len() as i32,
        strength: attacker_strength(f),
    }
}

/// `FindReadyAlternativeTarget`: another frontier camp the group is ready
/// for, supplied to reach; the weakest, then the nearest.
pub fn ready_alternative_target(ai: &mut ModernAi, f: &Field, current: usize, policy: &AiPolicy) -> Option<usize> {
    let now = progress(f);
    let mut best: Option<(usize, f32, i32)> = None;
    for l in 0..f.sites().sites.len() {
        if l == current || l == f.w.here || f.sites().is_city(l) {
            continue;
        }
        if deferrals::is_deferred(ai, f.day(), l, f.sites().owner(l), &now) {
            continue;
        }
        if !is_hostile(f.sites(), l, f.player())
            || !is_territorial_frontier_target(f, l)
            || (f.party.len() as i32) < required_attack_group_size(ai, f, l, policy)
            || !is_target_allowed(ai, f, l, policy)
            || !is_suitable(ai, f, l, policy)
        {
            continue;
        }
        let Some(r) = f.route(f.w.here, l).filter(|r| supplies::has_route_supplies(f.group, r, true, 0, 0)) else { continue };
        let strength = estimate(ai, f, l).estimated_strength;
        if best.is_none_or(|b| strength < b.1 || strength == b.1 && r.days < b.2) {
            best = Some((l, strength, r.days));
        }
    }
    best.map(|b| b.0)
}

// --- the plan's state --------------------------------------------------

/// `StrategicTarget = null`: no target, no deadline, not a settlement.
pub fn clear_target(ai: &mut ModernAi) {
    ai.target = None;
    ai.attack_plan_until = None;
    ai.target_was_neutral = false;
}

/// `SetSettlementTarget`: a neutral target (none clears it); at Reststop
/// only when a bible can be brought along (`PrepareReststopSettlement`,
/// the world's to do). False when it cannot.
pub fn set_settlement_target(ai: &mut ModernAi, target: Option<usize>, prepare_reststop: &mut dyn FnMut() -> bool) -> bool {
    if target == Some(super::camps::RESTSTOP) && !prepare_reststop() {
        return false;
    }
    ai.target = target;
    ai.attack_plan_until = None;
    ai.target_was_neutral = target.is_some();
    true
}

/// `StartAttackPlan`: the target, hostile, with `attack_plan_turns` to get
/// ready — unless that plan already runs.
pub fn start_attack_plan(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) {
    if ai.target == Some(target) && ai.attack_plan_until.is_some() {
        return;
    }
    ai.target = Some(target);
    ai.target_was_neutral = false;
    ai.attack_plan_until = Some(f.day() + policy.attack_plan_turns);
    if ai.telemetry.is_on() {
        // `Estimate` and `RequiredAttackGroupSize` only for the line; both are
        // reads (`intel::estimate` takes the state by reference).
        let defense = estimate(ai, f, target);
        let size = required_attack_group_size(ai, f, target, policy);
        let n = defense.expected_defenders;
        ai.telemetry.report(|| {
            alloc::format!(
                "started attack plan for {}: {} about {n} defender{}, preparing {size} attackers with {} days available",
                super::telemetry::place(target),
                if defense.based_on_contact { "observed" } else { "inferred" },
                if n == 1 { "" } else { "s" },
                policy.attack_plan_turns
            )
        });
    }
}

/// `MarkAttackPlanReady`: the deadline goes.
pub fn mark_attack_plan_ready(ai: &mut ModernAi, target: usize) {
    if ai.target == Some(target) && ai.attack_plan_until.is_some() {
        ai.attack_plan_until = None;
        ai.telemetry.report(|| alloc::format!("attack group for {} is ready; preparation deadline removed", super::telemetry::place(target)));
    }
}

/// `IsAttackPlanExpired`.
pub fn attack_plan_expired(ai: &ModernAi, day: i32) -> bool {
    ai.attack_plan_until.is_some_and(|u| day > u)
}

/// `DeferAttackPlan`: put aside for `attack_plan_retry_delay`, the target
/// and the attack-recruitment city dropped.
pub fn defer_attack_plan(ai: &mut ModernAi, f: &Field, target: usize, policy: &AiPolicy) {
    ai.attack_recruitment_city = None;
    let now = progress(f);
    deferrals::defer_for_turns(ai, f.day(), target, f.sites().owner(target), policy.attack_plan_retry_delay, &now);
    clear_target(ai);
}

/// `RecordFailedAttack`: remembered for `failed_attack_memory_turns`, or
/// `progressing_attack_retry_turns` when damage was done.
pub fn record_failed_attack(ai: &mut ModernAi, day: i32, camp: usize, group: i32, attackers: f32, defenders: f32, policy: &AiPolicy, progress: bool) {
    let turns = if progress { policy.progressing_attack_retry_turns } else { policy.failed_attack_memory_turns };
    ai.failed_attack = Some(FailedAttack { camp, until_day: day + turns, group_size: group, attacker_strength: attackers, defender_strength: defenders });
    let at = super::telemetry::place(camp);
    ai.telemetry.report(|| {
        if progress {
            alloc::format!("dealt damage at {at}; a retry must establish whether that damage survives recovery")
        } else {
            alloc::format!("will reconsider {at} only after recruiting, re-equipping, or weakening its defenders")
        }
    });
}

/// `ClassicAiState.RecordAttack`: one of this player's people at `place`
/// was struck by `attacker`'s (`Character.ResolveSingleAttack` and its
/// kin — a local fight; the strategic fight does not call it). The place is
/// remembered as contested; a human attacker is retaliated against for
/// `retaliation_turns`.
pub fn record_attack(ai: &mut ModernAi, me: usize, day: i32, attacker: Option<usize>, attacker_human: bool, place: usize, policy: &AiPolicy) {
    let Some(attacker) = attacker.filter(|&a| a != me) else { return };
    ai.contested = Some((place, day + policy.contested_camp_memory_turns));
    if !attacker_human {
        return;
    }
    ai.retaliating = Some((attacker, day + policy.retaliation_turns));
    ai.telemetry.report(|| alloc::format!("will retaliate against {} after the attack at {}", super::telemetry::player(attacker), super::telemetry::place(place)));
}

/// `MarkRecentlyCaptured`.
pub fn mark_recently_captured(ai: &mut ModernAi, day: i32, camp: usize, policy: &AiPolicy) {
    ai.contested = Some((camp, day + policy.contested_camp_memory_turns));
    if ai.failed_attack.as_ref().is_some_and(|fa| fa.camp == camp) {
        ai.failed_attack = None;
    }
    ai.post_capture_pause = ai.post_capture_pause.max(policy.attack_cooldown_turns);
}

/// `ValidatePersistentTarget`: drop a plan no longer on the frontier,
/// without a weapon or no longer allowed; switch to a ready target when
/// this one is not; mark ready; drop it when its time ran out. The target
/// kept, if any.
pub fn validate_persistent_target(ai: &mut ModernAi, f: &Field, target: usize, route: &Route, policy: &AiPolicy) -> Option<usize> {
    let t = super::telemetry::place(target);
    let why = if !is_territorial_frontier_target(f, target) {
        Some("released attack plan for {t}: viable neutral territory still blocks the frontier")
    } else if !has_group_weapon(f) {
        Some("released attack plan for {t}: waiting for an opportunity weapon")
    } else if !is_target_allowed(ai, f, target, policy) {
        Some("abandoned attack plan for {t}: target is no longer permitted")
    } else {
        None
    };
    if let Some(why) = why {
        ai.telemetry.report(|| why.replace("{t}", &t));
        clear_target(ai);
        return None;
    }
    let size = required_attack_group_size(ai, f, target, policy);
    let ready = f.party.len() as i32 >= size && is_suitable(ai, f, target, policy) && supplies::has_route_supplies(f.group, route, true, 0, 0);
    if !ready {
        if let Some(alt) = ready_alternative_target(ai, f, target, policy) {
            ai.telemetry.report(|| alloc::format!("switched attack plan from {t} to ready target {}", super::telemetry::place(alt)));
            start_attack_plan(ai, f, alt, policy);
            mark_attack_plan_ready(ai, alt);
            return Some(alt);
        }
    }
    if ready {
        mark_attack_plan_ready(ai, target);
    }
    if attack_plan_expired(ai, f.day()) {
        ai.telemetry.report(|| alloc::format!("abandoned attack plan for {t}: preparation time expired"));
        defer_attack_plan(ai, f, target, policy);
        return None;
    }
    Some(target)
}

/// `AiTurnController.FindNearestLogistics`: the nearest own camp other than
/// here — reachable on supplies, when asked.
pub fn nearest_logistics(f: &Field, require_reachable: bool) -> Option<usize> {
    (0..f.sites().sites.len())
        .filter(|&l| !f.sites().is_city(l) && f.sites().owner(l) == Some(f.player()) && l != f.w.here)
        .filter_map(|l| f.route(f.w.here, l).map(|r| (l, r)))
        .filter(|(_, r)| !require_reachable || supplies::has_route_supplies(f.group, r, false, 0, 0))
        .min_by_key(|(l, r)| (r.days, *l))
        .map(|(l, _)| l)
}

/// `TryAddImmediateResponse`: standing in a hostile camp, attack it when
/// suitable; otherwise give up a plan on it and retreat — to own supplied
/// logistics, any own camp, or back where the group came from — or wait.
pub fn try_add_immediate_response(ai: &mut ModernAi, f: &Field, policy: &AiPolicy, out: &mut Vec<Decision>) -> bool {
    let here = f.w.here;
    if !is_hostile(f.sites(), here, f.player()) {
        return false;
    }
    let living = f.defenders_here.iter().filter(|x| !x.dead()).count() as i32;
    let strength: f32 = f.defenders_here.iter().filter(|x| !x.dead()).map(|x| fighter_strength(&f.rules, x)).sum();
    intel::observe_encounter(ai, f.day(), here, f.sites().owner(here), living, strength);
    if is_suitable(ai, f, here, policy) {
        out.push(Decision::new(Action::AttackHostile, 1200.0, Some(here), Reason::HostileCampBlocksRoute));
        return true;
    }
    if ai.target == Some(here) && is_hostile(f.sites(), here, f.player()) {
        ai.telemetry.report(|| alloc::format!("abandoned attack plan for {}: observed defense is unsuitable", super::telemetry::place(here)));
        defer_attack_plan(ai, f, here, policy);
    }
    let supplied = nearest_logistics(f, true);
    let owned = supplied.or_else(|| nearest_logistics(f, false));
    let previous = f.previous.filter(|&p| !is_hostile(f.sites(), p, f.player()) && can_travel(f, here, p));
    match owned.or(previous) {
        Some(retreat) => {
            let reason = if owned.is_none() {
                Reason::RetreatToPrevious { place: retreat }
            } else if supplied.is_some() {
                Reason::RetreatToSafety
            } else {
                Reason::EmergencyRetreat
            };
            let mut d = Decision::new(Action::Travel, 1250.0, Some(retreat), reason);
            d.next_step = f.route(here, retreat).map(|r| r.next_step);
            out.push(d);
        }
        None => out.push(Decision::new(Action::Wait, 1250.0, Some(here), Reason::NoRetreatForUnsuitableAttack)),
    }
    true
}
