//! When the opponent hires: `Recruitment.AddCandidates` and the searches
//! it makes — a settler here, on the road, at the destination or in a safe
//! city; a guard for a camp; a follower for the group; the attack's
//! recruits — and the hire itself, checked again when it is made. At tag
//! `v1.2`, `AI/PlayerBehavior/Expansion/Recruitment.cs` and
//! `AiDecision.Execute`'s `Recruit`.
//!
//! The attack's size comes from attack planning
//! (`AttackPlanning.RequiredAttackGroupSize`), as an argument until that
//! part is built.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::Cell;

use crate::combat::Kit;
use crate::game::ItemBag;
use crate::rival::Rival;
use crate::spawn::Rng;

use super::claims;
use super::decision::{add_travel, Action, Decision, Risk, Traveller};
use super::group::{self, CITY_MINIMUM};
use super::needs::{Needs, World};
use super::reason::Reason;
use super::recruitment::{self, Candidate, Plan, Purse};
use super::reinforcement::{self, RecruitmentNeeds};
use super::route::{self, Route, RouteMap};
use super::state::ModernAi;
use super::supplies::{self, GroupSupplies};
use super::trading::{self, Barter, Context};

/// `StandingGroupFoodSurplus`.
pub const STANDING_GROUP_FOOD_SURPLUS: i32 = 4;
/// `InitialRecruitReserve`: the food and water a recruit is reckoned to bring.
pub const INITIAL_RECRUIT_RESERVE: i32 = 5;

/// Everything a hiring decision reads.
pub struct Planner<'a> {
    pub w: &'a World<'a>,
    pub needs: &'a Needs,
    pub r: &'a Rival,
    pub kit: &'a Kit,
    pub ai: &'a ModernAi,
    pub barter: Barter,
    pub previous: Option<usize>,
    /// The people at each place, by location.
    pub people: &'a [Vec<Candidate>],
    /// The rooms of the opponent's camps.
    pub camp_rooms: &'a [Vec<ItemBag>],
    /// Where the other living leaders stand (not on the road).
    pub rivals_at: &'a [usize],
    pub group: GroupSupplies,
    /// `AttackRecruitmentCity` as the decisions leave it, for the state.
    pub attack_recruitment_city: Cell<Option<usize>>,
}

impl<'a> Planner<'a> {
    pub fn new(
        w: &'a World<'a>,
        needs: &'a Needs,
        r: &'a Rival,
        kit: &'a Kit,
        ai: &'a ModernAi,
        barter: Barter,
        previous: Option<usize>,
        people: &'a [Vec<Candidate>],
        camp_rooms: &'a [Vec<ItemBag>],
        rivals_at: &'a [usize],
    ) -> Planner<'a> {
        Planner {
            w,
            needs,
            r,
            kit,
            ai,
            barter,
            previous,
            people,
            camp_rooms,
            rivals_at,
            group: group::supplies(r, w.items.types),
            attack_recruitment_city: Cell::new(ai.attack_recruitment_city),
        }
    }

    fn here(&self) -> usize {
        self.w.here
    }

    fn sites(&self) -> &'a super::camps::Sites {
        self.w.sites
    }

    fn size(&self) -> usize {
        group::size(self.r)
    }

    fn owned(&self) -> usize {
        self.w.owned_camps()
    }

    fn is_city(&self, l: usize) -> bool {
        self.sites().is_city(l)
    }

    fn neutral(&self, l: usize) -> bool {
        !self.is_city(l) && self.sites().owner(l).is_none()
    }

    fn route(&self, from: usize, to: usize) -> Option<Route> {
        route::find(self.sites(), self.w.player, self.previous, from, to)
    }

    fn purse(&self) -> Purse<'_> {
        Purse { items: self.w.items, r: self.r, leader_kit: self.kit, camp_rooms: self.camp_rooms, owned_camps: self.owned() }
    }

    fn people_at(&self, l: usize) -> &[Candidate] {
        self.people.get(l).map_or(&[][..], |p| p.as_slice())
    }

    /// `FindRecruitAt`.
    pub fn find_recruit_at(&self, l: usize, affordable: bool, generated: bool) -> Option<Plan> {
        recruitment::find_recruit_at(&self.purse(), self.w.policy, self.people_at(l), &self.ai.demobilized, affordable, generated)
    }

    /// `CanRecruit` here.
    pub fn can_recruit(&self, generated: bool) -> bool {
        recruitment::can_recruit(&self.purse(), self.w.policy, self.people_at(self.here()), &self.ai.demobilized, generated)
    }

    /// `CanRecruitAtCurrentLocation`.
    fn may_recruit_here(&self) -> bool {
        !self.ai.survival_releases.contains(&self.here())
    }

    /// `RecruitmentSupplyCost`.
    fn cost(plan: &Option<Plan>) -> Option<(i32, i32)> {
        plan.as_ref().filter(|p| p.funded).map(|p| (p.food, p.water))
    }

    fn traveller(&self) -> Traveller<'_> {
        Traveller { sites: self.sites(), items: self.w.items, player: self.w.player, previous: self.previous, here: self.here(), group: &self.group }
    }

    fn context(&self) -> Context<'_> {
        Context { w: self.w, needs: self.needs, r: self.r, leader_kit: self.kit, ai: self.ai, barter: self.barter }
    }

    fn candidate(&self, l: usize, record: usize) -> Option<&Candidate> {
        self.people_at(l).iter().find(|c| c.record == record)
    }

    pub fn has_settlement_plan(&self) -> bool {
        self.ai.target_was_neutral && self.ai.target.is_some()
    }

    /// `EmpireFoodSurplus`.
    fn empire_food_surplus(&self) -> i32 {
        self.sites().sites.iter().filter(|s| s.owner == Some(self.w.player)).map(|s| s.food_surplus(self.w.items)).sum()
    }

    /// `FindProvisionedReturnRoute`: the nearest own camp that can feed and
    /// water the group.
    fn provisioned_return(&self, start: usize) -> Option<Route> {
        let items = self.w.items;
        let size = self.size() as i32;
        (0..self.sites().sites.len())
            .filter(|&l| {
                let s = &self.sites().sites[l];
                s.owner == Some(self.w.player) && s.can_provision_food(items) && s.can_provision_group_water(items, size)
            })
            .filter_map(|l| self.route(start, l))
            .min_by_key(|r| r.days)
    }

    /// `CanRemainAtNewCamp`: water for two, and the tools the group, the
    /// recruit and the reserve bring (not the payment) feed both.
    fn can_remain_at_new_camp(&self, target: usize, recruit: Option<&Candidate>) -> bool {
        let Some(recruit) = recruit else { return false };
        let site = &self.sites().sites[target];
        if site.city || site.water < 2 {
            return false;
        }
        let t = self.w.items.types;
        let exact = recruit.hire_items.iter().find_map(|&k| (0..self.r.pack.len()).find(|&s| self.r.pack.get(s) == Some(k)));
        let production_of = |k: usize| t.name_of(k).and_then(|n| self.w.productions.worked_by(n));
        site.productions.iter().any(|p| {
            let carried = group::carried(self.r).iter().filter(|&&(m, s, k)| !(m == 0 && Some(s) == exact) && production_of(k) == Some(p.id)).count();
            let brought = recruit.pack.iter().filter(|&k| production_of(k) == Some(p.id)).count();
            let reserved: usize = self.ai.reserve.iter().filter(|&&(k, _)| production_of(k) == Some(p.id)).map(|&(_, c)| c as usize).sum();
            p.production.rate(carried + brought + reserved, 1, t.food_of(p.produce)).food_per_day >= 2
        })
    }

    /// `PlannedFutureSettlementPaymentType`: what a lone leader with a
    /// settlement plan keeps back to pay the settler at the target — what
    /// he carries first, then the cheapest.
    pub fn planned_settlement_payment(&self) -> Option<usize> {
        if self.owned() == 0 && self.size() == 1 {
            return None;
        }
        let target = self.ai.target.filter(|_| self.has_settlement_plan() && self.size() == 1)?;
        if !self.neutral(target) {
            return None;
        }
        let plan = self.find_recruit_at(target, true, false).or_else(|| self.find_recruit_at(target, false, false))?;
        let recruit = self.candidate(target, plan.record)?;
        let items = self.w.items;
        let mut asks = recruit.hire_items.clone();
        asks.sort_by(|&a, &b| {
            let carried = |k: usize| if self.r.pack.iter().any(|x| x == k) { 0 } else { 1 };
            carried(a).cmp(&carried(b)).then(items.value(a).partial_cmp(&items.value(b)).unwrap_or(core::cmp::Ordering::Equal))
        });
        asks.first().copied()
    }

    /// `CanRecallFollower`: standing in an own camp with more guards than it
    /// must keep.
    pub fn can_recall_follower(&self, garrison: i32) -> bool {
        let Some(site) = self.sites().get(self.here()).filter(|s| s.owner == Some(self.w.player)) else { return false };
        let minimum = if reinforcement::is_critical_camp(self.w.items, site) { reinforcement::sustainable_garrison_target(site, garrison) } else { 1 };
        site.living_guards(self.w.player) > minimum
    }

    /// `FindSafeLocalRecruit`: an affordable recruit here, unless hiring
    /// would leave somebody critical with no camp near to recover at.
    pub fn safe_local_recruit(&self) -> Option<Plan> {
        let generated = self.is_city(self.here()) && self.w.policy.allow_generated_recruit_payment_in_cities;
        let plan = self.find_recruit_at(self.here(), true, generated)?;
        let (food, water) = Self::cost(&Some(plan.clone()))?;
        let recruit = self.candidate(self.here(), plan.record)?;
        let (rf, rw) = (INITIAL_RECRUIT_RESERVE, INITIAL_RECRUIT_RESERVE);
        let critical = (0..self.size()).any(|m| {
            let n = group::needs(self.r, m);
            n.health < 40 || n.food <= 3 || n.water <= 2
        }) || recruit.health < 40
            || rf <= 3
            || rw <= 2;
        if !critical {
            return Some(plan);
        }
        let size = self.size() as i32 + 1;
        let items = self.w.items;
        let rich = |s: &super::camps::Site| s.owner == Some(self.w.player) && s.food_surplus(items) > size && s.water_surplus() > size;
        if self.sites().get(self.here()).is_some_and(rich) {
            return Some(plan);
        }
        let recovery = (0..self.sites().sites.len())
            .filter(|&l| rich(&self.sites().sites[l]))
            .filter_map(|l| self.route(self.here(), l))
            .any(|r| supplies::has_projected_recruit_route_supplies(&self.group, &r, rf, rw, food, water));
        recovery.then_some(plan)
    }

    /// `FindNearestRecruitmentCity`: another city with an affordable recruit
    /// the whole group can go to and come back from; the preferred one
    /// first, then the nearest.
    pub fn nearest_recruitment_city(&self, preferred: Option<usize>) -> Option<usize> {
        let generated = self.w.policy.allow_generated_recruit_payment_in_cities;
        let minimum = if self.owned() > 0 { CITY_MINIMUM } else { 0 };
        let mut found: Vec<(usize, i32)> = Vec::new();
        for city in 0..self.sites().sites.len() {
            if !self.is_city(city) || city == self.here() {
                continue;
            }
            let plan = self.find_recruit_at(city, true, generated);
            let Some((food, water)) = Self::cost(&plan) else { continue };
            let (Some(to), Some(back)) = (self.route(self.here(), city), self.provisioned_return(city)) else { continue };
            if supplies::has_staged_recruit_city_return_supplies(&self.group, &to, &back, minimum, INITIAL_RECRUIT_RESERVE, INITIAL_RECRUIT_RESERVE, food, water) {
                found.push((city, to.days));
            }
        }
        found.sort_by_key(|&(c, days)| (Some(c) != preferred, days));
        found.first().map(|f| f.0)
    }

    /// `FindAttackRecruitmentCity`: kept across stops, checked again.
    fn attack_recruitment_city(&self) -> Option<usize> {
        let city = self.nearest_recruitment_city(self.attack_recruitment_city.get());
        self.attack_recruitment_city.set(city);
        city
    }

    /// `FindSafeSettlementRecruitmentCity`: a city, other than `from`, where
    /// a lone leader can hire a settler and both reach `target`, with the
    /// returns the plan needs; the shortest way through.
    pub fn safe_settlement_recruitment_city(&self, from: usize, target: usize) -> Option<usize> {
        let owned = self.owned() > 0;
        let minimum = if owned { CITY_MINIMUM } else { 0 };
        let generated = self.w.policy.allow_generated_recruit_payment_in_cities;
        let (rf, rw) = (INITIAL_RECRUIT_RESERVE, INITIAL_RECRUIT_RESERVE);
        let mut best: Option<(usize, i32)> = None;
        for city in 0..self.sites().sites.len() {
            if !self.is_city(city) || city == from {
                continue;
            }
            let plan = self.find_recruit_at(city, true, generated);
            let Some((food, water)) = Self::cost(&plan) else { continue };
            let recruit = plan.as_ref().and_then(|p| self.candidate(city, p.record));
            let (Some(to), Some(onward)) = (self.route(from, city), self.route(city, target)) else { continue };
            let back = if owned { self.provisioned_return(city) } else { self.route(city, from) };
            let settlement_back = if owned {
                self.provisioned_return(target)
            } else if self.can_remain_at_new_camp(target, recruit) {
                Some(Route { next_step: target, days: 0, path: Vec::new(), effective_cost: 0 })
            } else {
                self.route(target, city)
            };
            let (Some(back), Some(settlement_back)) = (back, settlement_back) else { continue };
            let g = &self.group;
            if supplies::has_staged_recruit_settlement_supplies(g, &to, &onward, &settlement_back, minimum, rf, rw, food, water)
                && supplies::has_staging_city_return_supplies(g, &to, &back, minimum, food, water)
                && supplies::has_staged_recruit_city_return_supplies(g, &to, &back, minimum, rf, rw, food, water)
            {
                let days = to.days + onward.days;
                if best.is_none_or(|b| days < b.1) {
                    best = Some((city, days));
                }
            }
        }
        best.map(|b| b.0)
    }

    /// `FindFirstCampWaypoint`: a neighbouring free place a lone leader can
    /// claim, protected, sustainable and supplied to reach — with a settler
    /// there who can stay, or a safe city for one — the one with a local
    /// settler first, then the most worth holding, then the nearest.
    pub fn first_camp_waypoint(&self) -> Option<usize> {
        let here = self.here();
        let items = self.w.items;
        let mut best: Option<(usize, bool, i32, i32)> = None;
        for &(waypoint, d) in &self.sites().sites[here].roads {
            if d <= 0 || !self.neutral(waypoint) || claims::is_occupied_by_other_boss(self.rivals_at, waypoint) {
                continue;
            }
            let site = &self.sites().sites[waypoint];
            if !(site.acceptable_first_camp(items) || claims::can_found_with_resting_sustenance(self.r, self.w, waypoint))
                || !claims::can_claim(self.r, self.ai, self.w, &self.group, waypoint)
                || !claims::has_travelling_hazard_protection(self.r, items, site)
                || !claims::can_sustain_candidate(self.r, self.w, waypoint)
            {
                continue;
            }
            let Some(r) = self.route(here, waypoint).filter(|r| r.days > 0) else { continue };
            if !supplies::has_route_supplies(&self.group, &r, false, 0, 0) {
                continue;
            }
            let local = self.find_recruit_at(waypoint, true, false);
            let local_ok = Self::cost(&local).is_some() && self.can_remain_at_new_camp(waypoint, local.as_ref().and_then(|p| self.candidate(waypoint, p.record)));
            if !local_ok && self.safe_settlement_recruitment_city(waypoint, waypoint).is_none() {
                continue;
            }
            let key = (local.is_some(), self.sites().territorial_value(items, waypoint), r.days);
            let better = best.is_none_or(|b| (key.0, key.1, -key.2) > (b.1, b.2, -b.3));
            if better {
                best = Some((waypoint, key.0, key.1, key.2));
            }
        }
        best.map(|b| b.0)
    }

    /// `FindBestOwnedSettlementStagingCamp`: an own camp that can provision
    /// the group and a settler, reachable on supplies, nearest the target.
    fn settlement_staging_camp(&self, target: usize) -> Option<usize> {
        let items = self.w.items;
        let size = self.size() as i32 + 1;
        let mut best: Option<(usize, i32, i32)> = None;
        for l in 0..self.sites().sites.len() {
            let s = &self.sites().sites[l];
            if s.owner != Some(self.w.player) || l == self.here() || !s.can_provision_food(items) || !s.can_provision_group_water(items, size) {
                continue;
            }
            let (Some(to), Some(onward)) = (self.route(self.here(), l), self.route(l, target)) else { continue };
            if !supplies::has_territorial_route_supplies(&self.group, self.sites(), items, self.w.player, self.previous, self.here(), &to, false) {
                continue;
            }
            if best.is_none_or(|b| (onward.days, to.days) < (b.1, b.2)) {
                best = Some((l, onward.days, to.days));
            }
        }
        best.map(|b| b.0)
    }

    /// `AddDestinationRecruitCandidate`: a settler at the target, paid with
    /// what the leader carries, reached and returned from on supplies.
    fn destination_recruit(&self, target: usize, out: &mut Vec<Decision>) -> bool {
        let plan = self.find_recruit_at(target, true, false);
        let Some((food, water)) = Self::cost(&plan) else { return false };
        let outbound = self.route(self.here(), target);
        let back = if self.owned() > 0 { self.provisioned_return(target) } else { self.route(target, self.here()) };
        let (Some(outbound), Some(back)) = (outbound, back) else { return false };
        if !supplies::has_route_supplies(&self.group, &outbound, false, food, water) {
            return false;
        }
        let round_trip = supplies::has_settlement_round_trip_supplies(&self.group, &outbound, &back, food, water);
        let refill = self.sites().sites[target].can_provision_group_water(self.w.items, self.size() as i32);
        if !round_trip && !(refill && supplies::has_settlement_round_trip_food(&self.group, &outbound, &back)) {
            return false;
        }
        let reason = if self.owned() == 0 && self.size() == 1 {
            Reason::PickUpFreeFirstSettler { recruit: plan_record(&plan), target }
        } else {
            Reason::HireWithReservedItem { recruit: plan_record(&plan), target }
        };
        let mut d = Decision::new(Action::Travel, 2110.0 - outbound.days as f32, Some(target), reason);
        d.next_step = Some(outbound.next_step);
        out.push(d);
        true
    }

    /// `AddRouteRecruitCandidate`: a settler at a stop on a shortest road
    /// to the target, the stop nearest the target first.
    fn route_recruit(&self, target: usize, out: &mut Vec<Decision>) -> bool {
        let (Some(direct), Some(solo_back)) = (self.route(self.here(), target), self.provisioned_return(target)) else { return false };
        let items = self.w.items;
        let mut best: Option<(usize, Route, i32, usize)> = None;
        for l in 0..self.sites().sites.len() {
            if l == self.here() || l == target {
                continue;
            }
            let generated = self.is_city(l) && self.w.policy.allow_generated_recruit_payment_in_cities;
            let plan = self.find_recruit_at(l, true, generated);
            let Some((food, water)) = Self::cost(&plan) else { continue };
            let site = &self.sites().sites[l];
            if !(site.city || site.owner == Some(self.w.player) || site.can_sustain_camp(items)) {
                continue;
            }
            let (Some(to), Some(onward)) = (self.route(self.here(), l), self.route(l, target)) else { continue };
            if to.days + onward.days != direct.days {
                continue;
            }
            let recruit = plan.as_ref().and_then(|p| self.candidate(l, p.record));
            let solo_days = if self.can_remain_at_new_camp(target, recruit) { 0 } else { solo_back.days };
            let minimum = if site.city { CITY_MINIMUM } else { 0 };
            if !supplies::has_staged_recruit_outbound_supplies(&self.group, &to, &onward, minimum, INITIAL_RECRUIT_RESERVE, INITIAL_RECRUIT_RESERVE, food, water, solo_days) {
                continue;
            }
            if best.as_ref().is_none_or(|b| (onward.days, to.days) < (b.2, b.1.days)) {
                best = Some((l, to, onward.days, plan_record(&plan)));
            }
        }
        let Some((stop, to, _, recruit)) = best else { return false };
        let mut d = Decision::new(Action::Travel, 2108.0 - to.days as f32, Some(stop), Reason::PickUpAtRouteStop { recruit, stop, target });
        d.next_step = Some(to.next_step);
        out.push(d);
        true
    }

    /// `TryAddCriticalRouteRecruitContinuation`: with supplies critical and a
    /// settlement plan, a lone leader goes on to a stop on a shortest road
    /// where a settler can be hired, if he survives the way there.
    pub fn critical_route_recruit_continuation(&self, out: &mut Vec<Decision>) -> bool {
        let Some(target) = self.ai.target else { return false };
        if !self.has_settlement_plan() || !self.neutral(target) || self.size() != 1 {
            return false;
        }
        let Some(direct) = self.route(self.here(), target) else { return false };
        let items = self.w.items;
        let mut best: Option<(usize, Route, i32, usize)> = None;
        for l in 0..self.sites().sites.len() {
            if l == self.here() || l == target {
                continue;
            }
            let s = &self.sites().sites[l];
            if !(s.city || s.owner == Some(self.w.player) || s.can_sustain_camp(items)) {
                continue;
            }
            let generated = s.city && self.w.policy.allow_generated_recruit_payment_in_cities;
            let Some(plan) = self.find_recruit_at(l, true, generated) else { continue };
            let (Some(to), Some(onward)) = (self.route(self.here(), l), self.route(l, target)) else { continue };
            if to.days + onward.days != direct.days || !supplies::can_survive_recovery_route(&self.group, &to) {
                continue;
            }
            if best.as_ref().is_none_or(|b| (onward.days, to.days) < (b.2, b.1.days)) {
                best = Some((l, to, onward.days, plan.record));
            }
        }
        let Some((stop, to, _, recruit)) = best else { return false };
        let mut d = Decision::new(Action::Travel, 1115.0 - to.days as f32, Some(stop), Reason::SoloApproachToRouteRecruit { recruit, stop });
        d.next_step = Some(to.next_step);
        out.push(d);
        true
    }

    fn travel(&self, out: &mut Vec<Decision>, target: Option<usize>, score: f32, reason: Reason) {
        add_travel(&self.traveller(), out, target, score, reason, Risk::default(), None, false);
    }

    /// `AddRecruitOrCityCandidate`.
    fn recruit_or_city(&self, out: &mut Vec<Decision>, recruit_score: f32, travel_score: f32, reason: Reason) {
        let generated = self.is_city(self.here()) && self.w.policy.allow_generated_recruit_payment_in_cities;
        if self.may_recruit_here() && self.can_recruit(generated) {
            if let Some(plan) = self.safe_local_recruit() {
                let mut d = Decision::new(Action::Recruit, recruit_score, Some(self.here()), reason);
                d.recruit = Some(plan.record);
                out.push(d);
                return;
            }
        }
        let city = self.attack_recruitment_city();
        let prep = if city.is_none() { trading::best_camp_for_city_preparation(&self.context(), self.previous) } else { None };
        let why = if prep.is_none() { reason } else { Reason::CollectRecruitmentSupplies(Box::new(reason)) };
        self.travel(out, prep.or(city), travel_score, why);
    }

    /// `AddFailedCityAttackRecruitmentCancellation`.
    fn failed_city_attack_recruitment(&self, target: usize, out: &mut Vec<Decision>) -> bool {
        if !self.is_city(self.here()) {
            return false;
        }
        let generated = self.w.policy.allow_generated_recruit_payment_in_cities;
        if self.can_recruit(generated) && self.safe_local_recruit().is_some() {
            return false;
        }
        if let Some(city) = self.attack_recruitment_city() {
            self.travel(out, Some(city), 1070.0, Reason::ContinueAttackRecruitment { city, here: self.here() });
            return true;
        }
        out.push(Decision::new(Action::CancelAttackPlan, 1200.0, Some(target), Reason::CancelAttackNoRecruit { target, here: self.here() }));
        true
    }

    /// `AddAttackPreparationCandidates`.
    fn attack_preparation(&self, target: usize, attack_size: i32, out: &mut Vec<Decision>) -> RecruitmentNeeds {
        let policy = self.w.policy;
        let travel_size = policy.group_size;
        let size = self.size() as i32;
        if size < travel_size {
            if self.can_recall_follower(policy.critical_garrison_target) {
                out.push(Decision::new(Action::RecallFollower, 1060.0, Some(self.here()), Reason::RecallBeforeAttack { target }));
            }
            if self.failed_city_attack_recruitment(target, out) {
                return RecruitmentNeeds::default();
            }
            self.recruit_or_city(out, 1040.0, 1030.0, Reason::RestoreTravelGroup { target });
            return RecruitmentNeeds::default();
        }
        if attack_size <= travel_size || size >= attack_size {
            return RecruitmentNeeds::default();
        }
        let staging_target = attack_size - travel_size + 1;
        let staging = reinforcement::attack_staging_camp(self.w.items, self.sites(), self.w.player, self.previous, self.here(), target, staging_target);
        let staged = staging.map_or(0, |c| self.sites().sites[c].living_guards(self.w.player));
        if staging == Some(self.here()) && staged > 1 {
            out.push(Decision::new(Action::MobilizeFrontierFollower, 1100.0, staging, Reason::MobilizeFrontierGuard { target }));
            return RecruitmentNeeds::default();
        }
        if staging.is_some() && staged >= staging_target {
            self.travel(out, staging, 1090.0, Reason::AssembleAtFrontier { target, camp: staging.unwrap_or(target) });
            return RecruitmentNeeds::default();
        }
        if self.can_recall_follower(policy.critical_garrison_target) {
            out.push(Decision::new(Action::RecallFollower, 1085.0, Some(self.here()), Reason::RecallToAssemble { size: attack_size, target }));
            return RecruitmentNeeds::default();
        }
        let before = out.len();
        if self.failed_city_attack_recruitment(target, out) {
            return RecruitmentNeeds::default();
        }
        self.recruit_or_city(out, 1080.0, 1070.0, Reason::RecruitAttackers { size: attack_size, target });
        if out.len() > before {
            return RecruitmentNeeds::default();
        }
        if staging.is_none() {
            out.push(Decision::new(Action::CancelAttackPlan, 1200.0, Some(target), Reason::DeferAttackNoAssembly { target, size: attack_size }));
            return RecruitmentNeeds::default();
        }
        RecruitmentNeeds { camp: staging, target: staging_target, attack_staging: true }
    }

    /// `Recruitment.AddCandidates`. `preparing_attack` with the size the
    /// attack wants; `visit_trader` is `Trading.ShouldVisitTrader`.
    pub fn add_candidates(&self, target: Option<usize>, attack: Option<i32>, visit_trader: bool, out: &mut Vec<Decision>) -> RecruitmentNeeds {
        if let (Some(t), Some(size)) = (target, attack) {
            return self.attack_preparation(t, size, out);
        }
        let policy = self.w.policy;
        let here = self.here();
        let items = self.w.items;
        let generated = self.is_city(here) && policy.allow_generated_recruit_payment_in_cities;
        let size = self.size() as i32;
        let desired_group = policy.group_size;
        if target.is_none() && self.owned() == 0 && size == 1 {
            let waypoint = self.first_camp_waypoint();
            self.travel(out, waypoint, 2095.0, Reason::AdvanceToFirstCampWaypoint { waypoint });
        }
        let settlement = target.filter(|&t| self.neutral(t));
        let committed_settler = settlement.is_some() && size > 1;
        let camp = if committed_settler {
            None
        } else {
            reinforcement::best_camp_for_reinforcement(items, self.sites(), self.w.player, policy, self.previous, here, policy.critical_garrison_target)
        };
        let camp_target = camp.map_or(0, |c| reinforcement::sustainable_garrison_target(&self.sites().sites[c], policy.critical_garrison_target));
        let needs_settler = settlement.is_some() && size == 1;
        let needs_guard = camp.is_some() && size == 1;
        let standing = self.empire_food_surplus() >= STANDING_GROUP_FOOD_SURPLUS;
        let needs_follower = size < desired_group && (needs_settler || needs_guard || standing);

        if !visit_trader && needs_follower && self.can_recall_follower(policy.critical_garrison_target) {
            let score = if needs_settler { 2130.0 } else { 1010.0 };
            let reason = match (settlement, camp) {
                (Some(t), _) if needs_settler => Reason::RecallSettler { target: t },
                (_, Some(c)) if needs_guard => Reason::RecallGuardForDelivery { camp: c },
                _ => Reason::RecallSurplusFollower { desired: if standing { desired_group } else { 1 } },
            };
            out.push(Decision::new(Action::RecallFollower, score, Some(here), reason));
        }
        let can_recruit = self.may_recruit_here() && self.can_recruit(generated);
        let mut settlement_ready = true;
        let mut local_settler: Option<Plan> = None;
        let mut settlement_route: Option<Route> = None;
        if let Some(t) = settlement.filter(|_| needs_settler) {
            settlement_route = self.route(here, t);
            local_settler = self.find_recruit_at(here, true, generated);
            let funded = Self::cost(&local_settler);
            let solo_back = if self.owned() > 0 { self.provisioned_return(t) } else { self.route(t, here) };
            let (rf, rw) = (INITIAL_RECRUIT_RESERVE, INITIAL_RECRUIT_RESERVE);
            let recruit = local_settler.as_ref().and_then(|p| self.candidate(here, p.record));
            settlement_ready = match (&settlement_route, funded, &solo_back) {
                (Some(r), Some((food, _)), Some(back)) => {
                    supplies::has_projected_recruit_territorial_supplies(&self.group, self.sites(), items, self.w.player, self.previous, here, r, rf, rw)
                        && supplies::has_projected_recruit_solo_return_food(&self.group, r, rf, if self.can_remain_at_new_camp(t, recruit) { 0 } else { back.days }, food)
                }
                _ => false,
            };
        }
        let remote_settler = match settlement.filter(|_| needs_settler) {
            Some(t) => self.destination_recruit(t, out) || self.route_recruit(t, out),
            None => false,
        };
        if let Some(t) = settlement.filter(|_| needs_settler && !remote_settler && can_recruit && !settlement_ready) {
            let staging = self.settlement_staging_camp(t);
            self.travel(out, staging, 2120.0, Reason::ProvisionBeforeSettler { target: t });
            out.push(Decision::new(Action::Wait, 2110.0, Some(here), Reason::DelayRecruitment { target: t }));
        }
        let safe_follower = if needs_follower && !needs_settler && can_recruit { self.safe_local_recruit() } else { None };
        if needs_follower && !remote_settler && can_recruit && (!needs_settler || settlement_ready) && (needs_settler || safe_follower.is_some()) {
            let planned = if needs_settler { local_settler.as_ref().map(|p| p.record) } else { safe_follower.as_ref().map(|p| p.record) };
            let score = if needs_settler {
                2100.0
            } else if needs_guard {
                1040.0
            } else if self.is_city(here) {
                990.0
            } else {
                980.0
            };
            let reason = match (settlement, camp) {
                (Some(t), _) if needs_settler => Reason::RecruitSettler { role: claims::strategic_role(items, &self.sites().sites[t]), target: t },
                (_, Some(c)) if needs_guard => Reason::CriticalCampNeedsGuard { camp: c },
                _ if self.is_city(here) => Reason::CityRecruitOpportunity { desired: if standing { desired_group } else { 1 } },
                _ => Reason::GroupNeedsRecruit,
            };
            let mut d = Decision::new(Action::Recruit, score, if needs_settler { settlement } else { Some(here) }, reason);
            d.next_step = if needs_settler { settlement_route.as_ref().map(|r| r.next_step) } else { None };
            d.recruit = planned;
            out.push(d);
        } else if needs_follower && !remote_settler && !can_recruit {
            let prep = trading::best_camp_for_city_preparation(&self.context(), self.previous);
            let city = match settlement.filter(|_| needs_settler) {
                Some(t) => self.safe_settlement_recruitment_city(here, t),
                None => self.nearest_recruitment_city(None),
            };
            let waypoint = if needs_settler && self.owned() == 0 && prep.is_none() { self.first_camp_waypoint() } else { None };
            let score = if needs_settler {
                2090.0
            } else if needs_guard {
                1030.0
            } else {
                970.0
            };
            let reason = match (settlement, camp) {
                (Some(t), _) if needs_settler => Reason::FindStagedSettler { target: t },
                (_, Some(c)) if needs_guard => Reason::FindGuardForDelivery { camp: c },
                _ if prep.is_none() => Reason::LeaderNeedsRecruit,
                _ => Reason::FillCaravanBeforeRecruiting,
            };
            self.travel(out, prep.or(city), score, reason);
            self.travel(out, waypoint, 2095.0, Reason::AdvanceToFirstCampWaypoint { waypoint });
        }
        RecruitmentNeeds { camp, target: camp_target, attack_staging: false }
    }
}

/// What carrying out a `Recruit` decision did.
#[derive(Clone, Debug, PartialEq)]
pub enum Hired {
    /// Taken into the group, and paid for.
    Hired { record: usize },
    /// Nothing changed: nobody to hire, no room, or no payment any more.
    Refused,
}

/// `AiDecision.Execute(Recruit)` → `HireNpc`: the planned recruit, or one
/// drawn from the affordable when none was planned, checked again — still
/// here, hireable, room in the group, the payment still there — then paid,
/// and handed to `hire` (the world's `Character.Hire`). If the world does
/// not take them, the payment is not taken either. What follows a hire —
/// a water container and the reserve's weapon for the recruit, the group's
/// equipment, setting off with a settler — is the controller's.
#[allow(clippy::too_many_arguments)]
pub fn execute_recruit(
    r: &mut Rival,
    kit: &mut Kit,
    camp_rooms: &mut [Vec<ItemBag>],
    w: &World,
    ai: &ModernAi,
    people_here: &[Candidate],
    planned: Option<usize>,
    rng: &mut Rng,
    hire: impl FnOnce(&mut Rival, usize) -> bool,
) -> Hired {
    let generated = w.sites.get(w.here).is_some_and(|s| s.city) && w.policy.allow_generated_recruit_payment_in_cities;
    let (record, plan, line) = {
        let purse = Purse { items: w.items, r, leader_kit: kit, camp_rooms, owned_camps: w.owned_camps() };
        let free_first = w.owned_camps() == 0 && group::size(r) == 1;
        let hireable = recruitment::hireable(people_here, w.policy, &ai.demobilized);
        let affordable: Vec<&Candidate> = hireable.iter().copied().filter(|c| recruitment::recruitment_plan(&purse, c, generated).funded).collect();
        let chosen = match planned {
            Some(p) => hireable.iter().copied().find(|c| c.record == p),
            None if affordable.is_empty() => None,
            None => Some(affordable[rng.below(affordable.len() as u32) as usize]),
        };
        let Some(c) = chosen else { return Hired::Refused };
        if group::size(r) >= super::intel::MAX_PEOPLE as usize {
            return Hired::Refused;
        }
        let plan = recruitment::recruitment_plan(&purse, c, generated);
        if !free_first && !plan.funded {
            return Hired::Refused;
        }
        // `HireNpc`'s reports of how the recruit was paid, in its words.
        let name = super::telemetry::person(c.record);
        let items = w.items;
        let id = |k: usize| alloc::string::String::from(items.types.name_of(k).unwrap_or(""));
        let line = if free_first {
            Some(alloc::format!("recruited first settler {name} for free"))
        } else {
            match &plan.payment {
                recruitment::Payment::Exact { slot } => r.pack.get(*slot).map(|k| {
                    let v = items.value(k);
                    alloc::format!("funded recruitment of {name} with requested {} (required value {v}, paid {v})", id(k))
                }),
                recruitment::Payment::Generated { kind, assets } => {
                    let kinds: Vec<usize> = assets.iter().filter_map(|&a| purse.kind_at(a)).collect();
                    let paid = super::reason::dotnet_sum(kinds.iter().map(|&k| items.value(k)));
                    let list: Vec<alloc::string::String> = kinds.iter().map(|&k| id(k)).collect();
                    Some(alloc::format!("funded recruitment of {name} with {} (required value {}, paid {paid})", list.join(", "), items.value(*kind)))
                }
                _ => None,
            }
        };
        (c.record, plan, line)
    };
    let (mut r2, mut kit2, mut rooms2) = (r.clone(), *kit, camp_rooms.to_vec());
    if !recruitment::pay(&mut r2, &mut kit2, &mut rooms2, &plan) {
        return Hired::Refused;
    }
    if !hire(&mut r2, record) {
        return Hired::Refused;
    }
    if let Some(line) = line {
        ai.telemetry.report(|| line);
    }
    *r = r2;
    *kit = kit2;
    camp_rooms.clone_from_slice(&rooms2);
    Hired::Hired { record }
}

fn plan_record(plan: &Option<Plan>) -> usize {
    plan.as_ref().map_or(usize::MAX, |p| p.record)
}

/// The attack's size for [`Planner::add_candidates`], from attack planning
/// (`AttackPlanning.RequiredAttackGroupSize`) when the opponent is preparing
/// an attack on `target`; none otherwise.
pub fn attack_size(ai: &mut ModernAi, f: &super::attack::Field, target: Option<usize>, preparing_attack: bool, policy: &super::policy::AiPolicy) -> Option<i32> {
    let t = target.filter(|_| preparing_attack)?;
    Some(super::attack::required_attack_group_size(ai, f, t, policy))
}
