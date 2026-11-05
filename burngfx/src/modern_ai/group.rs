//! The opponent's group looked after at the start of its turn: the city's
//! minimum, the spring, eating and drinking what is carried, the doctor.
//! At tag `v1.2`, `AI/PlayerBehavior/Group/GroupInventory.cs`,
//! `RecoveryServices.cs` and `AiTurnController.ExecuteImmediateRecovery`.
//!
//! The group is the opponent's own ([`Rival`]): the leader first, then the
//! companions in their order — `Player.Party`'s order, which every "first
//! of equals" below follows. What another part of the planner decides
//! (the payment kept back for a settler, what may be sold) comes in as an
//! argument until that part is built.

use alloc::vec::Vec;

use crate::game::{group_drink, group_eat, CharacterNeeds, ItemBag, WaterSource};
use crate::gamdat::Danger;
use crate::items::ItemTypes;
use crate::rival::Rival;
use crate::trade::{worth_of, Prices};

use super::supplies::GroupSupplies;

/// `RecoveryServices.CityMinimum`.
pub const CITY_MINIMUM: i32 = 3;
/// `RecoveryServices.HealthTarget`.
pub const HEALTH_TARGET: i32 = 70;
/// Health at or under which a doctor is wanted.
pub const PATIENT_HEALTH: i32 = 40;
/// `CampManagement.CampFoodItemReserve`.
pub const CAMP_FOOD_ITEM_RESERVE: usize = 0;

/// What the planner asks of an item kind.
#[derive(Clone, Copy)]
pub struct Items<'a> {
    pub types: &'a ItemTypes,
    pub prices: &'a Prices,
}

impl Items<'_> {
    /// `TradeValue`.
    pub fn value(&self, kind: usize) -> f32 {
        worth_of(kind, self.types, self.prices)
    }

    /// `AiItemPool.IsWaterContainer`: full and holding water, or empty with
    /// a full form that holds some.
    pub fn is_water_container(&self, kind: usize) -> bool {
        let t = self.types;
        t.empty_of(kind).is_some() && t.water_of(kind) > 0 || t.full_of(kind).is_some_and(|f| t.water_of(f) > 0)
    }

    /// `AiItemPool.WaterContainerCapacity`.
    pub fn water_capacity(&self, kind: usize) -> i32 {
        let t = self.types;
        if t.water_of(kind) > 0 { t.water_of(kind) } else { t.full_of(kind).map_or(0, |f| t.water_of(f)) }
    }

    /// `AiItemPool.IsHazardProtection`: any line for gas or radiation.
    pub fn is_hazard_protection(&self, kind: usize) -> bool {
        self.types.get(kind).is_some_and(|t| t.protection.iter().any(|(d, _)| matches!(d, Danger::Gas | Danger::Radiation)))
    }

    /// `IsFullWaterContainer`: water in it, and an empty form.
    pub fn is_full_water_container(&self, kind: usize) -> bool {
        self.types.water_of(kind) > 0 && self.types.empty_of(kind).is_some()
    }

    /// `Type.Full != null && WaterValue == 0`: what a spring fills.
    pub fn is_empty_water_container(&self, kind: usize) -> bool {
        self.types.full_of(kind).is_some() && self.types.water_of(kind) == 0
    }
}

/// The group's members, the leader first.
pub fn size(r: &Rival) -> usize {
    1 + r.party.companions.len()
}

pub fn needs(r: &Rival, member: usize) -> &CharacterNeeds {
    if member == 0 { &r.needs } else { &r.party.companions[member - 1].needs }
}

pub fn needs_mut(r: &mut Rival, member: usize) -> &mut CharacterNeeds {
    if member == 0 { &mut r.needs } else { &mut r.party.companions[member - 1].needs }
}

pub fn pack(r: &Rival, member: usize) -> &ItemBag {
    if member == 0 { &r.pack } else { &r.party.companions[member - 1].pack }
}

pub fn pack_mut(r: &mut Rival, member: usize) -> &mut ItemBag {
    if member == 0 { &mut r.pack } else { &mut r.party.companions[member - 1].pack }
}

/// Every carried item, `(member, slot, kind)`, in party and slot order.
pub fn carried(r: &Rival) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for m in 0..size(r) {
        let p = pack(r, m);
        for slot in 0..p.len() {
            if let Some(k) = p.get(slot) {
                out.push((m, slot, k));
            }
        }
    }
    out
}

/// The group as [`super::supplies`] counts it.
pub fn supplies(r: &Rival, types: &ItemTypes) -> GroupSupplies {
    let mut g = GroupSupplies::default();
    for m in 0..size(r) {
        let n = needs(r, m);
        g.food.push(n.food);
        g.water.push(n.water);
        g.max_food.push(n.max_food);
        g.max_water.push(n.max_water);
        g.health.push(n.health);
    }
    for (_, _, k) in carried(r) {
        g.food_carried += types.food_of(k);
        g.water_carried += types.water_of(k);
    }
    g
}

/// `ProvideCityMinimum`: in a city, everybody up to three food and/or
/// water — the leader not while the opponent holds no camp — and, with
/// `refill`, as many of the group's empty containers as it has members
/// filled, the largest first. Nothing is made: only what the group carries
/// is filled. `(food, water, containers)` given.
pub fn city_minimum(r: &mut Rival, items: Items, in_city: bool, owned_camps: usize, food: bool, water: bool, refill: bool) -> (i32, i32, Vec<usize>) {
    if !in_city {
        return (0, 0, Vec::new());
    }
    let (mut given_food, mut given_water) = (0, 0);
    for m in 0..size(r) {
        if m == 0 && owned_camps == 0 {
            continue;
        }
        let n = needs_mut(r, m);
        let food_target = if food { CITY_MINIMUM.min(n.max_food) } else { n.food };
        let water_target = if water { CITY_MINIMUM.min(n.max_water) } else { n.water };
        given_food += (food_target - n.food).max(0);
        given_water += (water_target - n.water).max(0);
        n.food = n.food.max(food_target);
        n.water = n.water.max(water_target);
    }
    let mut refilled = Vec::new();
    if refill {
        let mut empties: Vec<(usize, usize, usize)> =
            carried(r).into_iter().filter(|&(_, _, k)| items.is_empty_water_container(k)).collect();
        // OrderByDescending: stable, the first of equals stays first.
        empties.sort_by_key(|&(_, _, k)| core::cmp::Reverse(items.water_capacity(k)));
        empties.truncate(size(r));
        for (m, slot, k) in empties {
            if let Some(full) = items.types.full_of(k) {
                pack_mut(r, m).replace(slot, full);
                refilled.push(full);
            }
        }
    }
    (given_food, given_water, refilled)
}

/// `UseLocalWaterSource`: the group drinks from the spring's reserve, then
/// every empty container it carries is filled while the reserve holds a
/// whole one.
pub fn use_local_water_source(r: &mut Rival, items: Items, source: &mut WaterSource) {
    source.reserve = group_drink(&mut r.needs, &mut r.party.companions, source.reserve);
    for (m, slot, k) in carried(r) {
        if items.is_empty_water_container(k) {
            let _ = crate::game::fill_container(source, pack_mut(r, m), slot, items.types);
        }
    }
}

/// `RemoveAdviceItems`, for the group's own packs.
pub fn remove_advice(bag: &mut ItemBag, types: &ItemTypes) {
    let Some(advice) = types.index_of("item_advice") else { return };
    let mut slot = bag.len();
    while slot > 0 {
        slot -= 1;
        if bag.get(slot) == Some(advice) {
            bag.remove(slot);
        }
    }
}

/// Where a thing to eat lies: a member's pack or a room of the camp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holder {
    Member(usize),
    Room(usize),
}

/// `ConsumeAvailableSupplies`: while the leader has five food or less, or a
/// follower three or less, the group eats the least filling thing it has
/// (the cheapest of equals) — never the kind kept back to pay a settler,
/// and from the camp's rooms only when the camp may be looted and holds
/// more food than its reserve — as long as the group has room for all of
/// it. Then, while anybody has two water or less, the fullest container is
/// drunk. Whether anything was.
pub fn consume_available_supplies(r: &mut Rival, items: Items, reserved_payment: Option<usize>, mut camp_rooms: Option<&mut [ItemBag]>) -> bool {
    let t = items.types;
    let mut consumed = false;
    loop {
        let hungry = r.needs.food <= 5 || r.party.companions.iter().any(|c| c.needs.food <= 3);
        if !hungry {
            break;
        }
        let mut candidates: Vec<(Holder, usize, usize)> = carried(r)
            .into_iter()
            .filter(|&(_, _, k)| t.food_of(k) > 0 && Some(k) != reserved_payment)
            .map(|(m, s, k)| (Holder::Member(m), s, k))
            .collect();
        if let Some(rooms) = camp_rooms.as_deref() {
            let stored = rooms.iter().map(|b| b.iter().filter(|&k| t.food_of(k) > 0).count()).sum::<usize>();
            if stored > CAMP_FOOD_ITEM_RESERVE {
                for (i, room) in rooms.iter().enumerate() {
                    for slot in 0..room.len() {
                        if let Some(k) = room.get(slot).filter(|&k| t.food_of(k) > 0 && Some(k) != reserved_payment) {
                            candidates.push((Holder::Room(i), slot, k));
                        }
                    }
                }
            }
        }
        // OrderBy food, then trade value: the first of equals.
        let best = candidates.iter().copied().enumerate().min_by(|(ia, a), (ib, b)| {
            (t.food_of(a.2), items.value(a.2))
                .partial_cmp(&(t.food_of(b.2), items.value(b.2)))
                .unwrap_or(core::cmp::Ordering::Equal)
                .then(ia.cmp(ib))
        });
        let Some((_, (holder, slot, kind))) = best else { break };
        let room: i32 = (0..size(r)).map(|m| needs(r, m).max_food - needs(r, m).food).sum();
        if t.food_of(kind) > room {
            break;
        }
        group_eat(&mut r.needs, &mut r.party.companions, t.food_of(kind));
        match holder {
            Holder::Member(m) => {
                pack_mut(r, m).remove(slot);
            }
            Holder::Room(i) => {
                if let Some(rooms) = camp_rooms.as_deref_mut() {
                    rooms[i].remove(slot);
                }
            }
        }
        consumed = true;
    }
    loop {
        if !(0..size(r)).any(|m| needs(r, m).water <= 2) {
            break;
        }
        // The most water; the first of equals.
        let mut best: Option<(usize, usize, usize)> = None;
        for (m, s, k) in carried(r) {
            if t.water_of(k) > 0 && Some(k) != reserved_payment && best.is_none_or(|(_, _, b)| t.water_of(k) > t.water_of(b)) {
                best = Some((m, s, k));
            }
        }
        let Some((m, slot, kind)) = best else { break };
        group_drink(&mut r.needs, &mut r.party.companions, t.water_of(kind));
        match t.empty_of(kind) {
            Some(empty) => {
                pack_mut(r, m).replace(slot, empty);
            }
            None => {
                pack_mut(r, m).remove(slot);
            }
        }
        consumed = true;
    }
    consumed
}

/// How the doctor is paid, as `RuleBook`'s numbers.
#[derive(Clone, Copy, Debug)]
pub struct Doctor {
    pub healing_factor: f32,
    pub health_cap: i32,
}

/// One visit: who, how much, and with what (`(member, kind)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Treatment {
    pub patient: usize,
    pub healed: i32,
    pub paid: Vec<(usize, usize)>,
}

fn doctor_value(selected: &[(usize, usize, usize)], items: Items, factor: f32) -> i32 {
    let points: i32 = selected.iter().map(|&(_, _, k)| items.types.heal_of(k)).sum();
    crate::v12_economy::doctor_result(0, points, factor, i32::MAX)
}

/// `BuildDoctorPayment`: what heals, the safe to sell first, then full
/// water containers before other things and empty ones last, then by sale
/// priority and value; enough for `needed`, then whatever is not needed
/// handed back, the most healing first.
fn doctor_payment(
    r: &Rival,
    items: Items,
    needed: i32,
    factor: f32,
    can_sell: &impl Fn(usize) -> bool,
    sale_priority: &impl Fn(usize) -> i32,
) -> Vec<(usize, usize, usize)> {
    let mut candidates: Vec<(usize, usize, usize)> = carried(r).into_iter().filter(|&(_, _, k)| items.types.heal_of(k) > 0).collect();
    let water_rank = |k: usize| if items.is_full_water_container(k) { 0 } else if items.is_water_container(k) { 2 } else { 1 };
    candidates.sort_by(|a, b| {
        (!can_sell(a.2) as u8, water_rank(a.2), sale_priority(a.2))
            .cmp(&(!can_sell(b.2) as u8, water_rank(b.2), sale_priority(b.2)))
            .then(items.value(a.2).partial_cmp(&items.value(b.2)).unwrap_or(core::cmp::Ordering::Equal))
    });
    let mut selected = Vec::new();
    for c in candidates {
        selected.push(c);
        if doctor_value(&selected, items, factor) >= needed {
            break;
        }
    }
    let mut by_heal = selected.clone();
    by_heal.sort_by_key(|&(_, _, k)| core::cmp::Reverse(items.types.heal_of(k)));
    for c in by_heal {
        let without: Vec<_> = selected.iter().copied().filter(|&s| s != c).collect();
        if doctor_value(&without, items, factor) >= needed {
            selected = without;
        }
    }
    selected
}

/// `UseDoctor`: where there is a doctor, everybody at forty or less, the
/// worst first, is healed toward seventy (or the cap) with what the group
/// carries, when that heals anything.
pub fn use_doctor(
    r: &mut Rival,
    items: Items,
    has_doctor: bool,
    doctor: Doctor,
    can_sell: impl Fn(usize) -> bool,
    sale_priority: impl Fn(usize) -> i32,
) -> Vec<Treatment> {
    let mut done = Vec::new();
    if !has_doctor {
        return done;
    }
    let mut patients: Vec<usize> = (0..size(r)).filter(|&m| needs(r, m).health <= PATIENT_HEALTH).collect();
    patients.sort_by_key(|&m| needs(r, m).health);
    for p in patients {
        let health = needs(r, p).health;
        let needed = HEALTH_TARGET.min(doctor.health_cap) - health;
        let payment = doctor_payment(r, items, needed, doctor.healing_factor, &can_sell, &sale_priority);
        let points: i32 = payment.iter().map(|&(_, _, k)| items.types.heal_of(k)).sum();
        let result = crate::v12_economy::doctor_result(health, points, doctor.healing_factor, doctor.health_cap);
        if result - health <= 0 {
            continue;
        }
        // Highest slot first, so the others stay where they are.
        let mut taken = payment.clone();
        taken.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        for (m, slot, _) in taken {
            pack_mut(r, m).remove(slot);
        }
        needs_mut(r, p).health = result;
        done.push(Treatment { patient: p, healed: result - health, paid: payment.iter().map(|&(m, _, k)| (m, k)).collect() });
    }
    done
}

/// `NeedsDoctorPayment`.
pub fn needs_doctor_payment(r: &Rival, has_doctor: bool) -> bool {
    has_doctor && (0..size(r)).any(|m| needs(r, m).health <= PATIENT_HEALTH)
}

/// The leader's three kept places (`IsFoodRole`, `IsWaterRole`,
/// `IsProtectionRole`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Food,
    Water,
    Protection,
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Food, Role::Water, Role::Protection];

    pub fn fits(self, items: Items, kind: usize) -> bool {
        match self {
            Role::Food => items.types.food_of(kind) > 0,
            Role::Water => items.is_water_container(kind),
            Role::Protection => items.is_hazard_protection(kind),
        }
    }

    /// What makes one item better than another for the place.
    fn value(self, items: Items, kind: usize) -> i32 {
        match self {
            Role::Food => items.types.food_of(kind),
            Role::Water => items.water_capacity(kind),
            Role::Protection => items.types.defense_of(kind),
        }
    }
}

/// `MissingRoleSlots`.
pub fn missing_roles(items: Items, kinds: &[usize]) -> usize {
    Role::ALL.iter().filter(|r| !kinds.iter().any(|&k| r.fits(items, k))).count()
}

/// Where [`maintain_leader_roles`] may put what it moves out of the
/// leader's pack, and take what it fills his places with.
pub struct OwnCamp<'a> {
    /// `StoreItemInCamp`: true when the camp took it.
    pub store: &'a mut dyn FnMut(usize, Option<i32>) -> bool,
    /// The camp's rooms, when its loot may be taken (`CanCollectLocalLoot`).
    pub rooms: Option<&'a mut [ItemBag]>,
}

/// `MaintainLeaderRoleSlots`: while the leader has fewer free slots than
/// places missing, the least worth keeping of his other things (never his
/// weapon, his protection or the settler's payment) goes to the follower
/// carrying least, else into the camp, else stays and the attempt ends.
/// Then each missing place is filled — food, water, protection — with the
/// best one a follower carries, else one from the camp's rooms.
pub fn maintain_leader_roles(
    r: &mut Rival,
    kit: &mut crate::combat::Kit,
    items: Items,
    reserved_payment: Option<usize>,
    retention: impl Fn(usize) -> f32,
    mut camp: Option<OwnCamp>,
) {
    use super::equipment::{give, take};
    loop {
        let free = r.pack.capacity() - r.pack.len();
        if free >= missing_roles(items, &r.pack.iter().collect::<Vec<_>>()) {
            break;
        }
        let mut displaced: Option<(usize, usize)> = None;
        for slot in 0..r.pack.len() {
            let Some(k) = r.pack.get(slot) else { continue };
            if Role::ALL.iter().any(|role| role.fits(items, k)) || Some(slot) == kit.weapon || Some(slot) == kit.protection || Some(k) == reserved_payment {
                continue;
            }
            let worse = displaced.is_none_or(|(_, d)| {
                (retention(k), items.value(k)).partial_cmp(&(retention(d), items.value(d))) == Some(core::cmp::Ordering::Less)
            });
            if worse {
                displaced = Some((slot, k));
            }
        }
        let Some((slot, _)) = displaced else { return };
        let follower = r
            .party
            .companions
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.pack.is_full())
            .min_by_key(|(i, c)| (c.pack.len(), *i))
            .map(|(i, _)| i);
        let Some((kind, rounds)) = take(&mut r.pack, kit, slot) else { return };
        if let Some(f) = follower {
            if give(&mut r.party.companions[f].pack, kind, rounds).is_some() {
                continue;
            }
        }
        if let Some(c) = camp.as_mut() {
            if (c.store)(kind, rounds) {
                continue;
            }
        }
        give(&mut r.pack, kind, rounds);
        return;
    }
    for role in Role::ALL {
        if r.pack.iter().any(|k| role.fits(items, k)) || r.pack.is_full() {
            continue;
        }
        // OrderByDescending value, then trade value: the first of the best.
        let better = |a: usize, b: usize| {
            (role.value(items, a), items.value(a)).partial_cmp(&(role.value(items, b), items.value(b))) == Some(core::cmp::Ordering::Greater)
        };
        let mut best: Option<(usize, usize, usize)> = None;
        for (ci, c) in r.party.companions.iter().enumerate() {
            for slot in 0..c.pack.len() {
                if let Some(k) = c.pack.get(slot).filter(|&k| role.fits(items, k)) {
                    if best.is_none_or(|(_, _, b)| better(k, b)) {
                        best = Some((ci, slot, k));
                    }
                }
            }
        }
        if let Some((ci, slot, _)) = best {
            let c = &mut r.party.companions[ci];
            if let Some((kind, rounds)) = take(&mut c.pack, &mut c.kit, slot) {
                give(&mut r.pack, kind, rounds);
            }
            continue;
        }
        let Some(rooms) = camp.as_mut().and_then(|c| c.rooms.as_deref_mut()) else { continue };
        let mut stored: Option<(usize, usize, usize)> = None;
        for (ri, room) in rooms.iter().enumerate() {
            for slot in 0..room.len() {
                if let Some(k) = room.get(slot).filter(|&k| role.fits(items, k)) {
                    if stored.is_none_or(|(_, _, b)| better(k, b)) {
                        stored = Some((ri, slot, k));
                    }
                }
            }
        }
        if let Some((ri, slot, _)) = stored {
            let rounds = rooms[ri].rounds_left(slot);
            if let Some(kind) = rooms[ri].remove(slot) {
                give(&mut r.pack, kind, rounds);
            }
        }
    }
}
