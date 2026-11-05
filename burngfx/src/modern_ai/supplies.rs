//! `TravelSupplies`: whether a group can make a road on what it has. At tag
//! `v1.2`, `AI/PlayerBehavior/Travel/TravelSupplies.cs`, with
//! `Group.DistributeToLowest` and `GetLowestAfterDistribution`.
//!
//! Everything is counted for the whole group: what each member has, and
//! what the group carries shared out to whoever has least.

use alloc::vec::Vec;

use super::route::Route;

/// `DistributeToLowest`: a unit at a time to the first of the lowest.
pub fn distribute_to_lowest(reserves: &mut [i32], supply: i32) {
    for _ in 0..supply.max(0) {
        let Some(min) = reserves.iter().copied().min() else { return };
        if let Some(v) = reserves.iter_mut().find(|v| **v == min) {
            *v += 1;
        }
    }
}

/// `GetLowestAfterDistribution`; nought for nobody.
pub fn lowest_after_distribution(reserves: &[i32], supply: i32) -> i32 {
    let mut r = reserves.to_vec();
    distribute_to_lowest(&mut r, supply);
    r.iter().copied().min().unwrap_or(0)
}

/// The group as the planner sees it: each member's food and water (leader
/// first), and what all of them carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupSupplies {
    pub food: Vec<i32>,
    pub water: Vec<i32>,
    pub max_food: Vec<i32>,
    pub max_water: Vec<i32>,
    pub health: Vec<i32>,
    pub food_carried: i32,
    pub water_carried: i32,
}

impl GroupSupplies {
    pub fn size(&self) -> usize {
        self.food.len()
    }

    pub fn lowest_food_with_inventory(&self) -> i32 {
        lowest_after_distribution(&self.food, self.food_carried)
    }

    pub fn lowest_water_with_inventory(&self) -> i32 {
        lowest_after_distribution(&self.water, self.water_carried)
    }
}

/// `HasRouteSupplies`: a day's food and water a member for each day of the
/// road; for an attack, a day less of food and as much water again for the
/// fight and the way out.
pub fn has_route_supplies(g: &GroupSupplies, route: &Route, hostile_target: bool, unavailable_food: i32, unavailable_water: i32) -> bool {
    let food_required = route.days + if hostile_target { (route.days - 1).max(0) } else { 0 };
    let water_required = route.days + if hostile_target { route.days.max(0) } else { 0 };
    lowest_after_distribution(&g.food, (g.food_carried - unavailable_food).max(0)) >= food_required
        && lowest_after_distribution(&g.water, (g.water_carried - unavailable_water).max(0)) >= water_required
}

/// `HasSettlementRoundTripFood`.
pub fn has_settlement_round_trip_food(g: &GroupSupplies, outbound: &Route, back: &Route) -> bool {
    g.lowest_food_with_inventory() >= outbound.days + back.days
}

/// `HasSettlementRoundTripSupplies`.
pub fn has_settlement_round_trip_supplies(
    g: &GroupSupplies,
    outbound: &Route,
    back: &Route,
    unavailable_food: i32,
    unavailable_water: i32,
) -> bool {
    let required = outbound.days + back.days;
    lowest_after_distribution(&g.food, (g.food_carried - unavailable_food).max(0)) >= required
        && lowest_after_distribution(&g.water, (g.water_carried - unavailable_water).max(0)) >= required
}

/// `ExpectedRecoveryRouteDamage`: 25 a day short of food or of water.
pub fn expected_recovery_route_damage(g: &GroupSupplies, route: &Route) -> i32 {
    let food_short = (route.days - g.lowest_food_with_inventory()).max(0);
    let water_short = (route.days - g.lowest_water_with_inventory()).max(0);
    (food_short + water_short) * 25
}

/// `CanSurviveRecoveryRoute`: nobody of the group dies on the way.
pub fn can_survive_recovery_route(g: &GroupSupplies, route: &Route) -> bool {
    g.health.iter().copied().min().unwrap_or(0) > expected_recovery_route_damage(g, route)
}

/// `IsDehydrationTravelNoWorseThanWaiting`. `local_water` is what staying
/// would add to the group's carried water: stored, the source's reserve, and
/// the day's surplus for as many days as the road takes.
pub fn dehydration_travel_no_worse_than_waiting(g: &GroupSupplies, route: &Route, local_water: i32) -> bool {
    if !g.water.iter().any(|&w| w == 0) {
        return false;
    }
    let road_short = (route.days - g.lowest_water_with_inventory()).max(0);
    let wait_short = (route.days - lowest_after_distribution(&g.water, g.water_carried + local_water)).max(0);
    road_short <= wait_short
}

/// `HasProjectedRecruitRouteSupplies`: with a recruit who sets off with the
/// group on the day hired.
pub fn has_projected_recruit_route_supplies(
    g: &GroupSupplies,
    route: &Route,
    recruit_food: i32,
    recruit_water: i32,
    unavailable_food: i32,
    unavailable_water: i32,
) -> bool {
    let mut food = g.food.clone();
    food.push(recruit_food);
    let mut water = g.water.clone();
    water.push(recruit_water);
    lowest_after_distribution(&food, (g.food_carried - unavailable_food).max(0)) >= route.days
        && lowest_after_distribution(&water, (g.water_carried - unavailable_water).max(0)) >= route.days
}

/// `HasProjectedRecruitSoloReturnFood`.
pub fn has_projected_recruit_solo_return_food(g: &GroupSupplies, outbound: &Route, recruit_food: i32, return_days: i32, unavailable_food: i32) -> bool {
    if g.size() != 1 {
        return true;
    }
    g.food[0] + recruit_food + (g.food_carried - unavailable_food).max(0) >= outbound.days * 2 + return_days
}

/// `ProjectSoloArrival`: `(arrival, remaining carried)`, or none when the
/// road eats more than is carried.
fn project_solo_arrival(reserve: i32, maximum: i32, inventory: i32, days: i32, arrival_minimum: i32) -> Option<(i32, i32)> {
    let inventory = inventory.max(0);
    let deficit = (days - reserve).max(0);
    if inventory < deficit {
        return None;
    }
    Some((maximum.min(arrival_minimum.max((reserve - days).max(0))), inventory - deficit))
}

#[allow(clippy::too_many_arguments)]
fn staged_recruit_stat(reserve: i32, maximum: i32, inventory: i32, city_days: i32, onward_days: i32, return_days: i32, city_minimum: i32, recruit: i32) -> bool {
    let Some((arrival, remaining)) = project_solo_arrival(reserve, maximum, inventory, city_days, city_minimum) else { return false };
    let onward = (onward_days - arrival).max(0) + (onward_days - recruit).max(0);
    if remaining < onward {
        return false;
    }
    (arrival - onward_days).max(0) + remaining - onward >= return_days
}

/// `HasStagedRecruitSettlementSupplies`: alone to a city, a recruit there
/// (the city tops the leader up to `city_minimum`), on to the camp and back.
#[allow(clippy::too_many_arguments)]
pub fn has_staged_recruit_settlement_supplies(
    g: &GroupSupplies,
    to_city: &Route,
    onward: &Route,
    settlement_return: &Route,
    city_minimum: i32,
    recruit_food: i32,
    recruit_water: i32,
    unavailable_food: i32,
    unavailable_water: i32,
) -> bool {
    g.size() == 1
        && staged_recruit_stat(g.food[0], g.max_food[0], g.food_carried - unavailable_food, to_city.days, onward.days, settlement_return.days, city_minimum, recruit_food)
        && staged_recruit_stat(g.water[0], g.max_water[0], g.water_carried - unavailable_water, to_city.days, onward.days, settlement_return.days, city_minimum, recruit_water)
}

/// `HasStagedRecruitOutboundSupplies`.
#[allow(clippy::too_many_arguments)]
pub fn has_staged_recruit_outbound_supplies(
    g: &GroupSupplies,
    to_stop: &Route,
    onward: &Route,
    arrival_minimum: i32,
    recruit_food: i32,
    recruit_water: i32,
    unavailable_food: i32,
    unavailable_water: i32,
    solo_food_return_days: i32,
) -> bool {
    g.size() == 1
        && staged_recruit_stat(g.food[0], g.max_food[0], g.food_carried - unavailable_food, to_stop.days, onward.days, solo_food_return_days, arrival_minimum, recruit_food)
        && staged_recruit_stat(g.water[0], g.max_water[0], g.water_carried - unavailable_water, to_stop.days, onward.days, 0, arrival_minimum, recruit_water)
}

/// `HasStagingCityReturnSupplies`.
pub fn has_staging_city_return_supplies(g: &GroupSupplies, to_city: &Route, back: &Route, city_minimum: i32, unavailable_food: i32, unavailable_water: i32) -> bool {
    let stat = |reserve, maximum, inventory| {
        project_solo_arrival(reserve, maximum, inventory, to_city.days, city_minimum).is_some_and(|(a, r)| a + r >= back.days)
    };
    g.size() == 1
        && stat(g.food[0], g.max_food[0], g.food_carried - unavailable_food)
        && stat(g.water[0], g.max_water[0], g.water_carried - unavailable_water)
}

/// `HasStagedRecruitCityReturnSupplies`: the whole group to a city, the
/// city's minimum there, a recruit, and everybody back.
#[allow(clippy::too_many_arguments)]
pub fn has_staged_recruit_city_return_supplies(
    g: &GroupSupplies,
    to_city: &Route,
    back: &Route,
    city_minimum: i32,
    recruit_food: i32,
    recruit_water: i32,
    unavailable_food: i32,
    unavailable_water: i32,
) -> bool {
    let stat = |reserves: &[i32], inventory: i32, recruit: i32| {
        let mut r = reserves.to_vec();
        distribute_to_lowest(&mut r, inventory.max(0));
        if r.iter().any(|&v| v < to_city.days) {
            return false;
        }
        r.iter().map(|&v| city_minimum.max(v - to_city.days)).chain([recruit]).min().unwrap_or(0) >= back.days
    };
    stat(&g.food, g.food_carried - unavailable_food, recruit_food) && stat(&g.water, g.water_carried - unavailable_water, recruit_water)
}

/// `HasTerritorialRouteSupplies`: supplied for the whole road; or, a leg at
/// a time, when the first step is a city or the player's own camp — and an
/// own camp only if it can rebuild the reserve the group lacks (a surplus of
/// one more than the group a day) — and the group is supplied to it.
#[allow(clippy::too_many_arguments)]
pub fn has_territorial_route_supplies(
    g: &GroupSupplies,
    sites: &super::camps::Sites,
    items: super::group::Items,
    player: usize,
    previous: Option<usize>,
    start: usize,
    route: &Route,
    hostile_target: bool,
) -> bool {
    use super::route::RouteMap;
    if has_route_supplies(g, route, hostile_target, 0, 0) {
        return true;
    }
    let next = route.next_step;
    let own = sites.owner(next) == Some(player);
    if !own && !sites.is_city(next) {
        return false;
    }
    if own {
        let food_required = route.days + if hostile_target { (route.days - 1).max(0) } else { 0 };
        let water_required = route.days + if hostile_target { route.days } else { 0 };
        let rebuild = g.size() as i32 + 1;
        let Some(site) = sites.get(next) else { return false };
        if g.lowest_food_with_inventory() < food_required && site.food_surplus(items) < rebuild {
            return false;
        }
        if g.lowest_water_with_inventory() < water_required && site.water_surplus() < rebuild {
            return false;
        }
    }
    super::route::find(sites, player, previous, start, next).is_some_and(|leg| has_route_supplies(g, &leg, false, 0, 0))
}

/// `HasProjectedRecruitTerritorialSupplies`: with the recruit, supplied
/// for the whole road; or for its first leg, when that leg ends in a city
/// or an own camp that can feed and water the bigger group.
#[allow(clippy::too_many_arguments)]
pub fn has_projected_recruit_territorial_supplies(
    g: &GroupSupplies,
    sites: &super::camps::Sites,
    items: super::group::Items,
    player: usize,
    previous: Option<usize>,
    start: usize,
    route: &Route,
    recruit_food: i32,
    recruit_water: i32,
) -> bool {
    use super::route::RouteMap;
    if has_projected_recruit_route_supplies(g, route, recruit_food, recruit_water, 0, 0) {
        return true;
    }
    let next = route.next_step;
    let city = sites.is_city(next);
    if sites.owner(next) != Some(player) && !city {
        return false;
    }
    let first = super::route::find(sites, player, previous, start, next);
    let size = g.size() as i32 + 1;
    let sustainable = city
        || sites.get(next).is_some_and(|s| {
            (s.food_surplus(items) >= size || s.stored_food(items) >= size * 3) && s.can_provision_group_water(items, size)
        });
    sustainable && first.is_some_and(|leg| has_projected_recruit_route_supplies(g, &leg, recruit_food, recruit_water, 0, 0))
}
