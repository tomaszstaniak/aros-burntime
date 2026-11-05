//! The opponent at a trader's: what it would buy, what it may give, the
//! plan, and the exchange. At tag `v1.2`,
//! `AI/PlayerBehavior/Trading/Trading.Priorities.cs` and
//! `Trading.Exchange.cs`, with `RuleFormulas.AcceptTrade` and its
//! `trade_benefit`.
//!
//! At most two exchanges a visit. Each is planned in full, tried on copies
//! of the group, the reserve and the trader's stock, and only then made: a
//! refused or impossible exchange changes nothing.

use alloc::vec::Vec;

use crate::combat::Kit;
use crate::game::ItemBag;
use crate::rival::Rival;
use crate::v12_economy::{accept_trade_with, effective_trade_factor, effective_trade_value};

use super::equipment::{give, is_firearm, loaded_shots, reserve_accepts, take};
use super::group::{self, Items, Role};
use super::needs::{is_pump, Needs, World};
use super::production::CONSTRUCTION_MATERIALS;
use super::state::ModernAi;

/// `RecoveryServices.ReturnTripSupply`.
pub const RETURN_TRIP_SUPPLY: i32 = 3;
/// Exchanges a visit (`TradeWithTrader`).
pub const EXCHANGES: usize = 2;

/// The difficulty's barter factor and the policy's `trade_benefit`.
#[derive(Clone, Copy, Debug)]
pub struct Barter {
    pub factor: i32,
    pub benefit: f32,
}

impl Barter {
    /// `TradeFactor`.
    pub fn trade_factor(&self) -> f32 {
        effective_trade_factor(self.factor, self.benefit)
    }
    /// `TradeValue`.
    pub fn value(&self, values: &[f32]) -> f32 {
        effective_trade_value(values, self.factor, self.benefit)
    }
    /// `CanAfford`.
    pub fn can_afford(&self, offers: &[f32], targets: &[f32]) -> bool {
        accept_trade_with(offers, targets, self.factor, self.benefit)
    }
}

/// Something offered: an item a member carries, or a kind from the reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asset {
    Carried { member: usize, slot: usize, kind: usize },
    Pool { kind: usize },
}

impl Asset {
    pub fn kind(&self) -> usize {
        match *self {
            Asset::Carried { kind, .. } | Asset::Pool { kind } => kind,
        }
    }
    fn from_pool(&self) -> bool {
        matches!(self, Asset::Pool { .. })
    }
}

/// `TradePlan`: slots of the trader's stock wanted, and what is offered.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub targets: Vec<usize>,
    pub offers: Vec<Asset>,
}

/// One exchange made: kinds given and kinds received.
#[derive(Clone, Debug, PartialEq)]
pub struct Exchange {
    pub gave: Vec<usize>,
    pub got: Vec<usize>,
    pub strategic: bool,
}

/// What a decision reads: the world, this turn's needs, the group.
pub struct Context<'a> {
    pub w: &'a World<'a>,
    pub needs: &'a Needs,
    pub r: &'a Rival,
    pub leader_kit: &'a Kit,
    pub ai: &'a ModernAi,
    pub barter: Barter,
}

impl Context<'_> {
    fn items(&self) -> Items<'_> {
        self.w.items
    }

    fn kit(&self, member: usize) -> &Kit {
        if member == 0 { self.leader_kit } else { &self.r.party.companions[member - 1].kit }
    }

    fn food_in_inventory(&self) -> i32 {
        group::carried(self.r).iter().map(|&(_, _, k)| self.items().types.food_of(k)).sum()
    }

    fn water_in_inventory(&self) -> i32 {
        group::carried(self.r).iter().map(|&(_, _, k)| self.items().types.water_of(k)).sum()
    }

    fn portable_water_capacity(&self) -> i32 {
        group::carried(self.r)
            .iter()
            .filter(|&&(_, _, k)| self.items().is_water_container(k))
            .map(|&(_, _, k)| self.items().water_capacity(k))
            .sum()
    }

    fn reserve_water_capacity(&self) -> i32 {
        self.ai
            .reserve
            .iter()
            .filter(|&&(k, n)| n > 0 && self.items().is_water_container(k))
            .map(|&(k, n)| n as i32 * self.items().water_capacity(k))
            .sum()
    }

    fn sum_over_group(&self, f: impl Fn(&crate::game::CharacterNeeds) -> i32) -> i32 {
        (0..group::size(self.r)).map(|m| f(group::needs(self.r, m))).sum()
    }

    fn returning_from_city(&self) -> bool {
        self.w.in_city() && self.w.owned_camps() > 0
    }

    /// The food the group must keep carrying: for the road home from a city,
    /// else the standing reserve less what everybody has eaten.
    fn required_food_inventory(&self) -> i32 {
        if self.returning_from_city() {
            self.sum_over_group(|n| (RETURN_TRIP_SUPPLY - n.food).max(0))
        } else {
            (self.needs.desired_portable_food - self.sum_over_group(|n| n.food)).max(0)
        }
    }

    fn required_water_inventory(&self) -> i32 {
        if self.returning_from_city() { self.sum_over_group(|n| (RETURN_TRIP_SUPPLY - n.water).max(0)) } else { 0 }
    }

    fn material(&self, kind: usize) -> Option<&'static str> {
        let name = self.items().types.name_of(kind)?;
        CONSTRUCTION_MATERIALS.iter().copied().find(|m| *m == name)
    }

    /// `PortableMaterialCount`.
    fn portable_material_count(&self, name: &str) -> i32 {
        let t = self.items().types;
        let reserve = t.index_of(name).map_or(0, |k| self.ai.reserve_count(k) as i32);
        reserve + group::carried(self.r).iter().filter(|&&(_, _, k)| t.name_of(k) == Some(name)).count() as i32
    }
}

/// `SalePriority`.
pub fn sale_priority(items: Items, kind: usize) -> i32 {
    let t = items.types.get(kind);
    if t.is_some_and(|t| t.is_class("useless")) {
        return 0;
    }
    let food = items.types.food_of(kind);
    if food > 0 {
        return if food <= 3 { 1 } else { 3 };
    }
    if t.is_some_and(|t| t.is_class("protection")) {
        return 2;
    }
    3
}

/// `CanSell`: whether this carried item may go without leaving the group
/// short of what it must keep.
pub fn can_sell(cx: &Context, member: usize, slot: usize) -> bool {
    match group::pack(cx.r, member).get(slot) {
        Some(kind) => can_sell_kind(cx, kind, Some((member, slot))),
        None => false,
    }
}

/// [`can_sell`] for a thing of `kind`, carried at `held` or lying
/// elsewhere (a camp's room: `CargoManagement.CanCollectForTrade`).
pub fn can_sell_kind(cx: &Context, kind: usize, held: Option<(usize, usize)>) -> bool {
    let items = cx.items();
    let t = items.types;
    let n = cx.needs;
    let w = cx.w;
    if Needs::resting_sustenance(items, kind) && n.global_item_count(items, kind) <= 1 {
        return false;
    }
    if n.planned_settlement_payment == Some(kind) {
        return false;
    }
    if let Some((member, slot)) = held {
        let kit = cx.kit(member);
        if kit.weapon == Some(slot) || kit.protection == Some(slot) || crate::table_combat::find_best_defense(group::pack(cx.r, member), None, t) == Some(slot) {
            return false;
        }
    }
    if w.produce_of(kind).is_some() && (n.needs_production(w, kind) || n.production_tool_stock <= n.production_tool_quota) {
        return false;
    }
    if is_pump(items, kind) && n.pump_stock <= n.pump_quota {
        return false;
    }
    if t.food_of(kind) > 0 && cx.food_in_inventory() - t.food_of(kind) < cx.required_food_inventory() {
        return false;
    }
    if items.is_water_container(kind) {
        if cx.returning_from_city() && t.water_of(kind) > 0 && cx.water_in_inventory() - t.water_of(kind) < cx.required_water_inventory() {
            return false;
        }
        let remaining = cx.portable_water_capacity() + cx.reserve_water_capacity() - items.water_capacity(kind);
        if remaining < n.desired_portable_water_capacity {
            return false;
        }
    }
    if items.is_hazard_protection(kind) && n.protection_stock <= n.protection_quota {
        return false;
    }
    if n.is_policy_attack_weapon(items, kind) && n.melee_weapon_stock <= n.melee_weapon_quota {
        return false;
    }
    if t.name_of(kind) == Some("item_ammunition") && group::carried(cx.r).iter().any(|&(_, _, k)| is_firearm(t, k)) {
        return false;
    }
    if let Some(m) = cx.material(kind) {
        if n.material_stock(m) <= n.material_quota(m) {
            return false;
        }
    }
    t.name_of(kind) != Some("item_advice")
}

/// The strategic ladder of `ShoppingPriority`: survival, the mission, the
/// camps, the standing reserves, upgrades. Nought for what is not wanted.
pub fn priority_of(cx: &Context, kind: usize) -> f32 {
    let items = cx.items();
    let t = items.types;
    let n = cx.needs;
    let w = cx.w;
    if !n.can_buy(w, kind, 0) {
        return 0.0;
    }
    let value = items.value(kind);
    let water = items.is_water_container(kind);
    let capacity = items.water_capacity(kind) as f32;
    let melee = n.can_acquire_attack_weapon(items, kind);
    let name = t.name_of(kind).unwrap_or("");
    if t.heal_of(kind) > 0 && n.doctor_payment_needed {
        return 6200.0 + t.heal_of(kind) as f32;
    }
    if t.food_of(kind) > 0 && n.immediate_food_needed {
        return 6100.0 + (t.food_of(kind) * 12) as f32 + value;
    }
    if water && n.immediate_water_needed {
        return 6000.0 + capacity;
    }
    if n.planned_settlement_payment == Some(kind) {
        return 5200.0 + value;
    }
    if water && n.attack_water_needed {
        return 5100.0 + capacity;
    }
    if items.is_hazard_protection(kind) && n.provides_mission_protection(items, kind) {
        return 5000.0 + value;
    }
    if melee && (n.has_attack_plan || n.urgent_weapon_needed) && n.melee_weapon_stock < n.melee_weapon_quota {
        return 4900.0 + t.damage_of(kind) as f32;
    }
    let shortfall = n.production_shortfall(w, kind);
    if shortfall > 0 {
        return 3200.0 + (shortfall * 40) as f32 + tool_priority(cx, kind);
    }
    if n.needs_material(name) {
        return 2600.0 + if n.completes_useful_recipe(name) { 300.0 } else { 0.0 } + (n.material_demand_breadth(name) * 30) as f32 + value;
    }
    if is_pump(items, kind) && n.pump_stock < n.pump_quota {
        return 2400.0 + if name == "item_industrial_pump" { 20.0 } else { 0.0 };
    }
    if Needs::resting_sustenance(items, kind) && n.global_item_count(items, kind) < 1 {
        return 2300.0 + value;
    }
    if water && n.camp_water_container_shortfall > 0 {
        return if n.has_critical_camp_water_container_shortfall { 2700.0 } else { 1700.0 } + capacity;
    }
    if water && n.portable_water_capacity < n.desired_attack_water_container_capacity {
        return 1800.0 + capacity;
    }
    if w.produce_of(kind).is_some() && n.portable_production_reserve_needed {
        return 1500.0 + tool_priority(cx, kind);
    }
    if t.food_of(kind) > 0 && n.portable_food < n.desired_portable_food {
        return 1400.0 + (t.food_of(kind) * 12) as f32 + value;
    }
    if water && n.portable_water_capacity < n.desired_portable_water_capacity {
        return 1300.0 + capacity;
    }
    if melee && n.melee_weapon_stock < n.melee_weapon_quota {
        return 1200.0 + t.damage_of(kind) as f32;
    }
    if items.is_hazard_protection(kind) && (n.needs_protection(items, kind) || n.protection_stock < n.protection_quota) {
        return 1100.0 + value;
    }
    if n.equipment_demand(kind) > 0 {
        return 900.0 + (t.weapon_priority_of(kind) + t.defense_of(kind)) as f32;
    }
    0.0
}

/// `ProductionTradePriority` of the production a tool works.
fn tool_priority(cx: &Context, kind: usize) -> f32 {
    let items = cx.items();
    let Some(produce) = cx.w.produce_of(kind) else { return 0.0 };
    let base = match produce {
        "item_meat" => 240.0,
        "item_snake" => 200.0,
        "item_rats" => 100.0,
        _ => 0.0,
    };
    let k = items.types.index_of(produce);
    base + k.map_or(0.0, |k| items.value(k) + items.types.food_of(k) as f32)
}

/// `IsTradeValueUpgrade`: two or more cheaper things the group may sell
/// would pay for something the reserve does not take.
pub fn is_trade_value_upgrade(cx: &Context, kind: usize) -> bool {
    let items = cx.items();
    let value = items.value(kind);
    if !cx.needs.can_buy(cx.w, kind, 0) || reserve_accepts(items.types, kind, items) || value <= 0.0 {
        return false;
    }
    let lower: Vec<f32> = group::carried(cx.r)
        .into_iter()
        .filter(|&(m, s, k)| can_sell(cx, m, s) && k != kind && items.value(k) > 0.0 && items.value(k) < value)
        .map(|(_, _, k)| items.value(k))
        .collect();
    lower.len() >= 2 && lower.iter().map(|v| v * cx.barter.trade_factor()).sum::<f32>() >= value
}

/// `ShoppingPriority(state, item)`: an empty gun is not wanted; otherwise
/// the ladder, or a trade-value upgrade at 500 and its worth.
pub fn shopping_priority(cx: &Context, kind: usize, rounds: Option<i32>) -> f32 {
    let t = cx.items().types;
    if is_firearm(t, kind) && loaded_shots(t, kind, rounds) <= 0 {
        return 0.0;
    }
    let strategic = priority_of(cx, kind);
    if strategic > 0.0 || !is_trade_value_upgrade(cx, kind) { strategic } else { 500.0 + cx.items().value(kind) }
}

/// `SnapshotSurplusPoolAssets`: what of the reserve may be traded away —
/// water containers above the need, protection above the quota (paper
/// helmets first), armour above the limit, production tools nobody needs,
/// and firearms, the dearest first.
pub fn surplus_pool_assets(cx: &Context) -> Vec<usize> {
    let items = cx.items();
    let t = items.types;
    let n = cx.needs;
    let mut remaining: Vec<usize> = cx.ai.reserve.iter().flat_map(|&(k, c)| core::iter::repeat_n(k, c as usize)).collect();
    let mut out = Vec::new();
    let take_out = |remaining: &mut Vec<usize>, out: &mut Vec<usize>, k: usize| {
        if let Some(i) = remaining.iter().position(|&x| x == k) {
            remaining.remove(i);
            out.push(k);
        }
    };
    let required = (n.desired_portable_water_capacity - cx.portable_water_capacity()).max(0);
    let mut pool_capacity = cx.reserve_water_capacity();
    let mut water: Vec<usize> = remaining.iter().copied().filter(|&k| items.is_water_container(k)).collect();
    water.sort_by_key(|&k| (items.water_capacity(k), t.water_of(k)));
    for k in water {
        let c = items.water_capacity(k);
        if pool_capacity - c < required {
            break;
        }
        take_out(&mut remaining, &mut out, k);
        pool_capacity -= c;
    }
    let surplus_protection = (n.protection_stock - n.protection_quota).max(0) as usize;
    let mut protection: Vec<usize> = remaining.iter().copied().filter(|&k| items.is_hazard_protection(k)).collect();
    protection.sort_by_key(|&k| match t.name_of(k) {
        Some("item_paper_helmet") => 0,
        Some("item_gas_mask") => 1,
        _ => 2,
    });
    for k in protection.into_iter().take(surplus_protection) {
        take_out(&mut remaining, &mut out, k);
    }
    let armour: Vec<usize> = remaining.iter().copied().filter(|&k| t.defense_of(k) > cx.w.policy.armour_limit && !items.is_hazard_protection(k)).collect();
    for k in armour {
        take_out(&mut remaining, &mut out, k);
    }
    let surplus_tools = (n.production_tool_stock - n.production_tool_quota).max(0) as usize;
    let mut tools: Vec<usize> = remaining.iter().copied().filter(|&k| cx.w.produce_of(k).is_some() && !n.needs_production(cx.w, k)).collect();
    tools.sort_by(|&a, &b| {
        let pv = |k: usize| cx.w.produce_of(k).and_then(|p| t.index_of(p)).map_or(0.0, |p| items.value(p));
        pv(a).partial_cmp(&pv(b)).unwrap_or(core::cmp::Ordering::Equal).then(t.damage_of(a).cmp(&t.damage_of(b)))
    });
    for k in tools.into_iter().take(surplus_tools) {
        take_out(&mut remaining, &mut out, k);
    }
    let mut guns: Vec<usize> = remaining.iter().copied().filter(|&k| is_firearm(t, k)).collect();
    guns.sort_by(|&a, &b| items.value(b).partial_cmp(&items.value(a)).unwrap_or(core::cmp::Ordering::Equal));
    for k in guns {
        take_out(&mut remaining, &mut out, k);
    }
    out
}

fn values(items: Items, kinds: impl Iterator<Item = usize>) -> Vec<f32> {
    kinds.map(|k| items.value(k)).collect()
}

/// `OfferRemovalGroup`: empty containers are handed back first, full ones
/// last.
fn removal_group(items: Items, kind: usize) -> u8 {
    let t = items.types;
    if t.water_of(kind) == 0 && t.full_of(kind).is_some_and(|f| t.water_of(f) > 0) {
        0
    } else if items.is_full_water_container(kind) {
        2
    } else {
        1
    }
}

/// `MissingLeaderRoleSlotsAfter`.
fn leader_roles_missing_after(cx: &Context, removed: &[usize], added: &[usize]) -> usize {
    let p = &cx.r.pack;
    let mut kinds: Vec<usize> = (0..p.len()).filter(|s| !removed.contains(s)).filter_map(|s| p.get(s)).collect();
    kinds.extend_from_slice(added);
    group::missing_roles(cx.items(), &kinds)
}

fn free_slots(r: &Rival) -> i32 {
    (0..group::size(r)).map(|m| group::pack(r, m).capacity() as i32 - group::pack(r, m).len() as i32).sum()
}

/// `BuildReceivedBasket`: the target, then whatever else the budget left
/// over buys — wanted things first, recipe completers, the dearest.
fn received_basket(cx: &Context, stock: &ItemBag, primary: usize, excluded: &[usize], budget: f32, allow_strategic: bool) -> Vec<usize> {
    let items = cx.items();
    let t = items.types;
    let mut targets = alloc::vec![primary];
    let mut ranked: Vec<(usize, bool, bool, f32)> = (0..stock.len())
        .filter(|&s| s != primary && !excluded.contains(&s))
        .filter_map(|s| stock.get(s).map(|k| (s, k)))
        .filter(|&(_, k)| items.value(k) > 0.0 && (allow_strategic || !cx.needs.is_strategic(cx.w, k)))
        .map(|(s, k)| {
            let name = t.name_of(k).unwrap_or("");
            (s, shopping_priority(cx, k, stock.rounds_left(s)) > 0.0, cx.needs.completes_useful_recipe(name), items.value(k))
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(b.3.partial_cmp(&a.3).unwrap_or(core::cmp::Ordering::Equal))
    });
    let mut remaining = budget - stock.get(primary).map_or(0.0, |k| items.value(k));
    while remaining >= 1.0 {
        let filler = ranked.iter().map(|r| r.0).find(|&s| {
            let Some(k) = stock.get(s) else { return false };
            let same = targets.iter().filter(|&&x| stock.get(x) == Some(k)).count() as i32;
            let guns = targets.iter().filter(|&&x| stock.get(x).is_some_and(|g| is_firearm(t, g))).count() as i32;
            !targets.contains(&s)
                && items.value(k) <= remaining
                && cx.needs.can_buy(cx.w, k, same)
                && (!is_firearm(t, k) || guns < cx.needs.equipment_demand(k))
        });
        let Some(f) = filler else { break };
        remaining -= stock.get(f).map_or(0.0, |k| items.value(k));
        targets.push(f);
    }
    targets
}

/// `CreateTradePlan`: for each of the three most wanted things the trader
/// has, offers gathered cheapest first (full water first) within the
/// floors, the surplus handed back, the basket filled; the first plan the
/// trader accepts, that fits, that is worth making and wastes little.
pub fn create_trade_plan(cx: &Context, stock: &ItemBag, excluded: &[usize], allow_strategic: bool) -> Option<Plan> {
    let items = cx.items();
    let t = items.types;
    let n = cx.needs;
    if stock.len() == 0 {
        return None;
    }
    let pool_assets = surplus_pool_assets(cx);
    let mut wanted: Vec<(usize, f32)> = (0..stock.len())
        .filter(|s| !excluded.contains(s))
        .filter_map(|s| stock.get(s).map(|k| (s, k)))
        .filter(|&(_, k)| items.value(k) > 0.0)
        .map(|(s, k)| (s, shopping_priority(cx, k, stock.rounds_left(s))))
        .filter(|&(s, p)| p > 0.0 && (allow_strategic || !n.is_strategic(cx.w, stock.get(s).unwrap_or(0))))
        .collect();
    wanted.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(items.value(stock.get(a.0).unwrap_or(0)).partial_cmp(&items.value(stock.get(b.0).unwrap_or(0))).unwrap_or(core::cmp::Ordering::Equal))
    });
    for &(target, _) in wanted.iter().take(3) {
        let Some(target_kind) = stock.get(target) else { continue };
        let target_value = items.value(target_kind);
        let mut candidates: Vec<Asset> = group::carried(cx.r)
            .into_iter()
            .filter(|&(m, s, _)| can_sell(cx, m, s))
            .map(|(member, slot, kind)| Asset::Carried { member, slot, kind })
            .chain(pool_assets.iter().map(|&kind| Asset::Pool { kind }))
            .collect();
        candidates.sort_by(|a, b| {
            let key = |x: &Asset| {
                let k = x.kind();
                let sale = match x {
                    Asset::Pool { .. } => 3,
                    Asset::Carried { .. } => sale_priority(items, k),
                };
                (if items.is_full_water_container(k) { 0 } else { 1 }, items.value(k), sale)
            };
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0).then(ka.1.partial_cmp(&kb.1).unwrap_or(core::cmp::Ordering::Equal)).then(ka.2.cmp(&kb.2))
        });
        let mut offers: Vec<Asset> = Vec::new();
        let mut food = cx.food_in_inventory();
        let required_food = cx.required_food_inventory();
        let mut water = cx.water_in_inventory();
        let required_water = cx.required_water_inventory();
        let water_reserve = cx.sum_over_group(|n| n.water);
        let mut capacity = water_reserve + cx.portable_water_capacity() + items.water_capacity(target_kind)
            + pool_assets.iter().map(|&k| items.water_capacity(k)).sum::<i32>();
        let mut melee = group::carried(cx.r).iter().filter(|&&(_, _, k)| t.damage_of(k) > 0 && !is_firearm(t, k)).count() as i32;
        let mut materials: Vec<(&str, i32)> = CONSTRUCTION_MATERIALS.iter().map(|&m| (m, cx.portable_material_count(m))).collect();
        let strategic = n.is_strategic(cx.w, target_kind);
        for c in candidates.iter().copied().filter(|c| c.kind() != target_kind) {
            let k = c.kind();
            if !strategic && items.value(k) >= target_value {
                continue;
            }
            if !c.from_pool() && t.food_of(k) > 0 && food - t.food_of(k) + t.food_of(target_kind) < required_food {
                continue;
            }
            if !c.from_pool() && t.water_of(k) > 0 && water - t.water_of(k) + t.water_of(target_kind) < required_water {
                continue;
            }
            if items.is_water_container(k) && capacity - items.water_capacity(k) < n.desired_portable_water_capacity {
                continue;
            }
            let is_melee = t.damage_of(k) > 0 && !is_firearm(t, k);
            if is_melee && melee <= n.melee_weapon_quota {
                continue;
            }
            let material = cx.material(k);
            if let Some(m) = material {
                if materials.iter().find(|(x, _)| *x == m).map_or(0, |(_, v)| *v) <= n.material_quota(m) {
                    continue;
                }
            }
            offers.push(c);
            if !c.from_pool() {
                food -= t.food_of(k);
                water -= t.water_of(k);
            }
            capacity -= items.water_capacity(k);
            if is_melee {
                melee -= 1;
            }
            if let Some(m) = material {
                if let Some(e) = materials.iter_mut().find(|(x, _)| *x == m) {
                    e.1 -= 1;
                }
            }
            if cx.barter.can_afford(&values(items, offers.iter().map(|o| o.kind())), &[target_value]) {
                break;
            }
        }
        // Hand back what is not needed: empty containers first, then the
        // dearest, full containers last.
        let mut order = offers.clone();
        order.sort_by(|a, b| {
            removal_group(items, a.kind())
                .cmp(&removal_group(items, b.kind()))
                .then(items.value(b.kind()).partial_cmp(&items.value(a.kind())).unwrap_or(core::cmp::Ordering::Equal))
        });
        for c in order {
            if offers.len() <= 1 {
                break;
            }
            let without: Vec<Asset> = offers.iter().copied().filter(|o| *o != c).collect();
            if cx.barter.can_afford(&values(items, without.iter().map(|o| o.kind())), &[target_value]) {
                offers = without;
            }
        }
        let budget = cx.barter.value(&values(items, offers.iter().map(|o| o.kind())));
        let targets = received_basket(cx, stock, target, excluded, budget, allow_strategic);
        let target_kinds: Vec<usize> = targets.iter().filter_map(|&s| stock.get(s)).collect();
        let freed = offers.iter().filter(|o| !o.from_pool()).count() as i32;
        let added: Vec<usize> = target_kinds.iter().copied().filter(|&k| !reserve_accepts(t, k, items)).collect();
        let removed_leader: Vec<usize> = offers
            .iter()
            .filter_map(|o| match *o {
                Asset::Carried { member: 0, slot, .. } => Some(slot),
                _ => None,
            })
            .collect();
        let reserved = leader_roles_missing_after(cx, &removed_leader, &added) as i32;
        let can_store = added.len() as i32 <= free_slots(cx.r) + freed - reserved;
        let compresses = strategic || offers.len() >= 2;
        let utility: f32 = target_kinds
            .iter()
            .map(|&k| items.value(k) * if n.completes_useful_recipe(t.name_of(k).unwrap_or("")) { 2.0 } else { 1.0 })
            .sum();
        let wasteless = utility >= budget * 0.65;
        let affords = cx.barter.can_afford(&values(items, offers.iter().map(|o| o.kind())), &values(items, target_kinds.iter().copied()));
        if !offers.is_empty() && compresses && affords && can_store && wasteless {
            return Some(Plan { targets, offers });
        }
    }
    None
}

/// `GroupInventory.CanCarryCargo` for a kind.
fn can_carry(r: &Rival, items: Items, member: usize, kind: usize) -> bool {
    let p = group::pack(r, member);
    if p.is_full() {
        return false;
    }
    if member != 0 {
        return true;
    }
    let kinds: Vec<usize> = p.iter().collect();
    let missing = Role::ALL.iter().filter(|role| !kinds.iter().any(|&k| role.fits(items, k)) && !role.fits(items, kind)).count();
    p.capacity() as i32 - p.len() as i32 - 1 >= missing as i32
}

/// `FindCargoCarrier`: the leader, when it fills one of his empty places
/// and fits; else the first of the group it fits.
pub fn cargo_carrier(r: &Rival, items: Items, kind: usize) -> Option<usize> {
    let leader: Vec<usize> = r.pack.iter().collect();
    let fills = Role::ALL.iter().any(|role| role.fits(items, kind) && !leader.iter().any(|&k| role.fits(items, k)));
    if fills && can_carry(r, items, 0, kind) {
        return Some(0);
    }
    (0..group::size(r)).find(|&m| can_carry(r, items, m, kind))
}

/// `PruneAndStoreTraderOffers`: what was given joins the trader's stock,
/// which keeps its dearest — useful before useless among equals — up to
/// what it can hold.
pub fn store_with_trader(stock: &mut ItemBag, items: Items, given: &[(usize, Option<i32>)]) -> usize {
    let mut all: Vec<(bool, usize, usize, Option<i32>)> = (0..stock.len())
        .filter_map(|s| stock.get(s).map(|k| (false, s, k, stock.rounds_left(s))))
        .chain(given.iter().enumerate().map(|(i, &(k, r))| (true, i, k, r)))
        .collect();
    let useless = |k: usize| items.types.get(k).is_some_and(|t| t.is_class("useless"));
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|&a, &b| {
        items.value(all[b].2).partial_cmp(&items.value(all[a].2)).unwrap_or(core::cmp::Ordering::Equal).then(useless(all[a].2).cmp(&useless(all[b].2)))
    });
    let kept: Vec<usize> = order.into_iter().take(stock.capacity()).collect();
    let mut slot = stock.len();
    while slot > 0 {
        slot -= 1;
        let i = all.iter().position(|e| !e.0 && e.1 == slot).unwrap_or(usize::MAX);
        if !kept.contains(&i) {
            stock.remove(slot);
        }
    }
    let mut added = 0;
    for (i, e) in all.iter_mut().enumerate() {
        if e.0 && kept.contains(&i) && give(stock, e.2, e.3).is_some() {
            added += 1;
        }
    }
    added
}

/// One exchange made on the group, the reserve and the stock, or none at
/// all: an offer no longer there or a target with nowhere to go refuses
/// the whole of it. `(given, received, slots of the stock now holding what
/// was given)`.
pub fn exchange(r: &mut Rival, kit: &mut Kit, ai: &mut ModernAi, stock: &mut ItemBag, items: Items, plan: &Plan) -> Option<(Vec<usize>, Vec<usize>, Vec<usize>)> {
    let (mut r2, mut kit2, mut ai2, mut stock2) = (r.clone(), *kit, ai.clone(), stock.clone());
    let received: Vec<(usize, Option<i32>)> = plan.targets.iter().map(|&s| Some((stock.get(s)?, stock.rounds_left(s)))).collect::<Option<_>>()?;
    let mut slots = plan.targets.clone();
    slots.sort_unstable_by(|a, b| b.cmp(a));
    for s in slots {
        stock2.remove(s)?;
    }
    // Carried offers come out of their packs, the highest slot first so the
    // others stay put; then everything given in the plan's order.
    let mut carried: Vec<(usize, usize, usize)> = plan
        .offers
        .iter()
        .filter_map(|o| match *o {
            Asset::Carried { member, slot, kind } => Some((member, slot, kind)),
            Asset::Pool { .. } => None,
        })
        .collect();
    carried.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
    let mut taken: Vec<((usize, usize), Option<i32>)> = Vec::new();
    for (member, slot, kind) in carried {
        let (k, rounds) = if member == 0 {
            take(&mut r2.pack, &mut kit2, slot)?
        } else {
            let c = r2.party.companions.get_mut(member - 1)?;
            take(&mut c.pack, &mut c.kit, slot)?
        };
        if k != kind {
            return None;
        }
        taken.push(((member, slot), rounds));
    }
    let mut given: Vec<(usize, Option<i32>)> = Vec::new();
    for o in &plan.offers {
        match *o {
            Asset::Pool { kind } => {
                if !ai2.reserve_take(kind) {
                    return None;
                }
                given.push((kind, None));
            }
            Asset::Carried { member, slot, kind } => {
                let rounds = taken.iter().find(|t| t.0 == (member, slot))?.1;
                given.push((kind, rounds));
            }
        }
    }
    let kept = store_with_trader(&mut stock2, items, &given);
    for &(k, rounds) in &received {
        if reserve_accepts(items.types, k, items) && ai2.reserve_add(k) {
            continue;
        }
        let m = cargo_carrier(&r2, items, k)?;
        give(group::pack_mut(&mut r2, m), k, rounds)?;
    }
    *r = r2;
    *kit = kit2;
    *ai = ai2;
    *stock = stock2;
    let sold_slots: Vec<usize> = (stock.len() - kept..stock.len()).collect();
    Some((given.iter().map(|g| g.0).collect(), received.iter().map(|g| g.0).collect(), sold_slots))
}

/// `TradeWithTrader`: up to two exchanges, the second not another strategic
/// purchase if the first was one, and never buying back what was just
/// sold. `needs_of` builds the needs again after each exchange
/// (`RefreshNeeds`); `after` sees to the equipment (`EquipmentPlanning`).
#[allow(clippy::too_many_arguments)]
pub fn trade_with_trader(
    r: &mut Rival,
    kit: &mut Kit,
    ai: &mut ModernAi,
    stock: &mut ItemBag,
    w: &World,
    barter: Barter,
    mut needs_of: impl FnMut(&Rival, &ModernAi) -> Needs,
    mut after: impl FnMut(&mut Rival, &mut Kit, &mut ModernAi),
) -> Vec<Exchange> {
    let mut done = Vec::new();
    let mut excluded: Vec<usize> = Vec::new();
    let mut strategic_made = false;
    let mut needs = needs_of(r, ai);
    let mut allow = true;
    for _ in 0..EXCHANGES {
        let plan = {
            let cx = Context { w, needs: &needs, r, leader_kit: kit, ai, barter };
            let Some(plan) = create_trade_plan(&cx, stock, &excluded, allow) else { break };
            let offers = values(w.items, plan.offers.iter().map(|o| o.kind()));
            let targets = values(w.items, plan.targets.iter().filter_map(|&s| stock.get(s)));
            if !barter.can_afford(&offers, &targets) {
                break;
            }
            plan
        };
        let strategic = plan.targets.iter().filter_map(|&s| stock.get(s)).any(|k| needs.is_strategic(w, k));
        let Some((gave, got, sold_slots)) = exchange(r, kit, ai, stock, w.items, &plan) else { break };
        excluded = sold_slots;
        after(r, kit, ai);
        strategic_made |= strategic;
        done.push(Exchange { gave, got, strategic });
        needs = needs_of(r, ai);
        allow = !strategic_made;
    }
    done
}

// --- city trips (`Trading.Planning`, `CargoManagement`) -----------------

/// `Trading.MinimumTradeItems`, `MinimumTradeValue`.
pub const MINIMUM_TRADE_ITEMS: i32 = 4;
pub const MINIMUM_TRADE_VALUE: f32 = 25.0;

/// `IsSubstantialTradeLot`.
pub fn is_substantial_trade_lot(items: i32, value: f32) -> bool {
    items >= MINIMUM_TRADE_ITEMS || value >= MINIMUM_TRADE_VALUE
}

/// `IsHighReturnReservePurchase`: a tool a camp lacks, a portable tool, or
/// a wanted material.
pub fn is_high_return_reserve_purchase(cx: &Context, kind: usize) -> bool {
    let n = cx.needs;
    n.production_shortfall(cx.w, kind) > 0
        || cx.w.produce_of(kind).is_some() && n.portable_production_reserve_needed
        || n.needs_material(cx.items().types.name_of(kind).unwrap_or(""))
}

/// `IsHighReturnLiquidReserve`.
pub fn is_high_return_liquid_reserve(items: Items, kind: usize) -> bool {
    items.types.food_of(kind) > 0 || items.is_water_container(kind) || items.types.damage_of(kind) > 0 && !is_firearm(items.types, kind)
}

/// `TradeCapital`: what the group may sell — and, when some city's trader
/// carries a high-return purchase, its food, water and melee weapons too.
pub fn trade_capital(cx: &Context) -> Vec<(usize, usize, usize)> {
    let liquidate = cx.w.traders.iter().any(|t| {
        t.shop.is_some_and(|city| cx.w.sites.get(city).is_some_and(|s| s.city)) && t.assortment.iter().any(|&k| is_high_return_reserve_purchase(cx, k))
    });
    group::carried(cx.r)
        .into_iter()
        .filter(|&(m, s, k)| can_sell(cx, m, s) || liquidate && is_high_return_liquid_reserve(cx.items(), k))
        .collect()
}

/// `CanCollectForTrade` for a thing in a camp's room.
fn can_collect_for_trade(cx: &Context, kind: usize) -> bool {
    let items = cx.items();
    if !items.is_water_container(kind) {
        return can_sell_kind(cx, kind, None);
    }
    let carried = group::carried(cx.r).iter().filter(|&&(_, _, k)| items.is_water_container(k)).count();
    let reserve: usize = cx.ai.reserve.iter().filter(|&&(k, _)| items.is_water_container(k)).map(|&(_, c)| c as usize).sum();
    carried + reserve >= group::size(cx.r)
}

/// `ProjectedProductionItems`: what the camp makes over `days`.
fn projected_production_items(items: Items, site: &super::camps::Site, days: i32) -> i32 {
    let Some(p) = site.current.and_then(|i| site.productions.get(i)) else { return 0 };
    if days <= 0 || site.owner.is_none() {
        return 0;
    }
    let rate = site.rate(items, p, p.installed, site.guards.len() as i32);
    if rate.item_drop_interval <= 0.0 { 0 } else { (days as f32 / rate.item_drop_interval) as i32 }
}

/// `ProjectedCampCollectibleValue` and `…Count`: the produce in stock and
/// what `days` more would add (to the stock's limit), and whatever else in
/// the rooms may be taken for trade.
pub fn projected_camp_collectibles(cx: &Context, site: &super::camps::Site, days: i32) -> (f32, i32) {
    let items = cx.items();
    let t = items.types;
    let (mut value, mut count) = (0.0f32, 0i32);
    let current = site.current.and_then(|i| site.productions.get(i));
    if let Some(p) = current {
        let active = site.stored.iter().filter(|&&k| k == p.produce).count() as i32;
        let free = (crate::game::MAX_STOCK_FOOD as i32 - site.stock).max(0);
        let stock = active + free.min(projected_production_items(items, site, days));
        let n = (stock - super::group::CAMP_FOOD_ITEM_RESERVE as i32).max(0);
        value += n as f32 * items.value(p.produce);
        count += n;
    }
    for &k in &site.stored {
        if current.is_some_and(|p| p.produce == k) {
            continue;
        }
        let tool_here = t.name_of(k).is_some_and(|name| site.productions.iter().any(|p| p.production.worked_by(name)));
        if tool_here || is_pump(items, k) || !can_collect_for_trade(cx, k) {
            continue;
        }
        value += items.value(k);
        count += 1;
    }
    (value, count)
}

/// `FindBestCampForCityPreparation`: away from its own camps and with room,
/// one of them on the way to a city — at most two days' detour — whose
/// pickup makes a substantial lot, the best value a day of travel.
pub fn best_camp_for_city_preparation(cx: &Context, previous: Option<usize>) -> Option<usize> {
    let sites = cx.w.sites;
    let (player, here) = (cx.w.player, cx.w.here);
    let free: i32 = (0..group::size(cx.r)).map(|m| group::pack(cx.r, m).capacity() as i32 - group::pack(cx.r, m).len() as i32).sum();
    if sites.get(here).is_some_and(|s| s.owner == Some(player)) || free == 0 {
        return None;
    }
    let nearest_city = |from: usize, prev: Option<usize>| {
        (0..sites.sites.len())
            .filter(|&c| sites.sites[c].city && c != from)
            .filter_map(|c| super::route::find(sites, player, prev, from, c))
            .min_by_key(|r| r.days)
    };
    let direct = nearest_city(here, previous)?;
    let capital = trade_capital(cx);
    let capital_value: f32 = capital.iter().map(|&(_, _, k)| cx.items().value(k)).sum();
    let mut best: Option<(usize, f32)> = None;
    for l in 0..sites.sites.len() {
        if l == here || sites.sites[l].owner != Some(player) {
            continue;
        }
        let Some(to_camp) = super::route::find(sites, player, previous, here, l) else { continue };
        let Some(to_city) = nearest_city(l, None) else { continue };
        if to_camp.days + to_city.days > direct.days + 2 {
            continue;
        }
        let detour = to_camp.days + to_city.days - direct.days;
        let (value, count) = projected_camp_collectibles(cx, &sites.sites[l], to_camp.days);
        if !is_substantial_trade_lot(capital.len() as i32 + count, capital_value + value) {
            continue;
        }
        let per_day = super::camps::trip_value_per_day(value, to_camp.days + detour);
        if best.is_none_or(|(_, b)| per_day > b) {
            best = Some((l, per_day));
        }
    }
    best.map(|b| b.0)
}

// --- cargo (`CargoManagement`) ------------------------------------------

/// `Trading.ConstructionMaterialPriority`.
pub fn construction_material_priority(cx: &Context, name: &str) -> f32 {
    let n = cx.needs;
    if !n.needs_material(name) {
        return 0.0;
    }
    2600.0 + if n.completes_useful_recipe(name) { 300.0 } else { 0.0 } + (n.material_demand_breadth(name) * 30) as f32
}

/// `Trading.NeedsProduction(state, type)`: with no camp, any tool; else an
/// own preferred camp that wants more of this tool and makes this
/// production best (by trade priority, then food).
pub fn tool_wanted(cx: &Context, kind: usize) -> bool {
    let w = cx.w;
    let t = w.items.types;
    let Some(p) = t.name_of(kind).and_then(|n| w.productions.worked_by(n)) else { return false };
    let camps: Vec<usize> = (0..w.sites.sites.len()).filter(|&l| w.sites.sites[l].owner == Some(w.player)).collect();
    if camps.is_empty() {
        return true;
    }
    camps.iter().any(|&l| {
        let site = &w.sites.sites[l];
        let Some(i) = site.productions.iter().position(|sp| sp.id == p) else { return false };
        if site.productions[i].installed >= super::production::desired_tool_count(w.items, w.sites, w.player, l, i) || !super::production::prefer_production(site) {
            return false;
        }
        let mut best: Option<(usize, f32, i32)> = None;
        for j in 0..site.productions.len() {
            let pr = super::production::trade_priority(w.items, site, j);
            let food = t.food_of(site.productions[j].produce);
            if best.is_none_or(|b| pr > b.1 || pr == b.1 && food > b.2) {
                best = Some((j, pr, food));
            }
        }
        best.is_some_and(|b| b.0 == i)
    })
}

/// `CargoRetentionValue`: materials first, then tools wanted, protection
/// wanted, a bible, food by worth and fill, the rest by worth.
pub fn cargo_retention_value(cx: &Context, sites: &super::camps::Sites, ai: &ModernAi, kind: usize) -> f32 {
    let items = cx.items();
    let t = items.types;
    let name = t.name_of(kind).unwrap_or("");
    let material = construction_material_priority(cx, name);
    if material > 0.0 {
        return 10000.0 + material;
    }
    if tool_wanted(cx, kind) {
        return 9000.0 + tool_priority(cx, kind);
    }
    if items.is_hazard_protection(kind) && super::production::needs_danger_protection(items, sites, cx.w.player, ai, kind) {
        return 8000.0 + items.value(kind);
    }
    if Needs::resting_sustenance(items, kind) {
        return 7500.0 + items.value(kind);
    }
    if t.food_of(kind) > 0 {
        return items.value(kind) + (t.food_of(kind) * 2) as f32;
    }
    items.value(kind)
}

/// `GroupInventory.CanReplaceCargo`.
fn can_replace_cargo(r: &Rival, items: Items, member: usize, removed_slot: usize, replacement: usize) -> bool {
    if member != 0 {
        return true;
    }
    let kinds: Vec<usize> = r.pack.iter().collect();
    let before = group::missing_roles(items, &kinds);
    let mut after_kinds: Vec<usize> = (0..r.pack.len()).filter(|&s| s != removed_slot).filter_map(|s| r.pack.get(s)).collect();
    after_kinds.push(replacement);
    let after = group::missing_roles(items, &after_kinds);
    let free_after = r.pack.capacity() as i32 - r.pack.len() as i32;
    after < before || free_after >= after as i32
}

/// `CargoManagement.TryReplaceCargo`: a sellable carried thing worth less
/// to keep than what was found, whose loss leaves food and water as they
/// must be — the one in the leader's empty place first, then the least
/// worth keeping. `(member, slot)` to swap out.
pub fn replace_cargo(cx: &Context, ai: &ModernAi, found: usize) -> Option<(usize, usize)> {
    let items = cx.items();
    let t = items.types;
    let r = cx.r;
    let food = cx.sum_over_group(|n| n.food) + cx.food_in_inventory();
    let water = cx.sum_over_group(|n| n.water) + cx.portable_water_capacity();
    let leader_kinds: Vec<usize> = r.pack.iter().collect();
    let fills_leader_role = Role::ALL.iter().any(|role| role.fits(items, found) && !leader_kinds.iter().any(|&k| role.fits(items, k)));
    let mut best: Option<(usize, usize, bool, f32, f32)> = None;
    for (m, s, k) in group::carried(r) {
        if !can_sell(cx, m, s) || !can_replace_cargo(r, items, m, s, found) {
            continue;
        }
        if food - t.food_of(k) + t.food_of(found) < food.min(cx.needs.desired_portable_food) {
            continue;
        }
        if water - items.water_capacity(k) + items.water_capacity(found) < water.min(cx.needs.desired_portable_water_capacity) {
            continue;
        }
        let first = m == 0 && fills_leader_role;
        let keep = cargo_retention_value(cx, cx.w.sites, ai, k);
        let value = items.value(k);
        let better = match best {
            None => true,
            Some((_, _, bf, bk, bv)) => (first && !bf) || first == bf && (keep < bk || keep == bk && value < bv),
        };
        if better {
            best = Some((m, s, first, keep, value));
        }
    }
    let (m, s, _, keep, _) = best?;
    if cargo_retention_value(cx, cx.w.sites, ai, found) <= keep {
        return None;
    }
    Some((m, s))
}
