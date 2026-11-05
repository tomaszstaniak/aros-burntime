//! What the opponent's camps make, how many tools each wants, and what it
//! builds from parts: `CampManagement.MaintainCampProductionPolicy`,
//! `CampEconomy.CalculateDesiredProductionToolCount`, `Trading.Production`
//! and `Construction`. At tag `v1.2`, `AI/PlayerBehavior/Camps/`,
//! `Trading/` and `Economy/`.
//!
//! The production a camp is set to is chosen by the rule every camp
//! follows ([`crate::v12_day::auto_select_food_production`]), for each of
//! the opponent's camps every turn. The rest reads [`Sites`] and the
//! reserve.

use alloc::string::String;
use alloc::vec::Vec;

use crate::combat::Kit;
use crate::construction::Constructions;
use crate::gamdat::Danger;
use crate::game::ItemBag;

use super::camps::{assess, Site, Sites};
use super::group::Items;
use super::route;
use super::state::ModernAi;

/// `ProductionTradePriority`: meat, snakes, rats first, then by the
/// produce's worth and food.
pub fn trade_priority(items: Items, site: &Site, production: usize) -> f32 {
    let Some(p) = site.productions.get(production) else { return f32::MIN };
    let base = match items.types.name_of(p.produce) {
        Some("item_meat") => 240.0,
        Some("item_snake") => 200.0,
        Some("item_rats") => 100.0,
        _ => 0.0,
    };
    base + items.value(p.produce) + items.types.food_of(p.produce) as f32
}

/// `ShouldPreferProductionAtCamp`: no danger and no threat.
pub fn prefer_production(site: &Site) -> bool {
    site.danger == Danger::None && !site.threatened
}

/// The best priority among the productions with a tool installed, or
/// `f32::MIN` with none.
fn installed_priority(items: Items, site: &Site) -> f32 {
    (0..site.productions.len())
        .filter(|&i| site.productions[i].installed > 0)
        .map(|i| trade_priority(items, site, i))
        .fold(f32::MIN, f32::max)
}

/// `CalculateDesiredProductionToolCount`: all the tools a production takes,
/// unless a second one would earn more at another of the player's camps
/// that has none — and pays for the road there within about a week — when
/// one is enough.
pub fn desired_tool_count(items: Items, sites: &Sites, player: usize, camp: usize, production: usize) -> i32 {
    let Some(id) = sites.sites[camp].productions.get(production).map(|p| p.id) else { return 0 };
    sites.cached_desired((camp, id), || calculate_desired_tool_count(items, sites, player, camp, production))
}

/// `CalculateDesiredProductionToolCount`.
pub fn calculate_desired_tool_count(items: Items, sites: &Sites, player: usize, camp: usize, production: usize) -> i32 {
    let site = &sites.sites[camp];
    let Some(p) = site.productions.get(production) else { return 0 };
    let max = p.max_tools();
    if max <= 1 {
        return max;
    }
    let guards = site.living_guards(player);
    let second = assess(items, site, Some(production), max, guards, None).sustainable_value_per_day
        - assess(items, site, Some(production), max - 1, guards, None).sustainable_value_per_day;
    let id = p.id;
    let worthwhile = sites.sites.iter().enumerate().any(|(l, other)| {
        if l == camp || other.owner != Some(player) || !prefer_production(other) {
            return false;
        }
        let Some(oi) = other.productions.iter().position(|q| q.id == id) else { return false };
        if other.productions[oi].installed != 0 || trade_priority(items, other, oi) < installed_priority(items, other) {
            return false;
        }
        let g = other.living_guards(player);
        let first = assess(items, other, Some(oi), 1, g, None).sustainable_value_per_day
            - assess(items, other, Some(oi), 0, g, None).sustainable_value_per_day;
        route::find(sites, player, None, camp, l).is_some_and(|r| first > second && r.days as f32 <= (first - second) * 7.0)
    });
    if worthwhile { 1 } else { max }
}

/// `NeedsProductionResult`: the camp makes `product`, has fewer tools for
/// it than it wants, and it is worth no less than what is installed.
pub fn needs_production_result(items: Items, sites: &Sites, player: usize, camp: usize, product: &str) -> bool {
    let site = &sites.sites[camp];
    let Some(i) = site.productions.iter().position(|p| items.types.name_of(p.produce) == Some(product)) else { return false };
    if site.productions[i].installed >= desired_tool_count(items, sites, player, camp, i) {
        return false;
    }
    trade_priority(items, site, i) >= installed_priority(items, site)
}

/// `HasOwnedProductionNeed`.
pub fn has_owned_production_need(items: Items, sites: &Sites, player: usize, product: &str) -> bool {
    (0..sites.sites.len()).any(|l| {
        sites.sites[l].owner == Some(player) && sites.sites[l].danger == Danger::None && needs_production_result(items, sites, player, l, product)
    })
}

/// `HasPotentialProductionNeed`: an owned need, or a free place that makes it.
pub fn has_potential_production_need(items: Items, sites: &Sites, player: usize, product: &str) -> bool {
    has_owned_production_need(items, sites, player, product)
        || sites.sites.iter().any(|s| !s.city && s.owner.is_none() && s.productions.iter().any(|p| items.types.name_of(p.produce) == Some(product)))
}

/// `MissingProductionToolCount`.
pub fn missing_tool_count(items: Items, sites: &Sites, player: usize, product: &str) -> i32 {
    let mut missing = 0;
    for (l, s) in sites.sites.iter().enumerate() {
        if s.owner != Some(player) || s.danger != Danger::None {
            continue;
        }
        for (i, p) in s.productions.iter().enumerate() {
            if items.types.name_of(p.produce) == Some(product) {
                missing += (desired_tool_count(items, sites, player, l, i) - p.installed).max(0);
            }
        }
    }
    missing
}

/// `NeedsPump`: a spring room with no pump, giving less than the camp
/// wants — three where meat or snakes could be made, else two, and always
/// one more than everybody camped there.
pub fn needs_pump(items: Items, site: &Site) -> bool {
    if !site.source_room || site.pump {
        return false;
    }
    let minimum = if site.advanced_food_potential(items) { super::camps::PLENTY_OF_WATER } else { 2 };
    site.water < minimum.max(site.guards.len() as i32 + 1)
}

pub fn needs_any_pump(items: Items, sites: &Sites, player: usize) -> bool {
    sites.sites.iter().any(|s| s.owner == Some(player) && needs_pump(items, s))
}

/// `NeedsCriticalPump`: a pump would help a camp that cannot water two
/// travellers.
pub fn needs_critical_pump(items: Items, sites: &Sites, player: usize) -> bool {
    sites.sites.iter().any(|s| s.owner == Some(player) && needs_pump(items, s) && s.travel_water_bottleneck(items, 2))
}

/// `BestEmpireProductionImprovement`.
pub fn best_empire_production_improvement(items: Items, sites: &Sites, player: usize) -> f32 {
    sites
        .sites
        .iter()
        .filter(|s| s.owner == Some(player) && prefer_production(s))
        .map(|s| {
            let l = sites.sites.iter().position(|x| core::ptr::eq(x, s)).unwrap_or(0);
            sites.marginal_camp(items, l, player).max(0.0)
        })
        .fold(0.0, f32::max)
}

/// The kinds of the reserve `HasGasMask` and `HasProtectionSuit` look for.
fn reserve_has(ai: &ModernAi, items: Items, names: &[&str]) -> bool {
    names.iter().any(|n| items.types.index_of(n).is_some_and(|k| ai.reserve_count(k) > 0))
}

/// `NeedsDangerProtection`: a guard of an owned dangerous camp has nothing
/// against its danger that this kind protects from, or a free dangerous
/// place awaits and the reserve has nothing for it.
pub fn needs_danger_protection(items: Items, sites: &Sites, player: usize, ai: &ModernAi, kind: usize) -> bool {
    let protects = |d: Danger| items.types.get(kind).is_some_and(|t| t.protection.iter().any(|(pd, _)| *pd == d));
    let owned = sites.sites.iter().any(|s| {
        s.owner == Some(player) && s.danger != Danger::None && protects(s.danger) && s.guards.iter().any(|g| g.owner == player && !g.protected)
    });
    let expansion = sites.sites.iter().any(|s| {
        !s.city
            && s.owner.is_none()
            && s.danger != Danger::None
            && protects(s.danger)
            && if s.danger == Danger::Radiation {
                !reserve_has(ai, items, &["item_protection_suit", "item_paper_helmet"])
            } else {
                !reserve_has(ai, items, &["item_gas_mask", "item_protection_suit", "item_paper_helmet"])
            }
    });
    owned || expansion
}

/// `ConstructionOpportunity`.
#[derive(Clone, Debug, PartialEq)]
pub struct Opportunity {
    pub result: &'static str,
    pub materials: &'static [&'static str],
    pub value: i32,
}

/// `UsefulConstructionOpportunities`: traps where camps want them (or, with
/// every camp served, where places could use them), pumps when a camp needs
/// one and nothing better is to be had, and the suit when protection is
/// wanted.
pub fn useful_construction_opportunities(items: Items, sites: &Sites, player: usize, ai: &ModernAi) -> Vec<Opportunity> {
    let mut out = Vec::new();
    let meat = missing_tool_count(items, sites, player, "item_meat");
    let rats = missing_tool_count(items, sites, player, "item_rats");
    let snake = missing_tool_count(items, sites, player, "item_snake");
    let owned_first = meat + rats + snake > 0;
    if meat > 0 || !owned_first && has_potential_production_need(items, sites, player, "item_meat") {
        out.push(Opportunity { result: "item_trap", materials: &["item_spring", "item_tin", "item_wire"], value: 90 + meat * 200 });
    }
    if rats > 0 || !owned_first && has_potential_production_need(items, sites, player, "item_rats") {
        out.push(Opportunity { result: "item_rat_trap", materials: &["item_wire", "item_woodpile", "item_screws"], value: 75 + rats * 200 });
    }
    if needs_any_pump(items, sites, player)
        && (best_empire_production_improvement(items, sites, player) <= 0.01 || needs_critical_pump(items, sites, player))
    {
        let pump_value = (sites
            .sites
            .iter()
            .filter(|s| s.owner == Some(player))
            .map(|s| {
                let l = sites.sites.iter().position(|x| core::ptr::eq(x, s)).unwrap_or(0);
                sites.marginal_water(items, l, player, needs_pump(items, s))
            })
            .fold(0.0, f32::max)
            * 100.0) as i32;
        out.push(Opportunity { result: "item_hand_pump", materials: &["item_broken_pump", "item_rags", "item_hose"], value: 45 + pump_value });
        out.push(Opportunity {
            result: "item_industrial_pump",
            materials: &["item_spare_parts", "item_iron_pipe", "item_rags", "item_hose"],
            value: 55 + pump_value,
        });
    }
    if items.types.index_of("item_protection_suit").is_some_and(|suit| needs_danger_protection(items, sites, player, ai, suit)) {
        out.push(Opportunity {
            result: "item_protective_suit",
            materials: &["item_gas_mask", "item_gloves", "item_protective_overall", "item_boots"],
            value: 50,
        });
    }
    out
}

/// `AiItemPool.ConstructionMaterialFilter`, in its order.
pub const CONSTRUCTION_MATERIALS: [&str; 17] = [
    "item_wire",
    "item_woodpile",
    "item_screws",
    "item_spring",
    "item_tin",
    "item_broken_pump",
    "item_spare_parts",
    "item_rags",
    "item_hose",
    "item_iron_pipe",
    "item_unloaded_rifle",
    "item_unloaded_pistol",
    "item_ammunition",
    "item_gas_mask",
    "item_gloves",
    "item_protective_overall",
    "item_boots",
];

fn material_count(ai: &ModernAi, items: Items, name: &str) -> u8 {
    items.types.index_of(name).map_or(0, |k| ai.reserve_count(k))
}

/// A pack the planner may take parts from, and whose chosen weapon and
/// protection are not to be taken.
pub struct Source<'a> {
    pub bag: &'a mut ItemBag,
    pub kit: Option<&'a mut Kit>,
}

/// `RefillConstructionReserve`: for each material the reserve has none of,
/// the first one found — in the camp's rooms, its guards, then the group,
/// never a chosen weapon or protection, a firearm, or ammunition carried
/// with a gun — goes into the reserve. The materials reserved.
pub fn refill_construction_reserve(ai: &mut ModernAi, items: Items, sources: &mut [Source]) -> Vec<String> {
    let mut reserved = Vec::new();
    for name in CONSTRUCTION_MATERIALS {
        let Some(kind) = items.types.index_of(name) else { continue };
        if ai.reserve_count(kind) > 0 {
            continue;
        }
        let found = sources.iter().enumerate().find_map(|(si, s)| {
            if name == "item_ammunition" && s.bag.iter().any(|k| super::equipment::is_firearm(items.types, k)) {
                return None;
            }
            (0..s.bag.len()).find_map(|slot| {
                let chosen = s.kit.as_ref().is_some_and(|k| k.weapon == Some(slot) || k.protection == Some(slot));
                (s.bag.get(slot) == Some(kind) && !chosen && !super::equipment::is_firearm(items.types, kind)).then_some((si, slot))
            })
        });
        let Some((si, slot)) = found else { continue };
        if !ai.reserve_add(kind) {
            continue;
        }
        let s = &mut sources[si];
        match s.kit.as_deref_mut() {
            Some(kit) => {
                super::equipment::take(s.bag, kit, slot);
            }
            None => {
                s.bag.remove(slot);
            }
        }
        reserved.push(String::from(name));
    }
    reserved
}

/// `Constructions.TryConstructAny` over the group's packs and the reserve:
/// the first wanted result whose parts are all there, built — parts from
/// the first pack holding each, else from the reserve. The kind made.
pub fn construct_any(ai: &mut ModernAi, items: Items, recipes: &Constructions, packs: &mut [Source], wanted: &[&str]) -> Option<usize> {
    for result in wanted {
        let Some(recipe) = recipes.iter().find(|r| r.result == *result) else { continue };
        let Some(made) = items.types.index_of(result) else { continue };
        // `ContainsAll` over the parts, then over the tools (which are
        // not taken).
        let enough = |list: &[String]| {
            list.iter().all(|need| {
                let want = list.iter().filter(|n| *n == need).count();
                let have = packs.iter().map(|s| s.bag.iter().filter(|&k| items.types.name_of(k) == Some(need)).count()).sum::<usize>()
                    + material_count(ai, items, need) as usize;
                have >= want
            })
        };
        if !enough(&recipe.items) || !enough(&recipe.tools) {
            continue;
        }
        for need in &recipe.items {
            let at = packs.iter().enumerate().find_map(|(si, s)| {
                (0..s.bag.len()).find(|&slot| s.bag.get(slot).is_some_and(|k| items.types.name_of(k) == Some(need))).map(|slot| (si, slot))
            });
            match at {
                Some((si, slot)) => {
                    let s = &mut packs[si];
                    match s.kit.as_deref_mut() {
                        Some(kit) => {
                            super::equipment::take(s.bag, kit, slot);
                        }
                        None => {
                            s.bag.remove(slot);
                        }
                    }
                }
                None => {
                    if let Some(k) = items.types.index_of(need) {
                        if CONSTRUCTION_MATERIALS.contains(&need.as_str()) {
                            ai.reserve_take(k);
                        }
                    }
                }
            }
        }
        return Some(made);
    }
    None
}

/// `ConstructPortableEconomicUpgrade`: a trap, rat trap or suit the
/// opportunities want, the most valuable first, built from the group and
/// the reserve. The kind made, for the caller to put in the reserve or at
/// the place (`StoreInReserveOrAtLocation`).
pub fn construct_portable_upgrade(
    ai: &mut ModernAi,
    items: Items,
    sites: &Sites,
    player: usize,
    recipes: &Constructions,
    party: &mut [Source],
) -> Option<usize> {
    let mut chances: Vec<Opportunity> = useful_construction_opportunities(items, sites, player, ai)
        .into_iter()
        .filter(|o| matches!(o.result, "item_trap" | "item_rat_trap" | "item_protective_suit"))
        .collect();
    chances.sort_by_key(|o| core::cmp::Reverse(o.value));
    let mut wanted: Vec<&str> = Vec::new();
    for o in &chances {
        if !wanted.contains(&o.result) {
            wanted.push(o.result);
        }
    }
    if wanted.is_empty() {
        return None;
    }
    construct_any(ai, items, recipes, party, &wanted)
}

/// `CollectRedundantProductionTools`'s count: how many tools of each
/// production are beyond what it wants (all of them, for one worth less
/// than what is installed), as `(production index, excess)`. The caller
/// takes that many — rooms first, then guards where the production counts
/// their packs — into the reserve while it accepts them.
pub fn redundant_tools(items: Items, sites: &Sites, player: usize, camp: usize) -> Vec<(usize, i32)> {
    let site = &sites.sites[camp];
    let best = installed_priority(items, site);
    (0..site.productions.len())
        .filter_map(|i| {
            let desired = if trade_priority(items, site, i) < best { 0 } else { desired_tool_count(items, sites, player, camp, i) };
            let excess = site.productions[i].installed - desired;
            (excess > 0).then_some((i, excess))
        })
        .collect()
}
