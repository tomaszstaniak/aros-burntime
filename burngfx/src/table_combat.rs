//! v1.2's table combat: a blow is one of four numbers from a table.
//!
//! Every v1.2 rule set fights this way (`combat = dos_unarmoured` or
//! `remaster_armour`, `Logic/Rules/TableCombat.cs`, `RuleFormulas.cs`). A
//! weapon's `damage` is sixteen values — four rolls for each of four
//! experience tiers, lowest tier first — and a blow picks the attacker's tier
//! by combat experience and one of its four rolls at random. There is **no
//! difficulty factor**: difficulty only chooses the creatures' own tables.
//! Under `remaster_armour` the defender's protection then takes a percentage
//! off.
//!
//! This module is the arithmetic and the item choices, pure and testable; the
//! random roll index is passed in. The encounter — who joins, reach,
//! retaliation, fleeing — is `LocalCombatEncounter`, separate.
//!
//! The v0.9.1 formula (`crate::combat`) is untouched: it is the legacy
//! engine's, and v1.2 ships nothing that uses it.

use crate::combat::{Kit, Weapons};
use crate::game::ItemBag;
use crate::rules::RuleBook;

/// What a table fight needs of the item table beyond [`Weapons`].
pub trait Tables: Weapons {
    /// The `damage` line as a table: empty, one value, or sixteen.
    fn damage_table(&self, kind: usize) -> &[i32];
    /// `WeaponPriority`: the line, or `DamageValue`.
    fn weapon_priority(&self, kind: usize) -> i32;
    /// A kind by its section name (`item_ammunition`, `item_unloaded_rifle`).
    fn kind_named(&self, name: &str) -> Option<usize>;
    /// A bare hand's table (`ItemTypes.UnarmedDamage`).
    fn unarmed(&self) -> &[i32];
}

impl Tables for crate::items::ItemTypes {
    fn damage_table(&self, kind: usize) -> &[i32] {
        self.get(kind).map_or(&[][..], |t| &t.damage_table[..])
    }
    fn weapon_priority(&self, kind: usize) -> i32 {
        self.weapon_priority_of(kind)
    }
    fn kind_named(&self, name: &str) -> Option<usize> {
        self.index_of(name)
    }
    fn unarmed(&self) -> &[i32] {
        crate::items::ItemTypes::unarmed(self)
    }
}

/// Why a table cannot be rolled on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    /// Not one value, four rolls, or four tiers of four
    /// (`OriginalDamageTable` throws).
    BadLength(usize),
    /// `experience_tier_width` is not positive (`ExperienceTier` throws).
    BadTierWidth(i32),
}

/// `CombatExperience`: a character outside the fight classes fights with
/// half his experience.
pub fn combat_experience(fight_class: bool, experience: i32) -> i32 {
    if fight_class { experience } else { experience / 2 }
}

/// `ExperienceTier`: `experience / width`, clamped to the four tiers.
pub fn experience_tier(combat_experience: i32, tier_width: i32) -> Result<usize, TableError> {
    if tier_width <= 0 {
        return Err(TableError::BadTierWidth(tier_width));
    }
    Ok((combat_experience / tier_width).clamp(0, 3) as usize)
}

/// `OriginalDamageTable`: the four rolls that apply. A table of one or four
/// is used as it is; sixteen is sliced by tier.
pub fn damage_row(table: &[i32], combat_experience: i32, tier_width: i32) -> Result<&[i32], TableError> {
    match table.len() {
        1 | 4 => Ok(table),
        16 => {
            let tier = experience_tier(combat_experience, tier_width)?;
            Ok(&table[tier * 4..tier * 4 + 4])
        }
        n => Err(TableError::BadLength(n)),
    }
}

/// `OriginalDamage`: roll `roll` (0 to 3) of the row. A one-value table
/// ignores the roll.
pub fn damage(table: &[i32], combat_experience: i32, tier_width: i32, roll: usize) -> Result<i32, TableError> {
    let row = damage_row(table, combat_experience, tier_width)?;
    Ok(if table.len() == 1 { row[0] } else { row[roll.min(3)] })
}

/// `OriginalCombatPreview`: the least and the most a blow can do.
pub fn preview(table: &[i32], combat_experience: i32, tier_width: i32) -> Result<(i32, i32), TableError> {
    let row = damage_row(table, combat_experience, tier_width)?;
    let min = row.iter().copied().min().unwrap_or(0);
    let max = row.iter().copied().max().unwrap_or(0);
    Ok((min, max))
}

/// `OriginalStrategicStrength`: the row's average plus a health term
/// (`health / 10` when detailed, else 10).
pub fn strategic_strength(
    table: &[i32],
    combat_experience: i32,
    tier_width: i32,
    health: i32,
    detailed: bool,
) -> Result<f32, TableError> {
    let row = damage_row(table, combat_experience, tier_width)?;
    let sum: i32 = row.iter().sum();
    Ok(sum as f32 / row.len() as f32 + if detailed { health as f32 / 10.0 } else { 10.0 })
}

/// `ApplyArmour`: take `protection` percent off, rounded half away from zero,
/// never below one for a blow that did anything.
pub fn apply_armour(damage: i32, protection_percent: i32) -> i32 {
    if damage <= 0 {
        return 0;
    }
    let keep = 100 - protection_percent.clamp(0, 100);
    // Math.Round(damage * keep / 100f, AwayFromZero), in integers: the
    // product is non-negative, so half-up is away from zero.
    let scaled = (damage * keep * 2 + 100) / 200;
    scaled.max(1)
}

/// A creature's four rolls at a difficulty (`GetTraderAttack` …). `None` for
/// anyone who fights with a weapon.
pub fn creature_table(book: &RuleBook, class: crate::gamdat::CharClass, difficulty: i32) -> Option<&[i32]> {
    use crate::gamdat::CharClass;
    let d = &book.difficulty[difficulty.clamp(0, 2) as usize];
    match class {
        CharClass::Trader => Some(&d.trader_attack),
        CharClass::Mutant => Some(&d.mutant_attack),
        CharClass::Dog => Some(&d.dog_attack),
        _ => None,
    }
}

/// `Item.ConsumesAmmo`: `ammo != 0`, the rounds left in this one.
pub fn consumes_ammo(pack: &ItemBag, slot: usize, t: &impl Tables) -> bool {
    match pack.get(slot) {
        Some(kind) => pack.rounds_left(slot).unwrap_or_else(|| t.ammo(kind)) != 0,
        None => false,
    }
}

/// `ItemList.FindBestWeapon` as v1.2 has it: **by `WeaponPriority`**, and
/// once one is held only something of higher priority that needs no
/// ammunition replaces it. With nothing held, the first thing that does
/// damage is taken, ammunition or not.
pub fn find_best_weapon(pack: &ItemBag, current: Option<usize>, t: &impl Tables) -> Option<usize> {
    let does_damage = |slot: usize| pack.get(slot).is_some_and(|k| t.damage(k) > 0);
    let priority = |slot: usize| pack.get(slot).map_or(0, |k| t.weapon_priority(k));
    let mut current = current;
    for slot in 0..pack.len() {
        if !does_damage(slot) {
            continue;
        }
        let take = match current {
            None => true,
            Some(held) => priority(held) < priority(slot) && !consumes_ammo(pack, slot, t),
        };
        if take {
            current = Some(slot);
        }
    }
    current
}

/// `Character.FindOriginalWeapon`.
///
/// The chosen weapon stands if it is still in the pack and does damage (an
/// unloaded rifle only when `allow_unloaded_rifle`), and is then improved by
/// [`find_best_weapon`]. With none chosen, **a human's man never reaches for
/// a firearm by himself** — "Firearms are an explicit ammunition-use choice
/// for human players" — and takes the highest-priority weapon that uses no
/// ammunition, the last of equals; a computer player's takes
/// [`find_best_weapon`]. Failing all that, an unloaded rifle if allowed.
pub fn find_original_weapon(
    pack: &ItemBag,
    chosen: Option<usize>,
    human: bool,
    allow_unloaded_rifle: bool,
    t: &impl Tables,
) -> Option<usize> {
    let unloaded = t.kind_named("item_unloaded_rifle");
    let mut selected = chosen.filter(|&s| pack.get(s).is_some());
    if let Some(s) = selected {
        let kind = pack.get(s).unwrap_or(usize::MAX);
        if t.damage(kind) == 0 && !(allow_unloaded_rifle && Some(kind) == unloaded) {
            selected = None;
        }
    }
    let mut selected = match selected {
        None if human => {
            let mut best: Option<usize> = None;
            for slot in 0..pack.len() {
                let Some(kind) = pack.get(slot) else { continue };
                if t.damage(kind) <= 0 || consumes_ammo(pack, slot, t) {
                    continue;
                }
                // `OrderBy(priority).LastOrDefault()`: a stable sort, so
                // the last of the highest.
                if best.is_none_or(|b| t.weapon_priority(pack.get(b).unwrap_or(kind)) <= t.weapon_priority(kind)) {
                    best = Some(slot);
                }
            }
            best
        }
        None => find_best_weapon(pack, None, t),
        Some(s) => find_best_weapon(pack, Some(s), t),
    };
    if allow_unloaded_rifle && selected.is_none() {
        selected = (0..pack.len()).find(|&s| pack.get(s).is_some() && pack.get(s) == unloaded);
    }
    selected
}

/// `Character.UseOriginalWeapon`: one round spent (`Item.Use`); a weapon left
/// doing no damage reloads from the first `item_ammunition` in the pack,
/// which is used up, or else is no longer the weapon.
///
/// Returns where the weapon now is: taking the ammunition out closes the
/// pack up, which moves a weapon that came after it.
pub fn use_original_weapon(pack: &mut ItemBag, kit: &mut Kit, slot: usize, t: &impl Tables) -> Option<usize> {
    let loaded = pack.get(slot)?;
    // `Item.Use`: ammo--; at nought the type becomes its empty form with
    // that form's own rounds.
    let held = pack.rounds_left(slot).unwrap_or_else(|| t.ammo(loaded));
    let left = held - 1;
    if left == 0 {
        match t.empty(loaded) {
            Some(empty) => {
                pack.replace(slot, empty);
            }
            None => pack.set_rounds_left(slot, Some(left)),
        }
    } else {
        pack.set_rounds_left(slot, Some(left));
    }
    let now = pack.get(slot)?;
    if t.damage(now) != 0 {
        return Some(slot);
    }
    let ammunition = t.kind_named("item_ammunition");
    let found = (0..pack.len()).find(|&s| ammunition.is_some() && pack.get(s) == ammunition);
    match found {
        Some(a) => {
            pack.remove(a);
            let slot = if a < slot { slot - 1 } else { slot };
            // `Reload(loadedType)`: the loaded type, with its full rounds.
            pack.replace(slot, loaded);
            kit.follow_removal(a);
            Some(slot)
        }
        None => {
            kit.weapon = None;
            Some(slot)
        }
    }
}

/// `TableCombat.ProtectionPercent` for a chosen piece: its defence, 0 to 100.
pub fn protection_percent(pack: &ItemBag, protection: Option<usize>, t: &impl Tables) -> i32 {
    protection.and_then(|s| pack.get(s)).map_or(0, |k| t.defense(k).clamp(0, 100))
}

/// `ItemList.FindBestDefense`: the highest defence, the first of equals,
/// starting from what is chosen.
pub fn find_best_defense(pack: &ItemBag, current: Option<usize>, t: &impl Tables) -> Option<usize> {
    let defense = |s: Option<usize>| s.and_then(|s| pack.get(s)).map_or(0, |k| t.defense(k));
    let mut current = current.filter(|&s| pack.get(s).is_some());
    for slot in 0..pack.len() {
        if defense(Some(slot)) > defense(current) {
            current = Some(slot);
        }
    }
    current
}

/// The word `fight_class` lists a class by.
pub fn class_word(class: crate::gamdat::CharClass) -> &'static str {
    use crate::gamdat::CharClass;
    match class {
        CharClass::Mercenary => "fighter",
        CharClass::Technician => "technician",
        CharClass::Doctor => "doctor",
        CharClass::Boss => "boss",
        CharClass::Mutant => "mutant",
        CharClass::Trader => "trader",
        CharClass::Dog => "dog",
        _ => "",
    }
}

/// Traders, mutants and dogs fight with the difficulty's own rows.
pub fn is_creature(class: crate::gamdat::CharClass) -> bool {
    use crate::gamdat::CharClass;
    matches!(class, CharClass::Trader | CharClass::Mutant | CharClass::Dog)
}
