//! Who carries which weapon and which armour: `EquipmentPlanning` and
//! `EquipmentNeeds`. At tag `v1.2`, `AI/PlayerBehavior/Group/` and
//! `Economy/`.
//!
//! The recipients are the group, then the guards of the opponent's
//! threatened camps, the most experienced first. Guns go only to the group,
//! within `firearm_limit`; armour moves only under Extended's rules, within
//! `armour_limit`, through the reserve. Items move as they are, rounds and
//! all ([`crate::game::move_item`]); a slot chosen as weapon or protection
//! follows its item.

use alloc::vec::Vec;

use crate::combat::Kit;
use crate::game::ItemBag;
use crate::items::ItemTypes;
use crate::table_combat::{self as tc, Tables as _};

use super::policy::AiPolicy;
use super::state::ModernAi;

/// `AiItemPool.IsFirearm`.
pub fn is_firearm(types: &ItemTypes, kind: usize) -> bool {
    matches!(
        types.name_of(kind),
        Some("item_loaded_rifle" | "item_loaded_rifle_1" | "item_loaded_pistol" | "item_unloaded_rifle" | "item_unloaded_pistol")
    )
}

/// `EquipmentNeeds.IsEquipment`.
pub fn is_equipment(types: &ItemTypes, kind: usize) -> bool {
    types.damage_of(kind) > 0 || types.defense_of(kind) > 0 || is_firearm(types, kind) || types.name_of(kind) == Some("item_ammunition")
}

/// `LoadedShots`: what is in the gun, then every full load down its chain
/// of emptier forms.
pub fn loaded_shots(types: &ItemTypes, kind: usize, current: Option<i32>) -> i32 {
    let mut shots = 0;
    let mut seen = Vec::new();
    let mut kind = Some(kind);
    let mut current = current;
    while let Some(k) = kind {
        if types.ammo_of(k) <= 0 || seen.contains(&k) {
            break;
        }
        seen.push(k);
        shots += current.take().unwrap_or(types.ammo_of(k));
        kind = types.empty_of(k);
    }
    shots
}

/// Somebody who may be given equipment.
pub struct Member<'a> {
    pub pack: &'a mut ItemBag,
    pub kit: &'a mut Kit,
    pub experience: i32,
    /// In the opponent's group, not a guard in a camp.
    pub in_party: bool,
    pub location: usize,
}

fn has_firearm(types: &ItemTypes, pack: &ItemBag) -> bool {
    pack.iter().any(|k| is_firearm(types, k))
}

/// `CanAssignFirearm`.
pub fn can_assign_firearm(types: &ItemTypes, recipient: usize, unit: &[Member], kind: usize, policy: &AiPolicy) -> bool {
    loaded_shots(types, kind, None) > 0
        && (policy.firearm_limit < 0
            || (unit.iter().enumerate().filter(|(i, m)| *i != recipient && has_firearm(types, m.pack)).count() as i32) < policy.firearm_limit)
}

/// Take a slot out of a pack, keeping the kit pointing at the same items.
/// `(kind, rounds)`.
pub fn take(pack: &mut ItemBag, kit: &mut Kit, slot: usize) -> Option<(usize, Option<i32>)> {
    let rounds = pack.rounds_left(slot);
    let kind = pack.remove(slot)?;
    let shift = |s: &mut Option<usize>| match *s {
        Some(x) if x == slot => *s = None,
        Some(x) if x > slot => *s = Some(x - 1),
        _ => {}
    };
    shift(&mut kit.weapon);
    shift(&mut kit.protection);
    Some((kind, rounds))
}

/// Put an item into a pack; its slot, or none when full.
pub fn give(pack: &mut ItemBag, kind: usize, rounds: Option<i32>) -> Option<usize> {
    if !pack.push(kind) {
        return None;
    }
    let slot = pack.len() - 1;
    pack.set_rounds_left(slot, rounds);
    Some(slot)
}

fn best_weapon(types: &ItemTypes, pack: &ItemBag) -> Option<usize> {
    tc::find_best_weapon(pack, None, types)
}

fn best_defense(types: &ItemTypes, pack: &ItemBag) -> Option<usize> {
    tc::find_best_defense(pack, None, types)
}

/// `DistributeFirearms`: each of the group, in order, takes the best spare
/// gun another carries that beats his own weapon and is loaded, within the
/// limit; his old weapon goes back the other way.
pub fn distribute_firearms(types: &ItemTypes, party: &mut [Member], policy: &AiPolicy) {
    for r in 0..party.len() {
        let current = best_weapon(types, party[r].pack);
        if party[r].pack.is_full() && current.is_none() {
            continue;
        }
        let current_priority = current.and_then(|s| party[r].pack.get(s)).map_or(0, |k| types.weapon_priority(k));
        // OrderByDescending priority: the first of the highest.
        let mut best: Option<(usize, usize, i32)> = None;
        for o in 0..party.len() {
            if o == r {
                continue;
            }
            let owner_best = best_weapon(types, party[o].pack);
            for slot in 0..party[o].pack.len() {
                let Some(k) = party[o].pack.get(slot) else { continue };
                let priority = types.weapon_priority(k);
                if is_firearm(types, k)
                    && Some(slot) != owner_best
                    && priority > current_priority
                    && loaded_shots(types, k, party[o].pack.rounds_left(slot)) > 0
                    && can_assign_firearm(types, r, party, k, policy)
                    && best.is_none_or(|(_, _, p)| priority > p)
                {
                    best = Some((o, slot, priority));
                }
            }
        }
        let Some((o, slot, _)) = best else { continue };
        let Some((gun, rounds)) = take(party[o].pack, party[o].kit, slot) else { continue };
        if let Some(c) = current {
            if let Some((old, old_rounds)) = take(party[r].pack, party[r].kit, c) {
                give(party[o].pack, old, old_rounds);
            }
        }
        let at = give(party[r].pack, gun, rounds);
        party[r].kit.weapon = at;
        party[o].kit.weapon = best_weapon(types, party[o].pack);
    }
}

/// `DistributeClothing`: each recipient in order takes the best armour a
/// later one carries that beats his own, within the limit.
pub fn distribute_clothing(types: &ItemTypes, recipients: &mut [Member], armour_limit: i32) {
    for r in 0..recipients.len() {
        let current = best_defense(types, recipients[r].pack);
        if recipients[r].pack.is_full() && current.is_none() {
            continue;
        }
        let current_defense = current.and_then(|s| recipients[r].pack.get(s)).map_or(0, |k| types.defense_of(k));
        let mut best: Option<(usize, usize, i32)> = None;
        for o in r + 1..recipients.len() {
            for slot in 0..recipients[o].pack.len() {
                let Some(k) = recipients[o].pack.get(slot) else { continue };
                let d = types.defense_of(k);
                if d > current_defense && d <= armour_limit && best.is_none_or(|(_, _, b)| d > b) {
                    best = Some((o, slot, d));
                }
            }
        }
        let Some((o, slot, _)) = best else { continue };
        let was_protection = recipients[o].kit.protection == Some(slot);
        let Some((cloth, rounds)) = take(recipients[o].pack, recipients[o].kit, slot) else { continue };
        if let Some(c) = current {
            if let Some((old, old_rounds)) = take(recipients[r].pack, recipients[r].kit, c) {
                give(recipients[o].pack, old, old_rounds);
            }
        }
        let at = give(recipients[r].pack, cloth, rounds);
        recipients[r].kit.protection = at;
        if was_protection {
            recipients[o].kit.protection = best_defense(types, recipients[o].pack);
        }
    }
}

/// `AiItemPool.Accepts`.
pub fn reserve_accepts(types: &ItemTypes, kind: usize, items: super::group::Items) -> bool {
    const FILTER: [&str; 11] = [
        "item_knife",
        "item_axe",
        "item_pitchfork",
        "item_loaded_rifle",
        "item_loaded_pistol",
        "item_gas_mask",
        "item_protection_suit",
        "item_paper_helmet",
        "item_rat_trap",
        "item_snake_trap",
        "item_trap",
    ];
    !is_firearm(types, kind)
        && (types.name_of(kind).is_some_and(|n| FILTER.contains(&n)) || items.is_water_container(kind) || types.defense_of(kind) > 0)
}

/// `AiItemPool.Insert`.
pub fn reserve_insert(ai: &mut ModernAi, items: super::group::Items, kind: usize) -> bool {
    reserve_accepts(items.types, kind, items) && ai.reserve_add(kind)
}

/// What [`maintain`] did that the world must see to: things that went
/// neither to somebody nor into the reserve (`StoreInReserveOrAtLocation`'s
/// camp or ground), by the member's location.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Equipped {
    pub left: Vec<(usize, usize, Option<i32>)>,
    /// `(member, kind)` given out of the reserve.
    pub from_reserve: Vec<(usize, usize)>,
}

/// `EquipmentPlanning.Maintain`. `recipients` are the group first, then the
/// threatened camps' guards; `camp_rooms` are the rooms of the opponent's
/// camps where recipients stand, whose armour within the limit goes into the
/// reserve.
pub fn maintain(
    items: super::group::Items,
    recipients: &mut [Member],
    ai: &mut ModernAi,
    extended: bool,
    policy: &AiPolicy,
    camp_rooms: &mut [&mut ItemBag],
) -> Equipped {
    let types = items.types;
    let mut out = Equipped::default();
    let party = recipients.iter().take_while(|m| m.in_party).count();
    distribute_firearms(types, &mut recipients[..party], policy);
    let ammunition = types.index_of("item_ammunition");
    for r in 0..recipients.len() {
        let gun = (0..recipients[r].pack.len()).find(|&s| recipients[r].pack.get(s).is_some_and(|k| is_firearm(types, k)));
        if let Some(mut gun) = gun {
            let mut ammo = (0..recipients[r].pack.len()).find(|&s| recipients[r].pack.get(s) == ammunition && ammunition.is_some());
            if ammo.is_none() && !recipients[r].pack.is_full() {
                let here = recipients[r].location;
                let donor = (0..party).find(|&d| {
                    d != r
                        && recipients[d].location == here
                        && !has_firearm(types, recipients[d].pack)
                        && recipients[d].pack.iter().any(|k| Some(k) == ammunition)
                });
                if let (Some(d), Some(a)) = (donor, ammunition) {
                    let slot = (0..recipients[d].pack.len()).find(|&s| recipients[d].pack.get(s) == Some(a));
                    if let Some(slot) = slot {
                        if let Some((k, rounds)) = take(recipients[d].pack, recipients[d].kit, slot) {
                            ammo = give(recipients[r].pack, k, rounds);
                        }
                    }
                } else if let Some(a) = ammunition.filter(|&a| ai.reserve_count(a) > 0) {
                    ai.reserve_take(a);
                    ammo = give(recipients[r].pack, a, None);
                    out.from_reserve.push((r, a));
                }
            }
            let gun_kind = recipients[r].pack.get(gun);
            if let (Some(g), Some(a)) = (gun_kind, ammo) {
                if types.damage_of(g) == 0 {
                    let loaded = if types.name_of(g) == Some("item_unloaded_pistol") { "item_loaded_pistol" } else { "item_loaded_rifle" };
                    if let Some(l) = types.index_of(loaded) {
                        take(recipients[r].pack, recipients[r].kit, a);
                        if a < gun {
                            gun -= 1;
                        }
                        recipients[r].pack.replace(gun, l);
                        recipients[r].pack.set_rounds_left(gun, Some(types.ammo_of(l)));
                    }
                }
            }
        }
        recipients[r].kit.weapon = best_weapon(types, recipients[r].pack);
    }
    if !extended {
        return out;
    }

    // Spare clothing to the reserve, or left where its wearer stands.
    for r in 0..recipients.len() {
        let pack = &*recipients[r].pack;
        let keep_kind_slot = (0..pack.len())
            .filter(|&s| pack.get(s).is_some_and(|k| types.defense_of(k) > 0 && types.defense_of(k) <= policy.armour_limit))
            .fold(None, |b: Option<usize>, s| match b {
                Some(b) if types.defense_of(pack.get(s).unwrap_or(0)) <= types.defense_of(pack.get(b).unwrap_or(0)) => Some(b),
                _ => Some(s),
            });
        let mut slot = recipients[r].pack.len();
        let mut keep = keep_kind_slot;
        while slot > 0 {
            slot -= 1;
            let Some(k) = recipients[r].pack.get(slot) else { continue };
            if types.defense_of(k) <= 0 || Some(slot) == keep {
                continue;
            }
            let was_protection = recipients[r].kit.protection == Some(slot);
            let Some((kind, rounds)) = take(recipients[r].pack, recipients[r].kit, slot) else { continue };
            if let Some(kk) = keep {
                if kk > slot {
                    keep = Some(kk - 1);
                }
            }
            if was_protection {
                recipients[r].kit.protection = keep;
            }
            if !reserve_insert(ai, items, kind) {
                out.left.push((recipients[r].location, kind, rounds));
            }
        }
    }
    // Clothing stored at those camps joins the reserve.
    for room in camp_rooms.iter_mut() {
        let mut slot = room.len();
        while slot > 0 {
            slot -= 1;
            let Some(k) = room.get(slot) else { continue };
            if types.defense_of(k) > 0 && types.defense_of(k) <= policy.armour_limit && reserve_insert(ai, items, k) {
                room.remove(slot);
            }
        }
    }
    distribute_clothing(types, recipients, policy.armour_limit);
    for r in 0..recipients.len() {
        let current = best_defense(types, recipients[r].pack);
        if recipients[r].pack.is_full() && current.is_none() {
            continue;
        }
        let have = current.and_then(|s| recipients[r].pack.get(s)).map_or(0, |k| types.defense_of(k));
        let upgrade = ai
            .reserve
            .iter()
            .filter(|&&(k, n)| n > 0 && types.defense_of(k) > have && types.defense_of(k) <= policy.armour_limit)
            .fold(None, |b: Option<usize>, &(k, _)| match b {
                Some(b) if types.defense_of(k) <= types.defense_of(b) => Some(b),
                _ => Some(k),
            });
        let Some(u) = upgrade else { continue };
        if !ai.reserve_take(u) {
            continue;
        }
        if let Some(c) = current {
            if let Some((old, rounds)) = take(recipients[r].pack, recipients[r].kit, c) {
                if !reserve_insert(ai, items, old) {
                    out.left.push((recipients[r].location, old, rounds));
                }
            }
        }
        recipients[r].kit.protection = give(recipients[r].pack, u, None);
        out.from_reserve.push((r, u));
    }
    out
}

/// `EquipmentNeeds.Opportunity`: a chance that holds for the whole day for
/// that player and item — a hash, not a draw.
pub fn opportunity(day: i32, player: usize, id: &str, chance: i32) -> bool {
    let mut hash = (day as u32).wrapping_mul(397).wrapping_add((player as u32).wrapping_mul(7919));
    for c in id.encode_utf16() {
        hash = (hash ^ c as u32).wrapping_mul(16_777_619);
    }
    (hash % 100) < chance.clamp(0, 100) as u32
}

/// `EquipmentNeeds.Demand`: how many more of `kind` the recipients want,
/// less what the reserve and their spare carrying already cover.
pub fn demand(types: &ItemTypes, recipients: &[Member], ai: &ModernAi, policy: &AiPolicy, extended: bool, day: i32, player: usize, kind: usize) -> i32 {
    let name = types.name_of(kind).unwrap_or("");
    if name == "item_ammunition" {
        let guns: usize = recipients.iter().map(|m| m.pack.iter().filter(|&k| is_firearm(types, k)).count()).sum();
        let ammo: usize = recipients.iter().map(|m| m.pack.iter().filter(|&k| k == kind).count()).sum::<usize>() + ai.reserve_count(kind) as usize;
        return (guns as i32 - ammo as i32).max(0);
    }
    let (damage, defense) = (types.damage_of(kind), types.defense_of(kind));
    if damage <= 0 && defense <= 0 {
        return 0;
    }
    let armour = defense > 0;
    if armour && (!extended || defense > policy.armour_limit) {
        return 0;
    }
    let firearm = is_firearm(types, kind);
    let party: Vec<usize> = (0..recipients.len()).filter(|&i| recipients[i].in_party).collect();
    let mut wanted = recipients
        .iter()
        .enumerate()
        .filter(|(i, m)| {
            if firearm && !m.in_party {
                return false;
            }
            let current = if armour { best_defense(types, m.pack) } else { best_weapon(types, m.pack) };
            if m.pack.is_full() && current.is_none() {
                return false;
            }
            let ck = current.and_then(|s| m.pack.get(s));
            let have = if armour { ck.map_or(0, |k| types.defense_of(k)) } else { ck.map_or(0, |k| types.weapon_priority(k)) };
            let offered = if armour { defense } else { types.weapon_priority(kind) };
            if have >= offered {
                return false;
            }
            if firearm && !can_assign_firearm(types, *i, recipients, kind, policy) {
                return false;
            }
            let chance = if firearm {
                policy.firearm_purchase_chance
            } else if !armour && current.is_none() && (ai.difficulty > 0 || name == "item_knife") {
                100
            } else {
                policy.equipment_upgrade_chance
            };
            opportunity(day, player, name, chance)
        })
        .count() as i32;
    if firearm && policy.firearm_limit >= 0 {
        let armed = party.iter().filter(|&&i| has_firearm(types, recipients[i].pack)).count() as i32;
        wanted = wanted.min((policy.firearm_limit - armed).max(0));
    }
    let reserve = ai.reserve.iter().filter(|(k, _)| *k == kind).map(|(_, n)| *n as i32).sum::<i32>();
    let spare: i32 = recipients
        .iter()
        .map(|m| {
            let best = if armour { best_defense(types, m.pack) } else { best_weapon(types, m.pack) };
            (0..m.pack.len()).filter(|&s| m.pack.get(s) == Some(kind) && Some(s) != best).count() as i32
        })
        .sum();
    (wanted - reserve - spare).max(0)
}

/// [`demand`] for every kind of the table, indexed by kind.
pub fn demands(types: &ItemTypes, recipients: &[Member], ai: &ModernAi, policy: &AiPolicy, extended: bool, day: i32, player: usize) -> Vec<i32> {
    (0..types.len()).map(|k| demand(types, recipients, ai, policy, extended, day, player, k)).collect()
}
