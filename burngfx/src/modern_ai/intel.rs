//! What the opponent knows of others' camps (`DefenseIntelligence`) and of
//! its own failed attacks (`AttackRetryMemory`). At tag `v1.2`,
//! `AI/PlayerBehavior/Combat/`.
//!
//! Both are knowledge, not the world: an observation is what was seen the
//! day it was seen, and a retry is weighed against forces met at the start
//! of an encounter, never against defenders far away. Both are kept in
//! [`ModernAi`] and saved.

use super::state::{Force, ModernAi, Observation, Retry};

/// `Group.MAX_PEOPLE`.
pub const MAX_PEOPLE: i32 = 5;

/// `DefenseEstimate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefenseEstimate {
    pub expected_defenders: i32,
    pub plausible_maximum: i32,
    pub estimated_strength: f32,
    pub based_on_contact: bool,
}

/// `UpdateKnowledge`: a camp whose owner is not the one remembered is
/// remembered anew, owned since today. `camps` are the places that are not
/// cities, with their owners.
pub fn update_knowledge(ai: &mut ModernAi, day: i32, camps: impl IntoIterator<Item = (usize, Option<usize>)>) {
    for (camp, owner) in camps {
        match ai.knowledge.binary_search_by_key(&camp, |o| o.camp) {
            Ok(i) if ai.knowledge[i].owner == owner => {}
            Ok(i) => ai.knowledge[i] = fresh(camp, owner, day),
            Err(i) => ai.knowledge.insert(i, fresh(camp, owner, day)),
        }
    }
}

fn fresh(camp: usize, owner: Option<usize>, day: i32) -> Observation {
    Observation { camp, owner, owner_since: day, observed_day: 0, defenders: 0, strength: 0.0 }
}

pub fn observation(ai: &ModernAi, camp: usize) -> Option<&Observation> {
    ai.knowledge.binary_search_by_key(&camp, |o| o.camp).ok().map(|i| &ai.knowledge[i])
}

/// `SustainableDefenderCapacity`: as many guards as the water feeds and as
/// some production keeps fed without starving the camp, at least one.
/// `sustains(guards)`: any valid production at its full tools gives
/// `guards` food a day without starving.
pub fn sustainable_capacity(water: i32, sustains: impl Fn(i32) -> bool) -> i32 {
    let water = water.clamp(1, MAX_PEOPLE);
    let mut food = 1;
    for guards in 1..=MAX_PEOPLE {
        if !sustains(guards) {
            break;
        }
        food = guards;
    }
    water.min(food).max(1)
}

/// What the estimate needs of the world about the camp.
#[derive(Clone, Copy, Debug)]
pub struct CampFacts {
    pub capacity: i32,
    /// Camps the remembered owner holds.
    pub owner_camps: i32,
    /// `IsStrategicallyImportant`: two roads or fewer, or a well-established
    /// potential.
    pub strategic: bool,
}

/// `Estimate`. Call [`update_knowledge`] first, as upstream does.
pub fn estimate(ai: &ModernAi, day: i32, camp: usize, facts: CampFacts) -> DefenseEstimate {
    let o = observation(ai, camp).cloned().unwrap_or_else(|| fresh(camp, None, day));
    let capacity = facts.capacity.max(1);
    let age = (day - o.owner_since).max(0);
    let owner_camps = if o.owner.is_none() { 0 } else { facts.owner_camps };
    let mut fortification = 0.05f32
        + (age as f32 / 120.0).min(0.35)
        + (day as f32 / 300.0).min(0.25)
        + (owner_camps as f32 * 0.025).min(0.20);
    if facts.strategic {
        fortification += 0.10;
    }
    if ai.was_recently_contested(camp, day) {
        fortification += 0.20;
    }
    let mut expected = 1;
    if capacity >= 2 && fortification >= 0.55 {
        expected = 2;
    }
    if capacity >= 3 && fortification >= 0.95 {
        expected = 3;
    }
    if capacity >= 4 && fortification >= 1.25 {
        expected = 4;
    }
    let contact = o.observed_day > 0 && day - o.observed_day <= 20;
    let mut per_defender = 22.0f32;
    if contact {
        let reinforcements = (day - o.observed_day).max(0) / 10;
        expected = expected.max(capacity.min(o.defenders + reinforcements));
        if o.defenders > 0 && o.strength > 0.0 {
            per_defender = o.strength / o.defenders as f32;
        }
    }
    let expected = expected.clamp(1, capacity);
    DefenseEstimate { expected_defenders: expected, plausible_maximum: capacity, estimated_strength: expected as f32 * per_defender, based_on_contact: contact }
}

/// `UpdateKnowledgeFromEncounter`: the living defenders met, today.
pub fn observe_encounter(ai: &mut ModernAi, day: i32, camp: usize, owner: Option<usize>, living: i32, strength: f32) {
    update_knowledge(ai, day, [(camp, owner)]);
    if let Ok(i) = ai.knowledge.binary_search_by_key(&camp, |o| o.camp) {
        let o = &mut ai.knowledge[i];
        o.observed_day = day;
        o.defenders = living;
        o.strength = strength;
    }
}

impl Force {
    /// `ImprovedOver`.
    pub fn improved_over(&self, previous: &Force) -> bool {
        self.count > previous.count || self.strength >= previous.strength * 1.10 || self.boss_health >= previous.boss_health + 10
    }

    /// `WeakerThan`.
    pub fn weaker_than(&self, previous: &Force) -> bool {
        self.count < previous.count
            || self.count == previous.count
                && (self.health < previous.health && self.strength <= previous.strength || self.strength < previous.strength * 0.85)
    }
}

/// `AttackRetryMemory.Record`: true when further attempts are blocked
/// until the forces change. A capture clears the record.
pub fn record_attempt(ai: &mut ModernAi, target: usize, owner: usize, attackers: Force, defenders: Force, killed: i32, captured: bool) -> Assessment {
    let at = ai.retries.iter().position(|r| r.target == target);
    if captured {
        if let Some(i) = at {
            ai.retries.remove(i);
        }
        return Assessment { comparison: alloc::string::String::from("defense cleared"), blocked: false };
    }
    let previous = at.map(|i| &ai.retries[i]).filter(|p| p.owner == Some(owner));
    let lasting = previous.is_some_and(|p| defenders.weaker_than(&p.defender));
    let improved = previous.is_some_and(|p| attackers.improved_over(&p.attacker));
    let blocked = previous.is_some() && !lasting && !improved && killed == 0;
    let mut comparison = alloc::string::String::from(if killed > 0 {
        "defender eliminated"
    } else if previous.is_none() {
        "first encounter; lasting progress unknown"
    } else if lasting {
        "defense weaker than at previous encounter start"
    } else if improved {
        "attacking party improved"
    } else {
        "no lasting progress since previous encounter start"
    });
    if let Some(p) = previous {
        comparison.push_str(&alloc::format!(
            " (starting defense {} defenders/{} HP -> {} defenders/{} HP)",
            p.defender.count, p.defender.health, defenders.count, defenders.health
        ));
    }
    let record = Retry { target, owner: Some(owner), attacker: attackers, defender: defenders, blocked };
    match at {
        Some(i) => ai.retries[i] = record,
        None => ai.retries.push(record),
    }
    Assessment { comparison, blocked }
}

/// `AttackRetryMemory.Assessment`: the comparison as upstream words it, and
/// whether further attempts wait for the forces to change.
#[derive(Clone, Debug, PartialEq)]
pub struct Assessment {
    pub comparison: alloc::string::String,
    pub blocked: bool,
}

/// `CanRetry`: no time limit — only stronger attackers or weaker
/// defenders, seen at the camp, end a stalemate.
pub fn can_retry(ai: &ModernAi, target: usize, owner: usize, attackers: &Force, observed: Option<&Force>) -> bool {
    match ai.retries.iter().find(|r| r.target == target) {
        Some(p) if p.owner == Some(owner) && p.blocked => {
            attackers.improved_over(&p.attacker) || observed.is_some_and(|d| d.weaker_than(&p.defender))
        }
        _ => true,
    }
}
