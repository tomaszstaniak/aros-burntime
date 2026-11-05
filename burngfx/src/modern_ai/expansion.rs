//! Where the opponent's territory grows: `ExpansionPlanning`. At tag
//! `v1.2`, `AI/PlayerBehavior/Expansion/ExpansionPlanning.cs`.
//!
//! The territorial plan — claim here, settle a neutral place, or attack a
//! hostile one — its scores, and its tie-breaks, kept as upstream has them:
//! candidates in the order of the world's places, the first of the best.
//! The ±20 jitter is drawn from the turn's generator, once for each scored
//! candidate in that order. Founding a camp
//! (`TryClaimCurrentAsLocalOpportunity`) is the world's, with the
//! controller.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use crate::combat::Kit;
use crate::game::ItemBag;
use crate::rival::Rival;
use crate::spawn::Rng;

use super::attack::{self, Field};
use super::claims;
use super::decision::{Action, Decision};
use super::deferrals;
use super::group::{self};
use super::policy::AiPolicy;
use super::reason::Reason;
use super::recruitment::{self, Candidate, Purse};
use super::route::{self, Route, RouteMap};
use super::state::ModernAi;
use super::supplies;

/// `FirstCampRouteDayPenalty`.
pub const FIRST_CAMP_ROUTE_DAY_PENALTY: i32 = 100;
/// `RestingSustenanceCampBonus`.
pub const RESTING_SUSTENANCE_CAMP_BONUS: i32 = 600;

/// `TerritorialPlan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub can_claim_current: bool,
    pub current_claim_score: f32,
    pub target: Option<usize>,
    pub target_score: f32,
    pub preparing_attack: bool,
}

/// What expansion reads beyond attack planning's [`Field`].
pub struct Theatre<'a> {
    pub f: &'a Field<'a>,
    pub r: &'a Rival,
    pub kit: &'a Kit,
    pub people: &'a [Vec<Candidate>],
    pub camp_rooms: &'a [Vec<ItemBag>],
    /// Where the other living leaders stand, not on the road.
    pub rivals_at: &'a [usize],
    /// Players out of the game (`Player.IsDead`).
    pub dead_players: &'a [usize],
    /// `Player.Destination` while travelling.
    pub travelling_to: Option<usize>,
    /// `DecisionContext.NeutralExpansionAllowed`.
    pub neutral_expansion_allowed: bool,
    /// `DecisionContext.CriticalSupplies`.
    pub critical_supplies: bool,
}

impl Theatre<'_> {
    fn sites(&self) -> &super::camps::Sites {
        self.f.w.sites
    }
    fn player(&self) -> usize {
        self.f.w.player
    }
    fn here(&self) -> usize {
        self.f.w.here
    }
    fn items(&self) -> super::group::Items<'_> {
        self.f.w.items
    }
    fn route(&self, from: usize, to: usize) -> Option<Route> {
        route::find(self.sites(), self.player(), self.f.previous, from, to)
    }
    fn hostile(&self, l: usize) -> bool {
        attack::is_hostile(self.sites(), l, self.player())
    }
    fn neutral(&self, l: usize) -> bool {
        !self.sites().is_city(l) && self.sites().owner(l).is_none()
    }
    fn open_roads(&self, l: usize) -> impl Iterator<Item = usize> + '_ {
        self.sites().ways(l).into_iter().filter(|&(_, d)| d > 0).map(|(n, _)| n)
    }
    fn owned_count(&self) -> usize {
        self.f.w.owned_camps()
    }
    fn has_owned_camp(&self) -> bool {
        self.owned_count() > 0
    }
    fn can_claim(&self, ai: &ModernAi, l: usize) -> bool {
        claims::can_claim(self.r, ai, self.f.w, self.f.group, l)
    }
    fn protected_for(&self, l: usize) -> bool {
        claims::has_travelling_hazard_protection(self.r, self.items(), &self.sites().sites[l])
    }
    fn occupied(&self, l: usize) -> bool {
        claims::is_occupied_by_other_boss(self.rivals_at, l)
    }
    fn sustain(&self, l: usize) -> bool {
        claims::can_sustain_candidate(self.r, self.f.w, l)
    }
    fn deferred(&self, ai: &mut ModernAi, l: usize) -> bool {
        let now = attack::progress(self.f);
        deferrals::is_deferred(ai, self.f.w.day, l, self.sites().owner(l), &now)
    }

    /// `CanStationCamp`: a follower to leave, or a recruit to be had here.
    pub fn can_station_camp(&self, ai: &ModernAi) -> bool {
        if group::size(self.r) > 1 {
            return true;
        }
        let purse = Purse { items: self.items(), r: self.r, leader_kit: self.kit, camp_rooms: self.camp_rooms, owned_camps: self.owned_count() };
        let people = self.people.get(self.here()).map_or(&[][..], |p| p.as_slice());
        recruitment::can_recruit(&purse, self.f.w.policy, people, &ai.demobilized, self.sites().is_city(self.here()))
    }

    /// `DistanceFromOwnedTerritory`: roads from the player's camps, through
    /// cities, own and free places but no other's camp, up to `maximum`.
    pub fn distance_from_owned(&self, target: usize, maximum: i32) -> Option<i32> {
        let sites = self.sites();
        let n = sites.locations();
        let mut visited = vec![false; n];
        let mut queue = VecDeque::new();
        for l in 0..n {
            if sites.owner(l) == Some(self.player()) {
                visited[l] = true;
                queue.push_back((l, 0));
            }
        }
        while let Some((at, distance)) = queue.pop_front() {
            if distance >= maximum {
                continue;
            }
            for nb in self.open_roads(at) {
                let d = distance + 1;
                if nb == target {
                    return Some(d);
                }
                if !sites.is_city(nb) && sites.owner(nb).is_some_and(|o| o != self.player()) {
                    continue;
                }
                if nb < n && !visited[nb] {
                    visited[nb] = true;
                    queue.push_back((nb, d));
                }
            }
        }
        None
    }

    /// `IsSustainableNeutralFrontier`.
    fn sustainable_neutral_frontier(&self, l: usize) -> bool {
        self.distance_from_owned(l, 4).is_some()
    }

    /// `FindSettlementReturnRoute`.
    fn settlement_return(&self, from: usize) -> Option<Route> {
        let items = self.items();
        (0..self.sites().sites.len())
            .filter(|&l| self.sites().owner(l) == Some(self.player()) && self.sites().sites[l].can_provision_food(items))
            .filter_map(|l| self.route(from, l))
            .min_by_key(|r| r.days)
    }

    /// `HasExpansionRouteSupplies`.
    fn expansion_route_supplies(&self, ai: &ModernAi, r: &Route) -> bool {
        if supplies::has_territorial_route_supplies(self.f.group, self.sites(), self.items(), self.player(), self.f.previous, self.here(), r, false) {
            return true;
        }
        let step = r.next_step;
        if self.sites().owner(step).is_some() || self.sites().is_city(step) || group::size(self.r) <= 1 || !self.can_claim(ai, step) {
            return false;
        }
        self.route(self.here(), step).is_some_and(|leg| supplies::has_route_supplies(self.f.group, &leg, false, 0, 0))
    }

    /// `HasSettlementRouteSupplies`.
    fn settlement_route_supplies(&self, ai: &ModernAi, target: usize, r: &Route) -> bool {
        if group::size(self.r) == 1 {
            return true;
        }
        if !self.expansion_route_supplies(ai, r) {
            return false;
        }
        let step = r.next_step;
        if !self.has_owned_camp() || self.sites().owner(step) == Some(self.player()) || self.sites().is_city(step) {
            return true;
        }
        let expedition = if self.can_claim(ai, step) { step } else { target };
        let Some(outbound) = self.route(self.here(), expedition) else { return false };
        self.settlement_return(expedition).is_some_and(|back| supplies::has_settlement_round_trip_food(self.f.group, &outbound, &back))
    }

    /// `CanWaitForSettlementFood`.
    fn can_wait_for_settlement_food(&self, ai: &ModernAi, target: usize, r: &Route) -> bool {
        let here = &self.sites().sites[self.here()];
        let step = r.next_step;
        if here.owner != Some(self.player())
            || here.current.is_none()
            || here.food_surplus(self.items()) <= 0
            || self.sites().owner(step) == Some(self.player())
            || self.sites().is_city(step)
        {
            return false;
        }
        let expedition = if self.can_claim(ai, step) { step } else { target };
        self.settlement_return(expedition).is_some()
    }

    fn preferred_first_settlement(&self, l: usize) -> bool {
        self.sites().sites[l].acceptable_first_camp(self.items()) || claims::can_found_with_resting_sustenance(self.r, self.f.w, l)
    }

    /// `HasReachableAcceptableFirstCamp`.
    fn reachable_acceptable_first_camp(&self, ai: &mut ModernAi) -> bool {
        for l in 0..self.sites().sites.len() {
            if l == self.here() || self.sites().is_city(l) || self.sites().owner(l).is_some() || self.occupied(l) {
                continue;
            }
            if self.deferred(ai, l) {
                continue;
            }
            if !self.sites().sites[l].acceptable_first_camp(self.items()) || !self.can_claim(ai, l) || !self.protected_for(l) {
                continue;
            }
            if self.route(self.here(), l).is_some_and(|r| self.settlement_route_supplies(ai, l, &r)) {
                return true;
            }
        }
        false
    }

    /// `IsSuitableCurrentClaim`.
    pub fn suitable_current_claim(&self, ai: &mut ModernAi) -> bool {
        let here = self.here();
        if self.occupied(here) || !self.sustain(here) {
            return false;
        }
        if self.has_owned_camp() || self.preferred_first_settlement(here) {
            return true;
        }
        !self.reachable_acceptable_first_camp(ai)
    }

    /// `RestingSustenanceScore`.
    fn resting_score(&self, l: usize) -> i32 {
        if claims::can_found_with_resting_sustenance(self.r, self.f.w, l) { RESTING_SUSTENANCE_CAMP_BONUS } else { 0 }
    }

    /// `NeutralTargetScore`.
    pub fn neutral_target_score(&self, policy: &AiPolicy, l: usize) -> f32 {
        policy.neutral_target_score
            + self.sites().territorial_value(self.items(), l) as f32
            + self.resting_score(l) as f32
            + if self.sites().connects_owned_camps(l, self.player()) { 900.0 } else { 0.0 }
    }

    /// `UnclaimedPocketScore`.
    fn unclaimed_pocket_score(&self, l: usize) -> i32 {
        let controlled = self.open_roads(l).filter(|&n| self.sites().owner(n).is_some_and(|o| o != self.player())).count();
        if controlled < 2 {
            return 0;
        }
        if self.here() == l || self.travelling_to == Some(l) { 210 } else { 90 }
    }

    /// `HasPortableCompatibleProduction`.
    fn portable_compatible_production(&self, ai: &ModernAi, l: usize) -> bool {
        let t = self.items().types;
        let works = |k: usize| t.name_of(k).and_then(|n| self.f.w.productions.worked_by(n)).is_some_and(|p| self.sites().sites[l].productions.iter().any(|sp| sp.id == p));
        group::carried(self.r).iter().any(|&(_, _, k)| works(k)) || ai.reserve.iter().any(|&(k, c)| c > 0 && works(k))
    }

    /// `EarlyNetworkOpeningScore`.
    fn early_network_opening_score(&self, l: usize) -> i32 {
        let outward = self.open_roads(l).filter(|&n| !self.sites().is_city(n) && self.sites().owner(n).is_none() && self.sustain(n)).count() as i32;
        outward.min(2) * 180
    }

    /// `NeutralFrontierScore`.
    fn neutral_frontier_score(&self, ai: &ModernAi, l: usize) -> i32 {
        if !self.has_owned_camp() {
            return 0;
        }
        let distance = self.distance_from_owned(l, 4);
        let locality = match distance {
            Some(1) => 500,
            Some(2) => 300,
            Some(3) => 100,
            Some(4) => 0,
            _ => -300,
        };
        let city_access = if self.sites().opens_city_access(l) { 250 } else { 0 };
        let early_food = if self.owned_count() < 2 && self.sites().sites[l].advanced_food_potential(self.items()) { 300 } else { 0 };
        let deploys = self.owned_count() == 1 && distance == Some(1) && self.portable_compatible_production(ai, l);
        let deployment = if deploys { 900 } else { 0 };
        let opening = if deploys { self.early_network_opening_score(l) } else { 0 };
        locality + city_access + early_food + deployment + opening
    }

    /// `HasForwardSustainableNeutral`: within two roads past `first`, a free
    /// place that could hold a camp, not through hostile camps.
    fn forward_sustainable_neutral(&self, origin: usize, first: usize) -> bool {
        let n = self.sites().locations();
        let mut visited = vec![false; n];
        visited[origin] = true;
        visited[first] = true;
        let mut queue = VecDeque::from([(first, 1)]);
        while let Some((at, hops)) = queue.pop_front() {
            if self.sites().owner(at).is_none() && self.sustain(at) {
                return true;
            }
            if hops >= 2 || self.hostile(at) {
                continue;
            }
            for nb in self.open_roads(at) {
                if nb < n && !self.hostile(nb) && !visited[nb] {
                    visited[nb] = true;
                    queue.push_back((nb, hops + 1));
                }
            }
        }
        false
    }

    /// `CorridorFrontierScore`.
    fn corridor_frontier_score(&self, l: usize) -> i32 {
        if !self.sustain(l) || self.distance_from_owned(l, 2).is_none() {
            return 0;
        }
        let forward = self
            .open_roads(l)
            .filter(|&n| self.sites().owner(n) != Some(self.player()) && !self.hostile(n))
            .filter(|&n| self.forward_sustainable_neutral(l, n))
            .count();
        match forward {
            0 => 0,
            1 => 180,
            _ => 240,
        }
    }

    /// `CountOwnedCamps`.
    fn count_owned(&self, player: usize) -> i32 {
        self.sites().sites.iter().filter(|s| s.owner == Some(player)).count() as i32
    }

    /// `TerritorialEliminationBonus`.
    fn elimination_bonus(&self, l: usize) -> i32 {
        let Some(opponent) = self.sites().owner(l).filter(|&o| o != self.player() && !self.dead_players.contains(&o)) else { return 0 };
        let remaining = self.count_owned(opponent);
        if !(1..=2).contains(&remaining) {
            return 0;
        }
        match (remaining, self.distance_from_owned(l, 2)) {
            (1, Some(1)) => 650,
            (1, Some(2)) => 400,
            (2, Some(1)) => 300,
            (2, Some(2)) => 150,
            _ => 0,
        }
    }

    /// `IsEnemyPocket`: the owner's camp cut off from the owner's others.
    fn enemy_pocket(&self, l: usize) -> bool {
        let Some(owner) = self.sites().owner(l).filter(|&o| o != self.player()) else { return false };
        let n = self.sites().locations();
        let mut visited = vec![false; n];
        visited[l] = true;
        let mut queue = VecDeque::from([l]);
        while let Some(at) = queue.pop_front() {
            for nb in self.open_roads(at) {
                if self.sites().owner(nb).is_some_and(|o| o != owner) || nb >= n || visited[nb] {
                    continue;
                }
                visited[nb] = true;
                if self.sites().owner(nb) == Some(owner) {
                    return false;
                }
                queue.push_back(nb);
            }
        }
        true
    }

    /// `ClaimedPocketScore`.
    fn claimed_pocket_score(&self, ai: &mut ModernAi, policy: &AiPolicy, l: usize) -> i32 {
        if !self.enemy_pocket(l) {
            return 0;
        }
        let mut score = 110;
        if attack::can_advance_plan(ai, self.f, l, policy) {
            score += 180;
        }
        if self.here() == l || self.travelling_to == Some(l) {
            score += 240;
        }
        score
    }

    /// `CurrentClaimScore`.
    fn current_claim_score(&self, ai: &ModernAi, l: usize) -> f32 {
        if self.sites().connects_owned_camps(l, self.player()) || ai.target.is_some_and(|t| t != l) {
            return 2300.0;
        }
        1050.0 + self.sites().territorial_value(self.items(), l) as f32 * 0.5 + self.resting_score(l) as f32 + self.unclaimed_pocket_score(l) as f32
    }

    /// `TargetScore`.
    fn target_score(&self, policy: &AiPolicy, target: Option<usize>) -> f32 {
        match target {
            None => 0.0,
            Some(t) if self.sites().owner(t).is_none() => self.neutral_target_score(policy, t),
            Some(_) => 1010.0,
        }
    }

    /// `IsAttackRecruitmentDeferred`.
    fn attack_recruitment_deferred(&self, ai: &mut ModernAi, recruit_here: bool) -> bool {
        let Some(a) = ai.attack_recruitment.clone() else { return false };
        let ready = group::size(self.r) as i32 > a.group_size || self.sites().is_city(self.here()) && recruit_here || self.f.w.day >= a.until_day;
        if ready {
            ai.attack_recruitment = None;
        }
        !ready
    }

    /// `SelectFirstSettlementTarget`.
    fn first_settlement_target(&self, ai: &mut ModernAi, policy: &AiPolicy) -> Option<usize> {
        if !self.neutral_expansion_allowed {
            ai.telemetry.report(|| "first-settlement plan blocked: neutral expansion is disabled".into());
            return None;
        }
        let mut candidates: Vec<(usize, Option<Route>)> = Vec::new();
        for l in 0..self.sites().sites.len() {
            if self.sites().is_city(l) || self.sites().owner(l).is_some() || self.occupied(l) {
                continue;
            }
            if self.deferred(ai, l) || !self.can_claim(ai, l) || !self.protected_for(l) || !self.sustain(l) {
                continue;
            }
            let r = if l == self.here() { None } else { self.route(self.here(), l) };
            if l == self.here() || r.as_ref().is_some_and(|r| self.settlement_route_supplies(ai, l, r)) {
                candidates.push((l, r));
            }
        }
        let preferred = candidates.iter().any(|(l, _)| self.preferred_first_settlement(*l));
        let mut best: Option<(usize, f32, i32)> = None;
        for (l, r) in candidates {
            if preferred && !self.preferred_first_settlement(l) {
                continue;
            }
            let days = r.map_or(0, |r| r.days);
            let score = self.neutral_target_score(policy, l) - (days * FIRST_CAMP_ROUTE_DAY_PENALTY) as f32;
            if best.is_none_or(|b| score > b.1 || score == b.1 && days < b.2) {
                best = Some((l, score, days));
            }
        }
        let target = best.map(|b| b.0);
        let line = match target {
            None => alloc::string::String::from("first-settlement plan found no reachable sustainable neutral camp"),
            Some(l) => alloc::format!(
                "first-settlement plan selected {}{}",
                super::telemetry::place(l),
                if self.preferred_first_settlement(l) { "" } else { " as a last-resort food camp" }
            ),
        };
        ai.telemetry.report(|| line);
        target
    }

    /// `Jitter`: −20 to 20.
    fn jitter(rng: &mut Rng) -> f32 {
        rng.range(-20, 21) as f32
    }

    /// `SelectTerritorialTarget`.
    fn territorial_target(&self, ai: &mut ModernAi, policy: &AiPolicy, recruit_here: bool, rng: &mut Rng) -> Option<usize> {
        if !self.has_owned_camp() {
            return self.first_settlement_target(ai, policy);
        }
        let mut targets: Vec<(usize, f32)> = Vec::new();
        let recruitment_deferred = self.attack_recruitment_deferred(ai, recruit_here);
        for l in 0..self.sites().sites.len() {
            if self.sites().is_city(l) || self.deferred(ai, l) {
                continue;
            }
            if l == self.here() {
                if self.sites().owner(l).is_none()
                    && self.neutral_expansion_allowed
                    && !self.occupied(l)
                    && self.can_claim(ai, l)
                    && self.protected_for(l)
                    && self.sustain(l)
                {
                    let score = self.neutral_target_score(policy, l) + self.neutral_frontier_score(ai, l) as f32 + self.unclaimed_pocket_score(l) as f32 + Self::jitter(rng);
                    targets.push((l, score));
                }
                continue;
            }
            let Some(r) = self.route(self.here(), l) else { continue };
            if self.sites().owner(l).is_none()
                && self.neutral_expansion_allowed
                && !self.occupied(l)
                && self.can_claim(ai, l)
                && self.protected_for(l)
                && self.sustainable_neutral_frontier(l)
                && (self.settlement_route_supplies(ai, l, &r) || self.can_wait_for_settlement_food(ai, l, &r))
            {
                let score = self.neutral_target_score(policy, l)
                    + self.neutral_frontier_score(ai, l) as f32
                    + self.corridor_frontier_score(l) as f32
                    + self.unclaimed_pocket_score(l) as f32
                    - (r.days * 24) as f32
                    + Self::jitter(rng);
                targets.push((l, score));
            } else if !recruitment_deferred
                && attack::has_group_weapon(self.f)
                && self.hostile(l)
                && attack::is_territorial_frontier_target(self.f, l)
                && attack::is_target_allowed(ai, self.f, l, policy)
            {
                let defence = attack::estimate(ai, self.f, l);
                let weakness = (100.0 - defence.estimated_strength).max(-100.0);
                let strategic = if super::reinforcement::is_strategic_location(self.items(), &self.sites().sites[l]) || ai.was_recently_contested(l, self.f.w.day) {
                    policy.strategic_hostile_target_bonus
                } else {
                    0.0
                };
                let elimination = self.elimination_bonus(l) as f32;
                let pocket = self.claimed_pocket_score(ai, policy, l) as f32;
                let proactive = if self.f.w.day >= policy.proactive_conflict_day { policy.proactive_conflict_bonus } else { 0.0 };
                let finishing = if defence.based_on_contact && defence.expected_defenders == 1 { 700.0 } else { 0.0 };
                let score = policy.hostile_target_score + weakness + strategic + elimination + pocket + proactive + finishing - (r.days * 6) as f32 + Self::jitter(rng);
                targets.push((l, score));
            }
        }
        let mut best: Option<(usize, f32)> = None;
        for (l, s) in targets {
            if best.is_none_or(|b| s > b.1) {
                best = Some((l, s));
            }
        }
        best.map(|b| b.0)
    }

    /// `ExpansionPlanning.ValidatePersistentTarget`.
    fn validate_persistent_target(&self, ai: &mut ModernAi, policy: &AiPolicy) -> Option<usize> {
        let target = ai.target.filter(|&t| !self.sites().is_city(t))?;
        let neutral = self.sites().owner(target).is_none();
        let t = super::telemetry::place(target);
        let settlement = ai.target_was_neutral;
        if target == self.here() {
            let claimable = neutral && self.can_claim(ai, target);
            if claimable && !self.can_station_camp(ai) {
                return Some(target);
            }
            if !claimable && neutral && settlement {
                ai.telemetry.report(|| alloc::format!("abandoned settlement at {t}: it can no longer be bootstrapped"));
            }
            attack::clear_target(ai);
            return None;
        }
        let Some(r) = self.route(self.here(), target) else {
            ai.telemetry.report(|| alloc::format!("abandoned target {t}: no permitted route remains"));
            attack::clear_target(ai);
            return None;
        };
        if neutral {
            if self.occupied(target) {
                ai.telemetry.report(|| alloc::format!("released settlement target {t}: another player is present"));
                attack::clear_target(ai);
                return None;
            }
            if !self.sustainable_neutral_frontier(target) {
                ai.telemetry.report(|| alloc::format!("abandoned settlement target {t}: an enemy takeover made its frontier route too remote"));
                attack::clear_target(ai);
                return None;
            }
            let claimable = self.can_claim(ai, target);
            if self.neutral_expansion_allowed
                && claimable
                && self.protected_for(target)
                && (self.settlement_route_supplies(ai, target, &r) || self.can_wait_for_settlement_food(ai, target, &r))
            {
                return Some(target);
            }
            if settlement {
                let why = if !claimable { "it can no longer be bootstrapped" } else { "its route is no longer supportable" };
                ai.telemetry.report(|| alloc::format!("abandoned settlement target {t}: {why}"));
            }
            attack::clear_target(ai);
            return None;
        }
        if self.sites().owner(target) == Some(self.player()) {
            attack::clear_target(ai);
            return None;
        }
        if settlement {
            ai.telemetry.report(|| alloc::format!("released settlement target {t}: another player claimed it first"));
            attack::clear_target(ai);
            return None;
        }
        attack::validate_persistent_target(ai, self.f, target, &r, policy)
    }

    /// `CreatePlan`. `recruit_here` is `Recruitment.HasSafeLocalRecruit`.
    pub fn create_plan(&self, ai: &mut ModernAi, policy: &AiPolicy, recruit_here: bool, rng: &mut Rng, prepare_reststop: &mut dyn FnMut() -> bool) -> Plan {
        let here = self.here();
        let suitable = self.suitable_current_claim(ai);
        let can_become = self.can_claim(ai, here) && suitable && self.protected_for(here);
        let can_claim_current = can_become && self.can_station_camp(ai);
        let mut target = self.validate_persistent_target(ai, policy);
        if target.is_none() && !(can_claim_current && ai.target.is_none()) {
            target = self.territorial_target(ai, policy, recruit_here, rng);
            match target {
                Some(t) if self.hostile(t) => {
                    // Both computed, as upstream does. Neither changes
                    // anything (`CanAdvancePlan` and what it asks only
                    // read), so the order is upstream's and not a decision.
                    let elimination = self.elimination_bonus(t);
                    let pocket = self.claimed_pocket_score(ai, policy, t);
                    let owner = self.sites().owner(t).unwrap_or(0);
                    if elimination > 0 {
                        let n = self.count_owned(owner);
                        ai.telemetry.report(|| {
                            alloc::format!("prioritized territorial elimination of {}: {n} nearby camp{} remain", super::telemetry::player(owner), if n == 1 { "" } else { "s" })
                        });
                    } else if pocket > 0 {
                        ai.telemetry.report(|| alloc::format!("prioritized isolated pocket at {} held by {}", super::telemetry::place(t), super::telemetry::player(owner)));
                    }
                    attack::start_attack_plan(ai, self.f, t, policy)
                }
                _ => {
                    if !attack::set_settlement_target(ai, target, prepare_reststop) {
                        target = None;
                    }
                }
            }
        }
        Plan {
            can_claim_current,
            current_claim_score: self.current_claim_score(ai, here),
            target,
            target_score: self.target_score(policy, target),
            preparing_attack: target.is_some_and(|t| self.hostile(t)),
        }
    }

    /// `CancelSettlementAtHostileWaypoint`.
    pub fn cancel_settlement_at_hostile_waypoint(&self, ai: &mut ModernAi) {
        if let Some(t) = ai.target.filter(|_| ai.target_was_neutral && self.hostile(self.here())) {
            report_cancelled_settlement(ai, t, self.here());
            attack::clear_target(ai);
        }
    }

    /// `AddImmediateClaimCandidate`.
    pub fn add_immediate_claim(&self, plan: &Plan, out: &mut Vec<Decision>) {
        if !plan.preparing_attack && plan.can_claim_current {
            let here = self.here();
            let reason = if self.sites().connects_owned_camps(here, self.player()) {
                Reason::ClaimConnecting
            } else {
                Reason::ClaimCurrent { role: claims::strategic_role(self.items(), &self.sites().sites[here]) }
            };
            out.push(Decision::new(Action::ClaimNeutral, plan.current_claim_score, Some(here), reason));
        }
    }

    /// `ExpansionPlanning.AddCandidates`.
    pub fn add_candidates(&self, ai: &mut ModernAi, policy: &AiPolicy, plan: &Plan, out: &mut Vec<Decision>) {
        if ai.post_capture_pause > 0 && !self.critical_supplies && !plan.preparing_attack {
            ai.post_capture_pause -= 1;
            out.push(Decision::new(Action::Wait, 900.0, None, Reason::PostCapturePause));
        }
        let Some(target) = plan.target else { return };
        if !plan.preparing_attack {
            if target == self.here() || group::size(self.r) <= 1 {
                return;
            }
            let Some(r) = self.route(self.here(), target) else { return };
            if !self.settlement_route_supplies(ai, target, &r) {
                out.push(Decision::new(Action::Wait, 10.0, Some(target), Reason::SettlementWaitingReserves { target }));
                return;
            }
            let mut d = Decision::new(Action::Travel, plan.target_score - r.days as f32, Some(target), Reason::AdvanceToward {
                role: claims::strategic_role(self.items(), &self.sites().sites[target]),
            });
            d.next_step = Some(r.next_step);
            out.push(d);
        } else if attack::can_advance_plan(ai, self.f, target, policy) {
            if let Some(r) = self.route(self.here(), target) {
                let mut d = Decision::new(Action::Travel, 1010.0 - r.days as f32, Some(target), Reason::AttackPlanReady { target });
                d.next_step = Some(r.next_step);
                out.push(d);
            }
        }
    }

    /// `NeedsExpansionTool`: free places exist and nothing the group carries
    /// or the reserve holds works any of them.
    pub fn needs_expansion_tool(&self, ai: &ModernAi) -> bool {
        let free: Vec<usize> = (0..self.sites().sites.len()).filter(|&l| self.neutral(l)).collect();
        if free.is_empty() {
            return false;
        }
        let t = self.items().types;
        let carried = group::carried(self.r).iter().any(|&(_, _, k)| {
            t.name_of(k).and_then(|n| self.f.w.productions.worked_by(n)).is_some_and(|p| free.iter().any(|&l| self.sites().sites[l].productions.iter().any(|sp| sp.id == p)))
        });
        let pooled = free.iter().any(|&l| claims::reserve_has_trap(ai, self.f.w, &claims::available_products(self.items(), &self.sites().sites[l])));
        !carried && !pooled
    }
}

/// `CancelSettlementAtHostileWaypoint`'s line: the target itself taken, or
/// a waypoint on the way.
pub fn report_cancelled_settlement(ai: &ModernAi, target: usize, here: usize) {
    ai.telemetry.report(|| {
        if target == here {
            alloc::format!("released settlement target {}: another player claimed it before arrival", super::telemetry::place(target))
        } else {
            alloc::format!("abandoned settlement target {}: waypoint {} became hostile", super::telemetry::place(target), super::telemetry::place(here))
        }
    });
}
