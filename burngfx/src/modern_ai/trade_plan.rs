//! Where and whether the opponent trades: `Trading.Planning`,
//! `Trading.AddCandidates`, `RouteOpportunities`, `CampDelivery` and
//! `CargoManagement.FillCityCaravan`. At tag `v1.2`,
//! `AI/PlayerBehavior/Trading/` and `Travel/RouteOpportunities.cs`.

use alloc::vec::Vec;

use super::camps::trip_value_per_day;
use super::decision::{add_travel, Action, Decision, Risk, Traveller};
use super::group;
use super::needs::TraderView;
use super::reason::Reason;
use super::route::{self, Route, RouteMap};
use super::state::ModernAi;
use super::trading::{self, Context};

/// `Trading.MaximumCaravanPeople`.
pub const MAXIMUM_CARAVAN_PEOPLE: usize = 2;

fn here(cx: &Context) -> usize {
    cx.w.here
}

fn player(cx: &Context) -> usize {
    cx.w.player
}

fn route(cx: &Context, previous: Option<usize>, from: usize, to: usize) -> Option<Route> {
    route::find(cx.w.sites, player(cx), previous, from, to)
}

fn free_slots(cx: &Context) -> usize {
    (0..group::size(cx.r)).map(|m| group::pack(cx.r, m).capacity() - group::pack(cx.r, m).len()).sum()
}

fn the_shop<'c>(cx: &'c Context, city: usize) -> Option<&'c TraderView> {
    cx.w.traders.iter().find(|t| t.shop == Some(city))
}

/// `TraderOpportunityScore`: the three most wanted kinds he carries, the
/// first whole and the others at a third and more.
pub fn trader_opportunity_score(cx: &Context, t: &TraderView) -> f32 {
    let mut best: Vec<(usize, f32)> = Vec::new();
    for s in 0..t.stock.len() {
        let Some(k) = t.stock.get(s) else { continue };
        let p = trading::shopping_priority(cx, k, t.stock.rounds_left(s));
        match best.iter_mut().find(|(x, _)| *x == k) {
            Some(e) => e.1 = e.1.max(p),
            None => best.push((k, p)),
        }
    }
    let mut ps: Vec<f32> = best.into_iter().map(|(_, p)| p).filter(|&p| p > 0.0).collect();
    ps.sort_by(|a, b| b.partial_cmp(a).unwrap_or(core::cmp::Ordering::Equal));
    ps.truncate(3);
    match ps.first() {
        None => 0.0,
        Some(&first) => first + ps[1..].iter().sum::<f32>() * 0.35,
    }
}

/// `TraderAssortmentOpportunityScore`: the three most wanted of what he may
/// stock, and how likely the useful trap parts or a snake trap turn up.
pub fn trader_assortment_score(cx: &Context, t: &TraderView) -> f32 {
    let (mut a, mut b, mut c) = (0.0f32, 0.0f32, 0.0f32);
    for &k in &t.assortment {
        let p = trading::priority_of(cx, k);
        if p > a {
            c = b;
            b = a;
            a = p;
        } else if p > b {
            c = b;
            b = p;
        } else if p > c {
            c = p;
        }
    }
    if a <= 0.0 {
        return 0.0;
    }
    let types = cx.w.items.types;
    let n = t.assortment.len().max(1) as f32;
    let meat_inputs = t
        .assortment
        .iter()
        .filter(|&&k| types.name_of(k).is_some_and(|nm| ["item_spring", "item_tin", "item_wire"].contains(&nm) && trading::construction_material_priority(cx, nm) > 0.0))
        .count() as f32;
    let snake = types.index_of("item_snake_trap");
    let snake_needed = snake.is_some_and(|k| cx.needs.needs_production(cx.w, k));
    let snake_likelihood = if t.assortment.iter().any(|&k| Some(k) == snake) && snake_needed { 1000.0 / n } else { 0.0 };
    a + (b + c) * 0.25 + meat_inputs * 600.0 / n + snake_likelihood
}

/// `FindBestTradeCity`: a city with a shop, by the cheapest supported trade
/// route; among equals, the most promising assortment and novelty, then the
/// nearest.
pub fn best_trade_city(cx: &Context, ai: &ModernAi, previous: Option<usize>) -> Option<usize> {
    let started = ai.telemetry.now();
    let city = find_best_trade_city(cx, ai, previous);
    if let (Some(s), Some(e)) = (started, ai.telemetry.now()) {
        if e - s >= 10 {
            ai.telemetry.report(|| alloc::format!("FindBestTradeCity took {} ms", e - s));
        }
    }
    city
}

fn find_best_trade_city(cx: &Context, ai: &ModernAi, previous: Option<usize>) -> Option<usize> {
    let mut reachable: Vec<(usize, Route, &TraderView)> = Vec::new();
    for t in cx.w.traders.iter().filter(|t| t.alive) {
        let Some(city) = t.shop.filter(|&c| c != here(cx) && cx.w.sites.is_city(c)) else { continue };
        if let Some(r) = route::find_supported_trade_route(cx.w.sites, player(cx), previous, here(cx), city) {
            reachable.push((city, r, t));
        }
    }
    // In the order of the world's places, as upstream walks them.
    reachable.sort_by_key(|(c, _, _)| *c);
    let best = reachable.iter().map(|(_, r, _)| r.effective_cost).min()?;
    let safest: Vec<&(usize, Route, &TraderView)> = reachable.iter().filter(|(_, r, _)| r.effective_cost == best).collect();
    if safest.len() <= 1 {
        return safest.first().map(|x| x.0);
    }
    let mut top: Option<(usize, f32, i32)> = None;
    for (c, r, t) in safest {
        let score = trader_assortment_score(cx, t) + super::support::trader_novelty(ai, t.id, cx.w.day) as f32;
        if top.is_none_or(|b| score > b.1 || score == b.1 && r.days < b.2) {
            top = Some((*c, score, r.days));
        }
    }
    top.map(|t| t.0)
}

/// `FindBestRegionalTradeStop`: from an own camp with a prepared lot, a
/// roaming trader within four days (and no farther than the city), the
/// best score less 25 a day.
pub fn best_regional_trade_stop(cx: &Context, ai: &ModernAi, previous: Option<usize>, city: Option<usize>) -> Option<usize> {
    if cx.w.sites.owner(here(cx)) != Some(player(cx)) || !has_prepared_trade_cargo(cx, previous) {
        return None;
    }
    let city_days = city.and_then(|c| route(cx, previous, here(cx), c)).map_or(i32::MAX, |r| r.days);
    let mut best: Option<(usize, f32, i32)> = None;
    for l in 0..cx.w.sites.sites.len() {
        if cx.w.sites.is_city(l) || l == here(cx) || cx.w.sites.owner(l).is_some_and(|o| o != player(cx)) {
            continue;
        }
        let traders: Vec<&TraderView> = cx.w.traders.iter().filter(|t| t.alive && t.at == l && t.shop.is_none()).collect();
        let Some(r) = route(cx, previous, here(cx), l) else { continue };
        if traders.is_empty() || r.days > city_days.min(4) {
            continue;
        }
        let score = traders.iter().map(|t| trader_assortment_score(cx, t) + super::support::trader_novelty(ai, t.id, cx.w.day) as f32).fold(f32::MIN, f32::max);
        if score <= 0.0 {
            continue;
        }
        let value = score - (r.days * 25) as f32;
        if best.is_none_or(|b| value > b.1 || value == b.1 && r.days < b.2) {
            best = Some((l, value, r.days));
        }
    }
    best.map(|b| b.0)
}

/// `SurvivalSafeEconomicCapital`: what may be sold, food and containers
/// above three days each, materials, and every melee weapon but one.
fn survival_safe_capital(cx: &Context) -> f32 {
    let items = cx.w.items;
    let t = items.types;
    let carried = group::carried(cx.r);
    let mut spendable: Vec<bool> = carried.iter().map(|&(m, s, _)| trading::can_sell(cx, m, s)).collect();
    let size = group::size(cx.r) as i32;
    let mut food = (0..group::size(cx.r)).map(|m| group::needs(cx.r, m).food).sum::<i32>() + carried.iter().map(|&(_, _, k)| t.food_of(k)).sum::<i32>();
    let mut order: Vec<usize> = (0..carried.len()).filter(|&i| t.food_of(carried[i].2) > 0 && !spendable[i]).collect();
    order.sort_by(|&a, &b| items.value(carried[a].2).partial_cmp(&items.value(carried[b].2)).unwrap_or(core::cmp::Ordering::Equal));
    for i in order {
        let f = t.food_of(carried[i].2);
        if food - f < size * 3 {
            continue;
        }
        spendable[i] = true;
        food -= f;
    }
    let mut water = (0..group::size(cx.r)).map(|m| group::needs(cx.r, m).water).sum::<i32>()
        + carried.iter().filter(|&&(_, _, k)| items.is_water_container(k)).map(|&(_, _, k)| items.water_capacity(k)).sum::<i32>();
    let mut order: Vec<usize> = (0..carried.len()).filter(|&i| items.is_water_container(carried[i].2) && !spendable[i]).collect();
    order.sort_by(|&a, &b| items.value(carried[a].2).partial_cmp(&items.value(carried[b].2)).unwrap_or(core::cmp::Ordering::Equal));
    for i in order {
        let c = items.water_capacity(carried[i].2);
        if water - c < size * 3 {
            continue;
        }
        spendable[i] = true;
        water -= c;
    }
    for (i, &(_, _, k)) in carried.iter().enumerate() {
        if t.name_of(k).is_some_and(|n| super::production::CONSTRUCTION_MATERIALS.contains(&n)) {
            spendable[i] = true;
        }
    }
    let mut melee: Vec<usize> = (0..carried.len()).filter(|&i| t.damage_of(carried[i].2) > 0 && !super::equipment::is_firearm(t, carried[i].2) && !spendable[i]).collect();
    melee.sort_by(|&a, &b| items.value(carried[a].2).partial_cmp(&items.value(carried[b].2)).unwrap_or(core::cmp::Ordering::Equal));
    let keep = melee.len().saturating_sub(1);
    for &i in melee.iter().take(keep) {
        spendable[i] = true;
    }
    carried.iter().enumerate().filter(|(i, _)| spendable[*i]).map(|(_, &(_, _, k))| items.value(k)).sum()
}

/// `ReachableHighReturnPurchaseTypes`: what the reachable cities' shops may
/// stock that is a high-return purchase, one of each kind.
fn reachable_high_return_types(cx: &Context, previous: Option<usize>) -> Vec<usize> {
    let mut out = Vec::new();
    let mut shops: Vec<&TraderView> = cx.w.traders.iter().filter(|t| t.shop.is_some_and(|c| c != here(cx) && cx.w.sites.is_city(c))).collect();
    shops.sort_by_key(|t| t.shop);
    for t in shops {
        let Some(c) = t.shop else { continue };
        if route(cx, previous, here(cx), c).is_none() {
            continue;
        }
        for &k in &t.assortment {
            if trading::is_high_return_reserve_purchase(cx, k) && !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// `HasAffordableHighReturnTradeCargo`.
pub fn has_affordable_high_return_cargo(cx: &Context, previous: Option<usize>) -> bool {
    let power = survival_safe_capital(cx) * cx.barter.trade_factor();
    power > 0.0 && reachable_high_return_types(cx, previous).iter().any(|&k| cx.w.items.value(k) <= power)
}

/// `HasSubstantialTradeCargo`.
pub fn has_substantial_trade_cargo(cx: &Context) -> bool {
    let capital = trading::trade_capital(cx);
    trading::is_substantial_trade_lot(capital.len() as i32, capital.iter().map(|&(_, _, k)| cx.w.items.value(k)).sum())
}

/// `HasPreparedTradeCargo`.
pub fn has_prepared_trade_cargo(cx: &Context, previous: Option<usize>) -> bool {
    if !reachable_high_return_types(cx, previous).is_empty() {
        has_affordable_high_return_cargo(cx, previous)
    } else {
        has_substantial_trade_cargo(cx)
    }
}

/// `DesiredCaravanSlots`: the slots of the first two of the group.
fn desired_caravan_slots(cx: &Context) -> usize {
    (0..group::size(cx.r).min(MAXIMUM_CARAVAN_PEOPLE)).map(|m| group::pack(cx.r, m).capacity()).sum()
}

/// `IsTradeCaravanReady`.
pub fn is_trade_caravan_ready(cx: &Context, previous: Option<usize>) -> bool {
    let occupied: usize = (0..group::size(cx.r)).map(|m| group::pack(cx.r, m).len()).sum();
    has_affordable_high_return_cargo(cx, previous) || has_substantial_trade_cargo(cx) && occupied >= desired_caravan_slots(cx)
}

/// `RouteOpportunities.FindCityTradePickupCamp`: an own camp on the way to
/// the city whose pickup makes a substantial lot, the best value a day.
pub fn city_trade_pickup_camp(cx: &Context, previous: Option<usize>, city: Option<usize>) -> Option<usize> {
    let free = free_slots(cx);
    if free == 0 || has_prepared_trade_cargo(cx, previous) {
        return None;
    }
    let city = city?;
    let capital = trading::trade_capital(cx);
    let capital_value: f32 = capital.iter().map(|&(_, _, k)| cx.w.items.value(k)).sum();
    let mut best: Option<(usize, f32, bool, bool, i32)> = None;
    for l in 0..cx.w.sites.sites.len() {
        if cx.w.sites.owner(l) != Some(player(cx)) || l == here(cx) {
            continue;
        }
        let (Some(to_camp), Some(to_city)) = (route(cx, previous, here(cx), l), route(cx, None, l, city)) else { continue };
        let neighbours = cx.w.sites.sites[l].roads.iter().any(|&(n, _)| n == city);
        let (value, slots) = trading::projected_camp_collectibles(cx, &cx.w.sites.sites[l], to_camp.days);
        if slots <= 0 || !trading::is_substantial_trade_lot(capital.len() as i32 + slots, capital_value + value) {
            continue;
        }
        let days = to_camp.days + to_city.days;
        let per_day = trip_value_per_day(value, days);
        let fills = slots as usize >= free;
        let key = (per_day, fills, neighbours, -days);
        let better = best.is_none_or(|b| {
            key.0 > b.1 || key.0 == b.1 && (key.1 && !b.2 || key.1 == b.2 && (key.2 && !b.3 || key.2 == b.3 && key.3 > -b.4))
        });
        if better {
            best = Some((l, per_day, fills, neighbours, days));
        }
    }
    best.map(|b| b.0)
}

/// `ShouldVisitTrader`.
pub fn should_visit_trader(cx: &Context, ai: &ModernAi, previous: Option<usize>, city: Option<usize>) -> bool {
    let in_city = cx.w.sites.is_city(here(cx));
    if in_city {
        return if has_prepared_trade_cargo(cx, previous) { city.is_some() } else { city_trade_pickup_camp(cx, previous, city).is_some() };
    }
    if city.is_none() {
        return false;
    }
    let settlement = ai.target_was_neutral && ai.target.is_some();
    let attack = ai.target.is_some_and(|t| super::attack::is_hostile(cx.w.sites, t, player(cx)));
    if settlement || attack {
        return false;
    }
    has_prepared_trade_cargo(cx, previous) || city_trade_pickup_camp(cx, previous, city).is_some()
}

/// `EncounteredTraders`: the city's shop and every roaming trader here with
/// goods, the most promising first.
pub fn encountered_traders<'w>(cx: &'w Context<'w>) -> Vec<&'w TraderView> {
    let mut out: Vec<&TraderView> = Vec::new();
    if cx.w.sites.is_city(here(cx)) {
        if let Some(t) = the_shop(cx, here(cx)) {
            out.push(t);
        }
    }
    for t in cx.w.traders.iter().filter(|t| t.alive && t.at == here(cx) && t.stock.len() > 0 && t.shop.is_none()) {
        if !out.iter().any(|x| x.index == t.index) {
            out.push(t);
        }
    }
    let mut scored: Vec<(f32, &TraderView)> = out.into_iter().map(|t| (trader_opportunity_score(cx, t), t)).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, t)| t).collect()
}

/// `ShouldContinueTrading`: in a city, a trader here a plan can be made with.
pub fn should_continue_trading(cx: &Context) -> bool {
    cx.w.sites.is_city(here(cx)) && encountered_traders(cx).iter().any(|t| trading::create_trade_plan(cx, &t.stock, &[], true).is_some())
}

/// `ShouldFillCityCaravanBeforeDeparture`.
pub fn should_fill_city_caravan(cx: &Context, previous: Option<usize>, city: Option<usize>) -> bool {
    let site = &cx.w.sites.sites[here(cx)];
    if site.owner != Some(player(cx)) || free_slots(cx) == 0 || city.is_none() || is_trade_caravan_ready(cx, previous) {
        return false;
    }
    let Some(p) = site.current.and_then(|i| site.productions.get(i)) else { return false };
    let rate = site.rate(cx.w.items, p, p.installed, site.guards.len() as i32);
    let export = (rate.food_per_day - site.living_guards(player(cx))).max(0);
    rate.item_drop_interval > 0.0 && export > group::size(cx.r) as i32
}

/// `ShouldReduceTradeCaravan`.
pub fn should_reduce_trade_caravan(cx: &Context) -> bool {
    let site = &cx.w.sites.sites[here(cx)];
    if site.owner != Some(player(cx)) || group::size(cx.r) <= MAXIMUM_CARAVAN_PEOPLE || !super::production::prefer_production(site) {
        return false;
    }
    super::reinforcement::can_support_additional_guard(cx.w.items, cx.w.sites, player(cx), cx.w.policy, site, site.living_guards(player(cx)))
}

/// `RecoveryServices.CanProvisionReturnTrip` (also used by the trade
/// routes): the group can come home from `destination`.
pub fn can_provision_return_trip(cx: &Context, destination: usize, outbound: &Route, back: &Route) -> bool {
    super::recovery::can_provision_return_trip(cx.w, cx.r, destination, outbound, back)
}

/// `RouteOpportunities.HasProvisionedReturn`.
fn has_provisioned_return(cx: &Context, previous: Option<usize>, destination: Option<usize>) -> bool {
    let Some(d) = destination else { return false };
    let outbound = if cx.w.sites.is_city(d) {
        route::find_supported_trade_route(cx.w.sites, player(cx), previous, here(cx), d)
    } else {
        route(cx, previous, here(cx), d)
    };
    let Some(outbound) = outbound else { return false };
    let size = group::size(cx.r) as i32;
    (0..cx.w.sites.sites.len())
        .filter(|&l| {
            let s = &cx.w.sites.sites[l];
            s.owner == Some(player(cx)) && s.can_provision_food(cx.w.items) && s.can_provision_group_water(cx.w.items, size)
        })
        .filter_map(|l| route(cx, None, d, l))
        .any(|back| can_provision_return_trip(cx, d, &outbound, &back))
}

/// `RouteOpportunities.AddCityTradeCandidate`.
pub fn add_city_trade_candidate(cx: &Context, ai: &ModernAi, t: &Traveller, out: &mut Vec<Decision>, city: Option<usize>, score: f32) {
    let previous = t.previous;
    let pickup = city_trade_pickup_camp(cx, previous, city);
    let regional = if pickup.is_none() { best_regional_trade_stop(cx, ai, previous, city) } else { None };
    let destination = pickup.or(regional).or(city);
    if !has_provisioned_return(cx, previous, destination) {
        return;
    }
    let Some(d) = destination else { return };
    let known = if cx.w.sites.is_city(d) { route::find_supported_trade_route(cx.w.sites, player(cx), previous, here(cx), d) } else { route(cx, previous, here(cx), d) };
    let reason = if pickup.is_some() {
        Reason::FillCaravanAtCamp
    } else if regional.is_some() {
        Reason::OfferToRoamingTrader
    } else {
        Reason::DeliverSurplusGoods
    };
    if known.is_none() {
        return;
    }
    add_travel(t, out, Some(d), score, reason, Risk::default(), known, false);
}

/// `FindNearestCity`.
pub fn nearest_city(cx: &Context, previous: Option<usize>) -> Option<usize> {
    (0..cx.w.sites.sites.len())
        .filter(|&l| cx.w.sites.is_city(l) && l != here(cx))
        .filter_map(|l| route(cx, previous, here(cx), l).map(|r| (l, r.days)))
        .min_by_key(|&(l, d)| (d, l))
        .map(|x| x.0)
}

/// What `Trading.AddCandidates` reads beyond the context.
pub struct TradeInputs {
    pub target: Option<usize>,
    pub preparing_attack: bool,
    pub visit_trader: bool,
    pub best_trade_city: Option<usize>,
    pub needs_expansion_tool: bool,
    pub improvement_return: f32,
    pub snake_campaign: bool,
}

/// `Trading.AddCandidates`.
/// Returns, when the trading candidates were weighed and a clock listens,
/// the times after the campaign, the continue-trading check and the trade
/// candidate (for `Trading.AddCandidates took …`).
pub fn add_candidates(cx: &Context, ai: &ModernAi, t: &Traveller, inputs: &TradeInputs, delivery: Option<(usize, f32, bool, Route)>, out: &mut Vec<Decision>) -> Option<[u64; 3]> {
    let settlement = ai.target_was_neutral && ai.target.is_some();
    if inputs.target.is_some_and(|l| !cx.w.sites.is_city(l) && cx.w.sites.owner(l).is_none()) && settlement && group::size(cx.r) > 1 {
        return None;
    }
    let size = group::size(cx.r) as i32;
    let carried: i32 = group::carried(cx.r).iter().filter(|&&(_, _, k)| cx.w.items.is_water_container(k)).map(|&(_, _, k)| cx.w.items.water_capacity(k)).sum();
    let needs_attack_water = carried < super::needs::desired_water_container_capacity(size);
    if inputs.preparing_attack && !needs_attack_water {
        return None;
    }
    let policy = cx.w.policy;
    let early = cx.w.owned_camps() < 3;
    let growth = inputs.improvement_return > 0.01;
    let funded_snake = inputs.snake_campaign && has_affordable_high_return_cargo(cx, t.previous);
    let t_campaign = ai.telemetry.now();
    let bonus = (inputs.improvement_return * 100.0).min(250.0);
    let prepared = if inputs.preparing_attack {
        1250.0
    } else if inputs.target.is_none() || growth {
        f32::INFINITY
    } else {
        300.0
    };
    let continue_trading = should_continue_trading(cx);
    let t_continue = ai.telemetry.now();
    if continue_trading {
        let base = if inputs.needs_expansion_tool { policy.expansion_economy_score } else if early { 900.0 } else { 740.0 };
        out.push(Decision::new(Action::Wait, prepared.min(base) + bonus, Some(here(cx)), Reason::ContinueTrading));
    } else if inputs.visit_trader {
        let city = inputs.best_trade_city.or_else(|| nearest_city(cx, t.previous));
        let base = if inputs.needs_expansion_tool { policy.expansion_economy_score } else if early { 880.0 } else { 720.0 };
        let mut score = prepared.min(base) + bonus;
        if funded_snake {
            score = score.max(1850.0);
        }
        if !inputs.preparing_attack && should_reduce_trade_caravan(cx) {
            out.push(Decision::new(Action::StationTradeFollower, score + 20.0, Some(here(cx)), Reason::LeaveFollowerForCaravan));
        } else if should_fill_city_caravan(cx, t.previous, inputs.best_trade_city) {
            out.push(Decision::new(Action::Wait, score, Some(here(cx)), Reason::WaitToFillCaravan));
        } else {
            add_city_trade_candidate(cx, ai, t, out, city, score);
        }
    }
    let t_trade = ai.telemetry.now();
    let base = if inputs.needs_expansion_tool { policy.expansion_economy_score - 20.0 } else if early { 840.0 } else { 690.0 };
    if let Some((camp, gain, critical, r)) = delivery {
        let base_score = prepared.min(base) + bonus;
        let score = (if critical { base_score.max(2200.0) } else { base_score }) + (gain * 180.0).min(400.0);
        let reason = if critical { Reason::SecureWaterWaypoint } else { Reason::DeliverUpgrade { gain } };
        add_travel(t, out, Some(camp), score, reason, Risk::default(), Some(r), false);
    }
    Some([t_campaign?, t_continue?, t_trade?])
}
