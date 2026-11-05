//! Whom the opponent may hire and what it costs: `GetHireableCandidates`,
//! `FindRecruitAt`, `CreateRecruitmentPlan`, `TryPlanRecruitmentPayment`
//! and the payment `HireNpc` makes. At tag `v1.2`,
//! `AI/PlayerBehavior/Core/AiState.cs` and `Expansion/Recruitment.cs`.
//!
//! The people come from the world as [`Candidate`]s; taking one into the
//! group is the world's (`Character.Hire`). Here: the choice, and the
//! payment taken out of the group and the camps — all of it or none.

use alloc::string::String;
use alloc::vec::Vec;

use crate::combat::Kit;
use crate::gamdat::CharClass;
use crate::game::ItemBag;
use crate::rival::Rival;

use super::equipment::take;
use super::group::{self, Items};
use super::policy::AiPolicy;
use super::trading::sale_priority;

/// Somebody at a place who might be hired.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Their record, their identity.
    pub record: usize,
    pub name: String,
    pub class: CharClass,
    pub experience: i32,
    pub health: i32,
    /// `HireItems`: what they ask for, any one of them.
    pub hire_items: Vec<usize>,
    pub pack: ItemBag,
    /// Dead, hired, a creature or a trader: never hireable.
    pub available: bool,
}

/// `GetHireableCandidates`: the living, unhired humans who are not traders
/// and were not sent home by this opponent; of them, those within the
/// policy's experience, when there are any.
pub fn hireable<'a>(candidates: &'a [Candidate], policy: &AiPolicy, demobilized: &[usize]) -> Vec<&'a Candidate> {
    let available: Vec<&Candidate> = candidates.iter().filter(|c| c.available && !demobilized.contains(&c.record)).collect();
    let preferred: Vec<&Candidate> = available
        .iter()
        .copied()
        .filter(|c| c.experience >= policy.minimum_recruit_experience && c.experience <= policy.maximum_recruit_experience)
        .collect();
    if preferred.is_empty() { available } else { preferred }
}

/// Where a payment comes from: a member's pack, or a camp's room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Member { member: usize, slot: usize },
    Room { camp: usize, room: usize, slot: usize },
}

/// `RecruitmentPlan`.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub record: usize,
    pub funded: bool,
    /// What carried food and water the payment takes.
    pub food: i32,
    pub water: i32,
    pub payment: Payment,
}

/// How a recruit is paid.
#[derive(Clone, Debug, PartialEq)]
pub enum Payment {
    /// The first recruit of a leader with no camp, or one who asks nothing.
    Free,
    /// What they asked for, carried by the leader: his slot.
    Exact { slot: usize },
    /// Food worth what they ask, gathered from the group and the camps.
    Generated { kind: usize, assets: Vec<Source> },
    None,
}

/// What the payment planner reads.
pub struct Purse<'a> {
    pub items: Items<'a>,
    pub r: &'a Rival,
    pub leader_kit: &'a Kit,
    /// The rooms of the opponent's camps, by camp.
    pub camp_rooms: &'a [Vec<ItemBag>],
    pub owned_camps: usize,
}

impl Purse<'_> {
    fn is_chosen(&self, member: usize, slot: usize) -> bool {
        let kit = if member == 0 { self.leader_kit } else { &self.r.party.companions[member - 1].kit };
        kit.weapon == Some(slot) || kit.protection == Some(slot)
    }

    fn food_supply(&self) -> i32 {
        let t = self.items.types;
        (0..group::size(self.r)).map(|m| group::needs(self.r, m).food).sum::<i32>()
            + group::carried(self.r).iter().map(|&(_, _, k)| t.food_of(k)).sum::<i32>()
    }

    pub fn kind_at(&self, s: Source) -> Option<usize> {
        match s {
            Source::Member { member, slot } => group::pack(self.r, member).get(slot),
            Source::Room { camp, room, slot } => self.camp_rooms.get(camp)?.get(room)?.get(slot),
        }
    }

    /// `CanUseForRecruitmentPayment`.
    fn usable(&self, s: Source, remaining_food: i32) -> bool {
        let Some(kind) = self.kind_at(s) else { return false };
        let t = self.items.types;
        if t.food_of(kind) <= 0 {
            return false;
        }
        if let Source::Member { member, slot } = s {
            if self.is_chosen(member, slot) {
                return false;
            }
            if remaining_food - t.food_of(kind) < group::size(self.r) as i32 * 3 {
                return false;
            }
        }
        self.items.value(kind) > 0.0
    }
}

/// `TryPlanRecruitmentPayment`: the cheapest thing asked for, paid in food
/// — never a weapon or protection in hand, never what leaves the group
/// under three days each — the cheapest first, then the surplus handed
/// back; and if the leader's pack is full, one of his own among it.
pub fn plan_generated_payment(p: &Purse, recruit: &Candidate) -> Option<(usize, Vec<Source>)> {
    let items = p.items;
    let kind = recruit
        .hire_items
        .iter()
        .copied()
        .fold(None, |b: Option<usize>, k| match b {
            Some(b) if items.value(k) >= items.value(b) => Some(b),
            _ => Some(k),
        })?;
    let price = items.value(kind);
    let mut remaining = p.food_supply();
    let mut all: Vec<Source> = group::carried(p.r).into_iter().filter(|&(_, _, k)| items.types.food_of(k) > 0).map(|(member, slot, _)| Source::Member { member, slot }).collect();
    for (camp, rooms) in p.camp_rooms.iter().enumerate() {
        for (room, bag) in rooms.iter().enumerate() {
            for slot in 0..bag.len() {
                if bag.get(slot).is_some_and(|k| items.types.food_of(k) > 0) {
                    all.push(Source::Room { camp, room, slot });
                }
            }
        }
    }
    let mut ordered: Vec<Source> = all.into_iter().filter(|&s| p.usable(s, remaining)).collect();
    ordered.sort_by(|&a, &b| {
        let (ka, kb) = (p.kind_at(a).unwrap_or(0), p.kind_at(b).unwrap_or(0));
        items.value(ka).partial_cmp(&items.value(kb)).unwrap_or(core::cmp::Ordering::Equal).then(sale_priority(items, ka).cmp(&sale_priority(items, kb)))
    });
    let mut paid = 0.0f32;
    let mut assets: Vec<Source> = Vec::new();
    for s in ordered {
        if !p.usable(s, remaining) {
            continue;
        }
        let k = p.kind_at(s).unwrap_or(0);
        assets.push(s);
        paid += items.value(k);
        if matches!(s, Source::Member { .. }) {
            remaining -= items.types.food_of(k);
        }
        if paid >= price {
            break;
        }
    }
    if paid < price {
        return None;
    }
    let mut by_value = assets.clone();
    by_value.sort_by(|&a, &b| items.value(p.kind_at(b).unwrap_or(0)).partial_cmp(&items.value(p.kind_at(a).unwrap_or(0))).unwrap_or(core::cmp::Ordering::Equal));
    for s in by_value {
        let v = items.value(p.kind_at(s).unwrap_or(0));
        if assets.len() <= 1 || paid - v < price {
            continue;
        }
        assets.retain(|&x| x != s);
        paid -= v;
    }
    if p.r.pack.is_full() && !assets.iter().any(|s| matches!(s, Source::Member { member: 0, .. })) {
        let leader = (0..p.r.pack.len())
            .map(|slot| Source::Member { member: 0, slot })
            .filter(|&s| p.usable(s, remaining))
            .fold(None, |b: Option<Source>, s| match b {
                Some(b) if items.value(p.kind_at(s).unwrap_or(0)) >= items.value(p.kind_at(b).unwrap_or(0)) => Some(b),
                _ => Some(s),
            })?;
        assets.push(leader);
    }
    Some((kind, assets))
}

/// `CreateRecruitmentPlan`.
pub fn recruitment_plan(p: &Purse, recruit: &Candidate, allow_generated: bool) -> Plan {
    let t = p.items.types;
    let plan = |funded, food, water, payment| Plan { record: recruit.record, funded, food, water, payment };
    if p.owned_camps == 0 && group::size(p.r) == 1 {
        return plan(true, 0, 0, Payment::Free);
    }
    let exact = recruit.hire_items.iter().find_map(|&k| (0..p.r.pack.len()).find(|&s| p.r.pack.get(s) == Some(k)));
    if let Some(slot) = exact {
        let k = p.r.pack.get(slot).unwrap_or(0);
        return plan(true, t.food_of(k), t.water_of(k), Payment::Exact { slot });
    }
    if recruit.hire_items.is_empty() {
        return plan(true, 0, 0, Payment::Free);
    }
    if !allow_generated {
        return plan(false, 0, 0, Payment::None);
    }
    match plan_generated_payment(p, recruit) {
        Some((kind, assets)) => {
            let carried = |f: fn(&crate::items::ItemTypes, usize) -> i32| {
                assets
                    .iter()
                    .filter(|s| matches!(s, Source::Member { .. }))
                    .map(|&s| f(t, p.kind_at(s).unwrap_or(0)))
                    .sum::<i32>()
            };
            plan(true, carried(crate::items::ItemTypes::food_of), carried(crate::items::ItemTypes::water_of), Payment::Generated { kind, assets })
        }
        None => plan(false, 0, 0, Payment::None),
    }
}

/// `FindRecruitAt`: of the hireable, the one whose cheapest request is
/// cheapest, then by name.
pub fn find_recruit_at(p: &Purse, policy: &AiPolicy, candidates: &[Candidate], demobilized: &[usize], affordable: bool, allow_generated: bool) -> Option<Plan> {
    let items = p.items;
    let cheapest = |c: &Candidate| c.hire_items.iter().map(|&k| items.value(k)).fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.min(v)))).unwrap_or(0.0);
    let mut plans: Vec<(&Candidate, Plan)> = hireable(candidates, policy, demobilized)
        .into_iter()
        .map(|c| (c, recruitment_plan(p, c, allow_generated)))
        .filter(|(_, plan)| !affordable || plan.funded)
        .collect();
    plans.sort_by(|a, b| cheapest(a.0).partial_cmp(&cheapest(b.0)).unwrap_or(core::cmp::Ordering::Equal).then(a.0.name.cmp(&b.0.name)));
    plans.into_iter().next().map(|(_, plan)| plan)
}

/// `CanRecruit`: an affordable hireable here, and room in the group.
pub fn can_recruit(p: &Purse, policy: &AiPolicy, candidates: &[Candidate], demobilized: &[usize], allow_generated: bool) -> bool {
    group::size(p.r) < super::intel::MAX_PEOPLE as usize
        && hireable(candidates, policy, demobilized).iter().any(|c| recruitment_plan(p, c, allow_generated).funded)
}

/// The payment `HireNpc` takes, from the leader, the group and the camps.
/// The formal payment (`Character.Hire`) is the requested item out of the
/// leader's pack; a generated one takes its assets instead. False, and
/// nothing taken, when it no longer stands.
pub fn pay(r: &mut Rival, kit: &mut Kit, camp_rooms: &mut [Vec<ItemBag>], plan: &Plan) -> bool {
    match &plan.payment {
        Payment::Free => true,
        Payment::None => false,
        Payment::Exact { slot } => take(&mut r.pack, kit, *slot).is_some(),
        Payment::Generated { assets, .. } => {
            let mut r2 = r.clone();
            let mut kit2 = *kit;
            let mut rooms2 = camp_rooms.to_vec();
            let mut sorted = assets.clone();
            sorted.sort_by(|a, b| {
                let key = |s: &Source| match *s {
                    Source::Member { member, slot } => (0, member, 0, slot),
                    Source::Room { camp, room, slot } => (1, camp, room, slot),
                };
                key(b).cmp(&key(a))
            });
            for s in sorted {
                let ok = match s {
                    Source::Member { member: 0, slot } => take(&mut r2.pack, &mut kit2, slot).is_some(),
                    Source::Member { member, slot } => {
                        let Some(c) = r2.party.companions.get_mut(member - 1) else { return false };
                        take(&mut c.pack, &mut c.kit, slot).is_some()
                    }
                    Source::Room { camp, room, slot } => rooms2.get_mut(camp).and_then(|c| c.get_mut(room)).and_then(|b| b.remove(slot)).is_some(),
                };
                if !ok {
                    return false;
                }
            }
            *r = r2;
            *kit = kit2;
            camp_rooms.clone_from_slice(&rooms2);
            true
        }
    }
}
