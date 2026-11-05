//! What a camp is worth to the opponent: `CampEconomy` and `EconomicReturn`.
//! At tag `v1.2`, `AI/PlayerBehavior/Camps/`.
//!
//! Over [`Sites`], a picture of the places as the planner reads them at the
//! start of its turn — water, productions and their tools, who is camped,
//! what is stored — built from the world by the game, or by hand in a test.
//! Nothing here changes the world.

use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::production::{Production, Rate};

use super::group::Items;
use super::route::RouteMap;

/// `ReststopLocationId`.
pub const RESTSTOP: usize = 25;
/// `PlentyOfWater`.
pub const PLENTY_OF_WATER: i32 = 3;
/// `TravelSupplySurplusTarget`.
pub const TRAVEL_SUPPLY_SURPLUS_TARGET: i32 = 2;

/// A production a place may be set to, and how many of its tools are
/// there (`GetProductionToolCount`).
#[derive(Clone, Debug, PartialEq)]
pub struct SiteProduction {
    pub id: usize,
    pub production: Production,
    /// The kind it makes.
    pub produce: usize,
    pub installed: i32,
}

impl SiteProduction {
    /// `MaxToolCount`.
    pub fn max_tools(&self) -> i32 {
        self.production.max_combination as i32
    }
}

/// Somebody camped at a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guard {
    pub owner: usize,
    pub alive: bool,
    /// Carries something with `resting_sustenance` (a bible): eats nothing.
    pub resting: bool,
    /// Carries something against the place's danger
    /// (`FindBestProtection(null, danger) != null`).
    pub protected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub city: bool,
    pub owner: Option<usize>,
    pub danger: Danger,
    /// `Source.Water`, nought with no source.
    pub water: i32,
    pub source_reserve: i32,
    /// `AvailableProducts != null`.
    pub has_products: bool,
    /// `ValidProductions`, in their order.
    pub productions: Vec<SiteProduction>,
    /// The selected one, as an index into `productions`.
    pub current: Option<usize>,
    /// `CampNPC`: everybody of anybody's camped here.
    pub guards: Vec<Guard>,
    /// Kinds lying in the rooms.
    pub stored: Vec<usize>,
    /// Kinds the opponent's guards here carry and that lie on the ground
    /// (`camp.Items`), for what it holds altogether.
    pub elsewhere: Vec<usize>,
    /// Kinds the owner's guards here carry (`CampNPC.SelectMany(Items)`).
    pub guard_items: Vec<usize>,
    /// `GetCurrentProductionStockCount`.
    pub stock: i32,
    /// Neighbours with way lengths, in their order.
    pub roads: Vec<(usize, i32)>,
    /// The technician's bonus this place's rate gets now.
    pub bonus: i32,
    /// It has a spring room (`SourceRoom`), and a working pump stands in it.
    pub source_room: bool,
    pub pump: bool,
    /// `ReinforcementPlanning.IsThreatened` for the planning player.
    pub threatened: bool,
}

/// Every place, by its index.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sites {
    pub sites: Vec<Site>,
    /// The turn's camp assessments (`AiTurnContext.Camps`), when the
    /// controller has captured them: values taken once and reused for the
    /// rest of the turn, as upstream reuses them.
    pub cache: Option<alloc::rc::Rc<core::cell::RefCell<CampCache>>>,
}

/// `CampAssessments`: what was captured of the opponent's own camps after
/// the turn's preparation, and what was worked out lazily since.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CampCache {
    /// The captured camps: `(camp, threatened)`.
    pub camps: Vec<(usize, bool)>,
    /// `DesiredProductionTools`: `((camp, production id), count)`.
    pub desired: Vec<((usize, usize), i32)>,
    pub marginal_camp: Vec<(usize, f32)>,
    pub marginal_water: Vec<(usize, f32)>,
}

impl Sites {
    /// The captured assessment's threat for a camp, if there is one.
    pub fn cached_threat(&self, camp: usize) -> Option<bool> {
        let c = self.cache.as_ref()?.borrow();
        c.camps.iter().find(|(l, _)| *l == camp).map(|(_, t)| *t)
    }

    fn captured(&self, camp: usize) -> bool {
        self.cache.as_ref().is_some_and(|c| c.borrow().camps.iter().any(|(l, _)| *l == camp))
    }

    /// Look a lazily computed per-camp value up in the turn's cache, or
    /// compute it and keep it when the camp was captured.
    pub fn cached_desired(&self, key: (usize, usize), compute: impl FnOnce() -> i32) -> i32 {
        if let Some(c) = self.cache.as_ref() {
            if let Some(&(_, v)) = c.borrow().desired.iter().find(|(k, _)| *k == key) {
                return v;
            }
        }
        let v = compute();
        if self.captured(key.0) {
            if let Some(c) = self.cache.as_ref() {
                c.borrow_mut().desired.push((key, v));
            }
        }
        v
    }

    fn cached_f32(&self, water: bool, camp: usize, compute: impl FnOnce() -> f32) -> f32 {
        if let Some(c) = self.cache.as_ref() {
            let b = c.borrow();
            let list = if water { &b.marginal_water } else { &b.marginal_camp };
            if let Some(&(_, v)) = list.iter().find(|(l, _)| *l == camp) {
                return v;
            }
        }
        let v = compute();
        if self.captured(camp) {
            if let Some(c) = self.cache.as_ref() {
                let mut b = c.borrow_mut();
                if water { b.marginal_water.push((camp, v)) } else { b.marginal_camp.push((camp, v)) }
            }
        }
        v
    }

    /// `EconomicReturn.MarginalCampImprovement` through the turn's cache.
    pub fn marginal_camp(&self, items: Items, camp: usize, player: usize) -> f32 {
        self.cached_f32(false, camp, || marginal_camp_improvement(items, &self.sites[camp], player))
    }

    /// `EconomicReturn.MarginalWaterImprovement` through the turn's cache.
    pub fn marginal_water(&self, items: Items, camp: usize, player: usize, needs_pump: bool) -> f32 {
        self.cached_f32(true, camp, || marginal_water_improvement(items, &self.sites[camp], player, needs_pump))
    }
}

impl RouteMap for Sites {
    fn locations(&self) -> usize {
        self.sites.len()
    }
    fn ways(&self, l: usize) -> Vec<(usize, i32)> {
        self.sites.get(l).map(|s| s.roads.clone()).unwrap_or_default()
    }
    fn is_city(&self, l: usize) -> bool {
        self.sites.get(l).is_some_and(|s| s.city)
    }
    fn owner(&self, l: usize) -> Option<usize> {
        self.sites.get(l).and_then(|s| s.owner)
    }
}

impl Site {
    /// `GetRate(tools, npcCount)` of one of its productions.
    pub fn rate(&self, items: Items, p: &SiteProduction, tools: i32, people: i32) -> Rate {
        p.production.rate_with(tools.max(0) as usize, people.max(0) as usize, items.types.food_of(p.produce), self.bonus)
    }

    /// `GetFoodProductionRate().FoodPerDay`: the selected production, its
    /// tools, everybody camped.
    pub fn food_per_day(&self, items: Items) -> i32 {
        match self.current.and_then(|i| self.productions.get(i)) {
            Some(p) if self.owner.is_some() => self.rate(items, p, p.installed, self.guards.len() as i32).food_per_day,
            _ => 0,
        }
    }

    fn produces(&self, name: &str, items: Items) -> bool {
        self.has_products && self.productions.iter().any(|p| items.types.name_of(p.produce) == Some(name))
    }

    /// `LivingGuardCount`.
    pub fn living_guards(&self, player: usize) -> i32 {
        self.guards.iter().filter(|g| g.owner == player && g.alive).count() as i32
    }

    /// `SupplyConsumingGuardCount`, of the owner's.
    fn eaters(&self) -> i32 {
        self.owner.map_or(0, |o| self.guards.iter().filter(|g| g.owner == o && g.alive && !g.resting).count() as i32)
    }

    /// `HasAdvancedFoodPotential`: meat or snakes.
    pub fn advanced_food_potential(&self, items: Items) -> bool {
        self.produces("item_meat", items) || self.produces("item_snake", items)
    }

    pub fn rat_food_potential(&self, items: Items) -> bool {
        self.produces("item_rats", items)
    }

    /// `HasFoodProductionPotential`: some production at its full tools feeds
    /// one without starving.
    pub fn food_production_potential(&self, items: Items) -> bool {
        !self.city
            && self.productions.iter().any(|p| {
                let r = self.rate(items, p, p.max_tools(), 1);
                !r.camp_starving && r.food_per_day >= 1
            })
    }

    pub fn plenty_of_water(&self) -> bool {
        self.water >= PLENTY_OF_WATER
    }

    pub fn well_established_potential(&self, items: Items) -> bool {
        self.advanced_food_potential(items) && self.plenty_of_water()
    }

    /// `IsWellEstablished`: held, making meat or snakes, and two to spare of
    /// food and of water a day.
    pub fn well_established(&self, items: Items) -> bool {
        self.owner.is_some()
            && self
                .current
                .and_then(|i| self.productions.get(i))
                .is_some_and(|p| matches!(items.types.name_of(p.produce), Some("item_meat" | "item_snake")))
            && self.water_surplus() >= TRAVEL_SUPPLY_SURPLUS_TARGET
            && self.food_surplus(items) >= TRAVEL_SUPPLY_SURPLUS_TARGET
    }

    /// `FoodSurplusPerDay`.
    pub fn food_surplus(&self, items: Items) -> i32 {
        self.food_per_day(items) - self.eaters()
    }

    /// `WaterSurplusPerDay`.
    pub fn water_surplus(&self) -> i32 {
        self.water - self.eaters()
    }

    pub fn stored_food(&self, items: Items) -> i32 {
        self.stored.iter().map(|&k| items.types.food_of(k)).sum()
    }

    pub fn stored_water(&self, items: Items) -> i32 {
        self.stored.iter().map(|&k| items.types.water_of(k)).sum()
    }

    /// `CanProvisionFood`.
    pub fn can_provision_food(&self, items: Items) -> bool {
        self.stored_food(items) > 0 || self.current.is_some() && self.food_surplus(items) > 0
    }

    /// `CanProvisionGroupWater`, with three days' reserve.
    pub fn can_provision_group_water(&self, items: Items, group: i32) -> bool {
        self.can_provision_group_water_for(items, group, 3)
    }

    pub fn can_provision_group_water_for(&self, items: Items, group: i32, reserve_days: i32) -> bool {
        let group = group.max(1);
        let residents = self.eaters();
        if self.water >= residents + group {
            return true;
        }
        if self.stored_water(items) + self.source_reserve >= group * reserve_days {
            return true;
        }
        self.water > residents && self.stored.iter().any(|&k| items.is_water_container(k))
    }

    /// `IsTravelWaterBottleneck`.
    pub fn travel_water_bottleneck(&self, items: Items, group: i32) -> bool {
        self.owner.is_some() && !self.can_provision_group_water(items, group)
    }

    /// `NeedsReusableWaterReserve`.
    pub fn needs_reusable_water_reserve(&self, group: i32) -> bool {
        let residents = self.eaters();
        self.water > residents && self.water < residents + group.max(1)
    }

    /// `IsAcceptableFirstCamp`.
    pub fn acceptable_first_camp(&self, items: Items) -> bool {
        self.advanced_food_potential(items) || self.rat_food_potential(items)
    }

    /// `CanSustainCamp`: water, and food for one at full tools.
    pub fn can_sustain_camp(&self, items: Items) -> bool {
        !self.city && self.water >= 1 && self.food_production_potential(items)
    }

    fn open_roads(&self) -> impl Iterator<Item = (usize, i32)> + '_ {
        self.roads.iter().copied().filter(|&(_, d)| d > 0)
    }

    /// `RouteSecurityValue`: a corridor is worth most, a hub some.
    pub fn route_security_value(&self) -> i32 {
        match self.open_roads().count() {
            2 => 120,
            n if n >= 3 => 40,
            _ => 0,
        }
    }

    /// `IsFoodStockCapped`.
    pub fn food_stock_capped(&self) -> bool {
        self.current.is_some() && self.stock >= crate::game::MAX_STOCK_FOOD as i32
    }

    pub fn tool_count(&self, production: usize) -> i32 {
        self.productions.iter().find(|p| p.id == production).map_or(0, |p| p.installed)
    }
}

impl Sites {
    pub fn get(&self, l: usize) -> Option<&Site> {
        self.sites.get(l)
    }

    /// `ConnectsOwnedCamps`: two or more open roads to the player's places.
    pub fn connects_owned_camps(&self, l: usize, player: usize) -> bool {
        self.sites[l].open_roads().filter(|&(n, _)| self.owner(n) == Some(player)).count() >= 2
    }

    /// `OpensCityAccess`.
    pub fn opens_city_access(&self, l: usize) -> bool {
        self.sites[l].open_roads().any(|(n, _)| self.is_city(n))
    }

    /// `WantedSupplySurplus`: two once the player holds a well-established
    /// camp, else one.
    pub fn wanted_supply_surplus(&self, items: Items, player: usize) -> i32 {
        if self.sites.iter().any(|s| s.owner == Some(player) && s.well_established(items)) {
            TRAVEL_SUPPLY_SURPLUS_TARGET
        } else {
            1
        }
    }

    /// `TerritorialValue`.
    pub fn territorial_value(&self, items: Items, l: usize) -> i32 {
        let s = &self.sites[l];
        let food = if s.has_products {
            s.productions
                .iter()
                .map(|p| match items.types.name_of(p.produce) {
                    Some("item_meat") => 360,
                    Some("item_snake") => 320,
                    Some("item_rats") => 160,
                    _ => 0,
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        let water = s.water.min(5) * 25;
        let established = if s.well_established_potential(items) { 300 } else { 0 };
        let route = if food == 0 { s.route_security_value() } else { 0 };
        let economic = (potential(items, s, 1).sustainable_value_per_day * 35.0) as i32;
        food + water + established + route + economic
    }
}

/// `EconomicReturnAssessment`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Return {
    /// The production, as its index in the site's list.
    pub production: Option<usize>,
    pub tools: i32,
    pub guards: i32,
    pub food_per_day: i32,
    pub surplus_food_per_day: i32,
    pub export_items_per_day: f32,
    pub trade_value_per_day: f32,
    pub sustainable_value_per_day: f32,
}

/// `EconomicReturn.Assess`: what the surplus would be worth sold, as items
/// of the produce, weighed by how dependable the water is.
pub fn assess(items: Items, site: &Site, production: Option<usize>, tools: i32, guards: i32, water: Option<i32>) -> Return {
    let Some(p) = production.and_then(|i| site.productions.get(i)) else {
        return Return { production: None, tools, guards, surplus_food_per_day: -guards, ..Return::default() };
    };
    let rate = site.rate(items, p, tools, guards);
    let surplus = rate.food_per_day - guards;
    let food_value = items.types.food_of(p.produce);
    let export = if surplus > 0 && food_value > 0 { surplus as f32 / food_value as f32 } else { 0.0 };
    let trade = export * items.value(p.produce);
    let reliability = match water.unwrap_or(site.water) {
        w if w >= PLENTY_OF_WATER => 1.0,
        2 => 0.75,
        1 => 0.5,
        _ => 0.25,
    };
    Return {
        production,
        tools,
        guards,
        food_per_day: rate.food_per_day,
        surplus_food_per_day: surplus,
        export_items_per_day: export,
        trade_value_per_day: trade,
        sustainable_value_per_day: trade * reliability,
    }
}

/// `CurrentCamp`.
pub fn current(items: Items, site: &Site, player: usize) -> Return {
    let tools = site.current.and_then(|i| site.productions.get(i)).map_or(0, |p| p.installed);
    assess(items, site, site.current, tools, site.living_guards(player), None)
}

/// `PotentialCamp`: the best of its productions at full tools; the most
/// sustainable value, then the most surplus, the first of equals.
pub fn potential(items: Items, site: &Site, guards: i32) -> Return {
    if !site.has_products {
        return assess(items, site, None, 0, guards, None);
    }
    let mut best: Option<Return> = None;
    for (i, p) in site.productions.iter().enumerate() {
        let r = assess(items, site, Some(i), p.max_tools(), guards, None);
        let better = best.is_none_or(|b| {
            r.sustainable_value_per_day > b.sustainable_value_per_day
                || r.sustainable_value_per_day == b.sustainable_value_per_day && r.surplus_food_per_day > b.surplus_food_per_day
        });
        if better {
            best = Some(r);
        }
    }
    best.unwrap_or_default()
}

/// `CalculateMarginalCampImprovement`: one more tool for any production,
/// against what the camp makes now.
pub fn marginal_camp_improvement(items: Items, site: &Site, player: usize) -> f32 {
    let now = current(items, site, player);
    let guards = now.guards.max(1);
    if !site.has_products {
        return 0.0;
    }
    let best = site
        .productions
        .iter()
        .enumerate()
        .map(|(i, p)| assess(items, site, Some(i), p.max_tools().min(p.installed + 1), guards, None).sustainable_value_per_day)
        .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))))
        .unwrap_or(now.sustainable_value_per_day);
    best - now.sustainable_value_per_day
}

/// `CalculateMarginalWaterImprovement`, `needs_pump` being
/// `Trading.NeedsPump` for the place.
pub fn marginal_water_improvement(items: Items, site: &Site, player: usize, needs_pump: bool) -> f32 {
    if !needs_pump {
        return 0.0;
    }
    let now = current(items, site, player);
    if now.production.is_none() {
        return 0.0;
    }
    let hand_pump = site.water + (site.water / 4).max(2);
    (assess(items, site, now.production, now.tools, now.guards, Some(hand_pump)).sustainable_value_per_day - now.sustainable_value_per_day).max(0.0)
}

/// `ProductionToolReturn`: the most one more tool of `production` would add
/// at any safe camp of the player's.
pub fn production_tool_return(items: Items, sites: &Sites, player: usize, production: usize) -> f32 {
    sites
        .sites
        .iter()
        .filter(|s| s.owner == Some(player) && s.danger == Danger::None)
        .filter_map(|s| {
            let i = s.productions.iter().position(|p| p.id == production)?;
            let p = &s.productions[i];
            let guards = s.living_guards(player).max(1);
            let now = current(items, s, player).sustainable_value_per_day;
            Some((assess(items, s, Some(i), p.max_tools().min(p.installed + 1), guards, None).sustainable_value_per_day - now).max(0.0))
        })
        .fold(0.0, f32::max)
}

/// `SustainableEmpireIncome`.
pub fn sustainable_empire_income(items: Items, sites: &Sites, player: usize) -> f32 {
    sites.sites.iter().filter(|s| s.owner == Some(player)).map(|s| current(items, s, player).sustainable_value_per_day).sum()
}

/// `TripValuePerDay`.
pub fn trip_value_per_day(collectible: f32, days: i32) -> f32 {
    collectible / (days + 1).max(1) as f32
}
