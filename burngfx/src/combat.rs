//! Blows.
//!
//! Ported from v0.9.1's `Character.Attack`, `Character.UseBestEquipment` and
//! the two values they work from. The whole exchange is short enough to
//! quote:
//!
//! ```csharp
//! static void attack(Character attacker, Character defender, bool useAmmo, float factor)
//! {
//!     int attackValue = attacker.UseBestEquipment(useAmmo);
//!     int damage = (int)Math.Max(1, (attackValue - defender.DefenseValue) * factor);
//!     defender.Health -= damage;
//! };
//!
//! foreach (var attacker in attackingGroup)
//! {
//!     attack(attacker, defender, useAmmo: true, isPlayer ? 1 : difficultyFactor);
//!     attack(defender, attacker, defendWithAmmo, isPlayer ? difficultyFactor : 1);
//!     if (defender.IsDead || attacker.IsDead) break;
//! }
//! ```
//!
//! **Not all of it is 1993.** Four of the rules below were written in the
//! last days of October and the first of November 2023, three months before
//! v0.9.1, and the remaster's own changelog calls them changes rather than
//! fixes. [`Contested`] says which, and `docs/combat-rules.md` sets out the
//! evidence for each. Faithfulness to the remaster is not faithfulness to
//! the DOS game, and this module no longer pretends the two are the same.
//!
//! Six things fall out of it that a summary would lose:
//!
//! * **Every blow lands.** `Math.Max(1, …)` — armour can never reduce a hit
//!   below one, and there is no miss.
//! * **A blow is answered in the same breath.** The defender hits back
//!   before the next attacker steps in, so attacking costs health even when
//!   it works.
//! * **The dying still answer.** The break is tested after both halves, so
//!   somebody struck down takes the attacker with them as often as not.
//! * **Difficulty weights whichever side is not the player**, by ten per
//!   cent a step. The comment above it in the source says twenty-five; the
//!   code says `1 + Difficulty * 0.1f`, and the code is what runs.
//! * **The whole group joins in**, but only when the *played* character is
//!   the one attacking, and only those standing within [`GROUP_REACH`].
//! * **It stops the moment either side falls**, so a group does not keep
//!   hitting a corpse.
//!
//! Both values scale with experience and both truncate:
//! `AttackValue = (Weapon?.DamageValue ?? BaseAttackValue) * Experience / 100`
//! and `DefenseValue = (10 + Protection?.DefenseValue ?? 0) * Experience / 100`.

use alloc::vec::Vec;

use crate::game::ItemBag;

/// `DEFAULT_ATTACK_VALUE`: a bare hand, and the floor of any defence.
pub const DEFAULT_ATTACK: i32 = 10;

/// How near the player must be to strike: `30 > (a.Position - b.Position).Length`
/// in `LocationScene.AttackCharacter`. Further away, the click walks
/// instead.
pub const REACH: i32 = 30;

/// How near a creature comes before it lunges — `CreatureMind` uses 20,
/// not the 30 a person needs.
pub const CREATURE_REACH: i32 = 20;

/// Who joins a group attack: `Player.Group.Where(ch => (ch.Position -
/// Position).Length < 25)`.
pub const GROUP_REACH: i32 = 25;

/// **The two rules nobody has decided about yet.**
///
/// Both come out of `auto equip weapons` (31 October 2023), the commit that
/// gave the remaster automatic selection of weapon and armour. Before it
/// the player chose both by hand, as the DOS game does — `SelectItem` is
/// still there and is years older. So neither of these is a reconstruction
/// of anything: they are new machinery, and they read like slips inside it.
///
/// They are **deliberately not tied to [`crate::game::Mode`] or
/// [`crate::game::Engine`]**. A game mode is an item table and an engine is
/// a release; a suspected defect in the reference is neither, and filing it
/// under one of them would be a decision disguised as a port. The default
/// is what v0.9.1 does, so nothing changes quietly — but the switch is here,
/// named, for the day the question is settled.
///
/// What would settle it: the DOS game, or the remaster's author. Until one
/// of them answers, `docs/combat-rules.md` holds what is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contested {
    /// **A swung axe is left with minus one round.**
    ///
    /// `UseBestEquipment` calls `Item.Use()` on whatever it just used, and
    /// `Use` decrements without asking what kind of thing it is. A knife
    /// goes from nought to minus one, `ConsumesAmmo` is `ammo != 0`, and
    /// the weapon is thereafter treated as one that burns ammunition — so
    /// the choice made at the end of the very same swing refuses it. Swing
    /// an axe once and you fight with your knife from then on.
    ///
    /// The `Use` call on a melee weapon is older than the auto-equip and
    /// was harmless while selection was manual: nothing read the count.
    /// It became visible the moment `ConsumesAmmo` started deciding what
    /// you hold. That is the shape of a bug, not of a rule.
    ///
    /// `true` is v0.9.1. `false` spends a round only on something that has
    /// rounds to spend.
    pub melee_spends_a_round: bool,
    /// **The first armour found is worn for good.**
    ///
    /// `FindBestDefense` is `current ??= item`, which is "take one if you
    /// have none", not "take the better of the two" — and it is called
    /// `FindBestDefense`. A leather jacket picked up before a steel helmet
    /// is never replaced by it.
    ///
    /// `false` is v0.9.1. `true` wears the best in the pack, which is what
    /// the name promises and what the extended table's four protections
    /// would otherwise be for.
    pub best_armour_wins: bool,
}

impl Default for Contested {
    /// What v0.9.1 does. **Nothing changes unless somebody chooses it.**
    fn default() -> Contested {
        Contested { melee_spends_a_round: true, best_armour_wins: false }
    }
}

/// Everything a blow is settled by that belongs to neither fighter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// `World.Difficulty`, 0 to 2.
    pub difficulty: i32,
    pub contested: Contested,
}

/// What a fight needs to know about an item.
///
/// Deliberately not part of [`crate::game::ItemFacts`]: eating and drinking
/// ask different questions of the same table, and a test that sets up a
/// larder should not have to invent damage values.
pub trait Weapons {
    /// `DamageValue`. Zero is not a weapon — an unloaded rifle included.
    fn damage(&self, kind: usize) -> i32;
    /// `DefenseValue`. Zero is not armour.
    fn defense(&self, kind: usize) -> i32;
    /// `AmmoValue`: how many rounds one of these holds when new.
    fn ammo(&self, kind: usize) -> i32;
    /// What it becomes when the last round is spent.
    fn empty(&self, kind: usize) -> Option<usize>;
}

/// What a character has chosen to fight with.
///
/// `Character.Weapon` and `Character.Protection` are **sticky**: the choice
/// outlives the fight, and `FindBestWeapon` will only replace it with
/// something strictly better that needs no ammunition. That is what keeps a
/// man who once picked up a knife from spending his last rifle round on a
/// dog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kit {
    /// The slot the weapon is in, not the kind — `Item.Use` changes a
    /// weapon's type in place, and the slot is what stays the same.
    pub weapon: Option<usize>,
    pub protection: Option<usize>,
}

impl Kit {
    /// Forget a choice that no longer names anything, after the pack has
    /// been rearranged by something other than a fight.
    pub fn forget_beyond(&mut self, len: usize) {
        self.weapon = self.weapon.filter(|&s| s < len);
        self.protection = self.protection.filter(|&s| s < len);
    }

    /// One slot was taken out and the pack closed up: a choice after it
    /// moves down one, and a choice of that slot is gone.
    pub fn follow_removal(&mut self, removed: usize) {
        let shift = |c: Option<usize>| match c {
            Some(s) if s == removed => None,
            Some(s) if s > removed => Some(s - 1),
            other => other,
        };
        self.weapon = shift(self.weapon);
        self.protection = shift(self.protection);
    }

    /// `Character.SelectItem`, the inventory's right click on something
    /// selectable: a weapon becomes the weapon, or stops being it if it
    /// already was; the same for protection. An item can be both.
    pub fn select(&mut self, slot: usize, is_weapon: bool, is_protection: bool) {
        if is_weapon {
            self.weapon = if self.weapon == Some(slot) { None } else { Some(slot) };
        }
        if is_protection {
            self.protection = if self.protection == Some(slot) { None } else { Some(slot) };
        }
    }

    /// **The choice follows the item, not the slot.** v0.9.1 holds the item
    /// itself; here a slot is kept, and a pack shifts down when something
    /// before it is taken out. Given the pack's kinds before and after, a
    /// chosen item that is still there — in order, the rest having been
    /// removed around it or added after — keeps being chosen where it now
    /// is, and one that left the pack is chosen no more.
    pub fn follow(&mut self, before: &[usize], after: &[usize]) {
        // Where each old slot went: a greedy walk, taking a kind that still
        // matches as the same item and anything else as removed.
        let mut to = alloc::vec![None; before.len()];
        let mut j = 0;
        for (i, &kind) in before.iter().enumerate() {
            if j < after.len() && after[j] == kind {
                to[i] = Some(j);
                j += 1;
            }
        }
        // Something that changed in place — a bottle filled beside it — breaks
        // the walk after that slot. The pack the same length and the chosen
        // slot holding the same kind is the item staying where it was.
        let same_len = before.len() == after.len();
        let map = |s: Option<usize>| {
            s.and_then(|s| {
                to.get(s).copied().flatten().or_else(|| {
                    (same_len && before.get(s).is_some() && before.get(s) == after.get(s)).then_some(s)
                })
            })
        };
        self.weapon = map(self.weapon);
        self.protection = map(self.protection);
    }
}

/// One side of a fight, borrowed from wherever it actually lives.
///
/// Nothing is copied in or out: the health that changes is the character's
/// own, and the pack a shot is spent from is the pack the player is looking
/// at. A fight that worked on copies would be the one place in this game
/// where an item could quietly become two.
pub struct Fighter<'a> {
    pub health: &'a mut i32,
    /// Both values scale with it, and a character with none does nothing.
    pub experience: i32,
    /// `BaseAttackValue` — [`DEFAULT_ATTACK`] for a person, `DogAttack` or
    /// `MutantAttack` out of the settings for a creature.
    pub base_attack: i32,
    pub pack: &'a mut ItemBag,
    pub kit: &'a mut Kit,
}

impl Fighter<'_> {
    /// `UseBestEquipment`, line for line — and it is not a query: it picks
    /// up armour, discards an emptied gun and **spends a round**.
    ///
    /// Two of its oddities are switchable and one is not:
    ///
    /// * which armour it puts on, and whether a swung axe loses a round,
    ///   are [`Contested`] — see there for why they are a question and not
    ///   a rule.
    /// * `if (weapon.DamageValue == 0) Weapon = null` clears the *chosen*
    ///   weapon when the one just fired ran dry, even when they are not the
    ///   same item. It can only fire when a gun has just emptied, which is
    ///   also the moment the chosen weapon should be dropped, so it changes
    ///   nothing any test can see. Kept as written.
    pub fn use_best_equipment(
        &mut self,
        allow_ammo: bool,
        contested: Contested,
        table: &impl Weapons,
    ) -> i32 {
        self.kit.protection =
            find_best_defense(self.pack, self.kit.protection, contested, table);

        // Make sure empty guns are not used.
        if self.kit.weapon.is_some_and(|s| self.damage_at(s, table) == 0) {
            self.kit.weapon = None;
        }

        let carrying = if allow_ammo { self.kit.weapon } else { None };
        let Some(slot) = find_best_weapon(self.pack, carrying, table) else {
            return self.base_attack * self.experience / 100;
        };

        // The value is the weapon's before it is fired.
        let attack_value = self.damage_at(slot, table);
        if allow_ammo {
            self.fire(slot, contested, table);
            if self.damage_at(slot, table) == 0 {
                self.kit.weapon = None;
            }
        }
        self.kit.weapon = find_best_weapon(self.pack, self.kit.weapon, table);
        attack_value * self.experience / 100
    }

    /// `Item.Use`, which is three lines and every one of them counts:
    ///
    /// ```csharp
    /// ammo--;
    /// if (ammo == 0) { Type = Type.Empty; ammo = Type.AmmoValue; }
    /// ```
    ///
    /// **It runs for a knife too**, because nothing asks first — see
    /// [`Contested::melee_spends_a_round`], which is where that stops being
    /// a description and starts being a question.
    ///
    /// The type only changes when the count lands exactly on nought, which
    /// is why the extended table's two-stage rifle gives the same two shots
    /// as the original's one-stage one.
    fn fire(&mut self, slot: usize, contested: Contested, table: &impl Weapons) {
        let Some(kind) = self.pack.get(slot) else { return };
        let held = self.pack.rounds_left(slot).unwrap_or_else(|| table.ammo(kind));
        if !contested.melee_spends_a_round && held <= 0 {
            // Nothing to spend: this is a club, not a gun.
            return;
        }
        let left = held - 1;
        if left != 0 {
            self.pack.set_rounds_left(slot, Some(left));
            return;
        }
        match table.empty(kind) {
            // `Type = Type.Empty; ammo = Type.AmmoValue`, both at once.
            Some(empty) => {
                self.pack.replace(slot, empty);
            }
            // Upstream would assign a null type here. No item in either
            // table can reach it: everything that carries rounds names what
            // it becomes when they run out.
            None => self.pack.set_rounds_left(slot, Some(left)),
        }
    }

    fn damage_at(&self, slot: usize, table: &impl Weapons) -> i32 {
        match self.pack.get(slot) {
            Some(kind) => table.damage(kind),
            None => 0,
        }
    }

    /// `AttackValue`, without touching anything. What a panel may show.
    pub fn attack_value(&self, table: &impl Weapons) -> i32 {
        let weapon = self.kit.weapon.map(|s| self.damage_at(s, table)).filter(|&d| d > 0);
        weapon.unwrap_or(self.base_attack) * self.experience / 100
    }

    /// `DefenseValue`. The ten is not a weapon's — it is
    /// `DEFAULT_ATTACK_VALUE` used as the floor of a defence, so somebody
    /// with nothing to wear still has one.
    ///
    /// **Read as it stands.** The defender's armour is only picked up when
    /// the defender themselves swings, so a man who has never fought
    /// defends bare even with a jacket in his pack.
    pub fn defense_value(&self, table: &impl Weapons) -> i32 {
        let worn = match self.kit.protection.and_then(|s| self.pack.get(s)) {
            Some(kind) => table.defense(kind),
            None => 0,
        };
        (DEFAULT_ATTACK + worn) * self.experience / 100
    }

    pub fn is_dead(&self) -> bool {
        *self.health <= 0
    }
}

/// `ItemList.FindBestDefense`, which finds the first and not the best —
/// unless [`Contested::best_armour_wins`] says otherwise.
fn find_best_defense(
    pack: &ItemBag,
    current: Option<usize>,
    contested: Contested,
    table: &impl Weapons,
) -> Option<usize> {
    let worn = |slot: Option<usize>| {
        slot.and_then(|s| pack.get(s)).map_or(0, |kind| table.defense(kind))
    };
    if contested.best_armour_wins {
        let mut best = current;
        for slot in 0..pack.len() {
            let defense = pack.get(slot).map_or(0, |kind| table.defense(kind));
            if defense > 0 && defense > worn(best) {
                best = Some(slot);
            }
        }
        return best;
    }
    if current.is_some() {
        return current;
    }
    (0..pack.len()).find(|&slot| pack.get(slot).is_some_and(|k| table.defense(k) > 0))
}

/// `ItemList.FindBestWeapon`: with nothing chosen, the first weapon in the
/// pack; after that, only something strictly better **that needs no
/// ammunition** may replace it.
fn find_best_weapon(
    pack: &ItemBag,
    current: Option<usize>,
    table: &impl Weapons,
) -> Option<usize> {
    let damage = |slot: usize| pack.get(slot).map_or(0, |kind| table.damage(kind));
    let consumes_ammo = |slot: usize| match pack.get(slot) {
        Some(kind) => pack.rounds_left(slot).unwrap_or_else(|| table.ammo(kind)) != 0,
        None => false,
    };

    let mut current = current;
    for slot in 0..pack.len() {
        if damage(slot) <= 0 {
            continue;
        }
        let better = match current {
            None => true,
            Some(held) => damage(held) < damage(slot) && !consumes_ammo(slot),
        };
        if better {
            current = Some(slot);
        }
    }
    current
}

/// `1 + Difficulty * 0.1f`.
pub fn difficulty_factor(difficulty: i32) -> f32 {
    1.0 + difficulty as f32 * 0.1
}

/// One blow: `Math.Max(1, (attackValue - defenderDefense) * factor)`.
///
/// **Never less than one.** Armour reduces, it does not stop.
pub fn blow(attack_value: i32, defense_value: i32, factor: f32) -> i32 {
    let raw = (attack_value - defense_value) as f32 * factor;
    if raw < 1.0 {
        1
    } else {
        raw as i32
    }
}

/// What one exchange did, from the attacking side's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Round {
    /// Which attacker struck, as an index into the group.
    pub who: usize,
    pub dealt: i32,
    pub taken: i32,
    pub defender_fell: bool,
    pub attacker_fell: bool,
}

/// `Character.Attack`: the whole group strikes in turn, each answered.
///
/// `player_attacks` is `isPlayer` — whether the side holding `attackers` is
/// the player's. It decides which way the difficulty weighting falls, and
/// nothing else. `defend_with_ammo` is the defender's: a creature is
/// attacked with `defendWithAmmo: false`, which is what stops a dog's lunge
/// from emptying the player's rifle.
///
/// One length of the same rule is *not* here and belongs to the caller:
/// whether anybody is in range at all. See [`in_reach`].
pub fn attack(
    attackers: &mut [Fighter],
    defender: &mut Fighter,
    defend_with_ammo: bool,
    player_attacks: bool,
    rules: Rules,
    table: &impl Weapons,
) -> Vec<Round> {
    let factor = difficulty_factor(rules.difficulty);
    let (on_attack, on_answer) = if player_attacks { (1.0, factor) } else { (factor, 1.0) };

    let mut rounds = Vec::new();
    for (who, attacker) in attackers.iter_mut().enumerate() {
        let value = attacker.use_best_equipment(true, rules.contested, table);
        let dealt = blow(value, defender.defense_value(table), on_attack);
        *defender.health -= dealt;

        // Answered before the next one steps in — and answered even from
        // the ground, because the break below is tested afterwards.
        let value = defender.use_best_equipment(defend_with_ammo, rules.contested, table);
        let taken = blow(value, attacker.defense_value(table), on_answer);
        *attacker.health -= taken;

        rounds.push(Round {
            who,
            dealt,
            taken,
            defender_fell: defender.is_dead(),
            attacker_fell: attacker.is_dead(),
        });

        if defender.is_dead() || attacker.is_dead() {
            break;
        }
    }
    rounds
}

/// What a creature leaves behind beyond what it carried.
///
/// Both overrides are one line of their own `Die`:
///
/// * `Dog.Die` drops `item_meat`, always. It is the reason to hunt one.
/// * `Mutant.Die` drops one `material`, `rare` or `useless` item that is
///   not `nodrop`, **one time in ten** — `GenerateClass(…, 0.1f)`.
///
/// The chance is drawn from `rng` rather than from the original's global
/// generator: the DOS sequence cannot be matched anyway, and a stream of
/// our own is what lets a fight be replayed.
///
/// The pick itself keeps the original's off-by-one. `Random.Next(n - 1)`
/// yields `0 .. n - 2`, so **the last candidate can never be drawn** —
/// the same slip `RandomCount` makes, and not one to quietly correct here.
pub fn creature_drop(
    class: crate::gamdat::CharClass,
    types: &crate::items::ItemTypes,
    rng: &mut crate::spawn::Rng,
) -> Option<usize> {
    use crate::gamdat::CharClass::*;
    match class {
        Dog => types.index_of("item_meat"),
        Mutant => {
            let filter = crate::settings::Filter {
                include: ["material", "rare", "useless"].iter().map(|s| (*s).into()).collect(),
                exclude: alloc::vec![alloc::string::String::from("nodrop")],
            };
            let candidates = crate::spawn::candidates(types, &filter);
            if candidates.len() < 2 {
                return None;
            }
            // `Random.NextSingle() <= 0.1f`, one time in ten.
            if rng.below(10) != 0 {
                return None;
            }
            candidates.get(rng.below(candidates.len() as u32 - 1) as usize).copied()
        }
        _ => None,
    }
}

/// Does this member of the group join the leader's attack?
///
/// `Player.Group.Where(ch => (ch.Position - Position).Length < 25)`, measured
/// from the character doing the attacking. **Strictly less**: a companion
/// standing exactly twenty-five away stays out of it. Before companions had
/// positions of their own everybody joined, which is the one thing this
/// function exists to stop.
pub fn joins_group_attack(leader: crate::game::Point, member: crate::game::Point) -> bool {
    in_reach(leader, member, GROUP_REACH)
}

/// Within reach to strike?
pub fn in_reach(a: crate::game::Point, b: crate::game::Point, reach: i32) -> bool {
    let (dx, dy) = ((a.x - b.x) as i64, (a.y - b.y) as i64);
    dx * dx + dy * dy < (reach as i64) * (reach as i64)
}
