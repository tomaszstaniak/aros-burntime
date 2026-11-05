//! The economic escape hatch and its timers: `EconomicSupport`. At tag
//! `v1.2`, `AI/PlayerBehavior/Economy/EconomicSupport.cs`.
//!
//! After twenty-five days without a new advanced trap the opponent is given
//! one missing part of a trap, a few times in a game (`slump_material_grants`).
//! The progress upstream keeps in a weak table is saved here
//! ([`super::state::Support`]).

use alloc::vec::Vec;

use crate::rival::Rival;

use super::camps::Sites;
use super::group::{self, Items};
use super::policy::AiPolicy;
use super::production::useful_construction_opportunities;
use super::state::ModernAi;

/// `AdvancedTrapSlumpTurns`.
pub const ADVANCED_TRAP_SLUMP_TURNS: i32 = 25;
const ADVANCED_TRAPS: [&str; 3] = ["item_rat_trap", "item_trap", "item_snake_trap"];

fn advanced(items: Items, kind: usize) -> bool {
    items.types.name_of(kind).is_some_and(|n| ADVANCED_TRAPS.contains(&n))
}

/// `AdvancedTrapCount`: in the reserve, carried, and at the camps (rooms and
/// guards).
pub fn advanced_trap_count(items: Items, sites: &Sites, r: &Rival, ai: &ModernAi, player: usize) -> i32 {
    let pooled: i32 = ai.reserve.iter().filter(|&&(k, _)| advanced(items, k)).map(|&(_, c)| c as i32).sum();
    let carried = group::carried(r).iter().filter(|&&(_, _, k)| advanced(items, k)).count() as i32;
    let established: i32 = sites
        .sites
        .iter()
        .filter(|s| s.owner == Some(player))
        .map(|s| s.stored.iter().chain(s.guard_items.iter()).filter(|&&k| advanced(items, k)).count() as i32)
        .sum();
    pooled + carried + established
}

/// `AdvancedTrapCoverage`: the share of camps with one.
pub fn advanced_trap_coverage(items: Items, sites: &Sites, player: usize) -> f32 {
    let camps: Vec<_> = sites.sites.iter().filter(|s| s.owner == Some(player)).collect();
    if camps.is_empty() {
        return 0.0;
    }
    let covered = camps.iter().filter(|s| s.stored.iter().chain(s.guard_items.iter()).any(|&k| advanced(items, k))).count();
    covered as f32 / camps.len() as f32
}

/// `HasPooledAdvancedTrap`.
pub fn has_pooled_advanced_trap(items: Items, ai: &ModernAi) -> bool {
    ai.reserve.iter().any(|&(k, c)| c > 0 && advanced(items, k))
}

/// `Trading.HasConstructionComponent`.
pub fn has_construction_component(items: Items, r: &Rival, ai: &ModernAi, name: &str) -> bool {
    let k = items.types.index_of(name);
    k.is_some_and(|k| ai.reserve_count(k) > 0) || group::carried(r).iter().any(|&(_, _, c)| Some(c) == k)
}

/// `GrantSlumpSupportIfNeeded`. The part granted, if one was.
pub fn grant_slump_support(items: Items, sites: &Sites, r: &Rival, ai: &mut ModernAi, player: usize, day: i32, policy: &AiPolicy) -> Option<usize> {
    let traps = advanced_trap_count(items, sites, r, ai, player);
    if !ai.support.initialized {
        ai.support.initialized = true;
        ai.support.last_trap_day = day;
        ai.support.trap_count = traps;
        return None;
    }
    if traps > ai.support.trap_count {
        ai.support.last_trap_day = day;
    }
    ai.support.trap_count = traps;
    if ai.slump_grants_used >= policy.slump_material_grant_limit || day - ai.support.last_trap_day < ADVANCED_TRAP_SLUMP_TURNS {
        return None;
    }
    let mut chances: Vec<(super::production::Opportunity, Vec<&str>)> = useful_construction_opportunities(items, sites, player, ai)
        .into_iter()
        .filter(|o| matches!(o.result, "item_trap" | "item_rat_trap"))
        .map(|o| {
            let missing: Vec<&str> = o.materials.iter().copied().filter(|m| !has_construction_component(items, r, ai, m)).collect();
            (o, missing)
        })
        .filter(|(_, m)| !m.is_empty())
        .collect();
    chances.sort_by(|(a, am), (b, bm)| {
        let rank = |o: &super::production::Opportunity, m: &Vec<&str>| {
            if o.result == "item_rat_trap" && m.len() == 1 {
                0
            } else if o.result == "item_trap" {
                1
            } else {
                2
            }
        };
        rank(a, am).cmp(&rank(b, bm)).then(am.len().cmp(&bm.len())).then(b.value.cmp(&a.value))
    });
    let (opportunity, _) = chances.into_iter().next()?;
    let mut missing: Vec<&str> = opportunity.materials.iter().copied().filter(|m| !has_construction_component(items, r, ai, m)).collect();
    missing.sort_by(|a, b| {
        let v = |n: &str| items.types.index_of(n).map_or(0.0, |k| items.value(k));
        v(b).partial_cmp(&v(a)).unwrap_or(core::cmp::Ordering::Equal)
    });
    let kind = items.types.index_of(missing.first()?)?;
    // `TryReserveConstructionMaterial`: a material, none in the reserve yet.
    if ai.reserve_count(kind) >= 1 || !ai.reserve_add(kind) {
        return None;
    }
    let stalled = day - ai.support.last_trap_day;
    ai.support.last_trap_day = day;
    ai.slump_grants_used += 1;
    let (component, result, used, limit) = (alloc::string::String::from(items.types.name_of(kind).unwrap_or("")), opportunity.result, ai.slump_grants_used, policy.slump_material_grant_limit);
    ai.telemetry.report(|| {
        alloc::format!("economic slump support generated {component} for {result} after {stalled} turns without a new advanced trap (grant {used}/{limit})")
    });
    Some(kind)
}

/// `TraderNoveltyScore`: 100 for a trader never seen, else five a day since,
/// at most eighty.
pub fn trader_novelty(ai: &ModernAi, trader: usize, day: i32) -> i32 {
    match ai.support.traders_seen.iter().find(|(t, _)| *t == trader) {
        None => 100,
        Some(&(_, last)) => ((day - last) * 5).clamp(0, 80),
    }
}

/// `RecordTraderExposure`: whether this is the first time today.
pub fn record_trader_exposure(ai: &mut ModernAi, trader: usize, day: i32) -> bool {
    match ai.support.traders_seen.iter_mut().find(|(t, _)| *t == trader) {
        Some(e) => {
            let first = e.1 != day;
            e.1 = day;
            first
        }
        None => {
            ai.support.traders_seen.push((trader, day));
            true
        }
    }
}

/// `HasBeenStrategicallyStalled`: the same place and the same number of
/// camps for `turns` days.
pub fn strategically_stalled(ai: &mut ModernAi, here: usize, camps: i32, day: i32, turns: i32) -> bool {
    match ai.support.strategic {
        Some((l, c, since)) if l == here && c == camps => day - since >= turns,
        _ => {
            ai.support.strategic = Some((here, camps, day));
            false
        }
    }
}

/// `IsSavingForSnakeTrap`, given `Trading.HasStrategicSnakeTrapNeed`.
pub fn saving_for_snake_trap(ai: &ModernAi, snake_trap_needed: bool) -> bool {
    ai.support.saving_for_snake_trap && snake_trap_needed
}
