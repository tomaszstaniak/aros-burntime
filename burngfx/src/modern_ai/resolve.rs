//! A fight for a camp and what follows it: `CombatResolver.Resolve` and
//! `LastChanceCombat`. At tag `v1.2`, `AI/PlayerBehavior/Combat/`.
//!
//! Done on the world itself ([`Seat`]): wounds and ammunition written back,
//! the dead buried with their things on the ground, loot taken up, the
//! camp freed and a guard left in it, or the group sent back toward its
//! own camps. `AI telemetry` lines are not written (see the audit).

use alloc::vec::Vec;

use crate::spawn::{Dropped, Rng};

use super::attack::{self, Field};
use super::encounter::{fight, Defence as Fight, MAX_ROUNDS};
use super::group::Items;
use super::intel;
use super::needs::{Needs, Plans, World};
use super::policy::AiPolicy;
use super::route::{self, RouteMap};
use super::seat::{Seat, View};

/// What a fight came to, for the controller and the tests.
#[derive(Clone, Debug, PartialEq)]
pub enum Result {
    /// Nothing to fight: the target was dropped.
    NoFight,
    /// The defenders are gone; `guard` is the companion left in the camp,
    /// if one could be.
    Captured { guard: Option<usize>, rounds: usize },
    /// The attack failed; the group retreated toward `retreat`, if it could.
    Failed { retreat: Option<usize>, rounds: usize, withdrew: bool },
    /// The last chance failed: the leader is dead.
    Died { rounds: usize },
}

fn world<'a>(seat: &'a Seat, v: &'a View, policy: &'a AiPolicy) -> World<'a> {
    World {
        items: Items { types: seat.types, prices: seat.prices },
        productions: seat.productions,
        sites: &v.sites,
        player: seat.me(),
        policy,
        day: seat.day,
        here: seat.here(),
        equipment_demand: &v.demand,
        traders: &[],
    }
}

fn field<'a>(seat: &'a Seat, w: &'a World<'a>, v: &'a View) -> Field<'a> {
    Field { w, rules: seat.rules(), party: &v.party, group: &v.group, previous: seat.rival.previous, humans: &v.humans, defenders_here: &v.defenders_here, travelling: seat.rival.journey.is_some() }
}

/// `SelectDefenderLoot`: equipment, the best weapon first, then defence,
/// then worth; as many as the policy allows.
fn defender_loot(seat: &Seat, drops: &[usize], limit: i32) -> Vec<usize> {
    let items = seat.items();
    let t = seat.types;
    let mut loot: Vec<usize> = drops.iter().copied().filter(|&g| super::equipment::is_equipment(t, seat.ground[g].kind)).collect();
    loot.sort_by(|&a, &b| {
        let (ka, kb) = (seat.ground[a].kind, seat.ground[b].kind);
        t.weapon_priority_of(kb)
            .cmp(&t.weapon_priority_of(ka))
            .then(t.defense_of(kb).cmp(&t.defense_of(ka)))
            .then(items.value(kb).partial_cmp(&items.value(ka)).unwrap_or(core::cmp::Ordering::Equal))
    });
    loot.truncate(limit.max(0) as usize);
    loot
}

/// `CollectCombatLoot` → `CollectGroundItems`: what was dropped here, the
/// food first (the most filling first), then the dearest — into the
/// reserve, the construction reserve, a carrier, in place of cargo worth
/// less, or the camp's rooms when it is ours.
pub fn collect_ground_items(seat: &mut Seat, policy: &AiPolicy, extended: bool, dropped: &[usize]) {
    if !seat.can_collect_local_loot() {
        return;
    }
    let here = seat.here();
    let mut order: Vec<usize> = dropped.iter().copied().filter(|&g| seat.ground.get(g).is_some_and(|d| d.location == here)).collect();
    {
        let items = seat.items();
        let t = seat.types;
        order.sort_by(|&a, &b| {
            let (ka, kb) = (seat.ground[a].kind, seat.ground[b].kind);
            (t.food_of(kb) > 0)
                .cmp(&(t.food_of(ka) > 0))
                .then(t.food_of(kb).cmp(&t.food_of(ka)))
                .then(items.value(kb).partial_cmp(&items.value(ka)).unwrap_or(core::cmp::Ordering::Equal))
        });
    }
    let mut taken: Vec<usize> = Vec::new();
    for g in order {
        let Dropped { kind, rounds, .. } = seat.ground[g].clone();
        let items = Items { types: seat.types, prices: seat.prices };
        if super::equipment::reserve_insert(seat.ai, items, kind) {
            taken.push(g);
            continue;
        }
        let material = seat.types.name_of(kind).is_some_and(|n| super::production::CONSTRUCTION_MATERIALS.contains(&n));
        if material && seat.ai.reserve_count(kind) < 1 && seat.ai.reserve_add(kind) {
            taken.push(g);
            continue;
        }
        if let Some(m) = super::trading::cargo_carrier(seat.rival, items, kind) {
            if super::equipment::give(super::group::pack_mut(seat.rival, m), kind, rounds).is_some() {
                taken.push(g);
                continue;
            }
        }
        let swap = {
            let v = seat.view(policy, extended);
            let w = world(seat, &v, policy);
            let needs = Needs::new(seat.rival, seat.ai, &w, Plans::default());
            let cx = super::trading::Context { w: &w, needs: &needs, r: seat.rival, leader_kit: &seat.ai.leader_kit, ai: seat.ai, barter: super::trading::Barter { factor: seat.book.barter_factor(seat.difficulty), benefit: policy.trade_benefit } };
            super::trading::replace_cargo(&cx, seat.ai, kind)
        };
        if let Some((m, s)) = swap {
            let out = if m == 0 {
                let mut kit = seat.ai.leader_kit;
                let got = super::equipment::take(&mut seat.rival.pack, &mut kit, s);
                seat.ai.leader_kit = kit;
                got
            } else {
                let c = &mut seat.rival.party.companions[m - 1];
                super::equipment::take(&mut c.pack, &mut c.kit, s)
            };
            if let Some((old, old_rounds)) = out {
                super::equipment::give(super::group::pack_mut(seat.rival, m), kind, rounds);
                let (was, now) = (alloc::string::String::from(seat.types.name_of(old).unwrap_or("")), alloc::string::String::from(seat.types.name_of(kind).unwrap_or("")));
                seat.ai.telemetry.report(|| alloc::format!("replaced cargo {was} with higher-value ground find {now}"));
                let at = seat.ground[g].at;
                seat.ground.push(Dropped { location: here, at, kind: old, rounds: old_rounds, icon: crate::spawn::PILE_UNDRAWN });
                taken.push(g);
                continue;
            }
        }
        let home = seat.camps.get(here).and_then(|c| c.owner) == Some(seat.me());
        if home && seat.store_item_in_camp(here, kind, rounds, true) {
            taken.push(g);
        }
    }
    taken.sort_unstable_by(|a, b| b.cmp(a));
    for g in taken {
        seat.ground.remove(g);
    }
}


/// `CombatResolver.Resolve` where the group stands.
pub fn resolve(seat: &mut Seat, policy: &AiPolicy, extended: bool, fight_to_death: bool, rng: &mut Rng) -> Result {
    let here = seat.here();
    let me = seat.me();
    let owner = seat.camps.get(here).and_then(|c| c.owner);
    let city = seat.gam.locations.get(here).is_some_and(|l| l.is_city);
    if owner.is_none() || owner == Some(me) || city {
        attack::clear_target(seat.ai);
        return Result::NoFight;
    }
    let owner = owner.unwrap_or(0);
    let rules = seat.rules();
    let mut attackers = seat.party_fighters();
    let defence = seat.defence_here();
    let mut defenders = defence.fighters.clone();
    let health_before: Vec<i32> = defenders.iter().map(|f| f.health).collect();
    let attacking = attack::capture(&rules, &attackers, attackers.first());
    let defending = attack::capture(&rules, &defenders, None);
    let attacker_strength: f32 = attackers.iter().filter(|x| !x.dead()).map(|x| attack::fighter_strength(&rules, x)).sum();
    let defender_strength: f32 = defenders.iter().map(|x| rules.strength(x, policy.use_detailed_combat_estimate)).sum();
    let observed: f32 = defenders.iter().filter(|x| !x.dead()).map(|x| attack::fighter_strength(&rules, x)).sum();
    intel::observe_encounter(seat.ai, seat.day, here, Some(owner), defenders.iter().filter(|x| !x.dead()).count() as i32, observed);
    let original_attackers = attackers.len() as i32;
    let at = super::telemetry::place(here);
    let (n_att, n_def) = (attackers.len(), defenders.len());
    seat.ai.telemetry.report(|| alloc::format!("attacks {}'s camp at {at}: {n_att} attackers against {n_def} defenders", super::telemetry::player(owner)));
    // Names, before the fight's dead are buried.
    let defender_names: Vec<alloc::string::String> = defence.who.iter().map(|d| seat.defender_name(*d)).collect();
    let attacker_names: Vec<alloc::string::String> = (0..attackers.len()).map(|i| if i == 0 { super::telemetry::player(me) } else { seat.companion_name(i - 1) }).collect();
    let attackers_before: Vec<i32> = attackers.iter().map(|f| f.health).collect();

    let outcome = fight(&rules, &mut attackers, Fight { fighters: &mut defenders, visiting: &defence.visiting, owner: defence.owner, lifted: &defence.lifted }, fight_to_death, rng);
    if outcome.defending_party_disengaged {
        seat.ai.telemetry.report_as(owner, || alloc::format!("disengaged defending party at {at} to protect the boss; party stays at the location"));
    }
    for (i, d) in defenders.iter().enumerate().filter(|(_, d)| d.dead()) {
        let _ = d;
        let n = defender_names[i].clone();
        seat.ai.telemetry.report(|| alloc::format!("defeated defender {n} at {at}"));
    }
    for (i, _) in attackers.iter().enumerate().skip(1).filter(|(_, a)| a.dead()) {
        let n = attacker_names[i].clone();
        seat.ai.telemetry.report(|| alloc::format!("lost follower {n} in the attack on {at}"));
    }
    let worn = |f: &super::encounter::Fighter| f.kit.protection.and_then(|s| f.pack.get(s));
    let carried = |f: &super::encounter::Fighter| f.pack.iter().filter(|&k| seat.types.get(k).is_some_and(|t| t.is_class("protection"))).collect::<Vec<usize>>();
    let mut record: Vec<super::seat::Worn> = attackers.iter().zip(&attackers_before).map(|(f, &b)| super::seat::Worn { attacker: true, before: b, after: f.health, worn: worn(f), carried: carried(f) }).collect();
    record.extend(defenders.iter().zip(&health_before).map(|(f, &b)| super::seat::Worn { attacker: false, before: b, after: f.health, worn: worn(f), carried: carried(f) }));
    seat.fights.push(super::seat::Fought { place: here, hazard: rules.hazard, fighters: record });
    // Who went down, named for the burial before anybody is moved.
    let order: Vec<crate::burial::Key> = outcome
        .fallen
        .iter()
        .filter_map(|&(attacker, i)| {
            if attacker {
                return match i {
                    0 => Some(crate::burial::Key::Leader(me)),
                    _ => seat.rival.party.companions.get(i - 1).map(|c| crate::burial::Key::Member { player: me, id: c.id }),
                };
            }
            match *defence.who.get(i)? {
                super::seat::Defender::Camper(r) => seat.camps.get(here).and_then(|c| c.residents.get(r)).map(|c| crate::burial::Key::Camper { id: c.id }),
                super::seat::Defender::HumanLeader => seat.human_here.as_ref().map(|h| crate::burial::Key::Leader(h.seat)),
                super::seat::Defender::HumanCompanion(c) => seat.human_here.as_ref().and_then(|h| h.party.companions.get(c).map(|x| crate::burial::Key::Member { player: h.seat, id: x.id })),
                super::seat::Defender::Leader(k) => seat.others.get(k).map(|r| crate::burial::Key::Leader(r.player)),
                super::seat::Defender::Companion(k, c) => seat.others.get(k).and_then(|r| r.party.companions.get(c).map(|x| crate::burial::Key::Member { player: r.player, id: x.id })),
            }
        })
        .collect();
    seat.write_party(&attackers);
    let mut written = defence;
    written.fighters = defenders.clone();
    seat.write_defence(&written);
    let fallen = seat.bury_in(order);
    collect_ground_items(seat, policy, extended, &fallen.own_drops);

    let surviving: Vec<usize> = (0..defenders.len())
        .filter(|&i| !defenders[i].dead() && !(outcome.defending_party_disengaged && written.visiting[i]))
        .collect();
    let defeated = surviving.is_empty();
    let damage: i32 = (0..defenders.len()).map(|i| (health_before[i] - defenders[i].health.max(0)).max(0)).sum();
    let killed = defenders.iter().filter(|x| x.dead()).count() as i32;
    let retry = intel::record_attempt(seat.ai, here, owner, attacking, defending, killed, defeated);
    seat.ai.telemetry.report(|| {
        alloc::format!(
            "combat outcome at {at}: {damage} damage this encounter, {killed} defenders killed; {}; {}",
            retry.comparison,
            if retry.blocked { "further attempts deferred until forces change" } else { "retry remains subject to readiness" }
        )
    });
    let surviving_strength: f32 = surviving.iter().map(|&i| attack::fighter_strength(&rules, &defenders[i])).sum();
    intel::observe_encounter(seat.ai, seat.day, here, Some(owner), surviving.len() as i32, surviving_strength);
    let _ = defending;

    if defeated {
        let loot = defender_loot(seat, &fallen.other_drops, policy.combat_loot_limit);
        collect_ground_items(seat, policy, extended, &loot);
        seat.ai.last_chance_target = None;
        // The weakest living follower stays (`OrderBy(Fighter)`): the first
        // of equals.
        let fighters = seat.party_fighters();
        let guard = (1..fighters.len())
            .filter(|&i| !fighters[i].dead())
            .fold(None, |b: Option<(usize, f32)>, i| {
                let s = attack::fighter_strength(&rules, &fighters[i]);
                match b {
                    Some((_, bs)) if s >= bs => b,
                    _ => Some((i, s)),
                }
            })
            .map(|b| b.0 - 1);
        seat.free_here();
        match guard {
            Some(g) => {
                let name = seat.companion_name(g);
                super::upkeep::create_camp(seat, policy, extended, g);
                attack::mark_recently_captured(seat.ai, seat.day, here, policy);
                seat.ai.telemetry.report(|| alloc::format!("captured {at} and stationed {name}"));
            }
            None => {
                attack::clear_target(seat.ai);
                seat.ai.telemetry.report(|| alloc::format!("won at {at}, but it remains neutral without a surviving follower"));
            }
        }
        return Result::Captured { guard, rounds: outcome.rounds };
    }
    attack::clear_target(seat.ai);
    if !fight_to_death {
        seat.ai.last_chance_target = None;
    }
    let progress = killed > 0 || damage > 0;
    attack::record_failed_attack(seat.ai, seat.day, here, original_attackers, attacker_strength, defender_strength, policy, progress);
    if fight_to_death {
        if seat.rival.needs.health > 0 {
            leader_dies(seat);
        }
        seat.ai.last_chance_target = None;
        return Result::Died { rounds: outcome.rounds };
    }
    let safe = {
        let v = seat.view(policy, extended);
        let w = world(seat, &v, policy);
        let f = field(seat, &w, &v);
        attack::nearest_logistics(&f, true).map(|l| (l, route::find(&v.sites, me, seat.rival.previous, here, l).map(|r| r.next_step)))
    };
    let step = safe.and_then(|(_, s)| s).or(seat.rival.previous);
    let retreat = step.filter(|&s| seat.can_travel(here, s));
    if let Some(s) = retreat {
        seat.travel(s);
        let toward = super::telemetry::place(safe.map_or(s, |(l, _)| l));
        let via = super::telemetry::place(s);
        let remain = surviving.len();
        let withdrew = outcome.attacker_withdrew && progress;
        seat.ai.telemetry.report(|| {
            if withdrew {
                alloc::format!("withdrew from {at} after dealing {damage} damage; {remain} defenders remain, toward {toward} via {via}")
            } else {
                alloc::format!("retreated from {at} toward {toward} via {via} before risking the leader")
            }
        });
    } else {
        seat.ai.telemetry.report(|| alloc::format!("failed to defeat {at}'s defenders; leader survived"));
    }
    Result::Failed { retreat, rounds: outcome.rounds, withdrew: outcome.attacker_withdrew }
}

/// `TerritorialEscape`.
#[derive(Clone, Debug, PartialEq)]
pub struct Escape {
    pub found: bool,
    pub blocking: Vec<usize>,
}

/// `FindTerritorialEscape`: from here, through all that is not hostile, to
/// an own camp, or a free place that could be claimed (here, only if a camp
/// could be manned now); the hostile camps met on the way block it.
pub fn territorial_escape(seat: &Seat, v: &View, policy: &AiPolicy) -> Escape {
    let w = world(seat, v, policy);
    let f = field(seat, &w, v);
    let me = seat.me();
    let here = seat.here();
    let n = v.sites.locations();
    let mut visited = alloc::vec![false; n];
    visited[here] = true;
    let mut blocking: Vec<usize> = Vec::new();
    let mut queue = alloc::collections::VecDeque::from([here]);
    let kit = seat.ai.leader_kit;
    let th = super::expansion::Theatre {
        f: &f,
        r: seat.rival,
        kit: &kit,
        people: &v.people,
        camp_rooms: &v.camp_rooms,
        rivals_at: &v.rivals_at,
        dead_players: &v.dead_players,
        travelling_to: None,
        neutral_expansion_allowed: true,
        critical_supplies: false,
    };
    while let Some(at) = queue.pop_front() {
        let s = &v.sites.sites[at];
        let escape = if s.owner == Some(me) {
            true
        } else if s.owner.is_some() || s.city || !s.can_sustain_camp(w.items) || !super::claims::can_claim(seat.rival, seat.ai, &w, &v.group, at) {
            false
        } else {
            at != here || th.can_station_camp(seat.ai)
        };
        if escape {
            return Escape { found: true, blocking };
        }
        for (nb, d) in v.sites.ways(at) {
            if d <= 0 || nb >= n {
                continue;
            }
            if at == here && !seat.can_travel(here, nb) {
                continue;
            }
            if attack::is_hostile(&v.sites, nb, me) {
                if !blocking.contains(&nb) {
                    blocking.push(nb);
                }
                continue;
            }
            if !visited[nb] {
                visited[nb] = true;
                queue.push_back(nb);
            }
        }
    }
    Escape { found: false, blocking }
}

fn assault(seat: &mut Seat, policy: &AiPolicy, extended: bool, target: usize, rng: &mut Rng) -> Result {
    let here = seat.here();
    let t = super::telemetry::place(target);
    if here == target {
        seat.ai.telemetry.report(|| alloc::format!("last-chance assault on {t}: committing every survivor until victory or death"));
        return resolve(seat, policy, extended, true, rng);
    }
    let step = {
        let v = seat.view(policy, extended);
        route::find(&v.sites, seat.me(), seat.rival.previous, here, target).map(|r| r.next_step)
    };
    match step.filter(|&s| seat.can_travel(here, s)) {
        Some(s) => {
            super::upkeep::prepare_water_for_departure(seat, policy, extended);
            seat.travel(s);
            let via = super::telemetry::place(s);
            seat.ai.telemetry.report(|| alloc::format!("last-chance assault advances on weakest reachable camp {t} via {via}"));
            Result::Failed { retreat: None, rounds: 0, withdrew: false }
        }
        None => {
            leader_dies(seat);
            seat.ai.telemetry.report(|| alloc::format!("could not advance the last-chance assault on {t} and dies"));
            Result::Died { rounds: 0 }
        }
    }
}

/// `LastChanceCombat.TryExecute`: a group trapped by hostile camps with no
/// way to an own or claimable place attacks the weakest of them, to the
/// death — or, on easy (`die_when_trapped`), its leader dies. Whether it
/// acted (the turn then ends), and how.
pub fn last_chance(seat: &mut Seat, policy: &AiPolicy, extended: bool, rng: &mut Rng) -> Option<Result> {
    let me = seat.me();
    if let Some(t) = seat.ai.last_chance_target {
        let hostile = seat.camps.get(t).and_then(|c| c.owner).is_some_and(|o| o != me) && !seat.gam.locations.get(t).is_some_and(|l| l.is_city);
        if hostile {
            return Some(assault(seat, policy, extended, t, rng));
        }
    }
    seat.ai.last_chance_target = None;
    let v = seat.view(policy, extended);
    let escape = territorial_escape(seat, &v, policy);
    if escape.found || escape.blocking.is_empty() {
        return None;
    }
    let here = super::telemetry::place(seat.here());
    if policy.die_when_trapped {
        leader_dies(seat);
        seat.ai.telemetry.report(|| alloc::format!("was trapped at {here} with no viable destination and dies on easy"));
        return Some(Result::Died { rounds: 0 });
    }
    let target = {
        let w = world(seat, &v, policy);
        let f = field(seat, &w, &v);
        let mut best: Option<(usize, f32, i32)> = None;
        let mut ai = seat.ai.clone();
        for &b in &escape.blocking {
            let Some(r) = route::find(&v.sites, me, seat.rival.previous, seat.here(), b) else { continue };
            let strength = attack::estimate(&mut ai, &f, b).estimated_strength;
            if best.is_none_or(|x| strength < x.1 || strength == x.1 && r.days < x.2) {
                best = Some((b, strength, r.days));
            }
        }
        // `Estimate` refreshes the knowledge as it goes.
        let knowledge = ai.knowledge;
        seat.ai.knowledge = knowledge;
        best.map(|b| b.0)
    };
    seat.ai.last_chance_target = target;
    match target {
        None => {
            leader_dies(seat);
            seat.ai.telemetry.report(|| alloc::format!("was trapped at {here} with no reachable hostile camp and dies"));
            Some(Result::Died { rounds: 0 })
        }
        Some(t) => Some(assault(seat, policy, extended, t, rng)),
    }
}

/// The fight's own limit, for the controller's checks.
pub const FIGHT_ROUNDS: usize = MAX_ROUNDS;

/// The leader's `Health = 0`: v1.2's setter runs `Die` there and then
/// (`Character.cs:197-200, 454-461`), so what he carried is on the ground
/// at his `Position` at once, after whatever the turn's blows brought down.
fn leader_dies(seat: &mut Seat) {
    seat.rival.needs.health = 0;
    let me = seat.me();
    seat.bury_in(alloc::vec![crate::burial::Key::Leader(me)]);
}
