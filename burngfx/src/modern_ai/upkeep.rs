//! Looking after the opponent's camps and group on the world itself:
//! `CampManagement`, `WeaponLoadout`, `GroupManagement` and the item
//! handling of `ClassicAiState` (`ManageLocalItems`, `HuntOneDogForMeat`,
//! the follower moves). At tag `v1.2`.
//!
//! Each function reads a fresh picture ([`Seat::snap`]) where upstream reads
//! the live world, decides as upstream does, and changes the world through
//! the seat.

use alloc::string::String;
use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::table_combat as tc;

use super::equipment::{give, is_firearm, take};
use super::group::{self, Items};
use super::policy::AiPolicy;
use super::seat::{Seat, Snap, Who};
use super::state::ModernAi;
use super::trading::Context;

// --- the reserve's own picks (`AiItemPool`) -------------------------------

pub(super) const WEAPON_ORDER: [&str; 5] = ["item_loaded_rifle", "item_loaded_pistol", "item_pitchfork", "item_axe", "item_knife"];
const WATER_CONTAINERS: [&str; 6] = ["item_full_wineskin", "item_empty_wineskin", "item_full_canteen", "item_empty_canteen", "item_water_bottle", "item_bottle"];

fn reserve_first(ai: &ModernAi, items: Items, names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| items.types.index_of(n).filter(|&k| ai.reserve_count(k) > 0))
}

/// `FindBestWeaponPoolItem`: rifle, pistol, pitchfork, axe, knife, the first
/// doing more than `minimum`, a tool only if allowed, a firearm only if
/// `allowed` says so (or never, with no rule).
pub(super) fn best_pool_weapon(ai: &ModernAi, items: Items, minimum: i32, allow_tool: bool, productions: &crate::production::Productions, allowed: Option<&dyn Fn(usize) -> bool>) -> Option<usize> {
    let t = items.types;
    WEAPON_ORDER.iter().find_map(|n| {
        let k = t.index_of(n).filter(|&k| ai.reserve_count(k) > 0)?;
        let tool = t.name_of(k).and_then(|n| productions.worked_by(n)).is_some();
        let ok = t.damage_of(k) > minimum && (allow_tool || !tool) && allowed.map_or(!is_firearm(t, k), |a| a(k));
        ok.then_some(k)
    })
}

/// `GetBestWaterContainer`: the largest, full first.
pub(super) fn reserve_best_water_container(ai: &ModernAi, items: Items) -> Option<usize> {
    reserve_first(ai, items, &WATER_CONTAINERS)
}

/// `TakeLeastWaterContainer`: the smallest, then the emptiest.
fn reserve_least_water_container(ai: &ModernAi, items: Items) -> Option<usize> {
    ai.reserve
        .iter()
        .filter(|&&(k, c)| c > 0 && items.is_water_container(k))
        .fold(None, |b: Option<usize>, &(k, _)| match b {
            Some(b) if (items.water_capacity(k), items.types.water_of(k)) >= (items.water_capacity(b), items.types.water_of(b)) => Some(b),
            _ => Some(k),
        })
}

/// `ShouldReserveProductionTool`: with free land to take, keep the last
/// portable tool — unless advanced traps cover under half the camps while
/// one waits in the reserve.
pub fn should_reserve_production_tool(seat: &Seat, snap: &Snap) -> bool {
    let items = seat.items();
    let sites = &snap.v.sites;
    if !sites.sites.iter().any(|s| !s.city && s.owner.is_none()) {
        return false;
    }
    if super::support::advanced_trap_coverage(items, sites, seat.me()) < 0.5 && super::support::has_pooled_advanced_trap(items, seat.ai) {
        return false;
    }
    let tool = |k: usize| items.types.name_of(k).and_then(|n| seat.productions.worked_by(n)).is_some();
    let pooled: i32 = seat.ai.reserve.iter().filter(|&&(k, _)| tool(k)).map(|&(_, c)| c as i32).sum();
    let carried = group::carried(seat.rival).iter().filter(|&&(_, _, k)| tool(k)).count() as i32;
    pooled + carried <= 1
}

/// A trade context over a picture, for `CanSell` and the cargo values.
pub fn with_context<R>(seat: &Seat, snap: &Snap, policy: &AiPolicy, f: impl FnOnce(&Context) -> R) -> R {
    let t = seat.tables();
    let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
    let cx = Context { w: &w, needs: &snap.needs, r: seat.rival, leader_kit: &seat.ai.leader_kit, ai: seat.ai, barter: snap.barter };
    f(&cx)
}

// --- weapons (`WeaponLoadout`) -------------------------------------------

fn best_weapon_slot(seat: &Seat, who: Who) -> Option<usize> {
    seat.pack_of(who).and_then(|p| tc::find_best_weapon(p, None, seat.types))
}

/// `WeaponAllowed`: a firearm only within the policy's limit for the unit.
fn weapon_allowed(seat: &Seat, unit: &[Who], recipient: Who, kind: usize, policy: &AiPolicy) -> bool {
    let t = seat.types;
    if !is_firearm(t, kind) {
        return true;
    }
    if super::equipment::loaded_shots(t, kind, None) <= 0 {
        return false;
    }
    policy.firearm_limit < 0
        || (unit.iter().filter(|&&w| w != recipient && seat.pack_of(w).is_some_and(|p| p.iter().any(|k| is_firearm(t, k)))).count() as i32) < policy.firearm_limit
}

/// `RefreshWeapons`.
pub fn refresh_weapons(seat: &mut Seat, unit: &[Who]) {
    let types = seat.types;
    for &w in unit {
        seat.with_pack(w, |p, kit| kit.weapon = tc::find_best_weapon(p, kit.weapon, types));
    }
}

/// `EquipWeapon`: a weapon from the reserve better than what is carried
/// (any, when unarmed; a stronger one when upgrading), making room by
/// returning the old weapon or dropping the least worth keeping.
pub fn equip_weapon(seat: &mut Seat, policy: &AiPolicy, extended: bool, who: Who, unit: &[Who], upgrade_weak: bool, maximum_priority: i32) {
    let t = seat.types;
    let current = best_weapon_slot(seat, who);
    let current_damage = current.and_then(|s| seat.pack_of(who)?.get(s)).map_or(0, |k| t.damage_of(k));
    let minimum = if upgrade_weak {
        current_damage
    } else if current_damage > 0 {
        i32::MAX
    } else {
        0
    };
    let snap = seat.snap(policy, extended);
    let reserve_tool = should_reserve_production_tool(seat, &snap);
    let allow_tool = current_damage == 0 || !reserve_tool;
    let pick = {
        let allowed = |k: usize| t.weapon_priority_of(k) <= maximum_priority && weapon_allowed(seat, unit, who, k, policy);
        if minimum == i32::MAX { None } else { best_pool_weapon(seat.ai, seat.items(), minimum, allow_tool, seat.productions, Some(&allowed)) }
    };
    let Some(weapon) = pick else {
        if let Some(c) = current {
            seat.with_pack(who, |_, kit| kit.weapon = Some(c));
        }
        return;
    };
    seat.ai.reserve_take(weapon);
    let at = seat.location_of(who);
    let full = seat.pack_of(who).is_some_and(|p| p.is_full());
    if full {
        if let Some(c) = current {
            if let Some(Some((old, rounds))) = seat.with_pack(who, |p, kit| take(p, kit, c)) {
                seat.store_in_reserve_or_at(at, old, rounds);
            }
        } else {
            let replaceable = with_context(seat, &snap, policy, |cx| {
                let pack = seat.pack_of(who)?;
                let member = match who {
                    Who::Leader => Some(0),
                    Who::Follower(i) => Some(i + 1),
                    Who::Guard(..) => None,
                };
                (0..pack.len())
                    .filter(|&s| member.is_some_and(|m| super::trading::can_sell(cx, m, s)))
                    .filter_map(|s| pack.get(s).map(|k| (s, k)))
                    .fold(None, |b: Option<(usize, f32, f32)>, (s, k)| {
                        let keep = super::trading::cargo_retention_value(cx, &snap.v.sites, seat.ai, k);
                        let value = seat.items().value(k);
                        match b {
                            Some((_, bk, bv)) if (keep, value) >= (bk, bv) => b,
                            _ => Some((s, keep, value)),
                        }
                    })
                    .map(|b| b.0)
            });
            match replaceable {
                Some(s) => {
                    if let Some(Some((old, rounds))) = seat.with_pack(who, |p, kit| take(p, kit, s)) {
                        let entry = seat.gam.locations.get(seat.here()).map_or(crate::game::Point::new(0, 0), |l| l.entry_point);
                        seat.ground.push(crate::spawn::Dropped { location: seat.here(), at: entry, kind: old, rounds, icon: crate::spawn::PILE_UNDRAWN });
                        let (id, name) = (String::from(seat.types.name_of(old).unwrap_or("")), seat.who_name(who));
                        seat.ai.telemetry.report(|| alloc::format!("dropped lower-value cargo {id} so {name} can carry a weapon"));
                    }
                }
                None => {
                    seat.store_in_reserve_or_at(at, weapon, None);
                    return;
                }
            }
        }
    }
    match seat.with_pack(who, |p, kit| {
        let slot = give(p, weapon, None);
        if slot.is_some() {
            kit.weapon = slot;
        }
        slot
    }) {
        Some(Some(_)) => {
            let (role, stationed) = match who {
                Who::Leader => ("leader", String::new()),
                Who::Follower(_) => ("follower", String::new()),
                Who::Guard(l, _) => ("frontier guard", alloc::format!(" at {}", super::telemetry::place(l))),
            };
            let (id, name) = (String::from(seat.types.name_of(weapon).unwrap_or("")), seat.who_name(who));
            seat.ai.telemetry.report(|| alloc::format!("equipped {role} {name}{stationed} with {id}"));
        }
        _ => seat.store_in_reserve_or_at(at, weapon, None),
    }
}

/// `EquipStoredCampWeapon`: a guard takes the best stored melee weapon that
/// beats his own, never the camp's working tool; his old one goes back in
/// its place. The kind taken.
pub fn equip_stored_camp_weapon(seat: &mut Seat, guard: Who, allowed: &dyn Fn(&Seat, usize) -> bool) -> Option<usize> {
    let Who::Guard(l, _) = guard else { return None };
    let t = seat.types;
    let current = best_weapon_slot(seat, guard);
    if seat.pack_of(guard).is_some_and(|p| p.is_full()) && current.is_none() {
        return None;
    }
    let current_kind = current.and_then(|s| seat.pack_of(guard)?.get(s));
    let current_priority = current_kind.map_or(0, |k| t.weapon_priority_of(k));
    let production = seat.camps.get(l).and_then(|c| c.production);
    let tool_of = |k: usize| t.name_of(k).and_then(|n| seat.productions.worked_by(n));
    let mut best: Option<(usize, usize, i32)> = None;
    for room in 0..seat.room_count(l) {
        let Some(bag) = seat.rooms.get(l, room) else { continue };
        for slot in 0..bag.len() {
            let Some(k) = bag.get(slot) else { continue };
            if !(t.damage_of(k) > 0 && !is_firearm(t, k)) || !allowed(seat, k) || t.weapon_priority_of(k) <= current_priority {
                continue;
            }
            let p = tool_of(k);
            let spare = p.is_none()
                || p != production
                || current_kind.is_some_and(|c| tool_of(c) == p)
                || p.is_some_and(|p| {
                    let installed = seat.sites().sites[l].tool_count(p);
                    let max = seat.productions.get(p).map_or(0, |x| x.max_combination as i32);
                    installed > max
                });
            if spare && best.is_none_or(|b| t.weapon_priority_of(k) > b.2) {
                best = Some((room, slot, t.weapon_priority_of(k)));
            }
        }
    }
    let (room, slot, _) = best?;
    let rounds = seat.rooms.get(l, room)?.rounds_left(slot);
    let kind = seat.rooms.get_mut(l, room)?.remove(slot)?;
    if let Some(c) = current {
        if let Some(Some((old, old_rounds))) = seat.with_pack(guard, |p, kit| take(p, kit, c)) {
            if let Some(b) = seat.rooms.get_mut(l, room) {
                give(b, old, old_rounds);
            }
        }
    }
    seat.with_pack(guard, |p, kit| kit.weapon = give(p, kind, rounds));
    Some(kind)
}

// --- the travelling group (`GroupManagement`) ----------------------------

/// `NormalizeCarriedProtection`: one piece of hazard protection each — the
/// chosen one, else the best — the rest to the reserve or the place.
pub fn normalize_carried_protection(seat: &mut Seat, unit: &[Who]) {
    let items = seat.items();
    for &who in unit {
        let Some(pack) = seat.pack_of(who) else { continue };
        let chosen = seat.kit_of(who).and_then(|k| k.protection);
        let mut slots: Vec<usize> = (0..pack.len()).filter(|&s| pack.get(s).is_some_and(|k| items.is_hazard_protection(k))).collect();
        slots.sort_by(|&a, &b| {
            (Some(b) == chosen)
                .cmp(&(Some(a) == chosen))
                .then(items.types.defense_of(pack.get(b).unwrap_or(0)).cmp(&items.types.defense_of(pack.get(a).unwrap_or(0))))
        });
        let Some(&keep) = slots.first() else { continue };
        let keep_kind = pack.get(keep);
        // Reported in upstream's order (`protection.Skip(1)`); taken out
        // from the highest slot down, so that the slots stay valid.
        let excess: Vec<String> = slots[1..].iter().filter_map(|&s| pack.get(s)).map(|k| String::from(items.types.name_of(k).unwrap_or(""))).collect();
        let mut extra: Vec<usize> = slots[1..].to_vec();
        extra.sort_unstable_by(|a, b| b.cmp(a));
        let at = seat.location_of(who);
        for s in extra {
            if let Some(Some((k, rounds))) = seat.with_pack(who, |p, kit| take(p, kit, s)) {
                seat.store_in_reserve_or_at(at, k, rounds);
            }
        }
        let name = seat.who_name(who);
        for id in excess {
            seat.ai.telemetry.report(|| alloc::format!("redistributed excess hazard protection {id} from {name}"));
        }
        seat.with_pack(who, |p, kit| kit.protection = (0..p.len()).find(|&s| p.get(s) == keep_kind));
    }
}

/// `CarryStandingWaterContainers`: while the group carries less container
/// room than it wants, the member carrying least takes one — from the
/// reserve, else from the camp here when it is ours.
pub fn carry_standing_water_containers(seat: &mut Seat) {
    loop {
        let items = seat.items();
        let carried: i32 = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum();
        let desired = super::needs::desired_water_container_capacity(group::size(seat.rival) as i32);
        if carried >= desired {
            return;
        }
        let carrier = seat
            .group()
            .into_iter()
            .filter(|&w| seat.pack_of(w).is_some_and(|p| !p.is_full()))
            .min_by_key(|&w| seat.pack_of(w).map_or(0, |p| p.iter().filter(|&k| items.is_water_container(k)).map(|k| items.water_capacity(k)).sum::<i32>()));
        let Some(carrier) = carrier else { return };
        let container = if let Some(k) = reserve_best_water_container(seat.ai, items) {
            seat.ai.reserve_take(k);
            Some((k, None))
        } else if seat.camps.get(seat.here()).and_then(|c| c.owner) == Some(seat.me()) {
            take_stored_water_container(seat, seat.here(), false)
        } else {
            None
        };
        let Some((k, rounds)) = container else { return };
        seat.with_pack(carrier, |p, _| give(p, k, rounds));
        let (id, name) = (String::from(items.types.name_of(k).unwrap_or("")), seat.who_name(carrier));
        seat.ai.telemetry.report(|| alloc::format!("added standing water reserve {id} to {name}"));
    }
}

/// `TakeStoredWaterContainer` (`by_full_capacity` false: the largest, then
/// the fullest) or `TakeBestStoredWaterContainer` (true: by what it holds
/// full, then what it holds).
fn take_stored_water_container(seat: &mut Seat, l: usize, by_full_capacity: bool) -> Option<(usize, Option<i32>)> {
    let items = seat.items();
    let t = seat.types;
    let mut best: Option<(usize, usize, i32, i32)> = None;
    for room in 0..seat.room_count(l) {
        let Some(bag) = seat.rooms.get(l, room) else { continue };
        for slot in 0..bag.len() {
            let Some(k) = bag.get(slot).filter(|&k| items.is_water_container(k)) else { continue };
            let first = if by_full_capacity {
                if t.water_of(k) > 0 { t.water_of(k) } else { t.full_of(k).map_or(0, |f| t.water_of(f)) }
            } else {
                items.water_capacity(k)
            };
            if best.is_none_or(|b| (first, t.water_of(k)) > (b.2, b.3)) {
                best = Some((room, slot, first, t.water_of(k)));
            }
        }
    }
    let (room, slot, _, _) = best?;
    let bag = seat.rooms.get_mut(l, room)?;
    let rounds = bag.rounds_left(slot);
    bag.remove(slot).map(|k| (k, rounds))
}

/// `CarryStrategicProtection`: as many pieces of hazard protection as the
/// reserve quota (at most one each), from the reserve, the best general one.
pub fn carry_strategic_protection(seat: &mut Seat, snap: &Snap, policy: &AiPolicy) {
    let items = seat.items();
    let desired = (group::size(seat.rival) as i32).min(super::needs::desired_protection_reserve(&snap.v.sites, seat.me(), policy));
    loop {
        let carried = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_hazard_protection(k)).count() as i32;
        let pooled = seat.ai.reserve.iter().any(|&(k, c)| c > 0 && items.is_hazard_protection(k));
        if carried >= desired || !pooled {
            return;
        }
        let carrier = seat.group().into_iter().find(|&w| seat.pack_of(w).is_some_and(|p| !p.is_full() && !p.iter().any(|k| items.is_hazard_protection(k))));
        let Some(carrier) = carrier else { return };
        let Some(k) = reserve_first(seat.ai, items, &["item_protection_suit", "item_gas_mask", "item_paper_helmet"]) else { return };
        seat.ai.reserve_take(k);
        seat.with_pack(carrier, |p, kit| kit.protection = give(p, k, None));
        let (id, name) = (String::from(items.types.name_of(k).unwrap_or("")), seat.who_name(carrier));
        seat.ai.telemetry.report(|| alloc::format!("carried {id} on {name} as strategic hazard protection"));
    }
}

/// `CanWithdrawRearWeapon`: not the camp's only working tool, unless the
/// camp could do without it.
fn can_withdraw_rear_weapon(seat: &Seat, policy: &AiPolicy, snap: &Snap, l: usize, kind: usize) -> bool {
    let t = seat.types;
    let p = t.name_of(kind).and_then(|n| seat.productions.worked_by(n));
    let production = seat.camps.get(l).and_then(|c| c.production);
    if p.is_none() || p != production || p.is_some_and(|p| snap.v.sites.sites[l].tool_count(p) > 1) {
        return true;
    }
    let tt = seat.tables();
    let w = snap.world(&tt, policy, seat.me(), seat.day, seat.here());
    super::claims::can_bootstrap_camp(&w, &snap.v.group, l)
}

/// `TransferRearCampWeapons`: unarmed travellers take a stored weapon (the
/// weakest that will do) or a rear guard's, through the reserve — at a
/// threatened camp only when the group is wholly unarmed and stalled.
pub fn transfer_rear_camp_weapons(seat: &mut Seat, policy: &AiPolicy, extended: bool, group_unit: &[Who]) {
    let here = seat.here();
    let t = seat.types;
    let unarmed = |seat: &Seat, w: Who| best_weapon_slot(seat, w).and_then(|s| seat.pack_of(w)?.get(s)).map_or(0, |k| t.damage_of(k)) == 0;
    let travellers: Vec<Who> = group_unit.iter().copied().filter(|&w| unarmed(seat, w)).collect();
    let all_unarmed = !travellers.is_empty() && group_unit.iter().all(|&w| unarmed(seat, w));
    let snap = seat.snap(policy, extended);
    let threatened = snap.v.sites.sites[here].threatened || seat.ai.was_recently_contested(here, seat.day);
    let camps = snap.v.sites.sites.iter().filter(|s| s.owner == Some(seat.me())).count() as i32;
    let emergency = threatened && all_unarmed && super::support::strategically_stalled(seat.ai, here, camps, seat.day, 12);
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) || threatened && !emergency {
        return;
    }
    for traveller in travellers {
        let snap = seat.snap(policy, extended);
        let mut stored: Option<(usize, usize, i32)> = None;
        for room in 0..seat.room_count(here) {
            let Some(bag) = seat.rooms.get(here, room) else { continue };
            for slot in 0..bag.len() {
                let Some(k) = bag.get(slot) else { continue };
                if t.damage_of(k) > 0 && !is_firearm(t, k) && can_withdraw_rear_weapon(seat, policy, &snap, here, k) && weapon_allowed(seat, group_unit, traveller, k, policy) && stored.is_none_or(|b| t.damage_of(k) < b.2) {
                    stored = Some((room, slot, t.damage_of(k)));
                }
            }
        }
        if let Some((room, slot, _)) = stored {
            let kind = seat.rooms.get(here, room).and_then(|b| b.get(slot)).unwrap_or(0);
            let items = seat.items();
            if super::equipment::reserve_insert(seat.ai, items, kind) {
                if let Some(b) = seat.rooms.get_mut(here, room) {
                    b.remove(slot);
                }
                let id = String::from(t.name_of(kind).unwrap_or(""));
                seat.ai.telemetry.report(|| alloc::format!("withdrew reserve {id} from {} for the travelling group", super::telemetry::place(here)));
                equip_weapon(seat, policy, extended, traveller, group_unit, false, i32::MAX);
            }
            continue;
        }
        let guard = seat
            .guards(here)
            .into_iter()
            .filter_map(|g| {
                let s = best_weapon_slot(seat, g)?;
                let k = seat.pack_of(g)?.get(s)?;
                (!is_firearm(t, k) && can_withdraw_rear_weapon(seat, policy, &snap, here, k) && weapon_allowed(seat, group_unit, traveller, k, policy)).then_some((g, s, k))
            })
            .min_by_key(|&(_, _, k)| t.damage_of(k));
        let Some((g, s, k)) = guard else { return };
        let items = seat.items();
        if !super::equipment::reserve_accepts(t, k, items) || seat.ai.reserve_count(k) >= super::state::RESERVE_CAP {
            continue;
        }
        seat.with_pack(g, |p, kit| take(p, kit, s));
        seat.ai.reserve_add(k);
        let (id, name, at) = (String::from(t.name_of(k).unwrap_or("")), seat.who_name(g), super::telemetry::place(here));
        seat.ai.telemetry.report(|| {
            if threatened {
                alloc::format!("borrowed {id} from guard {name} at threatened {at} to unblock the travelling group")
            } else {
                alloc::format!("transferred {id} from rear guard {name} at {at} to the travelling group")
            }
        });
        equip_weapon(seat, policy, extended, traveller, group_unit, false, i32::MAX);
    }
}

/// `MaintainGroupEquipment`.
pub fn maintain_group_equipment(seat: &mut Seat, policy: &AiPolicy, extended: bool, allow_camp_transfers: bool) {
    let unit = seat.group();
    refresh_weapons(seat, &unit);
    normalize_carried_protection(seat, &unit);
    for &w in &unit {
        equip_weapon(seat, policy, extended, w, &unit, false, i32::MAX);
    }
    if allow_camp_transfers && seat.can_collect_local_loot() {
        transfer_rear_camp_weapons(seat, policy, extended, &unit);
    }
    let attack_plan = seat.ai.target.is_some_and(|t| seat.camps.get(t).and_then(|c| c.owner).is_some_and(|o| o != seat.me()) && !seat.gam.locations[t].is_city);
    if attack_plan || seat.ai.difficulty > 0 {
        let unit = seat.group();
        for &w in &unit {
            equip_weapon(seat, policy, extended, w, &unit, true, i32::MAX);
        }
    }
    carry_standing_water_containers(seat);
    let snap = seat.snap(policy, extended);
    carry_strategic_protection(seat, &snap, policy);
}

/// `ProvisionGroupFromCampSurplus`: the group eats, the least filling first
/// (never the settler's payment), from its packs and — when the camp may be
/// looted and holds food — the camp's rooms, while it has room.
pub fn provision_from_camp_surplus(seat: &mut Seat, policy: &AiPolicy, extended: bool, l: usize) {
    let reserved = {
        let snap = seat.snap(policy, extended);
        snap.needs.planned_settlement_payment
    };
    let loot = seat.can_collect_local_loot();
    let t = seat.types;
    let (mut consumed_items, mut consumed_food) = (0, 0);
    loop {
        if !(0..group::size(seat.rival)).any(|m| {
            let n = group::needs(seat.rival, m);
            n.food < n.max_food
        }) {
            break;
        }
        let items = seat.items();
        let stored_food = (0..seat.room_count(l)).filter_map(|r| seat.rooms.get(l, r)).flat_map(|b| b.iter().collect::<Vec<_>>()).filter(|&k| t.food_of(k) > 0).count();
        // (holder, slot, kind): holder None is a room, by index.
        let mut candidates: Vec<(Option<usize>, usize, usize, usize)> = group::carried(seat.rival)
            .into_iter()
            .filter(|&(_, _, k)| t.food_of(k) > 0 && Some(k) != reserved)
            .map(|(m, s, k)| (Some(m), 0, s, k))
            .collect();
        if loot && stored_food > super::group::CAMP_FOOD_ITEM_RESERVE {
            for r in 0..seat.room_count(l) {
                let Some(bag) = seat.rooms.get(l, r) else { continue };
                for s in 0..bag.len() {
                    if let Some(k) = bag.get(s).filter(|&k| t.food_of(k) > 0 && Some(k) != reserved) {
                        candidates.push((None, r, s, k));
                    }
                }
            }
        }
        let best = candidates.iter().copied().enumerate().min_by(|(ia, a), (ib, b)| {
            (t.food_of(a.3), items.value(a.3))
                .partial_cmp(&(t.food_of(b.3), items.value(b.3)))
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(ia.cmp(ib))
        });
        let Some((_, (holder, room, slot, kind))) = best else { break };
        let capacity: i32 = (0..group::size(seat.rival)).map(|m| group::needs(seat.rival, m).max_food - group::needs(seat.rival, m).food).sum();
        if t.food_of(kind) > capacity {
            break;
        }
        crate::game::group_eat(&mut seat.rival.needs, &mut seat.rival.party.companions, t.food_of(kind));
        consumed_items += 1;
        consumed_food += t.food_of(kind);
        match holder {
            Some(0) => {
                let mut kit = seat.ai.leader_kit;
                take(&mut seat.rival.pack, &mut kit, slot);
                seat.ai.leader_kit = kit;
            }
            Some(m) => {
                let c = &mut seat.rival.party.companions[m - 1];
                take(&mut c.pack, &mut c.kit, slot);
            }
            None => {
                if let Some(b) = seat.rooms.get_mut(l, room) {
                    b.remove(slot);
                }
            }
        }
    }
    if consumed_items > 0 {
        seat.ai.telemetry.report(|| {
            alloc::format!("provisioned group at {} before collecting exports ({consumed_items} food items, {consumed_food} food value)", super::telemetry::place(l))
        });
    }
}

/// `PrepareCampWaterReservesForDeparture`: at an own camp, empty carried
/// containers are swapped for the camp's full ones (largest for fullest),
/// then the group drinks what it must.
pub fn prepare_water_for_departure(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) {
        return;
    }
    let items = seat.items();
    let t = seat.types;
    let mut empties: Vec<(Who, usize, usize)> = Vec::new();
    for w in seat.group() {
        if let Some(p) = seat.pack_of(w) {
            for s in 0..p.len() {
                if let Some(k) = p.get(s).filter(|&k| t.water_of(k) == 0 && items.is_water_container(k)) {
                    empties.push((w, s, k));
                }
            }
        }
    }
    empties.sort_by_key(|&(_, _, k)| core::cmp::Reverse(items.water_capacity(k)));
    let mut stored: Vec<(usize, usize, usize)> = Vec::new();
    for r in 0..seat.room_count(here) {
        let Some(bag) = seat.rooms.get(here, r) else { continue };
        for s in 0..bag.len() {
            if let Some(k) = bag.get(s).filter(|&k| t.water_of(k) > 0 && items.is_water_container(k)) {
                stored.push((r, s, k));
            }
        }
    }
    stored.sort_by_key(|&(_, _, k)| core::cmp::Reverse(t.water_of(k)));
    let n = empties.len().min(stored.len());
    if n == 0 {
        return;
    }
    // Swap in place: the slots stay where they are.
    let withdrawn: Vec<String> = stored[..n].iter().map(|&(_, _, k)| String::from(t.name_of(k).unwrap_or(""))).collect();
    for i in 0..n {
        let (w, s, empty) = empties[i];
        let (r, rs, full) = stored[i];
        seat.with_pack(w, |p, _| {
            p.replace(s, full);
        });
        if let Some(b) = seat.rooms.get_mut(here, r) {
            b.replace(rs, empty);
        }
    }
    seat.ai.telemetry.report(|| alloc::format!("exchanged empty containers for camp water reserves at {}: {}", super::telemetry::place(here), withdrawn.join(", ")));
    let reserved = seat.snap(policy, extended).needs.planned_settlement_payment;
    let items = Items { types: seat.types, prices: seat.prices };
    if super::group::consume_available_supplies(seat.rival, items, reserved, None) {
        seat.ai.telemetry.report(|| "consumed camp water reserves before departure".into());
    }
}

// --- the camps (`CampManagement`) ----------------------------------------

/// `CampWeaponReserve`.
pub const CAMP_WEAPON_RESERVE: usize = 2;

/// Every kind in a camp's rooms, `(room, slot, kind)`.
fn room_items(seat: &Seat, l: usize) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for r in 0..seat.room_count(l) {
        if let Some(b) = seat.rooms.get(l, r) {
            for s in 0..b.len() {
                if let Some(k) = b.get(s) {
                    out.push((r, s, k));
                }
            }
        }
    }
    out
}

fn is_melee(seat: &Seat, k: usize) -> bool {
    seat.types.damage_of(k) > 0 && !is_firearm(seat.types, k)
}

/// `CampStoredWeaponReserveItems`: the two strongest melee weapons in the
/// rooms, as `(room, slot)`.
fn stored_weapon_reserve(seat: &Seat, l: usize) -> Vec<(usize, usize)> {
    let mut w: Vec<(usize, usize, usize)> = room_items(seat, l).into_iter().filter(|&(_, _, k)| is_melee(seat, k)).collect();
    w.sort_by_key(|&(_, _, k)| core::cmp::Reverse(seat.types.damage_of(k)));
    w.into_iter().take(CAMP_WEAPON_RESERVE).map(|(r, s, _)| (r, s)).collect()
}

/// `CampStoredWaterReserveItems`: at a camp short of water, every stored
/// container stays.
fn stored_water_reserve(seat: &Seat, snap: &Snap, l: usize) -> Vec<(usize, usize)> {
    let site = &snap.v.sites.sites[l];
    let low = site.water_surplus() < super::camps::TRAVEL_SUPPLY_SURPLUS_TARGET;
    if !low && !site.needs_reusable_water_reserve(2) {
        return Vec::new();
    }
    let items = seat.items();
    room_items(seat, l).into_iter().filter(|&(_, _, k)| items.is_water_container(k)).map(|(r, s, _)| (r, s)).collect()
}

/// Remove from rooms by `(room, slot)`, the highest slot of a room first.
fn remove_from_rooms(seat: &mut Seat, l: usize, mut at: Vec<(usize, usize)>) -> Vec<(usize, Option<i32>)> {
    at.sort_unstable_by(|a, b| b.cmp(a));
    let mut out = Vec::new();
    for (r, s) in at {
        if let Some(b) = seat.rooms.get_mut(l, r) {
            let rounds = b.rounds_left(s);
            if let Some(k) = b.remove(s) {
                out.push((k, rounds));
            }
        }
    }
    out
}

/// `EnsureAiProductionStorage`: room kept for six of the produce, the
/// cheapest other stored things (not food, not the produce or its tools)
/// thrown away to make it.
pub fn ensure_production_storage(seat: &mut Seat, l: usize) -> usize {
    let Some(p) = seat.camps.get(l).and_then(|c| c.production) else { return 0 };
    if seat.camps.get(l).and_then(|c| c.owner) != Some(seat.me()) {
        return 0;
    }
    let Some(production) = seat.productions.get(p).cloned() else { return 0 };
    let produce = seat.types.index_of(&production.produce);
    let items = seat.items();
    let produced = room_items(seat, l).iter().filter(|&&(_, _, k)| Some(k) == produce).count();
    let required = crate::game::MAX_STOCK_FOOD.saturating_sub(produced);
    let mut free: usize = (0..seat.room_count(l)).map(|r| seat.room_capacity(l, r).saturating_sub(seat.rooms.get(l, r).map_or(0, |b| b.len()))).sum();
    if free >= required {
        return 0;
    }
    let mut removable: Vec<(usize, usize, usize)> = room_items(seat, l)
        .into_iter()
        .filter(|&(_, _, k)| seat.types.food_of(k) == 0 && Some(k) != produce && !seat.types.name_of(k).is_some_and(|n| production.worked_by(n)))
        .collect();
    removable.sort_by(|a, b| {
        items.value(a.2)
            .partial_cmp(&items.value(b.2))
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(seat.types.food_of(a.2).cmp(&seat.types.food_of(b.2)))
            .then(seat.types.name_of(a.2).cmp(&seat.types.name_of(b.2)))
    });
    let mut gone = Vec::new();
    for (r, s, _) in removable {
        if free >= required {
            break;
        }
        gone.push((r, s));
        free += 1;
    }
    let n = gone.len();
    remove_from_rooms(seat, l, gone);
    if n > 0 {
        let produce = production.produce.clone();
        seat.ai.telemetry.report(|| {
            alloc::format!("discarded {n} low-value stored items at {} to reserve {} slots for {produce} production", super::telemetry::place(l), crate::game::MAX_STOCK_FOOD)
        });
    }
    n
}

/// `MakeRoomForProductionChange`: with every room full, throw away the
/// cheapest thing that is not a tool of the current production.
fn make_room_for_production_change(seat: &mut Seat, l: usize, next: Option<usize>) -> bool {
    if (0..seat.room_count(l)).any(|r| seat.room_has_space(l, r)) {
        return true;
    }
    let current = seat.camps.get(l).and_then(|c| c.production);
    if seat.camps.get(l).and_then(|c| c.owner) != Some(seat.me()) || next.is_none() || next == current {
        return false;
    }
    let items = seat.items();
    let tool_of_current = |k: usize| current.is_some_and(|p| seat.types.name_of(k).and_then(|n| seat.productions.worked_by(n)) == Some(p));
    let pick = room_items(seat, l).into_iter().filter(|&(_, _, k)| !tool_of_current(k)).min_by(|a, b| {
        items.value(a.2)
            .partial_cmp(&items.value(b.2))
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(seat.types.food_of(a.2).cmp(&seat.types.food_of(b.2)))
            .then(seat.types.name_of(a.2).cmp(&seat.types.name_of(b.2)))
    });
    match pick {
        Some((r, s, k)) => {
            remove_from_rooms(seat, l, alloc::vec![(r, s)]);
            let id = String::from(seat.types.name_of(k).unwrap_or(""));
            let produce = next.and_then(|p| seat.productions.get(p)).map_or_else(String::new, |p| p.produce.clone());
            seat.ai.telemetry.report(|| alloc::format!("discarded low-value {id} at {} to install {produce} production", super::telemetry::place(l)));
            true
        }
        None => false,
    }
}

/// `CollectRedundantProductionTools`: tools beyond what each production
/// wants into the reserve, from the rooms (not the camp's weapon reserve)
/// and then from guards where the production counts their packs.
pub fn collect_redundant_tools(seat: &mut Seat, snap: &Snap, l: usize) {
    let items = seat.items();
    let excess = super::production::redundant_tools(items, &snap.v.sites, seat.me(), l);
    let reserved = stored_weapon_reserve(seat, l);
    let mut collected: Vec<usize> = Vec::new();
    for (i, n) in excess {
        let Some(sp) = snap.v.sites.sites[l].productions.get(i) else { continue };
        let production = sp.production.clone();
        let works = |k: usize| seat.types.name_of(k).is_some_and(|name| production.worked_by(name));
        /// A candidate: a room's slot, or a guard's.
        enum From {
            Room(usize, usize),
            Guard(Who, usize),
        }
        // The rooms' tools (not the weapon reserve), then — where the
        // production counts packs — the guards' (not the weapon in hand),
        // the first `excess` of them together (`Concat`, `Take`).
        let mut candidates: Vec<(From, usize)> = room_items(seat, l).into_iter().filter(|&(r, s, k)| works(k) && !reserved.contains(&(r, s))).map(|(r, s, k)| (From::Room(r, s), k)).collect();
        if production.allow_inventory {
            for g in seat.guards(l) {
                let weapon = seat.kit_of(g).and_then(|k| k.weapon);
                if let Some(p) = seat.pack_of(g) {
                    for s in 0..p.len() {
                        if let Some(k) = p.get(s).filter(|&k| Some(s) != weapon && works(k)) {
                            candidates.push((From::Guard(g, s), k));
                        }
                    }
                }
            }
        }
        candidates.truncate(n.max(0) as usize);
        let mut from_rooms: Vec<(usize, usize)> = Vec::new();
        let mut from_guards: Vec<(Who, usize)> = Vec::new();
        for (from, k) in candidates {
            if !super::equipment::reserve_insert(seat.ai, items, k) {
                continue;
            }
            collected.push(k);
            match from {
                From::Room(r, s) => from_rooms.push((r, s)),
                From::Guard(g, s) => from_guards.push((g, s)),
            }
        }
        remove_from_rooms(seat, l, from_rooms);
        // Highest slot first within a pack, so the others stay valid.
        from_guards.sort_by(|a, b| b.1.cmp(&a.1));
        for (g, s) in from_guards {
            seat.with_pack(g, |p, kit| take(p, kit, s));
        }
    }
    if !collected.is_empty() {
        let list = super::telemetry::grouped_ids(items.types, &collected);
        seat.ai.telemetry.report(|| alloc::format!("reclaimed redundant production tools from {}: {list}", super::telemetry::place(l)));
    }
}

/// `InstallProductionFromPool`: the best reserve tool for a production the
/// camp wants more of, installed — unless the last tool is to be kept.
pub fn install_production_from_pool(seat: &mut Seat, policy: &AiPolicy, snap: &Snap, l: usize) {
    if should_reserve_production_tool(seat, snap) {
        return;
    }
    let items = seat.items();
    let me = seat.me();
    let site = &snap.v.sites.sites[l];
    let mut wanted: Vec<(String, f32)> = Vec::new();
    for (i, p) in site.productions.iter().enumerate() {
        let name = items.types.name_of(p.produce).unwrap_or("");
        if super::production::needs_production_result(items, &snap.v.sites, me, l, name) {
            wanted.push((String::from(name), super::production::trade_priority(items, site, i)));
        }
    }
    wanted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
    let products: Vec<&str> = wanted.iter().map(|(n, _)| n.as_str()).collect();
    let t = seat.tables();
    let w = snap.world(&t, policy, me, seat.day, seat.here());
    if products.is_empty() || !super::claims::reserve_has_trap(seat.ai, &w, &products) {
        return;
    }
    let tool = ["item_trap", "item_snake_trap", "item_rat_trap", "item_knife"]
        .iter()
        .find_map(|n| items.types.index_of(n).filter(|&k| seat.ai.reserve_count(k) > 0 && w.produce_of(k).is_some_and(|p| products.contains(&p))));
    let Some(tool) = tool else { return };
    seat.ai.reserve_take(tool);
    let production = items.types.name_of(tool).and_then(|n| seat.productions.worked_by(n));
    make_room_for_production_change(seat, l, production);
    if !seat.store_item_in_camp(l, tool, None, true) {
        super::equipment::reserve_insert(seat.ai, items, tool);
        return;
    }
    let id = String::from(items.types.name_of(tool).unwrap_or(""));
    seat.ai.telemetry.report(|| alloc::format!("installed {id} for food production at {}", super::telemetry::place(l)));
}

/// `StockCurrentCampWeaponReserve`: two melee weapons kept in the camp
/// here, from the reserve.
pub fn stock_weapon_reserve(seat: &mut Seat) {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) {
        return;
    }
    loop {
        let stored = room_items(seat, here).iter().filter(|&&(_, _, k)| is_melee(seat, k)).count();
        let has_space = (0..seat.room_count(here)).any(|r| seat.room_has_space(here, r));
        if stored >= CAMP_WEAPON_RESERVE || !has_space {
            return;
        }
        let items = seat.items();
        let Some(w) = best_pool_weapon(seat.ai, items, 0, true, seat.productions, Some(&|k| !is_firearm(items.types, k))) else { return };
        seat.ai.reserve_take(w);
        if !seat.store_item_in_camp(here, w, None, true) {
            super::equipment::reserve_insert(seat.ai, items, w);
            return;
        }
        let id = String::from(items.types.name_of(w).unwrap_or(""));
        seat.ai.telemetry.report(|| alloc::format!("stored reserve weapon {id} at {}", super::telemetry::place(here)));
    }
}

/// `EquipDangerProtection`: a guard of a dangerous camp with nothing against
/// it gets a gas mask or a suit from the reserve.
pub fn equip_danger_protection(seat: &mut Seat, guard: Who, danger: Danger) {
    let items = seat.items();
    let protected = seat.pack_of(guard).is_some_and(|p| p.iter().any(|k| items.types.get(k).is_some_and(|t| t.protection.iter().any(|(d, _)| *d == danger))));
    if protected || seat.pack_of(guard).is_none_or(|p| p.is_full()) {
        return;
    }
    let names: &[&str] = if danger == Danger::Radiation { &["item_protection_suit", "item_paper_helmet"] } else { &["item_gas_mask", "item_paper_helmet", "item_protection_suit"] };
    let Some(k) = reserve_first(seat.ai, items, names) else { return };
    seat.ai.reserve_take(k);
    seat.with_pack(guard, |p, kit| kit.protection = give(p, k, None));
    let (name, id) = (seat.who_name(guard), String::from(items.types.name_of(k).unwrap_or("")));
    let l = seat.location_of(guard);
    let kind = if danger == Danger::Radiation { "radiation" } else { "gas" };
    seat.ai.telemetry.report(|| alloc::format!("equipped {name} at {} against {kind} with {id}", super::telemetry::place(l)));
}

/// `StockCampWaterContainers` (the network's): the most water-starved camps
/// first, a container each until they have one per person and one more.
fn stock_camp_water_containers(seat: &mut Seat, snap: &Snap, camps: &[usize]) {
    let items = seat.items();
    let mut order: Vec<usize> = camps.to_vec();
    order.sort_by_key(|&l| (!snap.v.sites.sites[l].travel_water_bottleneck(items, 2), snap.v.sites.sites[l].water));
    for l in order {
        loop {
            let held = room_items(seat, l).iter().filter(|&&(_, _, k)| items.is_water_container(k)).count() as i32;
            let desired = 1 + snap.v.sites.sites[l].guards.iter().filter(|g| g.alive).count() as i32;
            if held >= desired || reserve_best_water_container(seat.ai, items).is_none() {
                break;
            }
            if !(0..seat.room_count(l)).any(|r| seat.room_has_space(l, r)) {
                break;
            }
            let Some(k) = reserve_best_water_container(seat.ai, items) else { break };
            seat.ai.reserve_take(k);
            if !seat.store_item_in_camp(l, k, None, true) {
                super::equipment::reserve_insert(seat.ai, items, k);
                break;
            }
            let id = String::from(items.types.name_of(k).unwrap_or(""));
            seat.ai.telemetry.report(|| alloc::format!("stocked {id} as water reserve at {}", super::telemetry::place(l)));
        }
    }
}

/// `RedistributeCampProductionTools`: twice over the camps, preferred and
/// least productive first, redundant tools in, the reserve's out; then
/// each camp's production chosen and its storage kept.
fn redistribute_production_tools(seat: &mut Seat, policy: &AiPolicy, extended: bool, camps: &[usize]) {
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let mut order: Vec<usize> = camps.to_vec();
    order.sort_by_key(|&l| (!super::production::prefer_production(&snap.v.sites.sites[l]), snap.v.sites.sites[l].food_per_day(items)));
    for _ in 0..2 {
        for &l in &order {
            let snap = seat.snap(policy, extended);
            collect_redundant_tools(seat, &snap, l);
        }
        for &l in &order {
            let snap = seat.snap(policy, extended);
            install_production_from_pool(seat, policy, &snap, l);
        }
    }
    for &l in &order {
        seat.auto_select_production(l);
        ensure_production_storage(seat, l);
    }
}

/// `MaterializeLegacyRestingSustenance`: a bible from the reserve into a
/// camp (here first, never Reststop) or a member's pack.
fn materialize_resting_sustenance(seat: &mut Seat, camps: &[usize]) {
    let items = seat.items();
    let Some(k) = seat.ai.reserve.iter().find(|&&(k, c)| c > 0 && super::needs::Needs::resting_sustenance(items, k)).map(|&(k, _)| k) else { return };
    let room = |seat: &Seat, l: usize| (0..seat.room_count(l)).any(|r| seat.room_has_space(l, r));
    let storage = camps
        .iter()
        .copied()
        .find(|&l| l == seat.here() && l != super::camps::RESTSTOP && room(seat, l))
        .or_else(|| camps.iter().copied().find(|&l| l != super::camps::RESTSTOP && room(seat, l)));
    let carrier = if storage.is_none() { seat.group().into_iter().find(|&w| seat.pack_of(w).is_some_and(|p| !p.is_full())) } else { None };
    if storage.is_none() && carrier.is_none() {
        return;
    }
    seat.ai.reserve_take(k);
    match (storage, carrier) {
        (Some(l), _) => {
            seat.store_item_in_camp(l, k, None, false);
        }
        (None, Some(w)) => {
            seat.with_pack(w, |p, _| give(p, k, None));
        }
        _ => {}
    }
}

/// `MaintainCampNetwork`.
pub fn maintain_camp_network(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let me = seat.me();
    let camps: Vec<usize> = (0..seat.gam.locations.len()).filter(|&l| seat.camps.get(l).and_then(|c| c.owner) == Some(me)).collect();
    for &l in &camps {
        let g = seat.guards(l);
        refresh_weapons(seat, &g);
    }
    let snap = seat.snap(policy, extended);
    let mut frontier: Vec<Who> = camps.iter().filter(|&&l| snap.v.sites.sites[l].threatened).flat_map(|&l| seat.guards(l)).collect();
    frontier.sort_by_key(|&g| core::cmp::Reverse(seat.experience_of(g)));
    for guard in frontier {
        let l = seat.location_of(guard);
        let garrison = seat.guards(l);
        let allowed = |seat: &Seat, k: usize| weapon_allowed(seat, &garrison, guard, k, policy);
        if let Some(k) = equip_stored_camp_weapon(seat, guard, &allowed) {
            let (name, id) = (seat.who_name(guard), String::from(seat.types.name_of(k).unwrap_or("")));
            seat.ai.telemetry.report(|| alloc::format!("equipped frontier guard {name} at {} with stored {id}", super::telemetry::place(l)));
        }
        equip_weapon(seat, policy, extended, guard, &garrison, true, i32::MAX);
    }
    stock_weapon_reserve(seat);
    for &l in &camps {
        let danger = seat.gam.locations[l].danger;
        if danger != Danger::None {
            for g in seat.guards(l) {
                equip_danger_protection(seat, g, danger);
            }
        }
    }
    redistribute_production_tools(seat, policy, extended, &camps);
    let snap = seat.snap(policy, extended);
    stock_camp_water_containers(seat, &snap, &camps);
    materialize_resting_sustenance(seat, &camps);
    if let Some(&reststop) = camps.iter().find(|&&l| l == super::camps::RESTSTOP) {
        seat.equip_reststop_caretaker_at(reststop);
    }
}

// --- the leader's kept places (`GroupInventory.MaintainLeaderRoleSlots`) ---

/// `MaintainLeaderRoleSlots` on the world: as [`super::group::maintain_leader_roles`],
/// with the camp here (when ours) to take what no follower can, and its
/// rooms to fill a place from (when its loot may be taken).
pub fn maintain_leader_roles(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    use super::group::Role;
    let items = seat.items();
    let here = seat.here();
    let home = seat.camps.get(here).and_then(|c| c.owner) == Some(seat.me());
    loop {
        let kinds: Vec<usize> = seat.rival.pack.iter().collect();
        let free = seat.rival.pack.capacity() - seat.rival.pack.len();
        if free >= group::missing_roles(items, &kinds) {
            break;
        }
        let snap = seat.snap(policy, extended);
        let reserved = snap.needs.planned_settlement_payment;
        let kit = seat.ai.leader_kit;
        let displaced = with_context(seat, &snap, policy, |cx| {
            let mut best: Option<(usize, f32, f32)> = None;
            for slot in 0..seat.rival.pack.len() {
                let Some(k) = seat.rival.pack.get(slot) else { continue };
                if Role::ALL.iter().any(|r| r.fits(items, k)) || Some(slot) == kit.weapon || Some(slot) == kit.protection || Some(k) == reserved {
                    continue;
                }
                let keep = super::trading::cargo_retention_value(cx, &snap.v.sites, seat.ai, k);
                let value = items.value(k);
                if best.is_none_or(|b| (keep, value) < (b.1, b.2)) {
                    best = Some((slot, keep, value));
                }
            }
            best.map(|b| b.0)
        });
        let Some(slot) = displaced else { return };
        let follower = seat
            .rival
            .party
            .companions
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.pack.is_full())
            .min_by_key(|(i, c)| (c.pack.len(), *i))
            .map(|(i, _)| i);
        let Some(Some((kind, rounds))) = seat.with_pack(super::seat::Who::Leader, |p, kit| take(p, kit, slot)) else { return };
        let id = String::from(items.types.name_of(kind).unwrap_or(""));
        let leader = seat.who_name(super::seat::Who::Leader);
        if let Some(f) = follower {
            if give(&mut seat.rival.party.companions[f].pack, kind, rounds).is_some() {
                let to = seat.who_name(super::seat::Who::Follower(f));
                seat.ai.telemetry.report(|| alloc::format!("moved {id} from {leader} to {to} to preserve leader survival slots"));
                continue;
            }
        }
        if home && seat.store_item_in_camp(here, kind, rounds, true) {
            seat.ai.telemetry.report(|| alloc::format!("stored {id} at {} to preserve leader survival slots", super::telemetry::place(here)));
            continue;
        }
        give(&mut seat.rival.pack, kind, rounds);
        return;
    }
    let loot = home && seat.can_collect_local_loot();
    for role in Role::ALL {
        if seat.rival.pack.iter().any(|k| role.fits(items, k)) || seat.rival.pack.is_full() {
            continue;
        }
        let value = |k: usize| match role {
            Role::Food => items.types.food_of(k),
            Role::Water => items.water_capacity(k),
            Role::Protection => items.types.defense_of(k),
        };
        let better = |a: usize, b: usize| (value(a), items.value(a)).partial_cmp(&(value(b), items.value(b))) == Some(core::cmp::Ordering::Greater);
        let mut best: Option<(usize, usize, usize)> = None;
        for (ci, c) in seat.rival.party.companions.iter().enumerate() {
            for s in 0..c.pack.len() {
                if let Some(k) = c.pack.get(s).filter(|&k| role.fits(items, k)) {
                    if best.is_none_or(|b| better(k, b.2)) {
                        best = Some((ci, s, k));
                    }
                }
            }
        }
        if let Some((ci, s, _)) = best {
            let c = &mut seat.rival.party.companions[ci];
            if let Some((k, rounds)) = take(&mut c.pack, &mut c.kit, s) {
                give(&mut seat.rival.pack, k, rounds);
            }
            continue;
        }
        if !loot {
            continue;
        }
        let stored = room_items(seat, here).into_iter().filter(|&(_, _, k)| role.fits(items, k)).fold(None, |b: Option<(usize, usize, usize)>, x| match b {
            Some(b) if !better(x.2, b.2) => Some(b),
            _ => Some(x),
        });
        if let Some((r, s, _)) = stored {
            if let Some((k, rounds)) = remove_from_rooms(seat, here, alloc::vec![(r, s)]).pop() {
                give(&mut seat.rival.pack, k, rounds);
            }
        }
    }
}

// --- the current camp (`CampManagement.MaintainCurrentCamp`) -------------

/// `ConstructForCamp`: traps the camp wants, pumps it needs, a suit where it
/// is dangerous — built from the group, the camp's guards and rooms and the
/// reserve, by anybody living there.
fn construct_for_camp(seat: &mut Seat, policy: &AiPolicy, snap: &Snap, l: usize) {
    let items = seat.items();
    let me = seat.me();
    let site = &snap.v.sites.sites[l];
    let mut wanted: Vec<&str> = Vec::new();
    for (i, p) in site.productions.iter().enumerate() {
        let name = items.types.name_of(p.produce);
        let short = p.installed < super::production::desired_tool_count(items, &snap.v.sites, me, l, i);
        if name == Some("item_meat") && short && !wanted.contains(&"item_trap") {
            wanted.push("item_trap");
        }
        if name == Some("item_rats") && short && !wanted.contains(&"item_rat_trap") {
            wanted.push("item_rat_trap");
        }
    }
    if super::production::needs_pump(items, site) {
        wanted.push("item_industrial_pump");
        wanted.push("item_hand_pump");
    }
    if site.danger != Danger::None {
        wanted.push("item_protective_suit");
    }
    let builders = seat.rival.needs.health > 0 || !seat.guards(l).is_empty();
    if wanted.is_empty() || !builders {
        return;
    }
    let mut leader_kit = seat.ai.leader_kit;
    let made = {
        let Seat { rival, camps, rooms, ai, constructions, .. } = &mut *seat;
        let mut sources: Vec<super::production::Source> = Vec::new();
        sources.push(super::production::Source { bag: &mut rival.pack, kit: Some(&mut leader_kit) });
        for c in rival.party.companions.iter_mut() {
            sources.push(super::production::Source { bag: &mut c.pack, kit: Some(&mut c.kit) });
        }
        if let Some(camp) = camps.at(l) {
            for r in camp.residents.iter_mut() {
                sources.push(super::production::Source { bag: &mut r.pack, kit: Some(&mut r.kit) });
            }
        }
        for b in rooms.bags_at(l) {
            sources.push(super::production::Source { bag: b, kit: None });
        }
        let _ = policy;
        super::production::construct_any(ai, items, constructions, &mut sources, &wanted)
    };
    seat.ai.leader_kit = leader_kit;
    if let Some(k) = made {
        install_constructed(seat, l, k);
        // The first living builder: the group, leader first, then the guards.
        let builder = if seat.rival.needs.health > 0 {
            seat.who_name(Who::Leader)
        } else if let Some(i) = seat.rival.party.companions.iter().position(|c| c.needs.health > 0) {
            seat.who_name(Who::Follower(i))
        } else {
            seat.guards(l).first().map_or_else(String::new, |&g| seat.who_name(g))
        };
        let id = String::from(items.types.name_of(k).unwrap_or(""));
        seat.ai.telemetry.report(|| alloc::format!("{builder} constructed {id} at {}", super::telemetry::place(l)));
    }
}

/// `InstallConstructedItem`: a pump into the spring's room, a tool into the
/// rooms (room made for it), anything else to the reserve or the place.
fn install_constructed(seat: &mut Seat, l: usize, kind: usize) {
    let items = seat.items();
    if super::needs::is_pump(items, kind) {
        let source = (0..seat.room_count(l)).find(|&r| seat.is_water_room(l, r));
        match source {
            Some(r) if seat.room_has_space(l, r) => {
                let cap = seat.room_capacity(l, r);
                give(seat.rooms.get_or_insert(l, r, || crate::game::ItemBag::new(cap)), kind, None);
            }
            _ => {
                seat.store_item_in_camp(l, kind, None, true);
            }
        }
        return;
    }
    let production = items.types.name_of(kind).and_then(|n| seat.productions.worked_by(n));
    if production.is_some() {
        make_room_for_production_change(seat, l, production);
        if !seat.store_item_in_camp(l, kind, None, true) {
            seat.store_in_reserve_or_at(l, kind, None);
        }
        return;
    }
    seat.store_in_reserve_or_at(l, kind, None);
}

/// `InstallLoosePump`: a camp that needs one takes a pump the group, its
/// guards or its rooms hold — industrial first — into the spring's room.
fn install_loose_pump(seat: &mut Seat, snap: &Snap, l: usize) {
    let items = seat.items();
    if !super::production::needs_pump(items, &snap.v.sites.sites[l]) {
        return;
    }
    let Some(source) = (0..seat.room_count(l)).find(|&r| seat.is_water_room(l, r)) else { return };
    if !seat.room_has_space(l, source) {
        return;
    }
    // Local sources in upstream's order: the group, the guards, the rooms.
    let mut found: Vec<(u8, Who, usize, usize, Option<(usize, usize)>)> = Vec::new();
    for w in seat.group().into_iter().chain(seat.guards(l)) {
        if let Some(p) = seat.pack_of(w) {
            if let Some(s) = (0..p.len()).find(|&s| p.get(s).is_some_and(|k| super::needs::is_pump(items, k))) {
                found.push((0, w, s, p.get(s).unwrap_or(0), None));
            }
        }
    }
    for r in 0..seat.room_count(l) {
        if let Some(b) = seat.rooms.get(l, r) {
            if let Some(s) = (0..b.len()).find(|&s| b.get(s).is_some_and(|k| super::needs::is_pump(items, k))) {
                found.push((1, Who::Leader, s, b.get(s).unwrap_or(0), Some((r, s))));
            }
        }
    }
    let industrial = items.types.index_of("item_industrial_pump");
    let pick = found.iter().find(|f| Some(f.3) == industrial).or(found.first()).cloned();
    let Some((_, w, s, kind, room)) = pick else { return };
    match room {
        Some((r, rs)) => {
            remove_from_rooms(seat, l, alloc::vec![(r, rs)]);
        }
        None => {
            seat.with_pack(w, |p, kit| take(p, kit, s));
        }
    }
    let cap = seat.room_capacity(l, source);
    give(seat.rooms.get_or_insert(l, source, || crate::game::ItemBag::new(cap)), kind, None);
    let id = String::from(items.types.name_of(kind).unwrap_or(""));
    seat.ai.telemetry.report(|| alloc::format!("installed {id} at {}'s water source", super::telemetry::place(l)));
}

/// `StockCriticalWaterContainerFromGroup`: at a camp that cannot water two
/// travellers, the smallest container the group can spare (a full one,
/// unless the camp has water left to fill it) is left in the camp. Where it
/// went, `(room, kind)`.
fn stock_critical_water_container(seat: &mut Seat, snap: &Snap, l: usize) -> Option<usize> {
    let items = seat.items();
    let site = &snap.v.sites.sites[l];
    if !site.travel_water_bottleneck(items, 2) {
        return None;
    }
    let portable: i32 = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum();
    let desired = super::needs::desired_water_container_capacity(group::size(seat.rival) as i32);
    let reusable = site.needs_reusable_water_reserve(2);
    let pick = group::carried(seat.rival)
        .into_iter()
        .filter(|&(_, _, k)| items.is_water_container(k) && (reusable || items.types.water_of(k) > 0) && portable - items.water_capacity(k) >= desired)
        .min_by_key(|&(_, _, k)| items.water_capacity(k));
    let (m, s, k) = pick?;
    let who = if m == 0 { Who::Leader } else { Who::Follower(m - 1) };
    let rounds = seat.pack_of(who).and_then(|p| p.rounds_left(s));
    if !seat.store_item_in_camp(l, k, rounds, true) {
        return None;
    }
    seat.with_pack(who, |p, kit| take(p, kit, s));
    let id = String::from(items.types.name_of(k).unwrap_or(""));
    seat.ai.telemetry.report(|| alloc::format!("stocked {id} as critical water reserve at {}", super::telemetry::place(l)));
    Some(k)
}

/// `CollectFutureRecruitmentPayment`: the settler's payment, if the leader
/// lacks it and has room, from the camp's rooms.
fn collect_future_recruitment_payment(seat: &mut Seat, snap: &Snap, l: usize) {
    let Some(kind) = snap.needs.planned_settlement_payment else { return };
    if seat.rival.pack.iter().any(|k| k == kind) || seat.rival.pack.is_full() {
        return;
    }
    let Some((r, s, _)) = room_items(seat, l).into_iter().find(|&(_, _, k)| k == kind) else { return };
    if let Some((k, rounds)) = remove_from_rooms(seat, l, alloc::vec![(r, s)]).pop() {
        give(&mut seat.rival.pack, k, rounds);
        let id = String::from(seat.types.name_of(k).unwrap_or(""));
        let target = seat.ai.target.map_or_else(String::new, super::telemetry::place);
        seat.ai.telemetry.report(|| alloc::format!("reserved {id} from {} for the recruit at {target}", super::telemetry::place(l)));
    }
}

/// Take a room's thing into the group: a carrier, else (with no free slot)
/// in place of cargo worth less. Whether it was taken.
fn take_from_room_into_group(seat: &mut Seat, policy: &AiPolicy, extended: bool, l: usize, r: usize, s: usize) -> bool {
    let items = seat.items();
    let Some(kind) = seat.rooms.get(l, r).and_then(|b| b.get(s)) else { return false };
    let free: usize = (0..group::size(seat.rival)).map(|m| group::pack(seat.rival, m).capacity() - group::pack(seat.rival, m).len()).sum();
    if free == 0 {
        let snap = seat.snap(policy, extended);
        let swap = with_context(seat, &snap, policy, |cx| super::trading::replace_cargo(cx, seat.ai, kind));
        let Some((m, ms)) = swap else { return false };
        let who = if m == 0 { Who::Leader } else { Who::Follower(m - 1) };
        let Some(Some((old, old_rounds))) = seat.with_pack(who, |p, kit| take(p, kit, ms)) else { return false };
        let Some((k, rounds)) = remove_from_rooms(seat, l, alloc::vec![(r, s)]).pop() else { return false };
        seat.with_pack(who, |p, _| give(p, k, rounds));
        if let Some(b) = seat.rooms.get_mut(l, r) {
            give(b, old, old_rounds);
        }
        return true;
    }
    let Some(m) = super::trading::cargo_carrier(seat.rival, items, kind) else { return false };
    let Some((k, rounds)) = remove_from_rooms(seat, l, alloc::vec![(r, s)]).pop() else { return false };
    give(group::pack_mut(seat.rival, m), k, rounds).is_some()
}

/// `CollectProducedSurplus`: the camp's produce, above its reserve, into the
/// group while it can carry it (or swap it for cargo worth less).
fn collect_produced_surplus(seat: &mut Seat, policy: &AiPolicy, extended: bool, l: usize) {
    let Some(produce) = seat.camps.get(l).and_then(|c| c.production).and_then(|p| seat.productions.get(p)).and_then(|p| seat.types.index_of(&p.produce)) else { return };
    let mut stock = room_items(seat, l).iter().filter(|&&(_, _, k)| k == produce).count();
    let mut collected = 0;
    loop {
        if stock <= super::group::CAMP_FOOD_ITEM_RESERVE {
            break;
        }
        let Some((r, s, _)) = room_items(seat, l).into_iter().find(|&(_, _, k)| k == produce) else { break };
        if !take_from_room_into_group(seat, policy, extended, l, r, s) {
            break;
        }
        stock -= 1;
        collected += 1;
    }
    if collected > 0 {
        let items = seat.items();
        let id = String::from(items.types.name_of(produce).unwrap_or(""));
        let value = super::reason::format_fixed(collected as f32 * items.value(produce), 0);
        seat.ai.telemetry.report(|| alloc::format!("collected surplus {id} x{collected} from {} for trade (trade value {value})", super::telemetry::place(l)));
    }
}

/// `CollectStoredTradeGoods`: the camp's other goods worth trading, food
/// first, wanted materials, the most filling, the dearest — never the
/// weapon or water reserve, the produce, tools of this camp, or pumps.
fn collect_stored_trade_goods(seat: &mut Seat, policy: &AiPolicy, extended: bool, l: usize, deposited: Option<usize>) {
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let weapons = stored_weapon_reserve(seat, l);
    let mut water = stored_water_reserve(seat, &snap, l);
    if let Some(k) = deposited {
        if let Some((r, s, _)) = room_items(seat, l).into_iter().find(|&(_, _, x)| x == k) {
            water.push((r, s));
        }
    }
    let produce = seat.camps.get(l).and_then(|c| c.production).and_then(|p| seat.productions.get(p)).and_then(|p| seat.types.index_of(&p.produce));
    let site = &snap.v.sites.sites[l];
    let mut candidates: Vec<(usize, usize, usize, f32)> = with_context(seat, &snap, policy, |cx| {
        room_items(seat, l)
            .into_iter()
            .filter(|&(r, s, _)| !weapons.contains(&(r, s)) && !water.contains(&(r, s)))
            .filter(|&(_, _, k)| Some(k) != produce)
            .filter(|&(_, _, k)| !items.types.name_of(k).is_some_and(|n| site.productions.iter().any(|p| p.production.worked_by(n))))
            .filter(|&(_, _, k)| !super::needs::is_pump(items, k) && can_collect_for_trade(cx, k))
            .map(|(r, s, k)| (r, s, k, super::trading::construction_material_priority(cx, items.types.name_of(k).unwrap_or(""))))
            .collect()
    });
    candidates.sort_by(|a, b| {
        let t = items.types;
        (t.food_of(b.2) > 0)
            .cmp(&(t.food_of(a.2) > 0))
            .then(b.3.partial_cmp(&a.3).unwrap_or(core::cmp::Ordering::Equal))
            .then(t.food_of(b.2).cmp(&t.food_of(a.2)))
            .then(items.value(b.2).partial_cmp(&items.value(a.2)).unwrap_or(core::cmp::Ordering::Equal))
    });
    // Taken in that order, each by what it is: slots shift as things go.
    let kinds: Vec<usize> = candidates.iter().map(|c| c.2).collect();
    let mut collected: Vec<usize> = Vec::new();
    for kind in kinds {
        let free: usize = (0..group::size(seat.rival)).map(|m| group::pack(seat.rival, m).capacity() - group::pack(seat.rival, m).len()).sum();
        let Some((r, s, _)) = room_items(seat, l).into_iter().find(|&(_, _, k)| k == kind) else { continue };
        let taken = take_from_room_into_group(seat, policy, extended, l, r, s);
        if taken {
            collected.push(kind);
        } else if free > 0 {
            break;
        }
    }
    if !collected.is_empty() {
        let list = super::telemetry::grouped_ids(items.types, &collected);
        let value = super::reason::format_fixed(super::reason::dotnet_sum(collected.iter().map(|&k| items.value(k))), 0);
        seat.ai.telemetry.report(|| alloc::format!("collected stored trade goods from {}: {list} (trade value {value})", super::telemetry::place(l)));
    }
}

/// `CargoManagement.CanCollectForTrade`.
fn can_collect_for_trade(cx: &Context, kind: usize) -> bool {
    let items = cx.w.items;
    if !items.is_water_container(kind) {
        return super::trading::can_sell_kind(cx, kind, None);
    }
    let carried = group::carried(cx.r).iter().filter(|&&(_, _, k)| items.is_water_container(k)).count();
    let pooled: usize = cx.ai.reserve.iter().filter(|&&(k, _)| items.is_water_container(k)).map(|&(_, c)| c as usize).sum();
    carried + pooled >= group::size(cx.r)
}

/// `StageRestingSustenance`: a bible carried is left in the camp, unless
/// this is Reststop or the group is on its way to settle it.
fn stage_resting_sustenance(seat: &mut Seat, l: usize) {
    let items = seat.items();
    let to_reststop = seat.ai.target_was_neutral && seat.ai.target == Some(super::camps::RESTSTOP);
    if l == super::camps::RESTSTOP || to_reststop {
        return;
    }
    let Some((m, s, k)) = group::carried(seat.rival).into_iter().find(|&(_, _, k)| super::needs::Needs::resting_sustenance(items, k)) else { return };
    let who = if m == 0 { Who::Leader } else { Who::Follower(m - 1) };
    if seat.store_item_in_camp(l, k, None, false) {
        seat.with_pack(who, |p, kit| take(p, kit, s));
        let id = String::from(items.types.name_of(k).unwrap_or(""));
        seat.ai.telemetry.report(|| alloc::format!("secured {id} in camp storage until Reststop can be settled"));
    }
}

/// `MaintainCurrentCamp`.
pub fn maintain_current_camp(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let l = seat.here();
    maintain_leader_roles(seat, policy, extended);
    let snap = seat.snap(policy, extended);
    construct_for_camp(seat, policy, &snap, l);
    let snap = seat.snap(policy, extended);
    install_production_from_pool(seat, policy, &snap, l);
    let snap = seat.snap(policy, extended);
    install_loose_pump(seat, &snap, l);
    seat.auto_select_production(l);
    ensure_production_storage(seat, l);
    let loot = seat.can_collect_local_loot();
    if loot {
        let snap = seat.snap(policy, extended);
        collect_redundant_tools(seat, &snap, l);
    }
    let snap = seat.snap(policy, extended);
    let deposited = stock_critical_water_container(seat, &snap, l);
    if loot {
        let snap = seat.snap(policy, extended);
        collect_future_recruitment_payment(seat, &snap, l);
        collect_produced_surplus(seat, policy, extended, l);
        collect_stored_trade_goods(seat, policy, extended, l, deposited);
    }
    stage_resting_sustenance(seat, l);
}

// --- local items (`ClassicAiState.ManageLocalItems`) ----------------------

/// `AiItemPool.EnforceCaps`: no kind above five. How many were let go.
pub fn enforce_reserve_caps(ai: &mut ModernAi) -> usize {
    let mut gone = 0;
    for e in ai.reserve.iter_mut() {
        if e.1 > super::state::RESERVE_CAP {
            gone += (e.1 - super::state::RESERVE_CAP) as usize;
            e.1 = super::state::RESERVE_CAP;
        }
    }
    gone
}

/// `CollectGroundItems` for everything lying here, unless it is somebody
/// else's camp.
pub fn collect_local_ground(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    if !seat.can_collect_local_loot() {
        return;
    }
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner).is_some_and(|o| o != seat.me()) {
        return;
    }
    let all: Vec<usize> = (0..seat.ground.len()).filter(|&g| seat.ground[g].location == here).collect();
    super::resolve::collect_ground_items(seat, policy, extended, &all);
}

/// `ClassicAiState.StockCampWaterContainers`: at home, the smallest
/// containers from the reserve until the camp has one per person and one
/// more.
fn stock_home_water_containers(seat: &mut Seat) {
    let here = seat.here();
    let items = seat.items();
    loop {
        let held = room_items(seat, here).iter().filter(|&&(_, _, k)| items.is_water_container(k)).count() as i32;
        let desired = 1 + seat.camps.get(here).map_or(0, |c| c.residents.iter().filter(|r| r.needs.health > 0).count() as i32);
        if held >= desired {
            return;
        }
        let Some(k) = reserve_least_water_container(seat.ai, items) else { return };
        seat.ai.reserve_take(k);
        if !seat.store_item_in_camp(here, k, None, true) {
            super::equipment::reserve_insert(seat.ai, items, k);
            return;
        }
    }
}

/// `EquipTravelWaterContainers`: until the group carries the room it
/// wants, a member without a container takes one — the reserve's best, or
/// at home the camp's best.
fn equip_travel_water_containers(seat: &mut Seat) {
    let items = seat.items();
    loop {
        let carried: i32 = group::carried(seat.rival).iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum();
        if carried >= super::needs::desired_water_container_capacity(group::size(seat.rival) as i32) {
            return;
        }
        let carrier = seat.group().into_iter().find(|&w| seat.pack_of(w).is_some_and(|p| !p.is_full() && !p.iter().any(|k| items.is_water_container(k))));
        let Some(carrier) = carrier else { return };
        let home = seat.camps.get(seat.here()).and_then(|c| c.owner) == Some(seat.me());
        let container = if let Some(k) = reserve_best_water_container(seat.ai, items) {
            seat.ai.reserve_take(k);
            Some((k, None))
        } else if home {
            take_stored_water_container(seat, seat.here(), true)
        } else {
            None
        };
        let Some((k, rounds)) = container else { return };
        seat.with_pack(carrier, |p, _| give(p, k, rounds));
    }
}

/// `ManageLocalItems`.
pub fn manage_local_items(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    collect_local_ground(seat, policy, extended);
    if seat.camps.get(seat.here()).and_then(|c| c.owner) == Some(seat.me()) {
        stock_home_water_containers(seat);
    }
    equip_travel_water_containers(seat);
}

/// `HuntOneDogForMeat`: out of the cities, short of food, a living dog here
/// is killed for its meat — eaten, carried, or stored at home.
pub fn hunt_one_dog_for_meat(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let here = seat.here();
    if seat.gam.locations.get(here).is_some_and(|l| l.is_city) {
        return;
    }
    let snap = seat.snap(policy, extended);
    if snap.needs.portable_food >= snap.needs.desired_portable_food {
        return;
    }
    let dog = seat.people.at(here).find(|&i| seat.people.get(i).is_some_and(|p| p.class == crate::gamdat::CharClass::Dog && !p.dead));
    let Some(dog) = dog else { return };
    let items = seat.items();
    let Some(meat) = items.types.index_of("item_meat") else { return };
    let capacity: i32 = (0..group::size(seat.rival)).map(|m| group::needs(seat.rival, m).max_food - group::needs(seat.rival, m).food).sum();
    let can_eat = capacity >= items.types.food_of(meat);
    let can_carry = super::trading::cargo_carrier(seat.rival, items, meat).is_some();
    let home = seat.camps.get(here).and_then(|c| c.owner) == Some(seat.me());
    let can_store = home && seat.can_store_item_in_camp(here, meat);
    if !can_eat && !can_carry && !can_store {
        return;
    }
    // `Die`: the dog falls and is due back; its meat is had at once.
    let Some(record) = seat.people.get(dog).map(|p| p.record) else { return };
    seat.people.died(record);
    let days = seat.book.difficulty[seat.difficulty.clamp(0, 2) as usize].dog_respawn;
    seat.respawns.schedule(crate::game::Pending { record, location: here, days, food: 0, water: 0, revive_health: crate::game::MAX_HEALTH });
    let destination = if can_eat {
        crate::game::group_eat(&mut seat.rival.needs, &mut seat.rival.party.companions, items.types.food_of(meat));
        String::from("immediately consumed by the travelling group")
    } else if let Some(m) = super::trading::cargo_carrier(seat.rival, items, meat) {
        give(group::pack_mut(seat.rival, m), meat, None);
        String::from("in the travelling group")
    } else if can_store && seat.store_item_in_camp(here, meat, None, true) {
        alloc::format!("in a room at {}", super::telemetry::place(here))
    } else {
        return;
    };
    seat.ai.telemetry.report(|| alloc::format!("killed one dog for free at {}; item_meat was {destination}", super::telemetry::place(here)));
}

// --- followers (`ClassicAiState`) ----------------------------------------

/// The follower's order for leaving him somewhere: neither doctor nor
/// technician first, a fighter next, then the weakest (`StationSurplusFollower`,
/// `DismissSurplusFollower`).
fn surplus_order(seat: &Seat, policy: &AiPolicy) -> Vec<usize> {
    use crate::gamdat::CharClass;
    let rules = seat.rules();
    let party = seat.party_fighters();
    let class_rank = |c: CharClass| match c {
        CharClass::Doctor | CharClass::Technician => 3,
        CharClass::Mercenary => 2,
        _ => 1,
    };
    let mut order: Vec<usize> = (0..seat.rival.party.companions.len()).collect();
    order.sort_by(|&a, &b| {
        let (fa, fb) = (&party[a + 1], &party[b + 1]);
        class_rank(fa.class)
            .cmp(&class_rank(fb.class))
            .then(super::attack::fighter_strength(&rules, fa).partial_cmp(&super::attack::fighter_strength(&rules, fb)).unwrap_or(core::cmp::Ordering::Equal))
    });
    let _ = policy;
    order
}

/// `PreserveTravelSuppliesBeforeStationing`: before a follower stays, the
/// food and water containers the rest need come off him, while they can be
/// carried (cheap cargo going into the camp to make room). Whether the rest
/// are then supplied — or the camp can build what they lack.
fn preserve_travel_supplies(seat: &mut Seat, snap: &Snap, follower: usize) -> bool {
    const MINIMUM_TRAVEL_DAYS: i32 = 5;
    let items = seat.items();
    let t = items.types;
    let here = seat.here();
    let remaining: Vec<usize> = (0..group::size(seat.rival)).filter(|&m| m != follower + 1).collect();
    if remaining.is_empty() {
        return false;
    }
    let lowest_food_days = |seat: &Seat| {
        let food: Vec<i32> = remaining.iter().map(|&m| group::needs(seat.rival, m).food).collect();
        let carried: i32 = remaining.iter().flat_map(|&m| group::pack(seat.rival, m).iter().collect::<Vec<_>>()).map(|k| t.food_of(k)).sum();
        super::supplies::lowest_after_distribution(&food, carried)
    };
    let container_capacity = |seat: &Seat| -> i32 {
        remaining.iter().flat_map(|&m| group::pack(seat.rival, m).iter().collect::<Vec<_>>()).map(|k| items.water_capacity(k)).sum()
    };
    // `MoveSurvivalItemFromFollower`.
    let move_item = |seat: &mut Seat, kind: usize| -> bool {
        let carrier = remaining.iter().copied().find(|&m| !group::pack(seat.rival, m).is_full());
        let carrier = match carrier {
            Some(c) => c,
            None => {
                let mut best: Option<(usize, usize, usize)> = None;
                for &m in &remaining {
                    let who = if m == 0 { Who::Leader } else { Who::Follower(m - 1) };
                    let kit = seat.kit_of(who).unwrap_or_default();
                    let p = group::pack(seat.rival, m);
                    for s in 0..p.len() {
                        let Some(k) = p.get(s) else { continue };
                        let tool = t.name_of(k).and_then(|n| seat.productions.worked_by(n)).is_some();
                        if t.food_of(k) == 0 && !items.is_water_container(k) && Some(s) != kit.weapon && Some(s) != kit.protection && !tool && !super::needs::is_pump(items, k) && best.is_none_or(|b| items.value(k) < items.value(b.2)) {
                            best = Some((m, s, k));
                        }
                    }
                }
                let Some((m, s, k)) = best else { return false };
                let who = if m == 0 { Who::Leader } else { Who::Follower(m - 1) };
                let rounds = seat.pack_of(who).and_then(|p| p.rounds_left(s));
                if !seat.store_item_in_camp(here, k, rounds, true) {
                    return false;
                }
                seat.with_pack(who, |p, kit| take(p, kit, s));
                m
            }
        };
        let f = Who::Follower(follower);
        let Some(slot) = seat.pack_of(f).and_then(|p| (0..p.len()).find(|&s| p.get(s) == Some(kind))) else { return false };
        let Some(Some((k, rounds))) = seat.with_pack(f, |p, kit| take(p, kit, slot)) else { return false };
        let to = if carrier == 0 { Who::Leader } else { Who::Follower(carrier - 1) };
        if seat.with_pack(to, |p, _| give(p, k, rounds)).flatten().is_some() {
            return true;
        }
        seat.with_pack(f, |p, _| give(p, k, rounds));
        false
    };
    let mut foods: Vec<usize> = group::pack(seat.rival, follower + 1).iter().filter(|&k| t.food_of(k) > 0).collect();
    foods.sort_by_key(|&k| core::cmp::Reverse(t.food_of(k)));
    for k in foods {
        if lowest_food_days(seat) >= MINIMUM_TRAVEL_DAYS || !move_item(seat, k) {
            break;
        }
    }
    let desired_water = remaining.len() as i32 * 3;
    let mut containers: Vec<usize> = group::pack(seat.rival, follower + 1).iter().filter(|&k| items.is_water_container(k)).collect();
    containers.sort_by_key(|&k| core::cmp::Reverse(items.water_capacity(k)));
    for k in containers {
        if container_capacity(seat) >= desired_water || !move_item(seat, k) {
            break;
        }
    }
    let site = &snap.v.sites.sites[here];
    let n = remaining.len() as i32;
    (lowest_food_days(seat) >= MINIMUM_TRAVEL_DAYS || site.food_surplus(items) > n) && (container_capacity(seat) >= desired_water || site.water_surplus() > n)
}

/// `StationSurplusFollower`: at home, when the camp can feed another guard,
/// the follower least needed on the road stays — once the rest are
/// supplied. The name of who stayed, if anybody did.
pub fn station_surplus_follower(seat: &mut Seat, policy: &AiPolicy, extended: bool) -> Option<String> {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) || group::size(seat.rival) <= 1 {
        return None;
    }
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let guards = snap.v.sites.sites[here].living_guards(seat.me());
    if !super::reinforcement::can_support_additional_guard(items, &snap.v.sites, seat.me(), policy, &snap.v.sites.sites[here], guards) {
        return None;
    }
    let f = *surplus_order(seat, policy).first()?;
    if !preserve_travel_supplies(seat, &snap, f) {
        return None;
    }
    let name = seat.companion_name(f);
    join_camp(seat, policy, extended, f).then_some(name)
}

/// `StationTradeFollower`: at home with three or more, the follower
/// carrying least of worth stays, while the camp can feed him. Who stayed.
pub fn station_trade_follower(seat: &mut Seat, policy: &AiPolicy, extended: bool) -> Option<String> {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) || group::size(seat.rival) <= 2 {
        return None;
    }
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let guards = snap.v.sites.sites[here].living_guards(seat.me());
    if guards >= policy.critical_garrison_target || !super::reinforcement::can_support_additional_guard(items, &snap.v.sites, seat.me(), policy, &snap.v.sites.sites[here], guards) {
        return None;
    }
    let rules = seat.rules();
    let party = seat.party_fighters();
    let f = (0..seat.rival.party.companions.len()).min_by(|&a, &b| {
        let worth = |i: usize| seat.rival.party.companions[i].pack.iter().map(|k| items.value(k)).sum::<f32>();
        worth(a)
            .partial_cmp(&worth(b))
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(super::attack::fighter_strength(&rules, &party[a + 1]).partial_cmp(&super::attack::fighter_strength(&rules, &party[b + 1])).unwrap_or(core::cmp::Ordering::Equal))
            .then(a.cmp(&b))
    })?;
    let name = seat.companion_name(f);
    join_camp(seat, policy, extended, f).then_some(name)
}

/// `RecallCampFollower`: at home, the strongest guard back into the group,
/// if the camp keeps its minimum. Who came back.
pub fn recall_camp_follower(seat: &mut Seat, policy: &AiPolicy, extended: bool) -> Option<String> {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) {
        return None;
    }
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let site = &snap.v.sites.sites[here];
    let minimum = if super::reinforcement::is_critical_camp(items, site) { super::reinforcement::sustainable_garrison_target(site, policy.critical_garrison_target) } else { 1 };
    strongest_guard_leaves(seat, here, minimum)
}

/// `MobilizeCampFollower(minimumGuards: 1)`. Who came.
pub fn mobilize_camp_follower(seat: &mut Seat) -> Option<String> {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) {
        return None;
    }
    strongest_guard_leaves(seat, here, 1)
}

fn strongest_guard_leaves(seat: &mut Seat, l: usize, minimum: i32) -> Option<String> {
    let guards = seat.guards(l);
    if guards.len() as i32 <= minimum {
        return None;
    }
    let rules = seat.rules();
    let d = seat.defence_of_own(l);
    let pick = guards.iter().enumerate().fold(None, |b: Option<(usize, f32)>, (i, _)| {
        let s = super::attack::fighter_strength(&rules, &d[i]);
        match b {
            Some((_, bs)) if s <= bs => b,
            _ => Some((i, s)),
        }
    });
    let (i, _) = pick?;
    let Who::Guard(_, resident) = guards[i] else { return None };
    if !seat.leave_camp(resident) {
        return None;
    }
    Some(seat.companion_name(seat.rival.party.companions.len() - 1))
}

/// `DismissSurplusFollower`: the follower least needed goes home, and is
/// not hired again. Who went.
pub fn dismiss_surplus_follower(seat: &mut Seat, policy: &AiPolicy) -> Option<String> {
    let &f = surplus_order(seat, policy).first()?;
    let name = seat.companion_name(f);
    if let Some(record) = seat.rival.party.companions[f].origin {
        if !seat.ai.demobilized.contains(&record) {
            seat.ai.demobilized.push(record);
        }
    }
    seat.dismiss_follower(f).then_some(name)
}

/// `ReleaseFollowerForSurvival`: the weakest follower — least health, then
/// water, then food — goes. Who went.
pub fn release_follower_for_survival(seat: &mut Seat) -> Option<String> {
    let f = (0..seat.rival.party.companions.len()).min_by_key(|&i| {
        let n = seat.rival.party.companions[i].needs;
        (n.health, n.water, n.food, i)
    })?;
    let name = seat.companion_name(f);
    seat.dismiss_follower(f).then_some(name)
}

/// `SelectCampNpc`: the first follower, or a recruit hired here (a
/// generated payment only in a city). The follower's index.
pub fn select_camp_npc(seat: &mut Seat, policy: &AiPolicy, extended: bool, rng: &mut crate::spawn::Rng) -> Option<usize> {
    if group::size(seat.rival) > 1 {
        return Some(0);
    }
    let snap = seat.snap(policy, extended);
    let people_here = snap.v.people.get(seat.here()).cloned().unwrap_or_default();
    let t = seat.tables();
    let w = snap.world(&t, policy, seat.me(), seat.day, seat.here());
    let mut kit = seat.ai.leader_kit;
    let ai = seat.ai.clone();
    let mut rooms = snap.v.camp_rooms.clone();
    let mut hired: Option<usize> = None;
    let got = super::hiring::execute_recruit(seat.rival, &mut kit, &mut rooms, &w, &ai, &people_here, None, rng, |_, record| {
        hired = Some(record);
        true
    });
    seat.ai.leader_kit = kit;
    match (got, hired) {
        (super::hiring::Hired::Hired { .. }, Some(record)) => {
            seat.write_camp_rooms(&rooms);
            seat.hire(record).then(|| seat.rival.party.companions.len() - 1)
        }
        _ => None,
    }
}

/// `JoinCamp` whole: the last portable tool kept when the place can start
/// without it (`CanBootstrapCamp && ShouldReserveProductionTool`), and the
/// leader's places seen to after (`MaintainLeaderRoleSlots`).
pub fn join_camp(seat: &mut Seat, policy: &AiPolicy, extended: bool, follower: usize) -> bool {
    let snap = seat.snap(policy, extended);
    let here = seat.here();
    let keep = {
        let t = seat.tables();
        let w = snap.world(&t, policy, seat.me(), seat.day, here);
        super::claims::can_bootstrap_camp(&w, &snap.v.group, here) && should_reserve_production_tool(seat, &snap)
    };
    let ok = seat.join_camp(follower, keep);
    if ok {
        maintain_leader_roles(seat, policy, extended);
    }
    ok
}

/// `CreateCamp`: the follower stays (`JoinCamp`) and the target is done.
pub fn create_camp(seat: &mut Seat, policy: &AiPolicy, extended: bool, follower: usize) -> bool {
    let ok = join_camp(seat, policy, extended, follower);
    if ok {
        super::attack::clear_target(seat.ai);
    }
    ok
}

/// `ImproveCamp`: at home, the reserve's best tool for this place, installed.
pub fn improve_camp(seat: &mut Seat, policy: &AiPolicy, extended: bool) -> bool {
    let here = seat.here();
    if seat.camps.get(here).and_then(|c| c.owner) != Some(seat.me()) {
        return false;
    }
    let snap = seat.snap(policy, extended);
    let t = seat.tables();
    let items = seat.items();
    let products: Vec<&str> = super::claims::available_products(items, &snap.v.sites.sites[here]);
    let w = snap.world(&t, policy, seat.me(), seat.day, here);
    let tool = ["item_trap", "item_snake_trap", "item_rat_trap", "item_knife"]
        .iter()
        .find_map(|n| items.types.index_of(n).filter(|&k| seat.ai.reserve_count(k) > 0 && w.produce_of(k).is_some_and(|p| products.contains(&p))));
    let Some(k) = tool else { return false };
    seat.ai.reserve_take(k);
    if !seat.store_item_in_camp(here, k, None, true) {
        super::equipment::reserve_insert(seat.ai, items, k);
        return false;
    }
    true
}

// --- equipment and trade on the world ------------------------------------

/// `EquipmentPlanning.Maintain` on the world: the group and the guards of
/// threatened camps (the most experienced first) as recipients. What it
/// left to store is put in the reserve, the camp, or on the ground.
pub fn maintain_equipment(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let snap = seat.snap(policy, extended);
    let me = seat.me();
    let here = seat.here();
    let items = seat.items();
    let threatened: Vec<usize> = (0..snap.v.sites.sites.len()).filter(|&l| snap.v.sites.sites[l].owner == Some(me) && snap.v.sites.sites[l].threatened).collect();
    let mut leader_kit = seat.ai.leader_kit;
    let mut names: Vec<String> = Vec::new();
    let out = {
        let Seat { rival, ai, camps, rooms, .. } = &mut *seat;
        let mut members: Vec<super::equipment::Member> = Vec::new();
        let person = |o: Option<usize>| o.map_or_else(String::new, super::telemetry::person);
        names.push(super::telemetry::player(me));
        members.push(super::equipment::Member { pack: &mut rival.pack, kit: &mut leader_kit, experience: rival.experience, in_party: true, location: here });
        for c in rival.party.companions.iter_mut() {
            names.push(person(c.origin));
            members.push(super::equipment::Member { pack: &mut c.pack, kit: &mut c.kit, experience: c.experience, in_party: true, location: here });
        }
        let mut guards: Vec<(super::equipment::Member, String)> = Vec::new();
        for (l, r) in camps.residents_in(&threatened) {
            if r.needs.health > 0 {
                let name = person(r.origin);
                guards.push((super::equipment::Member { pack: &mut r.pack, kit: &mut r.kit, experience: r.experience, in_party: false, location: l }, name));
            }
        }
        guards.sort_by_key(|g| core::cmp::Reverse(g.0.experience));
        for (g, name) in guards {
            names.push(name);
            members.push(g);
        }
        let mut room_bags: Vec<&mut crate::game::ItemBag> = Vec::new();
        let mut where_rooms: Vec<usize> = members.iter().map(|m| m.location).collect();
        where_rooms.sort_unstable();
        where_rooms.dedup();
        // The rooms of own camps where recipients stand (camp ownership is
        // read before the borrow).
        let own: Vec<usize> = where_rooms.into_iter().filter(|&l| snap.v.sites.sites[l].owner == Some(me)).collect();
        for b in rooms.bags_in(&own) {
            room_bags.push(b);
        }
        super::equipment::maintain(items, &mut members, ai, extended, policy, &mut room_bags)
    };
    seat.ai.leader_kit = leader_kit;
    for (l, k, rounds) in out.left {
        seat.store_in_reserve_or_at(l, k, rounds);
    }
    for (r, k) in out.from_reserve {
        let (name, id, d) = (names.get(r).cloned().unwrap_or_default(), String::from(items.types.name_of(k).unwrap_or("")), items.types.defense_of(k));
        seat.ai.telemetry.report(|| alloc::format!("equipped {name} with {id} ({d}% armour)"));
    }
}

/// `Trading.TradeWithTrader` on a trader of the world: up to two
/// exchanges, the second not strategic if the first was, never buying back
/// what was sold, the equipment seen to after each. Whether one was made.
pub fn trade_with_trader(seat: &mut Seat, policy: &AiPolicy, extended: bool, trader: usize) -> bool {
    let snap = seat.snap(policy, extended);
    let Some(view) = snap.v.traders.iter().find(|t| t.index == trader).cloned() else { return false };
    let name = seat.traders.get(trader).map_or_else(String::new, |t| super::telemetry::person(t.record));
    let day = seat.day;
    let first_today = super::support::record_trader_exposure(seat.ai, view.id, day);
    let snake = seat.types.index_of("item_snake_trap");
    let snake_available = snake.is_some_and(|k| view.stock.iter().any(|x| x == k));
    let snake_wanted = with_context(seat, &snap, policy, |cx| snake.is_some_and(|k| snake_available && cx.needs.needs_production(cx.w, k)));
    if snake_wanted {
        seat.ai.support.saving_for_snake_trap = true;
    }
    if first_today && snake_available {
        seat.ai.telemetry.report(|| {
            alloc::format!("encountered item_snake_trap with {name}{}", if snake_wanted { " for current or future camp production" } else { "" })
        });
    }
    if view.stock.len() == 0 {
        return false;
    }
    let mut excluded: Vec<usize> = Vec::new();
    let mut strategic_made = false;
    let mut allow = true;
    let mut completed = 0;
    for exchange in 0..super::trading::EXCHANGES {
        let snap = seat.snap(policy, extended);
        let stock = match seat.traders.get(trader) {
            Some(t) => t.stock.clone(),
            None => break,
        };
        let here_is_city = seat.gam.locations.get(seat.here()).is_some_and(|l| l.is_city);
        let previous = seat.rival.previous;
        let planned = with_context(seat, &snap, policy, |cx| {
            let plan = super::trading::create_trade_plan(cx, &stock, &excluded, allow)?;
            let items = cx.w.items;
            // The first plan is announced whether or not it can be paid for.
            let announce = (exchange == 0).then(|| {
                let capacity: usize = (0..group::size(cx.r)).map(|m| group::pack(cx.r, m).capacity()).sum();
                let cargo: usize = (0..group::size(cx.r)).map(|m| group::pack(cx.r, m).len()).sum();
                let sellable = super::reason::dotnet_sum((0..group::size(cx.r))
                    .flat_map(|m| (0..group::pack(cx.r, m).len()).map(move |s| (m, s)))
                    .filter(|&(m, s)| super::trading::can_sell(cx, m, s))
                    .filter_map(|(m, s)| group::pack(cx.r, m).get(s))
                    .map(|k| items.value(k)));
                let visit = if !here_is_city {
                    "roaming"
                } else if super::trade_plan::has_prepared_trade_cargo(cx, previous) {
                    "prepared city"
                } else {
                    "incidental city"
                };
                (visit, cargo, capacity, sellable)
            });
            let offers: Vec<f32> = plan.offers.iter().map(|o| items.value(o.kind())).collect();
            let targets: Vec<f32> = plan.targets.iter().filter_map(|&s| stock.get(s)).map(|k| items.value(k)).collect();
            let affordable = cx.barter.can_afford(&offers, &targets);
            let strategic = plan.targets.iter().filter_map(|&s| stock.get(s)).any(|k| cx.needs.is_strategic(cx.w, k));
            Some((plan, strategic, affordable, announce, cx.barter.trade_factor()))
        });
        let Some((plan, strategic, affordable, announce, factor)) = planned else { break };
        if let Some((visit, cargo, capacity, sellable)) = announce {
            let sellable = super::reason::format_fixed(sellable, 0);
            seat.ai.telemetry.report(|| alloc::format!("{visit} barter with {name}: cargo {cargo}/{capacity} slots, sellable value {sellable}"));
        }
        if !affordable {
            break;
        }
        let items = seat.items();
        let offered: Vec<usize> = plan.offers.iter().map(|o| o.kind()).collect();
        let wanted: Vec<usize> = plan.targets.iter().filter_map(|&s| stock.get(s)).collect();
        let mut kit = seat.ai.leader_kit;
        let made = {
            let Seat { rival, ai, traders, .. } = &mut *seat;
            let Some(t) = traders.get_mut(trader) else { break };
            super::trading::exchange(rival, &mut kit, ai, &mut t.stock, items, &plan)
        };
        seat.ai.leader_kit = kit;
        let Some((_, got, sold_slots)) = made else { break };
        excluded = sold_slots;
        maintain_equipment(seat, policy, extended);
        completed += 1;
        if snake.is_some_and(|k| got.contains(&k)) {
            seat.ai.support.saving_for_snake_trap = false;
        }
        strategic_made |= strategic;
        let ids = |ks: &[usize]| ks.iter().map(|&k| String::from(items.types.name_of(k).unwrap_or(""))).collect::<Vec<_>>().join(", ");
        let (gave, took) = (ids(&offered), ids(&wanted));
        let offered_value = super::reason::format_fixed(super::reason::dotnet_sum(offered.iter().map(|&k| items.value(k))), 0);
        let received_value = super::reason::format_fixed(super::reason::dotnet_sum(wanted.iter().map(|&k| items.value(k))), 0);
        let factor = super::reason::format_fixed(factor, 2);
        let action = if strategic { "traded" } else { "consolidated" };
        seat.ai.telemetry.report(|| alloc::format!("{action} {gave} for {took} with {name} (value {offered_value} -> {received_value}, barter value x{factor})"));
        // `AiTurnContext.RefreshNeeds`: the next exchange is planned on the
        // needs as this one left them.
        refresh_turn_needs(seat, policy, extended);
        allow = !strategic_made;
    }
    if completed > 0 {
        seat.ai.telemetry.trade_succeeded();
        return true;
    }
    if seat.ai.telemetry.is_on() {
        let snap = seat.snap(policy, extended);
        let stock = seat.traders.get(trader).map(|t| t.stock.clone()).unwrap_or_else(|| crate::game::ItemBag::new(0));
        let failed = with_context(seat, &snap, policy, |cx| {
            let wanted = (0..stock.len()).any(|s| stock.get(s).is_some_and(|k| super::trading::shopping_priority(cx, k, stock.rounds_left(s)) > 0.0));
            let sellable = (0..group::size(cx.r)).any(|m| (0..group::pack(cx.r, m).len()).any(|s| super::trading::can_sell(cx, m, s)));
            wanted && sellable
        });
        if failed && seat.ai.telemetry.new_trade_failure(trader) {
            seat.ai.telemetry.report(|| alloc::format!("could not complete a useful trade with {name}: insufficient safe offers or inventory space"));
        }
    }
    false
}

/// `AiTurnContext.RefreshNeeds`: the turn's cached needs taken again.
pub fn refresh_turn_needs(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    if seat.needs_cache.is_none() {
        return;
    }
    let needs = seat.fresh_needs(policy, extended);
    if let Some(c) = seat.needs_cache.as_ref() {
        *c.borrow_mut() = Some(needs);
    }
}

/// `Construction.RefillConstructionReserve` on the world: the first copy of
/// each missing material from the camp here (when ours and lootable) and
/// the group.
pub fn refill_construction_reserve(seat: &mut Seat) {
    let here = seat.here();
    let loot = seat.can_collect_local_loot() && seat.camps.get(here).and_then(|c| c.owner) == Some(seat.me());
    let items = seat.items();
    let mut leader_kit = seat.ai.leader_kit;
    {
        let Seat { rival, ai, camps, rooms, .. } = &mut *seat;
        let mut sources: Vec<super::production::Source> = Vec::new();
        if loot {
            for b in rooms.bags_at(here) {
                sources.push(super::production::Source { bag: b, kit: None });
            }
            if let Some(camp) = camps.at(here) {
                for r in camp.residents.iter_mut() {
                    sources.push(super::production::Source { bag: &mut r.pack, kit: Some(&mut r.kit) });
                }
            }
        }
        sources.push(super::production::Source { bag: &mut rival.pack, kit: Some(&mut leader_kit) });
        for c in rival.party.companions.iter_mut() {
            sources.push(super::production::Source { bag: &mut c.pack, kit: Some(&mut c.kit) });
        }
        let reserved = super::production::refill_construction_reserve(ai, items, &mut sources);
        if !reserved.is_empty() {
            ai.telemetry.report(|| alloc::format!("reserved construction materials: {}", reserved.join(", ")));
        }
    }
    seat.ai.leader_kit = leader_kit;
}

/// `Construction.ConstructPortableEconomicUpgrade` on the world: built from
/// the group and the reserve, the result to the reserve or the place.
pub fn construct_portable_upgrade(seat: &mut Seat, policy: &AiPolicy, extended: bool) {
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let me = seat.me();
    let mut leader_kit = seat.ai.leader_kit;
    let made = {
        let Seat { rival, ai, constructions, .. } = &mut *seat;
        let mut sources: Vec<super::production::Source> = Vec::new();
        sources.push(super::production::Source { bag: &mut rival.pack, kit: Some(&mut leader_kit) });
        for c in rival.party.companions.iter_mut() {
            sources.push(super::production::Source { bag: &mut c.pack, kit: Some(&mut c.kit) });
        }
        super::production::construct_portable_upgrade(ai, items, &snap.v.sites, me, constructions, &mut sources)
    };
    seat.ai.leader_kit = leader_kit;
    if let Some(k) = made {
        let here = seat.here();
        seat.store_in_reserve_or_at(here, k, None);
        let id = String::from(items.types.name_of(k).unwrap_or(""));
        seat.ai.telemetry.report(|| alloc::format!("assembled {id} from shared construction materials"));
    }
}

/// `CargoManagement.FillCityCaravan`: the camp's dearest trade goods (no
/// food, water, bible, pump, its own tools, or its weapon and water
/// reserves) onto whoever can carry them.
pub fn fill_city_caravan(seat: &mut Seat, policy: &AiPolicy, extended: bool, l: usize) {
    if !seat.can_collect_local_loot() {
        return;
    }
    let snap = seat.snap(policy, extended);
    let items = seat.items();
    let t = items.types;
    let weapons = stored_weapon_reserve(seat, l);
    let water = stored_water_reserve(seat, &snap, l);
    let site = &snap.v.sites.sites[l];
    let mut candidates: Vec<(usize, usize, usize)> = room_items(seat, l)
        .into_iter()
        .filter(|&(r, s, k)| {
            t.food_of(k) == 0
                && items.value(k) > 0.0
                && !weapons.contains(&(r, s))
                && !water.contains(&(r, s))
                && !items.is_water_container(k)
                && !super::needs::Needs::resting_sustenance(items, k)
                && !super::needs::is_pump(items, k)
                && !t.name_of(k).is_some_and(|n| site.productions.iter().any(|p| p.production.worked_by(n)))
        })
        .collect();
    candidates.sort_by(|a, b| items.value(b.2).partial_cmp(&items.value(a.2)).unwrap_or(core::cmp::Ordering::Equal));
    let kinds: Vec<usize> = candidates.iter().map(|c| c.2).collect();
    let mut loaded: Vec<usize> = Vec::new();
    for kind in kinds {
        if (0..group::size(seat.rival)).all(|m| group::pack(seat.rival, m).is_full()) {
            break;
        }
        let Some(m) = super::trading::cargo_carrier(seat.rival, items, kind) else { break };
        let Some((r, s, _)) = room_items(seat, l).into_iter().find(|&(_, _, k)| k == kind) else { continue };
        if let Some((k, rounds)) = remove_from_rooms(seat, l, alloc::vec![(r, s)]).pop() {
            give(group::pack_mut(seat.rival, m), k, rounds);
            loaded.push(k);
        }
    }
    if !loaded.is_empty() {
        let list = super::telemetry::grouped_ids(items.types, &loaded);
        seat.ai.telemetry.report(|| alloc::format!("filled city caravan from {} with high-value reserve cargo: {list}", super::telemetry::place(l)));
    }
}
