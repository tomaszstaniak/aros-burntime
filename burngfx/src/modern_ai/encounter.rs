//! The opponent's fight for a camp: `StrategicEncounter`, with
//! `TableCombat.Strength`, `Preview` and `Roll` as the v1.2 rule book uses
//! them (`CalculateStrategicStrength`, `GetCombatPreview`,
//! `RollStrategicDamage`). At tag `v1.2`,
//! `AI/PlayerBehavior/Combat/StrategicEncounter.cs` and
//! `Logic/Rules/TableCombat.cs`.
//!
//! Not the fight on the map: rounds of blows, each attacker at the weakest
//! defender and a living defender striking back, until one side is gone,
//! the attacker withdraws, or [`MAX_ROUNDS`] pass. The same whether the camp
//! is a computer's or a human's. Armour counts under Extended's rule
//! (`RemasterArmour`) only.

use alloc::vec::Vec;

use crate::combat::Kit;
use crate::gamdat::{CharClass, Danger};
use crate::game::ItemBag;
use crate::items::ItemTypes;
use crate::rules::{Combat, RuleBook};
use crate::spawn::Rng;
use crate::table_combat::{self as tc, Tables as _};

pub const ATTACKING_BOSS_SAFETY_HEALTH: i32 = 40;
pub const DEFENDING_BOSS_SAFETY_HEALTH: i32 = 65;
/// A safety limit of the fight itself, apart from the turn's ten actions.
pub const MAX_ROUNDS: usize = 100;
/// A follower this hurt makes the attacker withdraw.
pub const FOLLOWER_WITHDRAWAL_HEALTH: i32 = 35;

/// Somebody in the fight.
#[derive(Clone, Debug, PartialEq)]
pub struct Fighter {
    pub class: CharClass,
    pub experience: i32,
    pub health: i32,
    pub pack: ItemBag,
    pub kit: Kit,
    /// A player's character (`SelectOriginalWeapon`'s `human`).
    pub human: bool,
}

impl Fighter {
    pub fn dead(&self) -> bool {
        self.health <= 0
    }
}

/// The rules a fight is fought under.
#[derive(Clone, Copy)]
pub struct Rules<'a> {
    pub book: &'a RuleBook,
    pub types: &'a ItemTypes,
    pub difficulty: i32,
    /// The place's danger, for `PreferredProtection`.
    pub hazard: Danger,
}

impl Rules<'_> {
    fn armour(&self) -> bool {
        self.book.rules.combat == Combat::RemasterArmour
    }

    fn experience(&self, f: &Fighter) -> i32 {
        tc::combat_experience(self.book.is_fight_class(tc::class_word(f.class)), f.experience)
    }

    /// `DamageValues` for the weapon in `slot`.
    fn table(&self, f: &Fighter, slot: Option<usize>) -> Vec<i32> {
        if tc::is_creature(f.class) {
            let d = &self.book.difficulty[self.difficulty.clamp(0, 2) as usize];
            return match f.class {
                CharClass::Trader => d.trader_attack.clone(),
                CharClass::Mutant => d.mutant_attack.clone(),
                _ => d.dog_attack.clone(),
            };
        }
        match slot.and_then(|s| f.pack.get(s)) {
            Some(k) if !self.types.damage_table(k).is_empty() => self.types.damage_table(k).to_vec(),
            _ => self.types.unarmed().to_vec(),
        }
    }

    fn original_weapon(&self, f: &Fighter) -> Option<usize> {
        if tc::is_creature(f.class) {
            return None;
        }
        tc::find_original_weapon(&f.pack, f.kit.weapon, f.human, false, self.types)
    }

    /// `ProtectionPercent` of what `PreferredProtection` picks; nought where
    /// armour does not count.
    pub fn protection(&self, f: &Fighter) -> i32 {
        if !self.armour() {
            return 0;
        }
        let chosen = crate::v12_items::preferred_protection(&f.pack, f.kit.protection, self.hazard, self.types);
        tc::protection_percent(&f.pack, chosen, self.types).clamp(0, 100)
    }

    /// `TableCombat.Strength`: the row's average for the weapon he would
    /// use, and a tenth of his health (a flat ten when not `detailed`),
    /// which armour stretches.
    pub fn strength(&self, f: &Fighter, detailed: bool) -> f32 {
        let table = self.table(f, self.original_weapon(f));
        // The row's average alone: health is added below.
        let average = tc::strategic_strength(&table, self.experience(f), self.book.rules.experience_tier_width, 0, true).unwrap_or(0.0);
        let mut health = if detailed { f.health as f32 / 10.0 } else { 10.0 };
        if self.armour() {
            health /= (1.0 - self.protection(f) as f32 / 100.0).max(0.01);
        }
        average + health
    }

    /// `TableCombat.EquippedPreview`: the least and most a blow with the
    /// weapon in hand does (`Character.Weapon`, not the one he would pick),
    /// and the defence of the protection worn when armour counts and it is
    /// above nought. What the inventory shows.
    pub fn equipped_preview(&self, f: &Fighter) -> (i32, i32, Option<i32>) {
        let table = self.table(f, f.kit.weapon);
        let (min, max) = tc::preview(&table, self.experience(f), self.book.rules.experience_tier_width).unwrap_or((0, 0));
        let defense = if self.armour() { tc::protection_percent(&f.pack, f.kit.protection, self.types) } else { 0 };
        (min, max, (defense > 0).then_some(defense))
    }

    /// `GetWeaponPreview`: the least and the most a blow with this weapon
    /// (by slot in the fighter's pack) would do, at his experience.
    pub fn weapon_preview(&self, f: &Fighter, slot: usize) -> (i32, i32) {
        let table = self.table(f, Some(slot));
        tc::preview(&table, self.experience(f), self.book.rules.experience_tier_width).unwrap_or((0, 0))
    }

    /// `Preview(...).Maximum`.
    pub fn maximum(&self, f: &Fighter) -> i32 {
        let table = self.table(f, self.original_weapon(f));
        tc::preview(&table, self.experience(f), self.book.rules.experience_tier_width).map_or(0, |(_, max)| max)
    }

    /// `TableCombat.Roll(attacker, armour ? defender : null, useAmmo: true)`:
    /// the weapon chosen, one of four rolls, ammunition used, and the
    /// defender's armour put on and taken off the blow.
    pub fn roll(&self, attacker: &mut Fighter, defender: &mut Fighter, rng: &mut Rng) -> i32 {
        let weapon = self.original_weapon(attacker);
        if !tc::is_creature(attacker.class) {
            attacker.kit.weapon = weapon;
        }
        let table = self.table(attacker, weapon);
        let mut damage = tc::damage(&table, self.experience(attacker), self.book.rules.experience_tier_width, rng.range(0, 4) as usize).unwrap_or(0);
        if let Some(slot) = weapon {
            if tc::consumes_ammo(&attacker.pack, slot, self.types) {
                let mut kit = attacker.kit;
                tc::use_original_weapon(&mut attacker.pack, &mut kit, slot, self.types);
                attacker.kit = kit;
            }
        }
        if self.armour() {
            defender.kit.protection = crate::v12_items::preferred_protection(&defender.pack, defender.kit.protection, self.hazard, self.types);
            damage = tc::apply_armour(damage, self.protection(defender));
        }
        damage
    }

    /// `NeedsWithdrawal`: at the safety health, or the opponent's best blow
    /// through his armour would put him under it.
    pub fn needs_withdrawal(&self, boss: &Fighter, opponent: Option<&Fighter>, safety: i32) -> bool {
        if boss.health <= safety {
            return true;
        }
        let Some(o) = opponent else { return false };
        boss.health - tc::apply_armour(self.maximum(o), self.protection(boss)) < safety
    }
}

/// `Result`, and how many rounds were fought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub attacker_withdrew: bool,
    pub defending_party_disengaged: bool,
    pub rounds: usize,
    /// Who went down, in the order of the blows: `(attacker, index)`.
    pub fallen: Vec<(bool, usize)>,
}

/// The defending side: everyone at the camp, and which of them came with
/// the owner's own party (`visiting`, who may leave together), with the
/// owner's character among them when he is there.
pub struct Defence<'a> {
    pub fighters: &'a mut [Fighter],
    pub visiting: &'a [bool],
    pub owner: Option<usize>,
    /// v1.2 only: alive below one point ([`crate::danger::Exposure::lifted`]),
    /// the copy holding 1 where `Health`'s getter reads 0. Missing: none.
    pub lifted: &'a [bool],
}

/// `StrategicEncounter.Fight`. `attackers[0]` is the attacking leader.
pub fn fight(rules: &Rules, attackers: &mut [Fighter], defence: Defence, fight_to_death: bool, rng: &mut Rng) -> Outcome {
    let Defence { fighters: defenders, visiting, owner, lifted } = defence;
    // `OrderBy(character => character.Health)`: the getter, 0 for 0 < h < 1
    // (`StrategicEncounter.cs:51`). Only an unhit copy can be lifted; a blow
    // (damage > 0) leaves it dead.
    let getter = |defenders: &[Fighter], i: usize| if lifted.get(i).copied().unwrap_or(false) && defenders[i].health == 1 { 0 } else { defenders[i].health };
    let starting: Vec<i32> = attackers.iter().map(|f| f.health).collect();
    let mut disengaged = false;
    let owner_visits = owner.is_some_and(|o| visiting.get(o).copied().unwrap_or(false));
    let living = |defenders: &[Fighter], disengaged: bool| -> Vec<usize> {
        (0..defenders.len())
            .filter(|&i| !defenders[i].dead() && !(disengaged && visiting.get(i).copied().unwrap_or(false)))
            .collect()
    };
    // `DisengageDefender`.
    let disengage = |defenders: &[Fighter], opponent: Option<&Fighter>, disengaged: &mut bool| -> bool {
        if !owner_visits {
            return false;
        }
        if let Some(o) = owner {
            if rules.needs_withdrawal(&defenders[o], opponent, DEFENDING_BOSS_SAFETY_HEALTH) {
                *disengaged = true;
            }
        }
        *disengaged
    };
    let mut fallen: Vec<(bool, usize)> = Vec::new();
    let out = |withdrew, disengaged, rounds, fallen: &mut Vec<(bool, usize)>| Outcome {
        attacker_withdrew: withdrew,
        defending_party_disengaged: disengaged,
        rounds,
        fallen: core::mem::take(fallen),
    };

    disengage(defenders, None, &mut disengaged);
    for round in 0..MAX_ROUNDS {
        if living(defenders, disengaged).is_empty() || attackers[0].dead() {
            return out(false, disengaged, round, &mut fallen);
        }
        if !fight_to_death
            && (rules.needs_withdrawal(&attackers[0], None, ATTACKING_BOSS_SAFETY_HEALTH) || attackers.len() < 2)
        {
            return out(true, disengaged, round, &mut fallen);
        }
        let before = living(defenders, disengaged).len();
        for a in 0..attackers.len() {
            if attackers[a].dead() || attackers[0].dead() {
                break;
            }
            let target = loop {
                // The lowest health first; equal, the first.
                let t = living(defenders, disengaged).into_iter().min_by_key(|&i| getter(defenders, i));
                match t {
                    Some(t) if Some(t) == owner && disengage(defenders, Some(&attackers[a]), &mut disengaged) => continue,
                    t => break t,
                }
            };
            let Some(t) = target else { break };
            if !fight_to_death && a == 0 && rules.needs_withdrawal(&attackers[0], Some(&defenders[t]), ATTACKING_BOSS_SAFETY_HEALTH) {
                return out(true, disengaged, round, &mut fallen);
            }
            let d = rules.roll(&mut attackers[a], &mut defenders[t], rng);
            if d > 0 {
                defenders[t].health -= d;
                if defenders[t].dead() {
                    fallen.push((false, t));
                }
            }
            if !defenders[t].dead() {
                let d = rules.roll(&mut defenders[t], &mut attackers[a], rng);
                if d > 0 {
                    let was = attackers[a].dead();
                    attackers[a].health -= d;
                    if !was && attackers[a].dead() {
                        fallen.push((true, a));
                    }
                }
            }
            disengage(defenders, None, &mut disengaged);
            if !fight_to_death
                && (rules.needs_withdrawal(&attackers[0], None, ATTACKING_BOSS_SAFETY_HEALTH)
                    || (1..attackers.len()).any(|i| {
                        attackers[i].dead() || attackers[i].health <= FOLLOWER_WITHDRAWAL_HEALTH && attackers[i].health < starting[i]
                    }))
            {
                return out(true, disengaged, round, &mut fallen);
            }
        }
        let now = living(defenders, disengaged).len();
        if !fight_to_death && now > 0 && now < before {
            return out(true, disengaged, round + 1, &mut fallen);
        }
    }
    out(false, disengaged, MAX_ROUNDS, &mut fallen)
}
