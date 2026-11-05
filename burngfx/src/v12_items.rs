//! v1.2's item rules that are not trade or combat: food rotting on the
//! ground, a mutant's loot, and what Extended's special items do.
//!
//! At tag `v1.2`: `GameRules.ProcessDroppedFoodDecay`, `LocationCycle`,
//! `Mutant.Die`, `ItemTypes.GenerateClass`, `MetalDetectorScavenging`,
//! `RadioIntel`, `GameRules.TurnEmployedCharacter` (the bible),
//! `CreatureMind` (bones and skulls), `TableCombat.PreferredProtection`,
//! `HazardRules`.
//!
//! Classic has none of it by its data: its `dropped_food_decay_interval` and
//! `mutant_drop_chance` are nought and no item in its table has a function.

use alloc::string::String;
use alloc::vec::Vec;

use crate::game::{ItemBag, ItemFacts};
use crate::gamdat::Danger;
use crate::items::ItemTypes;
use crate::spawn::{Dropped, Rng};

/// `LocationCycle.IsDue`: `interval > 0 && (day + location) % interval == 0`.
pub fn location_due(day: u16, location: usize, interval: i32) -> bool {
    interval > 0 && (day as i64 + location as i64) % interval as i64 == 0
}

/// `ProcessDroppedFoodDecay`: on the location's day, the first thing lying
/// there that is food (`FoodValue > 0`) is gone. Rooms are not the ground.
pub fn decay_dropped_food(
    ground: &mut Vec<Dropped>,
    location: usize,
    day: u16,
    interval: i32,
    facts: &impl ItemFacts,
) -> Option<Dropped> {
    if !location_due(day, location, interval) {
        return None;
    }
    let i = ground.iter().position(|d| d.location == location && facts.food(d.kind) > 0)?;
    Some(ground.remove(i))
}

/// `chance >= 1.0f || NextSingle() < chance`.
fn chance(rng: &mut Rng, p: f32) -> bool {
    p >= 1.0 || rng.single() < p
}

/// `Mutant.Die`: `GenerateClass(mutant_drop_type, ["nodrop"],
/// mutant_drop_chance)` — nothing when no type qualifies or the chance is
/// nought; else one of them, any one, when the chance falls.
pub fn mutant_drop(types: &ItemTypes, include: &[String], chance_of: f32, rng: &mut Rng) -> Option<usize> {
    let filter = crate::settings::Filter { include: include.to_vec(), exclude: alloc::vec![String::from("nodrop")] };
    let candidates = crate::spawn::candidates(types, &filter);
    if candidates.is_empty() || chance_of <= 0.0 || !chance(rng, chance_of) {
        return None;
    }
    candidates.get(rng.below(candidates.len() as u32) as usize).copied()
}

/// `HasItemFunction`: anything in this pack with that function
/// (`resting_sustenance`, `creature_deterrent`, `metal_detection`,
/// `remote_intel`).
pub fn has_function(pack: &ItemBag, types: &ItemTypes, function: &str) -> bool {
    pack.iter().any(|k| types.get(k).is_some_and(|t| t.functions.iter().any(|f| f == function)))
}

/// `MetalDetectorScavenging.DiscoveryChance`: one in a hundred a day of
/// the road, compounded.
pub fn discovery_chance(travel_days: i32) -> f64 {
    if travel_days <= 0 {
        return 0.0;
    }
    let mut miss = 1.0f64;
    for _ in 0..travel_days {
        miss *= 0.99;
    }
    1.0 - miss
}

/// `TryDiscoverAmmunition`'s roll: true when the ammunition is found.
/// `IsDiscovery(travelDays, NextSingle())`, reached only after
/// `travelDays <= 0` has already said no without a draw.
pub fn discovers_ammunition(travel_days: i32, rng: &mut Rng) -> bool {
    if travel_days <= 0 {
        return false;
    }
    let roll = rng.single() as f64;
    roll >= 0.0 && roll < discovery_chance(travel_days)
}

/// `RadioIntel.ThreatLevel`: nought for nobody, else 1..=4 by quarters of
/// a hundred of strategic strength.
pub fn threat_level(strength: f32) -> i32 {
    if strength <= 0.0 {
        return 0;
    }
    let q = strength / 25.0;
    let whole = q as i32;
    let up = if (whole as f32) < q { whole + 1 } else { whole };
    up.clamp(1, 4)
}

/// `TableCombat.PreferredProtection`: in a dangerous place the item that
/// protects best against it, when anything does; otherwise the best
/// defence (`FindBestDefense`, starting from what is chosen); else the first
/// thing that protects against anything. A slot of `pack`.
pub fn preferred_protection(pack: &ItemBag, selected: Option<usize>, hazard: Danger, types: &ItemTypes) -> Option<usize> {
    let selected = selected.filter(|&s| pack.get(s).is_some());
    let rate = |s: Option<usize>| s.and_then(|s| pack.get(s)).and_then(|k| types.get(k)).map_or(0, |t| t.protection_against(hazard));
    if hazard != Danger::None {
        let mut best = selected;
        for slot in 0..pack.len() {
            if rate(Some(slot)) > rate(best) {
                best = Some(slot);
            }
        }
        if rate(best) > 0 {
            return best;
        }
    }
    let defense = |s: Option<usize>| s.and_then(|s| pack.get(s)).map_or(0, |k| types.defense_of(k));
    let mut best = selected;
    for slot in 0..pack.len() {
        if defense(Some(slot)) > defense(best) {
            best = Some(slot);
        }
    }
    best.or_else(|| {
        (0..pack.len()).find(|&s| {
            pack.get(s).and_then(|k| types.get(k)).is_some_and(|t| !t.protection.is_empty())
        })
    })
}

/// `Character.GetHazardProtectionRate`, in percent: all of it for a face
/// immune to the danger, else what the protection worn gives against it.
pub fn hazard_protection_rate(pack: &ItemBag, protection: Option<usize>, hazard: Danger, face: u8, immune: &[i32], types: &ItemTypes) -> u32 {
    if immune.contains(&(face as i32)) {
        return 100;
    }
    protection.and_then(|s| pack.get(s)).and_then(|k| types.get(k)).map_or(0, |t| t.protection_against(hazard)).min(100)
}

/// `PassesDailyCheck`: no danger, full protection from what
/// [`preferred_protection`] chooses, or a face immune to it
/// (`gas_immune_faces`, `radiation_immune_faces`).
pub fn passes_daily_check(pack: &ItemBag, selected: Option<usize>, hazard: Danger, face: u8, immune: &[i32], types: &ItemTypes) -> bool {
    if hazard == Danger::None || immune.contains(&(face as i32)) {
        return true;
    }
    let chosen = preferred_protection(pack, selected, hazard, types);
    chosen.and_then(|s| pack.get(s)).and_then(|k| types.get(k)).map_or(0, |t| t.protection_against(hazard)) >= 100
}
