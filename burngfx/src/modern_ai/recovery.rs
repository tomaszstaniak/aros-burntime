//! Where the opponent goes to recover: `RecoveryServices.FindDestination`,
//! `FindLastChanceDestination`, `CanProvisionReturnTrip` and the local
//! checks. At tag `v1.2`, `AI/PlayerBehavior/Group/RecoveryServices.cs`.

use alloc::vec::Vec;

use crate::rival::Rival;

use super::group::{self, CITY_MINIMUM};
use super::needs::World;
use super::route::{self, Route, RouteMap};
use super::supplies::{self, distribute_to_lowest};

/// `TripMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trip {
    Normal,
    Escape,
}

fn foods(r: &Rival) -> Vec<i32> {
    (0..group::size(r)).map(|m| group::needs(r, m).food).collect()
}

fn waters(r: &Rival) -> Vec<i32> {
    (0..group::size(r)).map(|m| group::needs(r, m).water).collect()
}

/// `CanCoverReturnRoute`.
fn can_cover_return(reserves: &[i32], carried: i32, outbound: i32, back: i32, stock: i32, can_build: bool, floor: i32) -> bool {
    let mut r = reserves.to_vec();
    distribute_to_lowest(&mut r, carried);
    for v in r.iter_mut() {
        *v = floor.max((*v - outbound).max(0));
    }
    if can_build {
        return true;
    }
    distribute_to_lowest(&mut r, stock);
    r.iter().copied().min().unwrap_or(0) >= back
}

/// `CanProvisionReturnTrip`.
pub fn can_provision_return_trip(w: &World, r: &Rival, destination: usize, outbound: &Route, back: &Route) -> bool {
    let items = w.items;
    let site = &w.sites.sites[destination];
    let size = group::size(r) as i32;
    let owned_camp = !site.city && site.owner == Some(w.player);
    let food_surplus = if owned_camp && site.current.is_some() { site.food_surplus(items) } else { 0 };
    let floor = if site.city && w.owned_camps() > 0 { CITY_MINIMUM } else { 0 };
    let food_carried: i32 = group::carried(r).iter().map(|&(_, _, k)| items.types.food_of(k)).sum();
    let water_carried: i32 = group::carried(r).iter().map(|&(_, _, k)| items.types.water_of(k)).sum();
    let food = can_cover_return(&foods(r), food_carried, outbound.days, back.days, 0, food_surplus > size, floor);
    let water_surplus = if !site.city { site.water_surplus() } else { 0 };
    let water = can_cover_return(&waters(r), water_carried, outbound.days, back.days, 0, water_surplus > size, floor);
    food && water
}

fn provisions(w: &World, l: usize, size: i32) -> bool {
    let s = &w.sites.sites[l];
    s.owner == Some(w.player) && s.can_provision_food(w.items) && s.can_provision_group_water(w.items, size)
}

/// `HasSafeOnwardRecovery`.
fn safe_onward_recovery(w: &World, r: &Rival, previous: Option<usize>, destination: usize, outbound: &Route) -> bool {
    let size = group::size(r) as i32;
    (0..w.sites.sites.len())
        .filter(|&l| l != destination && provisions(w, l, size))
        .filter_map(|l| route::find(w.sites, w.player, previous, destination, l))
        .any(|onward| can_provision_return_trip(w, r, destination, outbound, &onward))
}

/// `IsStableRecoveryDestination`.
fn stable(w: &World, r: &Rival, previous: Option<usize>, destination: usize, outbound: &Route) -> bool {
    let s = &w.sites.sites[destination];
    if s.city && w.owned_camps() > 0 {
        return safe_onward_recovery(w, r, previous, destination, outbound);
    }
    if provisions(w, destination, group::size(r) as i32) {
        return true;
    }
    safe_onward_recovery(w, r, previous, destination, outbound)
}

/// `HelpScore`.
fn help_score(w: &World, r: &Rival, l: usize, need: (bool, bool, bool), starving: bool, dehydrated: bool, can_pay: bool, doctor_at: &dyn Fn(usize) -> bool) -> i32 {
    let items = w.items;
    let s = &w.sites.sites[l];
    let (needs_food, needs_water, needs_doctor) = need;
    let mut score = 0;
    let city_aid = s.city && w.owned_camps() > 0;
    let owned = !s.city && s.owner == Some(w.player);
    let producing = owned && s.current.is_some() && s.food_surplus(items) > 0;
    let n = group::size(r);
    if needs_food && (producing || city_aid && (0..n).any(|m| group::needs(r, m).food < CITY_MINIMUM)) {
        score += if starving { 10 } else { 3 };
    }
    let replenishing = !s.city && s.water_surplus() > 0;
    let empties = group::carried(r).iter().any(|&(_, _, k)| items.is_empty_water_container(k));
    if needs_water && (replenishing || city_aid && ((0..n).any(|m| group::needs(r, m).water < CITY_MINIMUM) || empties)) {
        score += if dehydrated { 10 } else { 1 };
    }
    if needs_doctor {
        if doctor_at(l) {
            score += if can_pay { 8 } else { 2 };
        }
        if !can_pay && s.owner == Some(w.player) && s.stored.iter().any(|&k| items.types.heal_of(k) > 0) {
            score += 7;
        }
    }
    score
}

/// `NeedsRecovery`.
pub fn needs_recovery(r: &Rival, types: &crate::items::ItemTypes, water_days: i32) -> bool {
    let g = group::supplies(r, types);
    (0..group::size(r)).any(|m| group::needs(r, m).health < 40) || g.lowest_food_with_inventory() <= 3 || g.lowest_water_with_inventory() <= water_days
}

/// `NeedsCityRecoveryStaging`: in a city, no own provisioning camp reached
/// on supplies.
pub fn needs_city_recovery_staging(w: &World, r: &Rival, previous: Option<usize>) -> bool {
    if !w.sites.is_city(w.here) || w.owned_camps() == 0 {
        return false;
    }
    let g = group::supplies(r, w.items.types);
    let size = group::size(r) as i32;
    !(0..w.sites.sites.len())
        .filter(|&l| provisions(w, l, size))
        .filter_map(|l| route::find(w.sites, w.player, previous, w.here, l))
        .any(|rt| supplies::has_route_supplies(&g, &rt, false, 0, 0))
}

/// `FindDestination`.
pub fn find_destination(w: &World, r: &Rival, previous: Option<usize>, require_reachable: bool, trip: Trip, doctor_at: &dyn Fn(usize) -> bool) -> Option<usize> {
    let items = w.items;
    let g = group::supplies(r, items.types);
    let n = group::size(r);
    let needs_food = g.lowest_food_with_inventory() <= 3;
    let needs_water = g.lowest_water_with_inventory() <= w.policy.recovery_water_days;
    let needs_doctor = (0..n).any(|m| group::needs(r, m).health <= 40);
    let starving = (0..n).any(|m| group::needs(r, m).food <= 1);
    let dehydrated = (0..n).any(|m| group::needs(r, m).water == 0);
    let can_pay = group::carried(r).iter().any(|&(_, _, k)| items.types.heal_of(k) > 0);
    let escape = trip == Trip::Escape;
    let mut best: Option<(usize, bool, i32, i32)> = None;
    for l in 0..w.sites.sites.len() {
        let s = &w.sites.sites[l];
        if l == w.here || !(if escape { !s.city && s.owner == Some(w.player) } else { s.city || s.owner.is_none() || s.owner == Some(w.player) }) {
            continue;
        }
        let help = help_score(w, r, l, (needs_food, needs_water, needs_doctor), starving, dehydrated, can_pay, doctor_at);
        if help <= 0 {
            continue;
        }
        let Some(rt) = route::find(w.sites, w.player, previous, w.here, l) else { continue };
        let back = if escape || starving || dehydrated { None } else { route::find(w.sites, w.player, None, l, w.here) };
        let is_stable = stable(w, r, previous, l, &rt);
        if escape && !is_stable {
            continue;
        }
        let viable = is_stable || escape || starving || dehydrated || back.as_ref().is_some_and(|b| can_provision_return_trip(w, r, l, &rt, b));
        if !viable {
            continue;
        }
        let reachable = !require_reachable || supplies::has_route_supplies(&g, &rt, false, 0, 0) || !escape && supplies::can_survive_recovery_route(&g, &rt);
        if !reachable {
            continue;
        }
        let key = (is_stable, help, rt.days);
        if best.is_none_or(|b| (key.0, key.1, -key.2) > (b.1, b.2, -b.3)) {
            best = Some((l, key.0, key.1, key.2));
        }
    }
    best.map(|b| b.0)
}

/// `CanSustainLocally`.
pub fn can_sustain_locally(w: &World, r: &Rival) -> bool {
    let n = group::size(r);
    let needs_food = (0..n).any(|m| group::needs(r, m).food <= 3);
    let needs_water = (0..n).any(|m| group::needs(r, m).water <= 2);
    let s = &w.sites.sites[w.here];
    let own = s.owner == Some(w.player);
    let size = n as i32;
    (!needs_food || own && s.food_surplus(w.items) >= size) && (!needs_water || own && s.water_surplus() >= size)
}

/// `WaitingForSuppliesWillBeFatal`.
pub fn waiting_will_be_fatal(w: &World, r: &Rival) -> bool {
    if can_sustain_locally(w, r) {
        return false;
    }
    let n = group::size(r);
    (0..n).any(|m| group::needs(r, m).food <= 1) || (0..n).any(|m| group::needs(r, m).water == 0)
}

/// `CanRecoverLocallyForTravel`.
pub fn can_recover_locally_for_travel(w: &World, r: &Rival) -> bool {
    if !can_sustain_locally(w, r) {
        return false;
    }
    let n = group::size(r);
    let size = n as i32;
    let needs_food = (0..n).any(|m| group::needs(r, m).food <= 3);
    let needs_water = (0..n).any(|m| group::needs(r, m).water <= 2);
    let s = &w.sites.sites[w.here];
    let neighbour_food = s.roads.iter().any(|&(nb, d)| d > 0 && w.sites.owner(nb) == Some(w.player) && w.sites.sites[nb].food_surplus(w.items) > 0);
    (!needs_food || neighbour_food || s.food_surplus(w.items) > size) && (!needs_water || s.water_surplus() > size)
}

/// `FindLastChanceDestination`: starving or parched, the place whose road
/// costs least health, then helps most, then is nearest.
pub fn last_chance_destination(w: &World, r: &Rival, previous: Option<usize>) -> Option<usize> {
    let n = group::size(r);
    let starving = (0..n).any(|m| group::needs(r, m).food <= 1);
    let dehydrated = (0..n).any(|m| group::needs(r, m).water == 0);
    if !starving && !dehydrated {
        return None;
    }
    let g = group::supplies(r, w.items.types);
    let mut best: Option<(usize, i32, i32, i32)> = None;
    for l in 0..w.sites.sites.len() {
        let s = &w.sites.sites[l];
        if l == w.here || !(s.city || s.owner.is_none() || s.owner == Some(w.player)) {
            continue;
        }
        let owned = !s.city && s.owner == Some(w.player);
        let mut help = 0;
        if starving && (owned && s.can_provision_food(w.items) || s.city && w.owned_camps() > 0) {
            help += 10;
        }
        if dehydrated && (!s.city && s.water_surplus() > 0 || s.city && w.owned_camps() > 0) {
            help += 10;
        }
        if help <= 0 {
            continue;
        }
        let Some(rt) = route::find(w.sites, w.player, previous, w.here, l) else { continue };
        let damage = supplies::expected_recovery_route_damage(&g, &rt);
        if best.is_none_or(|b| (damage, -help, rt.days) < (b.1, -b.2, b.3)) {
            best = Some((l, damage, help, rt.days));
        }
    }
    best.map(|b| b.0)
}
