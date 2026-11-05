//! Whether the opponent may found a camp at a place, and the expansion
//! tests recruitment needs: `AiState.CanCreateCamp` (`CanClaim`),
//! `ExpansionPlanning.CanBootstrapCamp`, `CanSustainCandidate`,
//! `HasTravellingHazardProtection`, `IsOccupiedByOtherBoss`, and
//! `CampEconomy.CanFoundWithRestingSustenance`. At tag `v1.2`.

use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::rival::Rival;

use super::camps::{Site, RESTSTOP};
use super::group::{self, Items};
use super::needs::{Needs, World};
use super::route;
use super::state::ModernAi;
use super::supplies::{can_survive_recovery_route, GroupSupplies};

/// `GetAvailableProducts`: what the place's productions make, by name.
pub fn available_products<'a>(items: Items<'a>, site: &Site) -> Vec<&'a str> {
    site.productions.iter().filter_map(|p| items.types.name_of(p.produce)).collect()
}

/// `AiItemPool.HasTrap`: a trap, snake trap, rat trap or knife in the
/// reserve whose production makes one of `products`.
pub fn reserve_has_trap(ai: &ModernAi, w: &World, products: &[&str]) -> bool {
    ["item_trap", "item_snake_trap", "item_rat_trap", "item_knife"].iter().any(|tool| {
        w.items.types.index_of(tool).is_some_and(|k| ai.reserve_count(k) > 0 && w.produce_of(k).is_some_and(|p| products.contains(&p)))
    })
}

/// `FindCompatibleGroupProduction`: the carried tool whose produce the
/// place makes and is the most filling. `(member, slot)`.
pub fn compatible_group_production(r: &Rival, w: &World, products: &[&str]) -> Option<(usize, usize)> {
    let t = w.items.types;
    let mut best: Option<(usize, usize, i32)> = None;
    for (m, s, k) in group::carried(r) {
        let Some(p) = w.produce_of(k).filter(|p| products.contains(p)) else { continue };
        let food = t.index_of(p).map_or(0, |pk| t.food_of(pk));
        if best.is_none_or(|b| food > b.2) {
            best = Some((m, s, food));
        }
    }
    best.map(|b| (b.0, b.1))
}

/// `HasNeighboringFoodSupport`: a camp of the player's one road away,
/// survivable to reach, feeding the travellers left after one stays.
fn neighbouring_food_support(w: &World, g: &GroupSupplies, l: usize) -> bool {
    let sites = w.sites;
    let travellers = (g.size() as i32 - 1).max(1);
    sites.sites[l].roads.iter().filter(|&&(_, d)| d > 0).any(|&(supplier, _)| {
        sites.get(supplier).is_some_and(|s| s.owner == Some(w.player))
            && route::find(sites, w.player, None, l, supplier).is_some_and(|r| can_survive_recovery_route(g, &r))
            && sites.sites[supplier].food_surplus(w.items) >= travellers
    })
}

/// `CanBootstrapCamp`: food can be made here, nobody threatens it, and the
/// place feeds one with no tool or a neighbour helps.
pub fn can_bootstrap_camp(w: &World, g: &GroupSupplies, l: usize) -> bool {
    let site = &w.sites.sites[l];
    if !site.food_production_potential(w.items) || site.threatened {
        return false;
    }
    // `GetRate(0, 1)`: no tool, one guard, no technician's bonus.
    let local = site.productions.iter().any(|p| p.production.rate(0, 1, w.items.types.food_of(p.produce)).food_per_day > 1);
    local || neighbouring_food_support(w, g, l)
}

/// Something of `resting_sustenance` (a bible).
fn resting(items: Items, kind: usize) -> bool {
    Needs::resting_sustenance(items, kind)
}

/// `CanFoundWithRestingSustenance`: at Reststop, which has water, with a
/// bible carried — or one stored in a camp and room to carry it.
pub fn can_found_with_resting_sustenance(r: &Rival, w: &World, l: usize) -> bool {
    let items = w.items;
    let Some(site) = w.sites.get(l) else { return false };
    if l != RESTSTOP || site.city || site.water < 1 {
        return false;
    }
    let carried = group::carried(r).iter().any(|&(_, _, k)| resting(items, k));
    let free = (0..group::size(r)).any(|m| !group::pack(r, m).is_full());
    let stored = w.sites.sites.iter().any(|s| s.owner == Some(w.player) && s.stored.iter().chain(s.elsewhere.iter()).any(|&k| resting(items, k)));
    carried || free && stored
}

/// `CanSustainCandidate`.
pub fn can_sustain_candidate(r: &Rival, w: &World, l: usize) -> bool {
    w.sites.sites[l].can_sustain_camp(w.items) || can_found_with_resting_sustenance(r, w, l)
}

/// `AiItemPool.HasGasMask` and `HasProtectionSuit`, by the names upstream
/// looks for.
pub fn reserve_protects(ai: &ModernAi, w: &World, danger: Danger) -> bool {
    let names: &[&str] = match danger {
        Danger::Gas => &["item_gas_mask", "item_protection_suit", "item_paper_helmet"],
        Danger::Radiation => &["item_protection_suit", "item_paper_helmet"],
        Danger::None => return true,
    };
    names.iter().any(|n| w.items.types.index_of(n).is_some_and(|k| ai.reserve_count(k) > 0))
}

/// `CanCreateCamp` (`CanClaim`): not a city, nobody's, something to make
/// food with (a tool in the reserve or the group, a bootstrap, a bible at
/// Reststop), and the reserve's protection against its danger.
pub fn can_claim(r: &Rival, ai: &ModernAi, w: &World, g: &GroupSupplies, l: usize) -> bool {
    let Some(site) = w.sites.get(l) else { return false };
    if site.city || site.owner.is_some() {
        return false;
    }
    let products = available_products(w.items, site);
    let tool = reserve_has_trap(ai, w, &products) || compatible_group_production(r, w, &products).is_some();
    if !tool && !can_bootstrap_camp(w, g, l) && !can_found_with_resting_sustenance(r, w, l) {
        return false;
    }
    reserve_protects(ai, w, site.danger)
}

/// `HasTravellingHazardProtection`: every one of the group carries
/// something listed against the place's danger.
pub fn has_travelling_hazard_protection(r: &Rival, items: Items, site: &Site) -> bool {
    if site.danger == Danger::None {
        return true;
    }
    (0..group::size(r)).all(|m| {
        group::pack(r, m).iter().any(|k| items.types.get(k).is_some_and(|t| t.protection.iter().any(|(d, _)| *d == site.danger)))
    })
}

/// `IsOccupiedByOtherBoss`: a living rival leader stands there, not on the
/// road. `rivals_at` are those leaders' places.
pub fn is_occupied_by_other_boss(rivals_at: &[usize], l: usize) -> bool {
    rivals_at.contains(&l)
}

/// `CampEconomy.StrategicRole`, for the reasons decisions give.
pub fn strategic_role(items: Items, site: &Site) -> &'static str {
    if site.well_established_potential(items) {
        "high-potential meat/snake camp"
    } else if site.advanced_food_potential(items) {
        "meat/snake camp needing better water"
    } else if site.rat_food_potential(items) {
        "starter rat camp"
    } else if site.route_security_value() > 0 {
        "secondary route camp"
    } else {
        "low-priority camp"
    }
}
