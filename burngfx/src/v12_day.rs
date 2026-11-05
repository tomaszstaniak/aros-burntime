//! v1.2's day for the people a player employs (`GameRules.TurnEmployedCharacter`).
//!
//! Two survival rules, one per preset:
//!
//! * **Classic, `dos_daily_consumption`** (`GameRules.TurnOriginalEmployed`):
//!   heal first; then spend a water and a food, and only a need that goes
//!   below nought is refilled — water from the camp's reserve for a camper at
//!   home, else the *cheapest* water within reach, made empty; food from the
//!   group's packs (the smallest), else the location's rooms (the smallest).
//!   A refill fills this one character, up to five water or his maximum food.
//!   Each need not met costs 25.
//! * **Extended, `remaster_supply_pool`** (`Character.TurnExtendedEmployed`):
//!   the day v0.9.1's port already has, with v1.2's details — an empty food
//!   level is filled for the whole group from the smallest food, an empty
//!   water level from the largest water; the doctor's stabilization; heal;
//!   then 25 for each need still at nought, and a level spent of each.
//!
//! Healing, both: `+4` with a doctor at 50 or more, else `+2` at 70 or more
//! — or at a player's own threshold (the Amiga opponent's is 50); capped at
//! a hundred (`Character.Health`'s setter).
//!
//! What a travelling group eats is shared (`GetGroup()` is the player's
//! party); a camper's larder is his own pack and his location.

use crate::danger::{spend, whole, Exposure};
use alloc::string::String;
use alloc::vec::Vec;

use crate::game::{
    group_drink, group_eat, group_find_water, CharacterNeeds, Death, ItemBag, ItemFacts, Party, Whose,
};
use crate::rules::{Survival, WaterOutput};

/// Everything a day of upkeep needs to know that is not in the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upkeep {
    pub survival: Survival,
    /// `doctor_stabilization` (Extended 1, Classic 0).
    pub stabilization: i32,
    /// A doctor among these people (`HasLocalDoctor`).
    pub doctor: bool,
    /// The player's own healing threshold, when its AI has one.
    pub threshold: Option<i32>,
    /// The group is on the road (`Player.IsTraveling`): a bible spares
    /// nobody then.
    pub travelling: bool,
}

/// `skipSupplies`: `(!IsWithBoss || !Player.IsTraveling) &&
/// HasItemFunction(RestingSustenance)` — one carrying a bible, at rest,
/// neither spends nor needs food and water.
fn resting(pack: &ItemBag, with_boss: bool, upkeep: Upkeep, facts: &impl ItemFacts) -> bool {
    (!with_boss || !upkeep.travelling) && pack.iter().any(|k| facts.has_function(k, "resting_sustenance"))
}

/// The heal and the doctor's stabilization, and nothing else: the day of
/// somebody resting (Extended's order).
///
/// Every `Health` read is the getter and every write goes through the
/// setter, so a danger's fraction carried is read as v1.2 reads it and spent
/// at the write ([`crate::danger::whole`], [`crate::danger::spend`]);
/// `IsDead` is the float's `health <= 0`, which the port's whole number
/// (rounded up while a fraction is carried) answers as it is.
fn extended_rest(n: &mut CharacterNeeds, e: &mut Exposure, upkeep: Upkeep) -> bool {
    let stabilize = upkeep.doctor && n.food > 0 && n.water > 0 && whole(n.health, e) < 50 && upkeep.stabilization > 0;
    if stabilize {
        spend(&mut n.health, e);
        n.health = (n.health + upkeep.stabilization).min(100);
    } else if whole(n.health, e) >= healing_threshold(upkeep.doctor, upkeep.threshold) {
        spend(&mut n.health, e);
        n.health = (n.health + if upkeep.doctor { 4 } else { 2 }).min(100);
    }
    n.health <= 0
}

/// Extended's heal, stabilization and the price of an empty pool, for one
/// who is not resting (`TurnExtendedEmployed`'s tail).
fn extended_upkeep(n: &mut CharacterNeeds, e: &mut Exposure, upkeep: Upkeep) -> bool {
    extended_rest(n, e, upkeep);
    if n.food == 0 {
        spend(&mut n.health, e);
        n.health -= 25;
    }
    if n.water == 0 {
        spend(&mut n.health, e);
        n.health -= 25;
    }
    if n.health <= 0 {
        return true;
    }
    n.food = (n.food - 1).max(0);
    n.water = (n.water - 1).max(0);
    false
}

/// The member's needs and his danger's carry: the leader's, or a
/// companion's.
fn member_mut<'a>(member: usize, leader: &'a mut CharacterNeeds, leader_e: &'a mut Exposure, party: &'a mut Party) -> (&'a mut CharacterNeeds, &'a mut Exposure) {
    if member == 0 {
        (leader, leader_e)
    } else {
        let c = &mut party.companions[member - 1];
        (&mut c.needs, &mut c.exposure)
    }
}

/// `RuleFormulas.NaturalHealingThreshold`.
pub fn healing_threshold(doctor: bool, own: Option<i32>) -> i32 {
    if doctor { 50 } else { own.unwrap_or(70) }
}

/// `RuleFormulas.WaterOutput`: a spring's day with its pumps.
pub fn water_output(rule: WaterOutput, base: i32, hand_pump: bool, industrial_pump: bool) -> i32 {
    match rule {
        WaterOutput::RemasterFixed => base + if industrial_pump { 5 } else if hand_pump { 2 } else { 0 },
        WaterOutput::DosProportional => {
            if industrial_pump {
                base + (base / 2).max(2)
            } else if hand_pump {
                base + (base / 4).max(1)
            } else {
                base
            }
        }
        WaterOutput::AmigaMinimum => {
            if industrial_pump {
                (base + base / 2).max(6)
            } else if hand_pump {
                (base + base / 4).max(3)
            } else {
                base
            }
        }
    }
}

/// `RuleFormulas.CampBossExperience` (`dos_camps`): the base plus three a
/// camp, at most 99.
pub fn camp_boss_experience(base: i32, camps: usize) -> i32 {
    (base + 3 * camps as i32).min(99)
}

/// `Group.FindFood` as v1.2 has it: the **smallest** food anywhere in the
/// group, the first of equals.
pub fn group_find_smallest_food(leader: &ItemBag, companions: &[crate::game::Companion], facts: &impl ItemFacts) -> Option<(Whose, usize)> {
    let mut best: Option<(Whose, usize, i32)> = None;
    let mut consider = |whose: Whose, bag: &ItemBag| {
        for slot in 0..bag.len() {
            let Some(kind) = bag.get(slot) else { continue };
            let v = facts.food(kind);
            if v > 0 && best.is_none_or(|(_, _, b)| v < b) {
                best = Some((whose, slot, v));
            }
        }
    };
    consider(Whose::Leader, leader);
    for (i, c) in companions.iter().enumerate() {
        consider(Whose::Companion(i), &c.pack);
    }
    best.map(|(w, s, _)| (w, s))
}

/// `ConsumeWater`'s search: the cheapest water in reach, then by item id.
pub fn group_find_cheapest_water(leader: &ItemBag, companions: &[crate::game::Companion], facts: &impl ItemFacts) -> Option<(Whose, usize)> {
    let mut best: Option<(Whose, usize, i32, &str)> = None;
    let mut consider = |whose: Whose, bag: &ItemBag| {
        for slot in 0..bag.len() {
            let Some(kind) = bag.get(slot) else { continue };
            let v = facts.water(kind);
            if v <= 0 {
                continue;
            }
            let name = facts.name_of(kind).unwrap_or("");
            // `OrderBy(WaterValue).ThenBy(ID).FirstOrDefault()`: stable, so
            // the first of full equals.
            if best.is_none_or(|(_, _, b, n)| v < b || (v == b && name < n)) {
                best = Some((whose, slot, v, name));
            }
        }
    };
    consider(Whose::Leader, leader);
    for (i, c) in companions.iter().enumerate() {
        consider(Whose::Companion(i), &c.pack);
    }
    best.map(|(w, s, _, _)| (w, s))
}

fn bag<'a>(leader: &'a mut ItemBag, party: &'a mut Party, whose: Whose) -> Option<&'a mut ItemBag> {
    match whose {
        Whose::Leader => Some(leader),
        Whose::Companion(i) => party.companions.get_mut(i).map(|c| &mut c.pack),
    }
}

/// `MakeEmpty`: the empty form in place, or gone.
fn make_empty(bag: &mut ItemBag, slot: usize, facts: &impl ItemFacts) {
    let Some(kind) = bag.get(slot) else { return };
    match facts.empty(kind) {
        Some(e) => {
            bag.replace(slot, e);
        }
        None => {
            bag.remove(slot);
        }
    }
}

/// What a day did to a travelling group: the leader's fate and the others'.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupDay {
    pub leader_died: bool,
    pub deaths: Vec<Death>,
    /// The leader died of the place's danger (`ApplyDaily`), not of want.
    pub leader_hazard: bool,
    /// What the leader carried, when he died: `Die` drops it at once
    /// (`Items.MoveTo(Location.Items)`), so the companions after him in the
    /// day reach into an empty pack. `None` when he lives.
    pub leader_dropped: Option<ItemBag>,
}

/// A travelling group's day under v1.2, leader first (`Party[0]`), each
/// member in turn. A member who dies leaves the group at once (`Die` →
/// `Dismiss`), so a corpse feeds nobody after him.
pub fn party_day(
    leader: &mut CharacterNeeds,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
) -> GroupDay {
    party_day_in(leader, leader_pack, party, facts, upkeep, None)
}

/// The danger of where a group or a camp spends the day, for
/// `HazardRules.ApplyDaily`.
#[derive(Clone, Copy, Debug)]
pub struct Hazard<'a> {
    pub danger: crate::gamdat::Danger,
    /// `IsHazardImmune`'s faces for it.
    pub immune: &'a [i32],
    pub leader_face: u8,
    /// Faces by record, for the people with him.
    pub faces: &'a [(usize, u8)],
    /// An AI player's own group is spared (`Player.Type == Ai`).
    pub exempt: bool,
}

impl Hazard<'_> {
    fn face_of(&self, record: Option<usize>) -> u8 {
        record.and_then(|r| self.faces.iter().find(|f| f.0 == r)).map_or(0, |f| f.1)
    }
}

/// `PassesDailyCheck`: no danger, a face it does not touch, or full
/// protection in his own pack (`PreferredProtection` takes the best against
/// the danger whenever anything protects at all).
pub fn passes_daily_check(pack: &ItemBag, danger: crate::gamdat::Danger, face: u8, immune: &[i32], facts: &impl ItemFacts) -> bool {
    danger == crate::gamdat::Danger::None
        || immune.contains(&(face as i32))
        || pack.iter().map(|k| facts.protection(k, danger)).max().unwrap_or(0) >= 100
}

/// [`party_day`], with `ApplyDaily` after each member's upkeep: in a
/// dangerous place, a group not on the road loses whoever is not fully
/// protected.
pub fn party_day_in(
    leader: &mut CharacterNeeds,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
    hazard: Option<&Hazard>,
) -> GroupDay {
    party_day_with(leader, leader_pack, party, facts, upkeep, hazard, None)
}

/// [`party_day_in`] as v1.2's `World.Turn` walks the group: `for (j = 0; j
/// < Party.Count; j++) Party[j].Turn()` (`World.cs:128-132`), over a list
/// `Die` changes as it goes (`StateLinkList`'s index enumerator,
/// `StateLinkList.cs:20-45`).
///
/// * A companion who dies is `Dismiss`ed out of the list at once, so **the
///   member after him has no day** — the index moves past the one who
///   shifted into his place.
/// * The leader's death dismisses nobody (`Die`: only `Player.Character !=
///   this`), so **the companions still have their day**; what he carried is
///   on the ground already ([`GroupDay::leader_dropped`]).
/// * With `doctors` (records), `HasLocalDoctor` is asked for each member as
///   the group is at his turn (`Character.cs:804-807`), not once for the
///   day: a doctor who died earlier in the pass helps nobody after him.
pub fn party_day_with(
    leader: &mut CharacterNeeds,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
    hazard: Option<&Hazard>,
    doctors: Option<&[usize]>,
) -> GroupDay {
    party_day_exposed(leader, &mut Exposure::default(), leader_pack, party, facts, upkeep, hazard, doctors)
}

/// [`party_day_with`] for a leader carrying part of a danger's point
/// (`leader_e`); the companions carry their own.
#[allow(clippy::too_many_arguments)]
pub fn party_day_exposed(
    leader: &mut CharacterNeeds,
    leader_e: &mut Exposure,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
    hazard: Option<&Hazard>,
    doctors: Option<&[usize]>,
) -> GroupDay {
    let mut out = GroupDay::default();
    let mut member = 0;
    while member <= party.companions.len() {
        let mut upkeep = upkeep;
        if let Some(d) = doctors {
            upkeep.doctor = party.companions.iter().any(|c| c.origin.is_some_and(|o| d.contains(&o)));
        }
        let mut died = match upkeep.survival {
            Survival::DosDailyConsumption => original_member(member, leader, leader_e, leader_pack, party, facts, upkeep),
            Survival::RemasterSupplyPool => extended_member(member, leader, leader_e, leader_pack, party, facts, upkeep),
        };
        if let Some(h) = hazard.filter(|h| !died && !h.exempt && !upkeep.travelling) {
            let (pack, face) = if member == 0 {
                (&*leader_pack, h.leader_face)
            } else {
                let c = &party.companions[member - 1];
                (&c.pack, h.face_of(c.origin))
            };
            if !passes_daily_check(pack, h.danger, face, h.immune, facts) {
                needs_of(member, leader, party).health = 0;
                died = true;
                if member == 0 {
                    out.leader_hazard = true;
                }
            }
        }
        if member == 0 {
            if died {
                out.leader_died = true;
                if doctors.is_none() {
                    return out;
                }
                // `health = 0`, and what he carried is on the ground.
                leader.health = 0;
                let cap = leader_pack.capacity();
                out.leader_dropped = Some(core::mem::replace(leader_pack, ItemBag::new(cap)));
            }
            member += 1;
            continue;
        }
        if !died {
            member += 1;
            continue;
        }
        let mut who = party.companions.remove(member - 1);
        who.needs.health = 0;
        out.deaths.push(Death {
            id: who.id,
            origin: who.origin,
            at: None,
            dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
            camp_lost: false,
            food: who.needs.food,
            water: who.needs.water,
            position: who.at,
        });
        // v1.2's index moves on past whoever shifted into his place.
        if doctors.is_some() {
            member += 1;
        }
    }
    out
}

fn needs_of<'a>(member: usize, leader: &'a mut CharacterNeeds, party: &'a mut Party) -> &'a mut CharacterNeeds {
    if member == 0 { leader } else { &mut party.companions[member - 1].needs }
}

/// Heal, as both rules do, capped at a hundred.
fn heal(n: &mut CharacterNeeds, e: &mut Exposure, upkeep: Upkeep) {
    if whole(n.health, e) >= healing_threshold(upkeep.doctor, upkeep.threshold) {
        spend(&mut n.health, e);
        n.health = (n.health + if upkeep.doctor { 4 } else { 2 }).min(100);
    }
}

/// `TurnOriginalEmployed` for one of a travelling group.
fn original_member(
    member: usize,
    leader: &mut CharacterNeeds,
    leader_e: &mut Exposure,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
) -> bool {
    {
        let (n, e) = member_mut(member, leader, leader_e, party);
        heal(n, e, upkeep);
    }
    let pack = if member == 0 { &*leader_pack } else { &party.companions[member - 1].pack };
    if resting(pack, true, upkeep, facts) {
        return needs_of(member, leader, party).health <= 0;
    }
    // `ConsumeWater`: a travelling man has no reserve to draw on.
    let n = needs_of(member, leader, party);
    n.water -= 1;
    if n.water < 0 {
        let found = group_find_cheapest_water(leader_pack, &party.companions, facts);
        let refilled = found.and_then(|(whose, slot)| {
            let b = bag(leader_pack, party, whose)?;
            let kind = b.get(slot)?;
            let value = facts.water(kind);
            make_empty(b, slot, facts);
            Some(value)
        });
        let (n, e) = member_mut(member, leader, leader_e, party);
        match refilled {
            Some(v) => n.water = (n.water + v).min(5),
            None => {
                n.water = 0;
                spend(&mut n.health, e);
                n.health -= 25;
                if n.health <= 0 {
                    return true;
                }
            }
        }
    }
    // `ConsumeFood`: the group's smallest.
    let n = needs_of(member, leader, party);
    n.food -= 1;
    if n.food < 0 {
        let found = group_find_smallest_food(leader_pack, &party.companions, facts);
        let refilled = found.and_then(|(whose, slot)| {
            let b = bag(leader_pack, party, whose)?;
            let kind = b.get(slot)?;
            let value = facts.food(kind);
            b.remove(slot);
            Some(value)
        });
        let (n, e) = member_mut(member, leader, leader_e, party);
        match refilled {
            Some(v) => n.food = (n.food + v).min(n.max_food),
            None => {
                n.food = 0;
                spend(&mut n.health, e);
                n.health -= 25;
            }
        }
    }
    needs_of(member, leader, party).health <= 0
}

/// `TurnExtendedEmployed` for one of a travelling group.
fn extended_member(
    member: usize,
    leader: &mut CharacterNeeds,
    leader_e: &mut Exposure,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
) -> bool {
    let pack = if member == 0 { &*leader_pack } else { &party.companions[member - 1].pack };
    if resting(pack, true, upkeep, facts) {
        let (n, e) = member_mut(member, leader, leader_e, party);
        return extended_rest(n, e, upkeep);
    }
    if needs_of(member, leader, party).food == 0 {
        if let Some((whose, slot)) = group_find_smallest_food(leader_pack, &party.companions, facts) {
            let kind = bag(leader_pack, party, whose).and_then(|b| b.get(slot));
            if let Some(kind) = kind {
                // `group.Eat(null, value)`; `owner.Remove(item)` whatever was eaten.
                group_eat(leader, &mut party.companions, facts.food(kind));
                if let Some(b) = bag(leader_pack, party, whose) {
                    b.remove(slot);
                }
            }
        }
    }
    if needs_of(member, leader, party).water == 0 {
        if let Some((whose, slot)) = group_find_water(leader_pack, &party.companions, facts) {
            let kind = bag(leader_pack, party, whose).and_then(|b| b.get(slot));
            if let Some(kind) = kind {
                group_drink(leader, &mut party.companions, facts.water(kind));
                if let Some(b) = bag(leader_pack, party, whose) {
                    make_empty(b, slot, facts);
                }
            }
        }
    }
    let (n, e) = member_mut(member, leader, leader_e, party);
    extended_upkeep(n, e, upkeep)
}

/// A v1.2 game's rules for the locations' day (`World.Turn` → `Location.Turn`).
#[derive(Clone, Debug, PartialEq)]
pub struct V12World {
    pub survival: Survival,
    pub stabilization: i32,
    pub water_output: WaterOutput,
    pub food_production: crate::rules::FoodProduction,
    /// Each seat's own healing threshold (the Amiga opponent's 50), by player.
    pub thresholds: [Option<i32>; 4],
    /// Records of doctors: a camp with one of its own among its people has a
    /// doctor (`HasLocalDoctor`).
    pub doctors: Vec<usize>,
    /// Each location's danger, and the faces each danger does not touch,
    /// and everybody's face by record, for `ApplyDaily` in the camps.
    pub dangers: Vec<crate::gamdat::Danger>,
    pub gas_immune: Vec<i32>,
    pub radiation_immune: Vec<i32>,
    pub faces: Vec<(usize, u8)>,
    /// Records of technicians, `technician_food_bonus`, and which places
    /// are cities: the production bonus (`GetFoodProductionRate`).
    pub technicians: Vec<usize>,
    pub technician_bonus: i32,
    pub cities: Vec<bool>,
}

/// One location's day under v1.2 (`Location.Turn` after the spring): the
/// production chosen and fed to the camp's people, the rest stored; then
/// each camper's day. Those who die leave the camp, the place freed when
/// the last of them goes. Returns the dead.
pub fn location_turn<S: crate::game::Storage>(
    location: usize,
    camp: &mut crate::game::Camp,
    mut spring: Option<&mut crate::game::WaterSource>,
    storage: &mut S,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
) -> Vec<Death> {
    production_turn(location, camp, storage, productions, facts, v12);
    let mut fell = Vec::new();
    let mut i = 0;
    while i < camp.residents.len() {
        match camper_day(location, camp, i, spring.as_deref_mut(), storage, facts, v12) {
            Some(death) => fell.push(death),
            None => i += 1,
        }
    }
    fell
}

/// `foreach (Character npc in Characters) npc.Turn()` (`Location.cs:453-455`)
/// as v1.2's enumerator walks it: by index over the live list
/// (`StateLinkList.cs:20-45`). `turn` has one character's day and says
/// whether he left the list in it (a wandering trader walking on). Then the
/// one after him has shifted into his index and the walk moves past: **he
/// has no day**. Whoever is appended to another place's list is that
/// place's to turn — again, when its turn is still to come.
pub fn turn_in_order<K: Copy>(mut list: Vec<K>, mut turn: impl FnMut(K) -> bool) {
    let mut i = 0;
    while i < list.len() {
        if turn(list[i]) {
            list.remove(i);
        }
        i += 1;
    }
}

/// "fill up bottles" (`Location.cs:457-463`): every item in a spring room
/// goes through `WaterSource.RefillItem` (`WaterSource.cs:69-82`) — one whose
/// full form holds water is filled when the reserve covers that water, and
/// the reserve pays. It comes after the people's day and before the cap
/// (`Source.EndTurn`), so the whole night's output can go into bottles.
pub fn refill_spring_room(spring: &mut crate::game::WaterSource, room: &mut ItemBag, facts: &impl ItemFacts) {
    for slot in 0..room.len() {
        let _ = crate::game::fill_container(spring, room, slot, facts);
    }
}

/// `Location.Turn`'s production under v1.2, **for every location**, whoever
/// holds it: `RefreshFoodProductionSelection` (`Location.cs:384-391`), then
/// `ProcessFoodProduction` — the camp's own people fed first under
/// `dos_feed_then_store`, the rest towards the next item (`ProduceFood`,
/// `:393-403`). A place nobody holds makes nothing: its selection goes back
/// to automatic and its progress to nought, every day.
pub fn production_turn<S: crate::game::Storage>(
    location: usize,
    camp: &mut crate::game::Camp,
    storage: &mut S,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
) {
    use crate::rules::FoodProduction;
    let mut available = choose_production(location, camp, storage, productions, facts, v12);
    if camp.owner.is_some() && v12.food_production == FoodProduction::DosFeedThenStore {
        for who in camp.residents.iter_mut() {
            if available == 0 {
                break;
            }
            if who.needs.health > 0 && who.needs.food < who.needs.max_food {
                who.needs.food += 1;
                available -= 1;
            }
        }
    }
    produce_food(location, camp, storage, productions, facts, available);
}

/// One camper's day in his location's turn (`Character.Turn` →
/// `TurnEmployedCharacter`, then `HazardRules.ApplyDaily`), `i` among the
/// camp's residents. `HasLocalDoctor` is asked as the camp is at his turn
/// (`Character.cs:804-807`: a living doctor of the owner's camped there).
/// One who dies leaves the camp (`Die` → `Dismiss`), the place nobody's when
/// he was the last (`IsLastInCamp`); his death is returned.
pub fn camper_day<S: crate::game::Storage>(
    location: usize,
    camp: &mut crate::game::Camp,
    i: usize,
    spring: Option<&mut crate::game::WaterSource>,
    storage: &mut S,
    facts: &impl ItemFacts,
    v12: &V12World,
) -> Option<Death> {
    let doctor = camp.residents.iter().any(|r| r.needs.health > 0 && r.origin.is_some_and(|o| v12.doctors.contains(&o)));
    let threshold = camp.owner.and_then(|o| v12.thresholds.get(o).copied().flatten());
    let upkeep = Upkeep { survival: v12.survival, stabilization: v12.stabilization, doctor, threshold, travelling: false };
    let mut died = match v12.survival {
        Survival::DosDailyConsumption => original_camper(location, camp, i, spring, storage, facts, upkeep),
        Survival::RemasterSupplyPool => extended_camper(location, camp, i, spring, storage, facts, upkeep),
    };
    // `HazardRules.ApplyDaily`: a stationed man needs full protection.
    let danger = v12.dangers.get(location).copied().unwrap_or(crate::gamdat::Danger::None);
    if !died {
        let immune = match danger {
            crate::gamdat::Danger::Gas => v12.gas_immune.as_slice(),
            crate::gamdat::Danger::Radiation => v12.radiation_immune.as_slice(),
            crate::gamdat::Danger::None => &[],
        };
        let r = &camp.residents[i];
        let face = r.origin.and_then(|o| v12.faces.iter().find(|f| f.0 == o)).map_or(0, |f| f.1);
        if !passes_daily_check(&r.pack, danger, face, immune, facts) {
            camp.residents[i].needs.health = 0;
            died = true;
        }
    }
    if !died {
        return None;
    }
    let mut who = camp.residents.remove(i);
    who.needs.health = 0;
    // `Dismiss` of the last one frees the place (`IsLastInCamp`).
    let camp_lost = camp.is_empty();
    if camp_lost {
        camp.owner = None;
    }
    Some(Death {
        id: who.id,
        origin: who.origin,
        at: Some(location),
        dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
        camp_lost,
        food: who.needs.food,
        water: who.needs.water,
        position: who.at,
    })
}

/// `Location.GetFoodProductionRate`: nought for nobody's place; else the
/// production's rate from the tools in the rooms and — where it allows —
/// in the campers' packs (`GetProductionToolCount`), with the technician's
/// bonus when a living one of the owner's is camped there and it is no city
/// (`technician_food_bonus`; nought in every preset v1.2 ships). The one
/// figure the camp's day, the radio's report and an opponent's plans read.
pub fn food_rate(
    location: usize,
    camp: &crate::game::Camp,
    storage: &impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
    index: usize,
) -> i32 {
    if camp.owner.is_none() {
        return 0;
    }
    rate_of(location, camp, storage, productions, facts, v12, index)
}

/// `Production.Rate.MaintenanceBonus`: the technician's bonus, when a
/// living one of the owner's is camped there and it is no city. Part of
/// [`food_rate`]; INFO shows it apart (`x+bonus`).
pub fn maintenance_bonus(location: usize, camp: &crate::game::Camp, v12: &V12World) -> i32 {
    let city = v12.cities.get(location).copied().unwrap_or(false);
    let technician = !city
        && camp.residents.iter().any(|r| r.needs.health > 0 && r.origin.is_some_and(|o| v12.technicians.contains(&o)));
    if technician { v12.technician_bonus } else { 0 }
}

fn rate_of(
    location: usize,
    camp: &crate::game::Camp,
    storage: &impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
    index: usize,
) -> i32 {
    let Some(p) = productions.get(index) else { return 0 };
    let Some(produce) = facts.kind_named(&p.produce) else { return 0 };
    let carried = if p.allow_inventory {
        camp.residents
            .iter()
            .flat_map(|r| r.pack.iter())
            .filter(|&k| facts.name_of(k).is_some_and(|n| p.worked_by(n)))
            .count()
    } else {
        0
    };
    let bonus = maintenance_bonus(location, camp, v12);
    p.rate_with(storage.tools(location, index) + carried, camp.head_count(), facts.food(produce), bonus).food_per_day
}

/// `RefreshFoodProductionSelection`: a player's own choice stands while it
/// makes anything; otherwise the best of those the place can make
/// ([`auto_select_food_production`]). Nobody's place makes nothing
/// (`Player is null`).
fn choose_production(
    location: usize,
    camp: &mut crate::game::Camp,
    storage: &impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
) -> i32 {
    // Nobody's place makes nothing (`Player is null`), so the selection
    // goes back to automatic, and the automatic one finds nothing better.
    let current = match camp.owner {
        Some(_) => camp.production.map_or(0, |p| rate_of(location, camp, storage, productions, facts, v12, p)),
        None => 0,
    };
    if camp.manual_production && current > 0 {
        return current;
    }
    camp.manual_production = false;
    auto_select_food_production(location, camp, storage, productions, facts, v12, false)
}

/// `Location.RefreshFoodProductionSelection` on its own, as v1.2's
/// inventory calls it when a production's tool is put into or taken out of
/// one of the place's rooms, or something is built there
/// (`InventoryScene.cs:127, 638, 758`): the player's choice kept while it
/// makes anything, else the best. The rate it settles on.
pub fn refresh_production(
    location: usize,
    camp: &mut crate::game::Camp,
    storage: &impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
) -> i32 {
    choose_production(location, camp, storage, productions, facts, v12)
}

/// `Location.AutoSelectFoodProduction`: the production making most food a
/// day, only a strictly better one replacing the current — with
/// `only_if_nothing`, only when the current makes nothing. It leaves
/// `manual_production` as it is. The opponent's production policy
/// (`CampManagement.MaintainCampProductionPolicy`) is this, every turn, for
/// each of its camps. The rate it settles on.
pub fn auto_select_food_production(
    location: usize,
    camp: &mut crate::game::Camp,
    storage: &impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    v12: &V12World,
    only_if_nothing: bool,
) -> i32 {
    if camp.owner.is_none() {
        return 0;
    }
    let mut best = camp.production.map_or(0, |p| rate_of(location, camp, storage, productions, facts, v12, p));
    if best > 0 && only_if_nothing {
        return best;
    }
    // v1.3's `GetAutomaticFoodProduction` (`36ecfc3`, "prefer rat trap
    // before knifes with 2 NPCs"): an equal positive rate goes to the
    // production with the higher `ID` — its place in `production.txt`, which
    // is this index (`GameCreation.LoadProductions` numbers them in order).
    // v1.2 kept the first found.
    let mut selected = camp.production;
    for candidate in storage.valid_productions(location) {
        let r = rate_of(location, camp, storage, productions, facts, v12, candidate);
        if r > best || (r == best && r > 0 && selected.is_none_or(|s| candidate > s)) {
            selected = Some(candidate);
            best = r;
        }
    }
    camp.production = selected;
    best
}

/// `Location.ProduceFood`: food value accumulated towards the next item,
/// kept at nought while there is nothing to make it with, no room, or six
/// already in stock.
fn produce_food(
    location: usize,
    camp: &mut crate::game::Camp,
    storage: &mut impl crate::game::Storage,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    food_value: i32,
) {
    let produced = camp.production.and_then(|i| productions.get(i).map(|p| (i, p)));
    let Some((index, production)) = produced else {
        camp.progress = 0.0;
        return;
    };
    let Some(kind) = facts.kind_named(&production.produce) else {
        camp.progress = 0.0;
        return;
    };
    if food_value <= 0
        || storage.count_in_rooms(location, kind) >= crate::game::MAX_STOCK_FOOD
        || !storage.any_room_has_space(location)
    {
        camp.progress = 0.0;
        return;
    }
    camp.progress += food_value as f32;
    let worth = facts.food(kind) as f32;
    if camp.progress >= worth {
        camp.progress -= worth;
        storage.store_near_tool(location, kind, index);
    }
}

/// A camper's larder, v1.2: `(owner, slot, kind)` where owner `None` is a
/// room (then `slot` is `(room, slot)` packed) — kept simple below.
enum Held {
    Room(usize, usize),
    Camper(usize, usize),
}

/// `TurnOriginalEmployed` for a camper.
fn original_camper(
    location: usize,
    camp: &mut crate::game::Camp,
    i: usize,
    spring: Option<&mut crate::game::WaterSource>,
    storage: &mut impl crate::game::Storage,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
) -> bool {
    {
        let r = &mut camp.residents[i];
        heal(&mut r.needs, &mut r.exposure, upkeep);
    }
    if resting(&camp.residents[i].pack, false, upkeep, facts) {
        return camp.residents[i].needs.health <= 0;
    }
    // `ConsumeWater`: at home, the reserve first; then the cheapest water
    // in the rooms and in the packs of the camp's own people.
    camp.residents[i].needs.water -= 1;
    if camp.residents[i].needs.water < 0 {
        let mut refilled = false;
        if let Some(s) = spring {
            if s.reserve > 0 {
                s.reserve -= 1;
                camp.residents[i].needs.water = 0;
                refilled = true;
            }
        }
        if !refilled {
            let mut best: Option<(Held, i32, String)> = None;
            let consider = |h: Held, kind: usize, best: &mut Option<(Held, i32, String)>| {
                let v = facts.water(kind);
                if v <= 0 {
                    return;
                }
                let name = String::from(facts.name_of(kind).unwrap_or(""));
                if best.as_ref().is_none_or(|(_, b, n)| v < *b || (v == *b && name < *n)) {
                    *best = Some((h, v, name));
                }
            };
            for (room, slot, kind) in storage.room_items(location) {
                consider(Held::Room(room, slot), kind, &mut best);
            }
            for (c, r) in camp.residents.iter().enumerate() {
                for (slot, kind) in r.pack.iter().enumerate() {
                    consider(Held::Camper(c, slot), kind, &mut best);
                }
            }
            if let Some((held, value, _)) = best {
                match held {
                    Held::Room(room, slot) => {
                        let empty = storage.room_items(location).into_iter().find(|&(r, s, _)| r == room && s == slot).and_then(|(_, _, k)| facts.empty(k));
                        match empty {
                            Some(e) => storage.replace_in_room(location, room, slot, e),
                            None => {
                                storage.take_from_room(location, room, slot);
                            }
                        }
                    }
                    Held::Camper(c, slot) => make_empty(&mut camp.residents[c].pack, slot, facts),
                }
                let n = &mut camp.residents[i].needs;
                n.water = (n.water + value).min(5);
                refilled = true;
            }
        }
        if !refilled {
            let r = &mut camp.residents[i];
            let n = &mut r.needs;
            n.water = 0;
            spend(&mut n.health, &mut r.exposure);
            n.health -= 25;
            if n.health <= 0 {
                return true;
            }
        }
    }
    // `ConsumeFood`: his own pack (his group is himself), then the rooms.
    camp.residents[i].needs.food -= 1;
    if camp.residents[i].needs.food < 0 {
        let value = take_smallest_food(location, &mut camp.residents[i].pack, storage, facts);
        let r = &mut camp.residents[i];
        let n = &mut r.needs;
        match value {
            Some(v) => n.food = (n.food + v).min(n.max_food),
            None => {
                n.food = 0;
                spend(&mut n.health, &mut r.exposure);
                n.health -= 25;
            }
        }
    }
    camp.residents[i].needs.health <= 0
}

/// `FindAccessibleFood` for a camper: the smallest food in his own pack,
/// else in the rooms. Taken, and its value returned.
fn take_smallest_food(location: usize, pack: &mut ItemBag, storage: &mut impl crate::game::Storage, facts: &impl ItemFacts) -> Option<i32> {
    let own = (0..pack.len())
        .filter_map(|s| pack.get(s).map(|k| (s, facts.food(k))))
        .filter(|&(_, v)| v > 0)
        .fold(None, |best: Option<(usize, i32)>, (s, v)| if best.is_none_or(|(_, b)| v < b) { Some((s, v)) } else { best });
    if let Some((slot, v)) = own {
        pack.remove(slot);
        return Some(v);
    }
    let room = storage
        .room_items(location)
        .into_iter()
        .map(|(r, s, k)| (r, s, facts.food(k)))
        .filter(|&(_, _, v)| v > 0)
        .fold(None, |best: Option<(usize, usize, i32)>, (r, s, v)| if best.is_none_or(|(_, _, b)| v < b) { Some((r, s, v)) } else { best });
    let (r, s, v) = room?;
    storage.take_from_room(location, r, s);
    Some(v)
}

/// `TurnExtendedEmployed` for a camper: his group is himself.
fn extended_camper(
    location: usize,
    camp: &mut crate::game::Camp,
    i: usize,
    spring: Option<&mut crate::game::WaterSource>,
    storage: &mut impl crate::game::Storage,
    facts: &impl ItemFacts,
    upkeep: Upkeep,
) -> bool {
    if resting(&camp.residents[i].pack, false, upkeep, facts) {
        let r = &mut camp.residents[i];
        return extended_rest(&mut r.needs, &mut r.exposure, upkeep);
    }
    if camp.residents[i].needs.food == 0 {
        if let Some(v) = take_smallest_food(location, &mut camp.residents[i].pack, storage, facts) {
            let n = &mut camp.residents[i].needs;
            n.food = (n.food + v).min(n.max_food);
        }
    }
    // `Source.Reserve = group.Drink(null, Source.Reserve)`: he drinks his
    // fill from the spring.
    if let Some(s) = spring {
        let n = &mut camp.residents[i].needs;
        let take = (n.max_water - n.water).clamp(0, s.reserve.max(0));
        n.water += take;
        s.reserve -= take;
    }
    if camp.residents[i].needs.water == 0 {
        // The rooms' smallest water, else his own largest.
        let room = storage
            .room_items(location)
            .into_iter()
            .map(|(r, s, k)| (r, s, k, facts.water(k)))
            .filter(|&(_, _, _, v)| v > 0)
            .fold(None, |best: Option<(usize, usize, usize, i32)>, x| if best.is_none_or(|b| x.3 < b.3) { Some(x) } else { best });
        let value = match room {
            Some((r, s, k, v)) => {
                match facts.empty(k) {
                    Some(e) => storage.replace_in_room(location, r, s, e),
                    None => {
                        storage.take_from_room(location, r, s);
                    }
                }
                Some(v)
            }
            None => {
                let pack = &mut camp.residents[i].pack;
                let own = (0..pack.len())
                    .filter_map(|s| pack.get(s).map(|k| (s, facts.water(k))))
                    .filter(|&(_, v)| v != 0)
                    .fold(None, |best: Option<(usize, i32)>, (s, v)| if best.is_none_or(|(_, b)| v > b) { Some((s, v)) } else { best });
                own.map(|(s, v)| {
                    make_empty(pack, s, facts);
                    v
                })
            }
        };
        if let Some(v) = value {
            let n = &mut camp.residents[i].needs;
            n.water = (n.water + v).min(n.max_water);
        }
    }
    let r = &mut camp.residents[i];
    extended_upkeep(&mut r.needs, &mut r.exposure, upkeep)
}
