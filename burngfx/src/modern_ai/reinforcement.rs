//! Which camps want another guard, and getting one there:
//! `ReinforcementPlanning`. At tag `v1.2`,
//! `AI/PlayerBehavior/Combat/ReinforcementPlanning.cs`.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use super::camps::{Site, Sites};
use super::decision::{add_travel, Action, Decision, Risk, Traveller};
use super::group::Items;
use super::policy::AiPolicy;
use super::reason::Reason;
use super::route::{self, RouteMap};

/// `CalculateIsThreatened`: an enemy camp or a living rival leader within
/// `threat_radius` roads of the camp, or at it.
pub fn is_threatened(sites: &Sites, player: usize, radius: i32, rivals_at: &[usize], origin: usize) -> bool {
    let threat = |l: usize| sites.owner(l).is_some_and(|o| o != player) || rivals_at.contains(&l);
    if threat(origin) {
        return true;
    }
    let mut visited = vec![false; sites.locations().max(origin + 1)];
    visited[origin] = true;
    let mut queue = VecDeque::from([(origin, 0)]);
    while let Some((at, distance)) = queue.pop_front() {
        if distance >= radius {
            continue;
        }
        for (n, d) in sites.ways(at) {
            if d <= 0 || n >= visited.len() || visited[n] {
                continue;
            }
            visited[n] = true;
            if threat(n) {
                return true;
            }
            queue.push_back((n, distance + 1));
        }
    }
    false
}

/// Every place's threat, for [`Site::threatened`]: `IsThreatened`, the
/// turn's captured assessment where there is one, else worked out now.
pub fn mark_threatened(sites: &mut Sites, player: usize, radius: i32, rivals_at: &[usize]) {
    let marks: Vec<bool> = (0..sites.sites.len())
        .map(|l| sites.cached_threat(l).unwrap_or_else(|| is_threatened(sites, player, radius, rivals_at, l)))
        .collect();
    for (s, m) in sites.sites.iter_mut().zip(marks) {
        s.threatened = m;
    }
}

/// `IsStrategicLocation`: two roads or fewer, any water, or four food a day.
pub fn is_strategic_location(items: Items, site: &Site) -> bool {
    let open = site.roads.iter().filter(|&&(_, d)| d > 0).count();
    open <= 2 || site.water > 0 || site.food_per_day(items) >= 4
}

/// `IsCriticalCamp`.
pub fn is_critical_camp(items: Items, site: &Site) -> bool {
    is_strategic_location(items, site) || site.threatened
}

/// `SustainableGarrisonTarget`.
pub fn sustainable_garrison_target(site: &Site, policy_target: i32) -> i32 {
    policy_target.min(site.water).max(1)
}

/// The rate of the camp's production with `guards` and its tools, without a
/// technician's bonus (`Production.GetRate(tools, guards)`).
fn projected(items: Items, site: &Site, guards: i32) -> Option<crate::production::Rate> {
    let p = site.current.and_then(|i| site.productions.get(i))?;
    Some(p.production.rate(p.installed.max(0) as usize, guards.max(0) as usize, items.types.food_of(p.produce)))
}

fn can_support_garrison(items: Items, sites: &Sites, player: usize, site: &Site, guards: i32) -> bool {
    let wanted = sites.wanted_supply_surplus(items, player);
    if site.current.is_none() || site.water - guards < wanted {
        return false;
    }
    projected(items, site, guards).is_some_and(|r| !r.camp_starving && r.food_per_day - guards >= wanted)
}

/// `CanSupportAdditionalGuard`: under the policy's garrison, and one more
/// leaves the wanted surplus of water and food.
pub fn can_support_additional_guard(items: Items, sites: &Sites, player: usize, policy: &AiPolicy, site: &Site, guards: i32) -> bool {
    guards < policy.critical_garrison_target && can_support_garrison(items, sites, player, site, guards + 1)
}

/// `WantsAdditionalGuard`: a critical camp of the player's under its
/// sustainable target — two at most while it holds fewer than four camps.
pub fn wants_additional_guard(items: Items, sites: &Sites, player: usize, policy: &AiPolicy, camp: usize, garrison_target: i32) -> bool {
    let site = &sites.sites[camp];
    if garrison_target <= 1 || site.owner != Some(player) || !is_critical_camp(items, site) {
        return false;
    }
    let owned = sites.sites.iter().filter(|s| s.owner == Some(player)).count();
    let effective = if owned < 4 { garrison_target.min(2) } else { garrison_target };
    let guards = site.living_guards(player);
    guards < sustainable_garrison_target(site, effective) && can_support_additional_guard(items, sites, player, policy, site, guards)
}

/// `FindBestCampForReinforcement`: the reachable one wanting a guard, the
/// fewest guards first, then the nearest.
pub fn best_camp_for_reinforcement(items: Items, sites: &Sites, player: usize, policy: &AiPolicy, previous: Option<usize>, here: usize, garrison_target: i32) -> Option<usize> {
    if garrison_target <= 1 {
        return None;
    }
    (0..sites.sites.len())
        .filter(|&l| wants_additional_guard(items, sites, player, policy, l, garrison_target))
        .filter_map(|l| route::find(sites, player, previous, here, l).map(|r| (l, sites.sites[l].living_guards(player), r.days)))
        .min_by_key(|&(l, guards, days)| (guards, days, l))
        .map(|(l, _, _)| l)
}

/// `FindAttackStagingCamp`: an own camp that can keep the staged guards,
/// nearest the target, then the wettest, then nearest here.
pub fn attack_staging_camp(items: Items, sites: &Sites, player: usize, previous: Option<usize>, here: usize, target: usize, staged: i32) -> Option<usize> {
    let mut best: Option<(usize, i32, i32, i32)> = None;
    for l in 0..sites.sites.len() {
        if l == target || sites.owner(l) != Some(player) {
            continue;
        }
        let (Some(to), Some(attack)) = (route::find(sites, player, previous, here, l), route::find(sites, player, None, l, target)) else { continue };
        if !can_support_garrison(items, sites, player, &sites.sites[l], staged) {
            continue;
        }
        let key = (attack.days, -sites.sites[l].water, to.days);
        if best.is_none_or(|b| key < (b.1, b.2, b.3)) {
            best = Some((l, key.0, key.1, key.2));
        }
    }
    best.map(|b| b.0)
}

/// `RecruitmentNeeds`: the camp to reinforce, its target, and whether the
/// guard is staged for an attack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecruitmentNeeds {
    pub camp: Option<usize>,
    pub target: i32,
    pub attack_staging: bool,
}

/// `ReinforcementPlanning.AddCandidates`: station a follower where a camp
/// wants one (here, or travel there); with a group bigger than its limit
/// and nowhere to put the surplus, dismiss one.
pub fn add_candidates(
    t: &Traveller,
    policy: &AiPolicy,
    group_size: usize,
    required_attack_size: Option<i32>,
    needs: RecruitmentNeeds,
    candidates: &mut Vec<Decision>,
) {
    let limit = required_attack_size.map_or(2, |r| r.max(2));
    let surplus = group_size as i32 > limit;
    let camp = needs.camp.filter(|&c| {
        let s = &t.sites.sites[c];
        can_support_additional_guard(t.items, t.sites, t.player, policy, s, s.living_guards(t.player))
    });
    if let (Some(camp), true) = (camp, group_size > 1) {
        let priority: f32 = if surplus || needs.attack_staging { 2000.0 } else { 830.0 };
        if camp == t.here {
            let reason = if surplus {
                Reason::DemobilizeIntoGarrison
            } else if needs.attack_staging {
                Reason::StageFrontierGuard { camp }
            } else {
                Reason::RaiseCriticalGarrison { target: needs.target }
            };
            candidates.push(Decision::new(Action::StationFollower, priority.max(2000.0), Some(t.here), reason));
        } else {
            let reason = if surplus {
                Reason::DemobilizeIntoGarrisonAt { camp }
            } else if needs.attack_staging {
                Reason::DeliverFrontierGuard { camp }
            } else {
                Reason::ReinforceCriticalCamp { target: needs.target }
            };
            add_travel(t, candidates, Some(camp), priority, reason, Risk::default(), None, true);
        }
    } else if surplus {
        candidates.push(Decision::new(Action::DismissFollower, 2000.0, Some(t.here), Reason::DismissSurplusFollower));
    }
}
