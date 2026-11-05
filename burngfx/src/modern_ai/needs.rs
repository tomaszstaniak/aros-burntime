//! What the opponent wants this turn: `StrategicNeeds`, the view the trade
//! planner prices everything by. At tag `v1.2`,
//! `AI/PlayerBehavior/Economy/StrategicNeeds.cs`, with the quotas of
//! `Trading.Planning` and `Trading.Priorities` it reads.
//!
//! Built once at the start of the turn and again after every exchange
//! (`AiTurnContext.RefreshNeeds`), from the group, the reserve and the
//! places. Counts keyed by item name keep the order the names first came
//! in, as upstream's dictionaries do.

use alloc::string::String;
use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::production::Productions;
use crate::rival::Rival;

use super::camps::Sites;
use super::group::{self, Items};
use super::policy::AiPolicy;
use super::production::{needs_pump, prefer_production, trade_priority, CONSTRUCTION_MATERIALS};
use super::state::ModernAi;

/// `MaximumPurchasedItemsPerType`.
pub const MAXIMUM_PURCHASED_PER_TYPE: i32 = 5;
/// `Trading.StandingFoodDays`.
pub const STANDING_FOOD_DAYS: i32 = 9;

const MEAT_RECIPE: [&str; 3] = ["item_spring", "item_tin", "item_wire"];
const RAT_RECIPE: [&str; 3] = ["item_wire", "item_woodpile", "item_screws"];
const PUMP_RECIPES: [&[&str]; 2] = [&["item_broken_pump", "item_rags", "item_hose"], &["item_spare_parts", "item_iron_pipe", "item_rags", "item_hose"]];
const PROTECTION_MATERIALS: [&str; 4] = ["item_gas_mask", "item_gloves", "item_protective_overall", "item_boots"];

/// Counts by name, in the order names first came.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tally(Vec<(String, i32)>);

impl Tally {
    pub fn add(&mut self, id: &str, n: i32) {
        match self.0.iter_mut().find(|(k, _)| k == id) {
            Some((_, v)) => *v += n,
            None => self.0.push((String::from(id), n)),
        }
    }
    pub fn get(&self, id: &str) -> i32 {
        self.0.iter().find(|(k, _)| k == id).map_or(0, |(_, v)| *v)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> {
        self.0.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// What the turn's plans add to the needs, decided by other parts.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plans {
    /// `Recruitment.PlannedFutureSettlementPaymentType`.
    pub settlement_payment: Option<usize>,
    pub has_attack_plan: bool,
    /// `Trading.NeedsAttackWaterPreparation`, with an attack plan.
    pub attack_water_needed: bool,
    /// The strategic target's danger, if it has one.
    pub mission_hazard: Option<Danger>,
    /// A doctor where the group stands.
    pub has_doctor: bool,
}

/// Where the opponent is and what it holds, as the needs read it.
pub struct World<'a> {
    pub items: Items<'a>,
    pub productions: &'a Productions,
    pub sites: &'a Sites,
    pub player: usize,
    pub policy: &'a AiPolicy,
    pub day: i32,
    pub here: usize,
    /// `EquipmentNeeds.Demand` for every kind ([`super::equipment::demand`]).
    pub equipment_demand: &'a [i32],
    /// The traders the opponent may meet.
    pub traders: &'a [TraderView],
}

/// A trader as the planner sees him.
#[derive(Clone, Debug, PartialEq)]
pub struct TraderView {
    /// His index among the world's traders.
    pub index: usize,
    /// `TraderId`, for novelty.
    pub id: usize,
    pub at: usize,
    /// The city whose `LocalTrader` he is, if he keeps a shop.
    pub shop: Option<usize>,
    pub alive: bool,
    pub stock: crate::game::ItemBag,
    /// `GetAssortment`: what he may restock, by kind, first of a kind kept.
    pub assortment: Vec<usize>,
}

impl World<'_> {
    /// `ItemType.Production`: the production this kind is a tool of, by its
    /// produce's name.
    pub fn produce_of(&self, kind: usize) -> Option<&str> {
        let name = self.items.types.name_of(kind)?;
        let p = self.productions.worked_by(name)?;
        Some(self.productions.get(p)?.produce.as_str())
    }

    pub fn owned_camps(&self) -> usize {
        self.sites.sites.iter().filter(|s| s.owner == Some(self.player)).count()
    }

    pub fn in_city(&self) -> bool {
        self.sites.get(self.here).is_some_and(|s| s.city)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Needs {
    production_demand: Tally,
    production_stock: Tally,
    material_quota: Tally,
    material_stock: Tally,
    global_item_stock: Tally,
    useful_recipes: Vec<Vec<String>>,
    required_hazards: Vec<Danger>,
    equipment_demand: Vec<i32>,
    reserve: Vec<(usize, u8)>,

    pub planned_settlement_payment: Option<usize>,
    pub doctor_payment_needed: bool,
    pub has_attack_plan: bool,
    pub attack_water_needed: bool,
    pub mission_hazard: Option<Danger>,
    pub immediate_food_needed: bool,
    pub immediate_water_needed: bool,
    pub portable_food: i32,
    pub desired_portable_food: i32,
    pub portable_water_capacity: i32,
    pub desired_portable_water_capacity: i32,
    pub desired_attack_water_container_capacity: i32,
    pub camp_water_container_shortfall: i32,
    pub has_critical_camp_water_container_shortfall: bool,
    pub melee_weapon_stock: i32,
    pub melee_weapon_quota: i32,
    pub urgent_weapon_needed: bool,
    pub protection_stock: i32,
    pub protection_quota: i32,
    pub pump_stock: i32,
    pub pump_quota: i32,
    pub production_tool_stock: i32,
    pub production_tool_quota: i32,
    pub has_owned_production_shortfall: bool,
    pub portable_production_reserve_needed: bool,
}

/// `DesiredWaterContainerCapacity(people)`: a wineskin for each two, a
/// canteen for one left over.
pub fn desired_water_container_capacity(people: i32) -> i32 {
    people / 2 * 5 + people % 2 * 3
}

/// `IsPump`.
pub fn is_pump(items: Items, kind: usize) -> bool {
    matches!(items.types.name_of(kind), Some("item_hand_pump" | "item_industrial_pump"))
}

fn is_melee(items: Items, kind: usize) -> bool {
    items.types.damage_of(kind) > 0 && !super::equipment::is_firearm(items.types, kind)
}

/// `Trading.CampWaterContainerShortfall`: one container for each camp and
/// one more for each living person there, less what its rooms hold.
pub fn camp_water_container_shortfall(items: Items, sites: &Sites, player: usize) -> i32 {
    sites
        .sites
        .iter()
        .filter(|s| s.owner == Some(player) && !s.city)
        .map(|s| {
            let desired = 1 + s.guards.iter().filter(|g| g.alive).count() as i32;
            let held = s.stored.iter().filter(|&&k| items.is_water_container(k)).count() as i32;
            (desired - held).max(0)
        })
        .sum()
}

/// `Trading.DesiredProtectionReserve`.
pub fn desired_protection_reserve(sites: &Sites, player: usize, policy: &AiPolicy) -> i32 {
    let camps: Vec<_> = sites.sites.iter().filter(|s| s.owner == Some(player)).collect();
    if camps.is_empty() {
        return 0;
    }
    let campaign = sites.sites.iter().any(|s| !s.city && s.owner != Some(player) && s.danger != Danger::None);
    if campaign {
        return policy.attack_group_size;
    }
    if camps.len() >= 5 || camps.iter().any(|s| s.danger != Danger::None) { 2 } else { 1 }
}

impl Needs {
    pub fn new(r: &Rival, ai: &ModernAi, w: &World, plans: Plans) -> Needs {
        let items = w.items;
        let t = items.types;
        let carried: Vec<usize> = group::carried(r).into_iter().map(|(_, _, k)| k).collect();
        let pool: Vec<(usize, i32)> = ai.reserve.iter().map(|&(k, n)| (k, n as i32)).collect();
        let size = group::size(r) as i32;
        let mut n = Needs {
            equipment_demand: w.equipment_demand.to_vec(),
            reserve: ai.reserve.clone(),
            planned_settlement_payment: plans.settlement_payment,
            doctor_payment_needed: group::needs_doctor_payment(r, plans.has_doctor),
            has_attack_plan: plans.has_attack_plan,
            attack_water_needed: plans.has_attack_plan && plans.attack_water_needed,
            mission_hazard: plans.mission_hazard,
            immediate_food_needed: (0..group::size(r)).any(|m| group::needs(r, m).food <= 3),
            immediate_water_needed: (0..group::size(r)).any(|m| group::needs(r, m).water <= 2),
            ..Needs::default()
        };
        let food_reserve: i32 = (0..group::size(r)).map(|m| group::needs(r, m).food).sum();
        let food_carried: i32 = carried.iter().map(|&k| t.food_of(k)).sum();
        n.portable_food = food_reserve + food_carried;
        n.desired_portable_food = size * STANDING_FOOD_DAYS;
        let portable_water_capacity: i32 = carried.iter().filter(|&&k| items.is_water_container(k)).map(|&k| items.water_capacity(k)).sum();
        let reserve_water_capacity: i32 = pool.iter().filter(|(k, _)| items.is_water_container(*k)).map(|&(k, c)| items.water_capacity(k) * c).sum();
        n.portable_water_capacity = portable_water_capacity + reserve_water_capacity;
        let max_water: i32 = (0..group::size(r)).map(|m| group::needs(r, m).max_water).sum();
        n.desired_portable_water_capacity = max_water + desired_water_container_capacity(size);
        n.desired_attack_water_container_capacity = desired_water_container_capacity(w.policy.attack_group_size);
        n.camp_water_container_shortfall = camp_water_container_shortfall(items, w.sites, w.player);
        n.has_critical_camp_water_container_shortfall = w.sites.sites.iter().any(|s| {
            s.owner == Some(w.player) && !s.city && s.travel_water_bottleneck(items, 2) && {
                let desired = 1 + s.guards.iter().filter(|g| g.alive).count();
                s.stored.iter().filter(|&&k| items.is_water_container(k)).count() < desired
            }
        });

        let is_pitchfork = |k: usize| t.name_of(k) == Some("item_pitchfork");
        let ordinary = carried.iter().filter(|&&k| is_melee(items, k) && !is_pitchfork(k)).count() as i32
            + pool.iter().filter(|&&(k, _)| !is_pitchfork(k) && is_melee(items, k)).map(|(_, c)| c).sum::<i32>();
        let pitchforks = carried.iter().filter(|&&k| is_pitchfork(k)).count() as i32 + pool.iter().filter(|&&(k, _)| is_pitchfork(k)).map(|(_, c)| c).sum::<i32>();
        n.melee_weapon_stock = ordinary + pitchforks;
        let group_weapon = carried.iter().any(|&k| t.damage_of(k) > 0);
        n.urgent_weapon_needed = w.day >= 100 && !group_weapon;
        n.melee_weapon_quota = w.policy.attack_group_size;
        n.protection_stock = carried.iter().filter(|&&k| items.is_hazard_protection(k)).count() as i32
            + pool.iter().filter(|&&(k, c)| c > 0 && items.is_hazard_protection(k)).map(|(_, c)| c).sum::<i32>();
        n.protection_quota = desired_protection_reserve(w.sites, w.player, w.policy);
        n.pump_stock = carried.iter().filter(|&&k| is_pump(items, k)).count() as i32;

        let is_material = |k: usize| t.name_of(k).is_some_and(|nm| CONSTRUCTION_MATERIALS.contains(&nm));
        for &k in &carried {
            if let Some(p) = w.produce_of(k) {
                n.production_stock.add(p, 1);
            }
            if is_material(k) {
                n.material_stock.add(t.name_of(k).unwrap_or(""), 1);
            }
        }
        for &(k, c) in &pool {
            if let Some(p) = w.produce_of(k) {
                n.production_stock.add(p, c);
            }
            if is_material(k) {
                n.material_stock.add(t.name_of(k).unwrap_or(""), c);
            }
        }
        for &k in &carried {
            n.global_item_stock.add(t.name_of(k).unwrap_or(""), 1);
        }
        for &(k, c) in &pool {
            n.global_item_stock.add(t.name_of(k).unwrap_or(""), c);
        }
        for s in w.sites.sites.iter().filter(|s| s.owner == Some(w.player) && !s.city) {
            for &k in s.stored.iter().chain(s.elsewhere.iter()) {
                n.global_item_stock.add(t.name_of(k).unwrap_or(""), 1);
            }
        }

        let mut unfilled: Vec<(Vec<String>, Vec<&[&str]>)> = Vec::new();
        for s in w.sites.sites.iter().filter(|s| s.owner == Some(w.player) && !s.city) {
            if needs_pump(items, s) {
                n.pump_quota += 1;
                n.add_opportunity(&PUMP_RECIPES);
            }
            if s.danger != Danger::None {
                if s.guards.iter().any(|g| g.owner == w.player && !g.protected) {
                    if !n.required_hazards.contains(&s.danger) {
                        n.required_hazards.push(s.danger);
                    }
                    n.add_opportunity(&[&PROTECTION_MATERIALS]);
                }
                continue;
            }
            if !prefer_production(s) {
                continue;
            }
            let useful: Vec<usize> = (0..s.productions.len()).filter(|&i| trade_priority(items, s, i) > 0.0).collect();
            if useful.iter().any(|&i| s.productions[i].installed > 0) {
                continue;
            }
            let mut products: Vec<String> = Vec::new();
            let mut recipes: Vec<&[&str]> = Vec::new();
            for &i in &useful {
                let product = t.name_of(s.productions[i].produce).unwrap_or("");
                if !products.iter().any(|p| p == product) {
                    products.push(String::from(product));
                }
                match product {
                    "item_meat" => recipes.push(&MEAT_RECIPE),
                    "item_rats" => recipes.push(&RAT_RECIPE),
                    _ => {}
                }
            }
            if !products.is_empty() {
                unfilled.push((products, recipes));
            }
        }
        // One tool fills one camp; the scarcest product first, the camp
        // making fewest things first.
        let mut stock: Vec<(String, i32)> = n.production_stock.iter().map(|(k, v)| (String::from(k), v)).collect();
        stock.sort_by_key(|(p, _)| unfilled.iter().filter(|c| c.0.iter().any(|x| x == p)).count());
        for (product, count) in stock {
            for _ in 0..count {
                let camp = unfilled
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.0.iter().any(|x| *x == product))
                    .min_by_key(|(i, c)| (c.0.len(), *i))
                    .map(|(i, _)| i);
                let Some(i) = camp else { break };
                unfilled.remove(i);
            }
        }
        n.has_owned_production_shortfall = !unfilled.is_empty();
        for (products, recipes) in &unfilled {
            for p in products {
                n.production_demand.add(p, 1);
            }
            if !recipes.is_empty() {
                n.add_opportunity(recipes);
            }
        }
        for s in &w.sites.sites {
            if !s.city && s.owner.is_none() && s.danger != Danger::None && !n.required_hazards.contains(&s.danger) {
                n.required_hazards.push(s.danger);
            }
        }
        n.production_tool_stock = n.production_stock.iter().map(|(_, v)| v).sum();
        let free_useful = |s: &super::camps::Site| !s.city && s.owner.is_none() && (0..s.productions.len()).any(|i| trade_priority(items, s, i) > 0.0);
        n.production_tool_quota = if w.sites.sites.iter().any(free_useful) { 1 } else { 0 };
        n.portable_production_reserve_needed = n.production_tool_stock < n.production_tool_quota;
        if n.portable_production_reserve_needed && !n.has_owned_production_shortfall {
            let mut products: Vec<&str> = Vec::new();
            for s in w.sites.sites.iter().filter(|s| !s.city && s.owner.is_none()) {
                for (i, p) in s.productions.iter().enumerate() {
                    let name = t.name_of(p.produce).unwrap_or("");
                    if trade_priority(items, s, i) > 0.0 && !products.contains(&name) {
                        products.push(name);
                    }
                }
            }
            let recipes: Vec<&[&str]> = products
                .iter()
                .filter_map(|p| match *p {
                    "item_meat" => Some(&MEAT_RECIPE[..]),
                    "item_rats" => Some(&RAT_RECIPE[..]),
                    _ => None,
                })
                .collect();
            if !recipes.is_empty() {
                n.add_opportunity(&recipes);
            }
        }
        n
    }

    /// `AddOpportunity`: each recipe is useful, and each of its materials
    /// wanted once more.
    fn add_opportunity(&mut self, recipes: &[&[&str]]) {
        let mut seen: Vec<&str> = Vec::new();
        for r in recipes {
            let mut set: Vec<String> = Vec::new();
            for m in r.iter() {
                if !set.iter().any(|x| x == m) {
                    set.push(String::from(*m));
                }
                if !seen.contains(m) {
                    seen.push(m);
                }
            }
            self.useful_recipes.push(set);
        }
        for m in seen {
            self.material_quota.add(m, 1);
        }
    }

    pub fn equipment_demand(&self, kind: usize) -> i32 {
        self.equipment_demand.get(kind).copied().unwrap_or(0)
    }

    pub fn needs_production(&self, w: &World, kind: usize) -> bool {
        match w.produce_of(kind) {
            Some(p) => self.production_demand.get(p) > 0 || self.portable_production_reserve_needed,
            None => false,
        }
    }

    pub fn production_shortfall(&self, w: &World, kind: usize) -> i32 {
        w.produce_of(kind).map_or(0, |p| self.production_demand.get(p))
    }

    pub fn material_quota(&self, id: &str) -> i32 {
        self.material_quota.get(id)
    }

    pub fn material_stock(&self, id: &str) -> i32 {
        self.material_stock.get(id)
    }

    pub fn needs_material(&self, id: &str) -> bool {
        self.material_stock(id) < self.material_quota(id)
    }

    pub fn completes_useful_recipe(&self, id: &str) -> bool {
        self.useful_recipes
            .iter()
            .any(|r| r.iter().any(|m| m == id) && r.iter().filter(|m| *m != id).all(|m| self.material_stock(m) > 0))
    }

    pub fn has_complete_useful_recipe(&self) -> bool {
        self.useful_recipes.iter().any(|r| r.iter().all(|m| self.material_stock(m) > 0))
    }

    pub fn material_demand_breadth(&self, id: &str) -> i32 {
        self.useful_recipes.iter().filter(|r| r.iter().any(|m| m == id)).count() as i32
    }

    fn protects(items: Items, kind: usize, danger: Danger) -> bool {
        items.types.get(kind).is_some_and(|t| t.protection.iter().any(|(d, _)| *d == danger))
    }

    pub fn needs_protection(&self, items: Items, kind: usize) -> bool {
        self.required_hazards.iter().any(|&d| Self::protects(items, kind, d))
    }

    pub fn provides_mission_protection(&self, items: Items, kind: usize) -> bool {
        self.mission_hazard.is_some_and(|d| d != Danger::None && Self::protects(items, kind, d))
    }

    pub fn can_acquire_attack_weapon(&self, items: Items, kind: usize) -> bool {
        items.types.damage_of(kind) > 0 && self.equipment_demand(kind) > 0
    }

    pub fn global_item_count(&self, items: Items, kind: usize) -> i32 {
        self.global_item_stock.get(items.types.name_of(kind).unwrap_or(""))
    }

    fn resting(items: Items, kind: usize) -> bool {
        items.types.get(kind).is_some_and(|t| t.functions.iter().any(|f| f == "resting_sustenance"))
    }

    /// `CanBuy`.
    pub fn can_buy(&self, w: &World, kind: usize, pending: i32) -> bool {
        let items = w.items;
        let t = items.types;
        if Self::resting(items, kind) {
            return self.global_item_count(items, kind) + pending < 1;
        }
        if super::equipment::is_equipment(t, kind) {
            let in_reserve: i32 = self.reserve.iter().filter(|(k, _)| *k == kind).map(|(_, c)| *c as i32).sum();
            if super::equipment::reserve_accepts(t, kind, items) && in_reserve + pending >= MAXIMUM_PURCHASED_PER_TYPE {
                return false;
            }
            let name = t.name_of(kind).unwrap_or("");
            let essential = self.planned_settlement_payment == Some(kind)
                || self.needs_production(w, kind)
                || self.needs_material(name)
                || items.is_hazard_protection(kind) && (self.provides_mission_protection(items, kind) || self.needs_protection(items, kind));
            return self.equipment_demand(kind) > pending
                || essential && self.global_item_count(items, kind) + pending < MAXIMUM_PURCHASED_PER_TYPE;
        }
        t.food_of(kind) > 0
            || items.is_water_container(kind) && pending < self.camp_water_container_shortfall
            || self.global_item_count(items, kind) + pending < MAXIMUM_PURCHASED_PER_TYPE
    }

    /// `IsStrategic`.
    pub fn is_strategic(&self, w: &World, kind: usize) -> bool {
        let items = w.items;
        let t = items.types;
        let name = t.name_of(kind).unwrap_or("");
        Self::resting(items, kind) && self.global_item_count(items, kind) < 1
            || self.planned_settlement_payment == Some(kind)
            || t.heal_of(kind) > 0 && self.doctor_payment_needed
            || items.is_water_container(kind)
                && (self.immediate_water_needed
                    || self.attack_water_needed
                    || self.portable_water_capacity < self.desired_portable_water_capacity
                    || self.portable_water_capacity < self.desired_attack_water_container_capacity
                    || self.camp_water_container_shortfall > 0)
            || t.food_of(kind) > 0 && self.portable_food < self.desired_portable_food
            || self.needs_production(w, kind)
            || self.needs_material(name)
            || self.equipment_demand(kind) > 0
            || is_pump(items, kind) && self.pump_stock < self.pump_quota
            || self.can_acquire_attack_weapon(items, kind) && self.melee_weapon_stock < self.melee_weapon_quota
            || items.is_hazard_protection(kind) && (self.needs_protection(items, kind) || self.protection_stock < self.protection_quota)
    }

    pub fn is_policy_attack_weapon(&self, items: Items, kind: usize) -> bool {
        is_melee(items, kind)
    }

    pub fn resting_sustenance(items: Items, kind: usize) -> bool {
        Self::resting(items, kind)
    }
}
