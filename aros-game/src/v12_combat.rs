//! v1.2 combat in the running game: the world side of
//! [`burngfx::encounter`] and of [`burngfx::table_combat`].
//!
//! Who fights is a [`Who`] — the played character, a companion by identity, or
//! somebody standing in the location by index. A blow is a table roll
//! (`TableCombat.Roll`); a human's attack is an [`Encounter`] that moves
//! people for several seconds; a creature's is `AttackWithoutRetaliation`, a
//! blow and then flight. Movement orders the encounter gives (approach, hold,
//! flee) are kept in [`Moves`] and carried out each step by [`advance`].
//!
//! **The roll is drawn from the game's one generator**, as the source draws
//! it from its one `Random`: `Next(0, 4)`, afresh for every blow. The save
//! carries the generator, so a game saved between two fights plays the next
//! one the same way after a load.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::combat::{Fighter, Kit};
use burngfx::encounter::{self, Arena};
use burngfx::game::{GameState, ItemBag, Party, Point};
use burngfx::gamdat::CharClass;
use burngfx::hire::People;
use burngfx::items::ItemTypes;
use burngfx::rules::{Combat, RuleBook};
use burngfx::table_combat::{self, Tables};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Leader,
    /// A companion, by identity.
    Friend(usize),
    /// Somebody standing in the location, by index into `People`.
    Person(usize),
}

/// What the encounter asked of people's feet.
#[derive(Default, Debug)]
pub struct Moves {
    /// `BeginCombatApproach`: walk at the target until within range.
    pub approach: Vec<(Who, Who, f32)>,
    /// `HoldForCombat`: stand still.
    pub held: Vec<Who>,
    /// `FleeFrom`: running away.
    pub flee: Vec<Flight>,
    /// Approaches begun since the last step: `BeginCombatApproach`'s
    /// `UpdateMovingTarget(…, forceRepath: true)` (`Character.cs:566-567`).
    pub fresh: Vec<Who>,
}

/// `FleeFrom`'s state on a character (`fleeTimeRemaining`, `fleeDestination`,
/// `fleeFromCharacter`, `Character.cs:62-67`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    pub who: Who,
    /// Whom from: a new point is found away from him whenever the last is
    /// reached.
    pub from: Who,
    pub to: Point,
    /// Steps of `FLEE_DURATION` left.
    pub left: u32,
}

impl Moves {
    fn forget(&mut self, who: Who) {
        self.approach.retain(|(w, _, _)| *w != who);
        self.held.retain(|w| *w != who);
        self.flee.retain(|f| f.who != who);
        self.fresh.retain(|w| *w != who);
    }

    /// Whether this one's movement belongs to a fight right now — and so
    /// his mind is not asked (`Character.cs:1018-1023`).
    pub fn owns(&self, who: Who) -> bool {
        self.approach.iter().any(|(w, _, _)| *w == who)
            || self.held.contains(&who)
            || self.flee.iter().any(|f| f.who == who)
    }

    pub fn fleeing(&self, who: Who) -> bool {
        self.flee.iter().any(|f| f.who == who)
    }

    pub fn approaching(&self, who: Who) -> Option<(Who, f32)> {
        self.approach.iter().find(|(w, _, _)| *w == who).map(|&(_, t, r)| (t, r))
    }

    pub fn is_empty(&self) -> bool {
        self.approach.is_empty() && self.held.is_empty() && self.flee.is_empty()
    }
}

/// One blow, for the console, the sparks and the dead.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub attacker: Who,
    pub defender: Who,
    pub damage: i32,
    /// The defender's health after it.
    pub left: i32,
}

/// The running game, borrowed for one step of a fight.
pub struct World<'a> {
    pub state: &'a mut GameState,
    pub carried: &'a mut ItemBag,
    pub party: &'a mut Party,
    pub people: &'a mut People,
    pub camps: &'a mut burngfx::game::Camps,
    pub types: &'a ItemTypes,
    pub book: &'a RuleBook,
    pub difficulty: i32,
    /// The game's generator (`Platform.Math.Random`).
    pub rng: &'a mut burngfx::spawn::Rng,
    pub here: usize,
    /// The location's danger: what `PreferredProtection` prefers there.
    pub hazard: burngfx::gamdat::Danger,
    pub leader_class: CharClass,
    pub leader_experience: i32,
    /// Seats played by a human: a man of theirs never reaches for a firearm
    /// by himself (`FindOriginalWeapon`).
    pub human_seats: &'a [usize],
    pub moves: &'a mut Moves,
    pub struck: &'a mut Vec<Strike>,
    pub walkable: &'a dyn Fn(Point) -> bool,
    /// The leader's part of a danger's point not yet spent (see
    /// [`burngfx::danger::Exposure`]).
    pub exposure: &'a mut burngfx::danger::Exposure,
}

/// `IsFightClass`'s names for a class.
use table_combat::{class_word, is_creature};

/// A camper's numbers are his camp's (`their_side` in the game).
fn camped_as(camps: &burngfx::game::Camps, record: usize) -> Option<(usize, usize)> {
    camps.iter().find_map(|(location, camp)| {
        camp.residents.iter().position(|w| w.origin == Some(record)).map(|at| (location, at))
    })
}

impl World<'_> {
    fn class(&self, who: Who) -> CharClass {
        match who {
            Who::Leader => self.leader_class,
            Who::Friend(id) => self
                .party
                .companions
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.origin)
                .and_then(|r| self.people.get_by_record(r))
                .map_or(CharClass::Mercenary, |p| p.class),
            Who::Person(i) => self.people.get(i).map_or(CharClass::Unknown(0), |p| p.class),
        }
    }

    fn human(&self, who: Who) -> bool {
        match who {
            Who::Leader | Who::Friend(_) => true,
            Who::Person(i) => self.people.get(i).and_then(|p| p.employer).is_some_and(|e| self.human_seats.contains(&e)),
        }
    }

    pub fn position(&self, who: Who) -> Option<Point> {
        match who {
            Who::Leader => Some(self.state.position),
            Who::Friend(id) => self.party.companions.iter().find(|c| c.id == id).map(|c| c.at),
            Who::Person(i) => self.people.get(i).map(|p| p.at),
        }
    }

    /// The danger's carry of whoever has one: the leader, a companion, a
    /// camper (`continuous_hazard` in the game).
    fn exposure(&mut self, who: Who) -> Option<&mut burngfx::danger::Exposure> {
        match who {
            Who::Leader => Some(&mut *self.exposure),
            Who::Friend(id) => self.party.companions.iter_mut().find(|c| c.id == id).map(|c| &mut c.exposure),
            Who::Person(i) => {
                let record = self.people.get(i)?.record;
                let (location, at) = camped_as(self.camps, record)?;
                self.camps.at(location)?.residents.get_mut(at).map(|r| &mut r.exposure)
            }
        }
    }

    fn fighter(&mut self, who: Who) -> Option<Fighter<'_>> {
        match who {
            Who::Leader => Some(Fighter {
                health: &mut self.state.needs.health,
                experience: self.leader_experience,
                base_attack: burngfx::combat::DEFAULT_ATTACK,
                pack: self.carried,
                kit: &mut self.state.kit,
            }),
            Who::Friend(id) => {
                let c = self.party.companions.iter_mut().find(|c| c.id == id)?;
                Some(Fighter {
                    health: &mut c.needs.health,
                    experience: c.experience,
                    base_attack: burngfx::combat::DEFAULT_ATTACK,
                    pack: &mut c.pack,
                    kit: &mut c.kit,
                })
            }
            Who::Person(i) => {
                let (record, base) = {
                    let p = self.people.get(i)?;
                    (p.record, p.base_attack)
                };
                if let Some((location, at)) = camped_as(self.camps, record) {
                    let r = self.camps.at(location)?.residents.get_mut(at)?;
                    return Some(Fighter {
                        health: &mut r.needs.health,
                        experience: r.experience,
                        base_attack: base,
                        pack: &mut r.pack,
                        kit: &mut r.kit,
                    });
                }
                let p = self.people.get_mut(i)?;
                Some(Fighter {
                    health: &mut p.needs.health,
                    experience: p.experience,
                    base_attack: p.base_attack,
                    pack: &mut p.pack,
                    kit: &mut p.kit,
                })
            }
        }
    }

    /// `Random.Next(0, 4)`: which of the row's four values the blow takes.
    fn roll(&mut self) -> usize {
        self.rng.range(0, 4) as usize
    }

    /// `TableCombat.Roll`, and the blow applied (`DealAttackDamage`).
    pub fn blow(&mut self, attacker: Who, defender: Who) -> i32 {
        let roll = self.roll();
        let class = self.class(attacker);
        let human = self.human(attacker);
        let fight_class = self.book.is_fight_class(class_word(class));
        let types = self.types;
        let tier_width = self.book.rules.experience_tier_width;
        let creature = is_creature(class).then(|| {
            let d = &self.book.difficulty[self.difficulty.clamp(0, 2) as usize];
            match class {
                CharClass::Trader => d.trader_attack.clone(),
                CharClass::Mutant => d.mutant_attack.clone(),
                _ => d.dog_attack.clone(),
            }
        });
        let damage = {
            let Some(f) = self.fighter(attacker) else { return 0 };
            let xp = table_combat::combat_experience(fight_class, f.experience);
            match creature {
                Some(table) => table_combat::damage(&table, xp, tier_width, roll).unwrap_or(0),
                None => {
                    // `SelectOriginalWeapon`, then its table or a bare hand's.
                    let weapon = table_combat::find_original_weapon(f.pack, f.kit.weapon, human, false, types);
                    f.kit.weapon = weapon;
                    let table: Vec<i32> = match weapon.and_then(|s| f.pack.get(s)) {
                        Some(kind) if !types.damage_table(kind).is_empty() => types.damage_table(kind).to_vec(),
                        _ => types.unarmed().to_vec(),
                    };
                    let d = table_combat::damage(&table, xp, tier_width, roll).unwrap_or(0);
                    if let Some(slot) = weapon {
                        if table_combat::consumes_ammo(f.pack, slot, types) {
                            let mut kit: Kit = *f.kit;
                            table_combat::use_original_weapon(f.pack, &mut kit, slot, types);
                            *f.kit = kit;
                        }
                    }
                    d
                }
            }
        };
        let armour = self.book.rules.combat == Combat::RemasterArmour;
        let hazard = self.hazard;
        // `Health -= dmg` through the property (`GameRules.cs:236-237`,
        // `Character.cs:196-200`): the value written is whole, so the
        // float's fraction is gone (`danger::spend`).
        let mut carry = self.exposure(defender).map(|e| core::mem::take(e));
        let Some(f) = self.fighter(defender) else { return 0 };
        if let Some(e) = carry.as_mut() {
            burngfx::danger::spend(f.health, e);
        }
        let damage = if armour {
            // `PreferredProtection`: in a dangerous place, what protects
            // against it first; else the best defence.
            let chosen = burngfx::v12_items::preferred_protection(f.pack, f.kit.protection, hazard, types);
            f.kit.protection = chosen;
            table_combat::apply_armour(damage, table_combat::protection_percent(f.pack, chosen, types))
        } else {
            damage
        };
        *f.health -= damage;
        let left = *f.health;
        self.struck.push(Strike { attacker, defender, damage, left });
        damage
    }

    /// `FindFleeDestination`: seventy pixels straight away from the other,
    /// or the first of the angles around it that is walkable; `None` when
    /// none is, and then there is no flight at all (`FleeFrom` returns when
    /// the destination is where he stands). Two standing on one spot have
    /// no "away": the direction is drawn — `(Next(0, 2) == 0 ? -1 : 1,
    /// Next(-1, 2))`, one of six — and then normalised
    /// (`Logic/Character/Character.cs:727-762`).
    ///
    /// The point is `Position + (Vector2)(direction * 70)`, and v1.2's
    /// `(Vector2)` is `(int)(x + 0.5f)` (`Platform/Math/Vector2.cs:331-334`):
    /// a half up and then toward nought, so seventy to the west is −69.
    pub(crate) fn flee_point(&mut self, who: Who, from: Who) -> Option<Point> {
        let (at, other) = (self.position(who)?, self.position(from)?);
        let (mut dx, mut dy) = ((at.x - other.x) as f32, (at.y - other.y) as f32);
        if sqrt(dx * dx + dy * dy) < 0.1 {
            dx = if self.rng.range(0, 2) == 0 { -1.0 } else { 1.0 };
            dy = self.rng.range(-1, 2) as f32;
        }
        let len = sqrt(dx * dx + dy * dy);
        dx /= len;
        dy /= len;
        let d = encounter::FLEE_DISTANCE;
        let cast = |v: f32| (v + 0.5) as i32;
        let direct = Point::new(at.x + cast(dx * d), at.y + cast(dy * d));
        if (self.walkable)(direct) {
            return Some(direct);
        }
        for (c, s) in FLEE_TURNS {
            let (ax, ay) = (dx * c - dy * s, dx * s + dy * c);
            let p = Point::new(at.x + cast(ax * d), at.y + cast(ay * d));
            if (self.walkable)(p) {
                return Some(p);
            }
        }
        None
    }

    /// `FleeFrom`: off to a point away from `from` for `FLEE_DURATION`,
    /// dropping any approach or hold; nothing at all when there is nowhere
    /// to go.
    pub(crate) fn begin_flight(&mut self, who: Who, from: Who) {
        if let Some(to) = self.flee_point(who, from) {
            if Some(to) == self.position(who) {
                return;
            }
            self.moves.forget(who);
            self.moves.flee.push(Flight { who, from, to, left: FLEE_STEPS });
        }
    }

    /// `Path.Stop(Position)`: where he walks to is where he stands, and the
    /// fraction of a pixel is dropped.
    pub(crate) fn stop(&mut self, who: Who) {
        match who {
            Who::Person(i) => {
                if let Some(p) = self.people.get_mut(i) {
                    p.move_to = p.at;
                    p.bearing.stride.stop(p.at);
                }
            }
            Who::Friend(id) => {
                if let Some(c) = self.party.companions.iter_mut().find(|c| c.id == id) {
                    c.move_to = c.at;
                    c.stride.stop(c.at);
                }
            }
            Who::Leader => {}
        }
    }
}

impl Arena<Who> for World<'_> {
    fn is_dead(&self, who: Who) -> bool {
        match who {
            Who::Leader => self.state.needs.health <= 0,
            Who::Friend(id) => self.party.companions.iter().find(|c| c.id == id).is_none_or(|c| c.needs.health <= 0),
            Who::Person(i) => match self.people.get(i) {
                None => true,
                Some(p) if p.dead => true,
                Some(p) => match camped_as(self.camps, p.record) {
                    Some((l, at)) => self.camps.get(l).and_then(|c| c.residents.get(at)).is_none_or(|r| r.needs.health <= 0),
                    None => p.needs.health <= 0,
                },
            },
        }
    }

    fn same_place(&self, a: Who, b: Who) -> bool {
        let here = |w: Who| match w {
            Who::Person(i) => self.people.get(i).is_some_and(|p| p.location == self.here && !p.hired),
            _ => true,
        };
        here(a) && here(b)
    }

    fn attack_range(&self, who: Who) -> f32 {
        let class = self.class(who);
        if is_creature(class) {
            return encounter::attack_range(class, None);
        }
        let (pack, kit) = match who {
            Who::Leader => (&*self.carried, self.state.kit),
            Who::Friend(id) => match self.party.companions.iter().find(|c| c.id == id) {
                Some(c) => (&c.pack, c.kit),
                None => return 16.0,
            },
            Who::Person(i) => match self.people.get(i) {
                Some(p) => match camped_as(self.camps, p.record)
                    .and_then(|(l, at)| self.camps.get(l).and_then(|c| c.residents.get(at)))
                {
                    Some(r) => (&r.pack, r.kit),
                    None => (&p.pack, p.kit),
                },
                None => return 16.0,
            },
        };
        let weapon = table_combat::find_original_weapon(pack, kit.weapon, self.human(who), false, self.types);
        let range = weapon.and_then(|s| pack.get(s)).and_then(|k| self.types.get(k)).map(|t| t.attack_range);
        encounter::attack_range(class, range)
    }

    fn in_range(&self, who: Who, target: Who, range: f32) -> bool {
        match (self.position(who), self.position(target)) {
            (Some(a), Some(b)) => {
                let (dx, dy) = ((a.x - b.x) as f32, (a.y - b.y) as f32);
                dx * dx + dy * dy <= range * range
            }
            _ => false,
        }
    }

    fn strike(&mut self, attacker: Who, defender: Who) -> bool {
        if self.is_dead(attacker) || self.is_dead(defender) {
            return false;
        }
        self.blow(attacker, defender);
        true
    }

    fn begin_approach(&mut self, who: Who, target: Who, range: f32) {
        self.moves.forget(who);
        self.moves.approach.push((who, target, range));
        self.moves.fresh.push(who);
    }

    fn clear_approach(&mut self, who: Who, target: Who) {
        let before = self.moves.approach.len();
        self.moves.approach.retain(|(w, t, _)| !(*w == who && *t == target));
        if self.moves.approach.len() != before {
            self.stop(who);
        }
    }

    fn hold(&mut self, who: Who) {
        self.moves.forget(who);
        self.moves.held.push(who);
        self.stop(who);
    }

    fn release(&mut self, who: Who) {
        self.moves.held.retain(|w| *w != who);
    }

    fn flee(&mut self, who: Who, from: Who) {
        self.begin_flight(who, from);
    }
}

/// `FLEE_DURATION`, 2.5 s, in steps.
pub const FLEE_STEPS: u32 = 5 * burngfx::mind::SECOND / 2;

/// `AttackWithoutRetaliation`: a creature's blow, then it runs off
/// (`Character.cs:664-672`).
pub fn creature_bites(w: &mut World<'_>, creature: Who, bitten: Who) {
    if w.is_dead(creature) || w.is_dead(bitten) {
        return;
    }
    w.blow(creature, bitten);
    w.begin_flight(creature, bitten);
}

/// For the console: who that is.
pub fn name(w: &World<'_>, who: Who) -> String {
    let mut s = String::new();
    match who {
        Who::Leader => s.push_str("you"),
        Who::Friend(id) => {
            s.push_str("companion ");
            push_num(&mut s, id as u32);
        }
        Who::Person(i) => {
            let class = w.people.get(i).map_or(CharClass::Unknown(0), |p| p.class);
            s.push_str("the ");
            s.push_str(match class {
                CharClass::Mercenary => "mercenary",
                CharClass::Technician => "technician",
                CharClass::Doctor => "doctor",
                CharClass::Boss => "boss",
                CharClass::Mutant => "mutant",
                CharClass::Trader => "trader",
                CharClass::Dog => "dog",
                _ => "stranger",
            });
        }
    }
    s
}

fn push_num(s: &mut String, mut v: u32) {
    let mut d = [0u8; 10];
    let mut n = 0;
    loop {
        d[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        s.push(d[n] as char);
    }
}

/// `TableCombat.Strength(character, detailed: true, armour)` — the rule
/// book's, [`burngfx::modern_ai::encounter::Rules::strength`]. Only people
/// with weapons are weighed here, so the difficulty (a creature's rows)
/// does not enter.
#[allow(clippy::too_many_arguments)]
pub fn strength(
    book: &burngfx::rules::RuleBook,
    types: &burngfx::items::ItemTypes,
    pack: &burngfx::game::ItemBag,
    kit: &burngfx::combat::Kit,
    class: CharClass,
    experience: i32,
    health: i32,
    human: bool,
    hazard: burngfx::gamdat::Danger,
) -> f32 {
    use burngfx::modern_ai::encounter::{Fighter, Rules};
    let rules = Rules { book, types, difficulty: 1, hazard };
    rules.strength(&Fighter { class, experience, health, pack: pack.clone(), kit: *kit, human }, true)
}

/// `FindFleeDestination`'s turns — 45°, −45°, 90°, −90°, 135°, −135°, 180°
/// — as `(cos, sin)` of `MathF`'s radians: the exact `f32` values of
/// `angle.to_radians().cos()` and `.sin()` on the host, written out so that
/// the `no_std` AROS build, which has no `cos` or `sin` in `core`, computes
/// the same points.
const FLEE_TURNS: [(f32, f32); 7] = [
    (f32::from_bits(0x3f3504f3), f32::from_bits(0x3f3504f3)),
    (f32::from_bits(0x3f3504f3), f32::from_bits(0xbf3504f3)),
    (f32::from_bits(0xb33bbd2e), f32::from_bits(0x3f800000)),
    (f32::from_bits(0xb33bbd2e), f32::from_bits(0xbf800000)),
    (f32::from_bits(0xbf3504f3), f32::from_bits(0x3f3504f3)),
    (f32::from_bits(0xbf3504f3), f32::from_bits(0xbf3504f3)),
    (f32::from_bits(0xbf800000), f32::from_bits(0xb3bbbd2e)),
];

/// `MathF.Sqrt`, correctly rounded on both builds.
#[cfg(not(target_os = "aros"))]
pub(crate) fn sqrt(x: f32) -> f32 {
    x.sqrt()
}
#[cfg(target_os = "aros")]
pub(crate) fn sqrt(x: f32) -> f32 {
    core::f32::math::sqrt(x)
}

#[cfg(test)]
mod flee_turns {
    #[test]
    fn the_table_is_the_hosts_cos_and_sin() {
        for (i, a) in [45.0f32, -45.0, 90.0, -90.0, 135.0, -135.0, 180.0].into_iter().enumerate() {
            let r = a.to_radians();
            assert_eq!((super::FLEE_TURNS[i].0.to_bits(), super::FLEE_TURNS[i].1.to_bits()), (r.cos().to_bits(), r.sin().to_bits()));
        }
    }
}

/// The game's own blows between two riflemen: the played leader, rifle in
/// hand, against a mercenary whose own pick is his rifle, both at the
/// rifle's lowest rows, where roll 0 does nothing. On the real map's people
/// and Classic's rules.
#[cfg(test)]
mod rifle_series {
    use super::*;
    use burngfx::game::{Camps, GameState, Party};
    use burngfx::spawn::Rng;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../burngfx/testdata");

    struct Setup {
        types: &'static ItemTypes,
        book: &'static RuleBook,
        state: GameState,
        carried: ItemBag,
        party: Party,
        people: People,
        camps: Camps,
        him: usize,
    }

    /// The data, read once: the item table, the rules, the map's people
    /// and which of them is the mercenary.
    fn data() -> &'static (ItemTypes, RuleBook, People, usize) {
        static DATA: std::sync::OnceLock<(ItemTypes, RuleBook, People, usize)> = std::sync::OnceLock::new();
        DATA.get_or_init(|| {
            let rules = |f: &str| std::fs::read_to_string(format!("{DIR}/v1.2/rules/{f}")).unwrap();
            let types = ItemTypes::parse_v12(&rules("dos/items.txt")).unwrap();
            let book = RuleBook::parse(&rules("classic/game.txt"));
            let bytes = std::fs::read(format!("{DIR}/burntime.pak")).expect("testdata pack");
            let pak = burngfx::pak::ClassicPak::open(&bytes).unwrap();
            let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();
            let settings = burngfx::settings::GameSettings::parse("[0]\nnpc_respawn = 10\n", 0);
            let people = People::from_records(&gam.characters, &types, &settings);
            let him = (0..people.len()).find(|&i| people.get(i).is_some_and(|p| p.class == CharClass::Mercenary)).unwrap();
            (types, book, people, him)
        })
    }

    fn setup() -> Setup {
        let (types, book, people, him) = data();
        let mut people = people.clone();
        let rifle = types.index_of("item_loaded_rifle").unwrap();
        {
            let p = people.get_mut(*him).unwrap();
            p.pack = ItemBag::with(6, &[rifle]);
            p.kit = Kit::default();
            p.experience = 37;
            p.needs.health = 100;
            p.employer = None;
        }
        let mut state = GameState::new(Point::new(0, 0));
        state.needs.health = 100;
        state.kit = Kit { weapon: Some(0), protection: None };
        Setup { types, book, state, carried: ItemBag::with(6, &[rifle]), party: Party::default(), people, camps: Camps::default(), him: *him }
    }

    /// Exchanges of one shot each way, from `s` as it is: the damage of each
    /// shot, drawn from `rng`.
    fn exchanges(s: &mut Setup, rng: &mut Rng, count: usize) -> Vec<(i32, i32)> {
        let (mut moves, mut struck) = (Moves::default(), Vec::new());
        let walkable = |_: Point| true;
        let mut w = World {
            state: &mut s.state,
            carried: &mut s.carried,
            party: &mut s.party,
            people: &mut s.people,
            camps: &mut s.camps,
            types: s.types,
            book: s.book,
            difficulty: 1,
            rng,
            here: 0,
            hazard: burngfx::gamdat::Danger::None,
            leader_class: CharClass::Boss,
            leader_experience: 37,
            human_seats: &[0],
            moves: &mut moves,
            struck: &mut struck,
            walkable: &walkable,
            exposure: &mut burngfx::danger::Exposure::default(),
        };
        let him = Who::Person(s.him);
        (0..count).map(|_| (w.blow(Who::Leader, him), w.blow(him, Who::Leader))).collect()
    }

    /// A whole magazine of misses each way is as likely as upstream's
    /// independent rolls make it — rare, and not certain after a first
    /// double miss (the old hash of the fighters' state repeated a miss
    /// that changed nothing it read, to the end of the magazine).
    #[test]
    fn a_double_miss_does_not_repeat_itself() {
        let (mut first, mut all) = (0, 0);
        let mut rng = Rng::game(7);
        for _ in 0..10_000 {
            let mut s = setup();
            let e = exchanges(&mut s, &mut rng, 6);
            if e[0] == (0, 0) {
                first += 1;
                if e.iter().all(|&x| x == (0, 0)) {
                    all += 1;
                }
            }
        }
        std::println!("first exchange two misses: {first}; all six: {all}");
        assert!(first > 0);
        assert!(all * 20 < first, "{all} of {first}");
    }

    /// **A fight taken up again in the same process rolls on**: two
    /// exchanges, then what a save holds — healths, packs with their rounds,
    /// kits — taken up afresh, and the rest fought from it with the
    /// process's generator where the first two left it, as v1.2's static
    /// `Random` is. No save carries the generator (version 25).
    #[test]
    fn a_fight_taken_up_in_the_same_process_rolls_on() {
        let mut straight = setup();
        let whole = exchanges(&mut straight, &mut Rng::game(1993), 6);
        let mut first = setup();
        let mut rng = Rng::game(1993);
        let mut head = exchanges(&mut first, &mut rng, 2);
        let mut loaded = rng;
        let mut resumed = setup();
        resumed.state = first.state;
        resumed.carried = first.carried.clone();
        resumed.people = first.people.clone();
        head.extend(exchanges(&mut resumed, &mut loaded, 4));
        assert_eq!(head, whole);
    }
}

/// **A creature's chain of bites**, on the game's own blows: a Classic dog
/// (`dog_attack = 2 3 4 6`, `dos_unarmoured`) biting the played leader again
/// and again, the dog's health never changing and only the leader's falling
/// — the case where the rolls must still be `TableCombat.Roll`'s
/// `Random.Next(0, 4)` (`Logic/Rules/TableCombat.cs:101`): drawn afresh for
/// every blow, each independent of the last.
#[cfg(test)]
mod bite_chains {
    use super::*;
    use burngfx::game::{Camps, GameState, Party};

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../burngfx/testdata");
    const ROW: [i32; 4] = [2, 3, 4, 6];
    const CHAINS: u64 = 20_000;
    const BITES: usize = 6;

    fn data() -> &'static (ItemTypes, RuleBook, People, usize) {
        static DATA: std::sync::OnceLock<(ItemTypes, RuleBook, People, usize)> = std::sync::OnceLock::new();
        DATA.get_or_init(|| {
            let rules = |f: &str| std::fs::read_to_string(format!("{DIR}/v1.2/rules/{f}")).unwrap();
            let types = ItemTypes::parse_v12(&rules("dos/items.txt")).unwrap();
            let book = RuleBook::parse(&rules("classic/game.txt"));
            let bytes = std::fs::read(format!("{DIR}/burntime.pak")).expect("testdata pack");
            let pak = burngfx::pak::ClassicPak::open(&bytes).unwrap();
            let gam = burngfx::gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();
            let settings = burngfx::settings::GameSettings::parse("[0]\nnpc_respawn = 10\n", 0);
            let people = People::from_records(&gam.characters, &types, &settings);
            let dog = (0..people.len()).find(|&i| people.get(i).is_some_and(|p| p.class == CharClass::Dog)).unwrap();
            (types, book, people, dog)
        })
    }

    /// One chain: a fresh leader at full health, bitten `BITES` times, the
    /// rolls from the game's generator `rng`. The roll of each bite, read
    /// back from its damage (the row's four values differ).
    fn chain(rng: &mut burngfx::spawn::Rng) -> Vec<usize> {
        let (types, book, people, dog) = data();
        let mut people = people.clone();
        let mut state = GameState::new(Point::new(0, 0));
        state.needs.health = 100;
        let (mut carried, mut party, mut camps) = (ItemBag::new(6), Party::default(), Camps::default());
        let (mut moves, mut struck) = (Moves::default(), Vec::new());
        let walkable = |_: Point| true;
        let mut w = World {
            state: &mut state,
            carried: &mut carried,
            party: &mut party,
            people: &mut people,
            camps: &mut camps,
            types,
            book,
            difficulty: 1,
            rng,
            here: 0,
            hazard: burngfx::gamdat::Danger::None,
            leader_class: CharClass::Boss,
            leader_experience: 37,
            human_seats: &[0],
            moves: &mut moves,
            struck: &mut struck,
            walkable: &walkable,
            exposure: &mut burngfx::danger::Exposure::default(),
        };
        (0..BITES)
            .map(|_| {
                let d = w.blow(Who::Person(*dog), Who::Leader);
                ROW.iter().position(|&v| v == d).unwrap_or_else(|| panic!("a bite of {d}"))
            })
            .collect()
    }

    /// **RULE-42**: a blow is `Health -= dmg` through the property
    /// (`GameRules.cs:236-237`), whose getter cuts the float health
    /// (`Character.cs:196-200`): what a danger has spent short of a point
    /// goes with the blow. Health 50 and a second of radiation unprotected:
    /// 48.65, held as 48 and 0.65 above. A bite of `d`: `48 - d` (a 5-point
    /// blow, 43) and nothing above; before RULE-42 the port gave `49 - d`.
    #[test]
    fn a_blow_spends_the_danger_s_fraction() {
        let (types, book, people, dog) = data();
        let mut people = people.clone();
        let mut state = GameState::new(Point::new(0, 0));
        state.needs.health = 50;
        let mut exposure = burngfx::danger::Exposure::default();
        exposure.wear(&mut state.needs.health, 1350, burngfx::gamdat::Danger::Radiation, 0, burngfx::mind::SECOND);
        assert_eq!(state.needs.health, 48);
        assert!(exposure.carried > 0);
        let (mut carried, mut party, mut camps) = (ItemBag::new(6), Party::default(), Camps::default());
        let (mut moves, mut struck) = (Moves::default(), Vec::new());
        let walkable = |_: Point| true;
        let mut rng = burngfx::spawn::Rng::game(1993);
        let mut w = World {
            state: &mut state, carried: &mut carried, party: &mut party, people: &mut people, camps: &mut camps,
            types, book, difficulty: 1, rng: &mut rng, here: 0, hazard: burngfx::gamdat::Danger::Radiation,
            leader_class: CharClass::Boss, leader_experience: 37, human_seats: &[0], moves: &mut moves,
            struck: &mut struck, walkable: &walkable, exposure: &mut exposure,
        };
        let d = w.blow(Who::Person(*dog), Who::Leader);
        assert!(ROW.contains(&d));
        assert_eq!(state.needs.health, 48 - d);
        assert_eq!(exposure.carried, 0);
        // Without a fraction, a blow is only the blow.
        let before = state.needs.health;
        let mut w = World {
            state: &mut state, carried: &mut carried, party: &mut party, people: &mut people, camps: &mut camps,
            types, book, difficulty: 1, rng: &mut rng, here: 0, hazard: burngfx::gamdat::Danger::Radiation,
            leader_class: CharClass::Boss, leader_experience: 37, human_seats: &[0], moves: &mut moves,
            struck: &mut struck, walkable: &walkable, exposure: &mut exposure,
        };
        let d = w.blow(Who::Person(*dog), Who::Leader);
        assert_eq!(state.needs.health, before - d);
    }

    /// `FindFleeDestination` with both on one spot: one of six directions,
    /// each about a sixth of the time — `x` from `Next(0, 2)`, `y` from
    /// `Next(-1, 2)` (`Character.cs:727-735`). The port fled east every
    /// time. The points are v1.2's: `(Vector2)` adds a half and cuts toward
    /// nought (`Platform/Math/Vector2.cs:331-334`), so 70 west is −69 and
    /// the diagonals −48 and 49.
    #[test]
    fn fleeing_from_the_same_spot_takes_one_of_six_directions() {
        let (types, book, people, dog) = data();
        let mut people = people.clone();
        let here = Point::new(400, 300);
        people.get_mut(*dog).unwrap().at = here;
        let mut state = GameState::new(here);
        let (mut carried, mut party, mut camps) = (ItemBag::new(6), Party::default(), Camps::default());
        let (mut moves, mut struck) = (Moves::default(), Vec::new());
        let walkable = |_: Point| true;
        let mut rng = burngfx::spawn::Rng::game(1993);
        let mut w = World {
            state: &mut state, carried: &mut carried, party: &mut party, people: &mut people, camps: &mut camps,
            types, book, difficulty: 1, rng: &mut rng, here: 0, hazard: burngfx::gamdat::Danger::None,
            leader_class: CharClass::Boss, leader_experience: 37, human_seats: &[0], moves: &mut moves,
            struck: &mut struck, walkable: &walkable, exposure: &mut burngfx::danger::Exposure::default(),
        };
        let mut seen: Vec<((i32, i32), u32)> = Vec::new();
        for _ in 0..6000 {
            let to = w.flee_point(Who::Person(*dog), Who::Leader).unwrap();
            let d = (to.x - here.x, to.y - here.y);
            match seen.iter_mut().find(|(k, _)| *k == d) {
                Some(e) => e.1 += 1,
                None => seen.push((d, 1)),
            }
        }
        seen.sort();
        std::println!("{seen:?}");
        let ways: Vec<(i32, i32)> = seen.iter().map(|e| e.0).collect();
        assert_eq!(ways, [(-69, 0), (-48, -48), (-48, 49), (49, -48), (49, 49), (70, 0)]);
        for (_, n) in seen {
            assert!((850..1150).contains(&n), "{n}");
        }
    }

    /// Consecutive bites are independent: over every pair of neighbours in
    /// every chain, the sixteen (roll, next roll) cells are equally full —
    /// χ² with 15 degrees of freedom under 37.7 (p = 0.001). The hash the
    /// port rolled with before measured in the thousands here (the audit's
    /// `roll*.py`: only the victim's health changed, and its last round of
    /// mixing could not hide that).
    #[test]
    fn consecutive_bites_are_independent() {
        let mut cells = [[0u64; 4]; 4];
        let mut rng = burngfx::spawn::Rng::game(1993);
        for _ in 0..CHAINS {
            let rolls = chain(&mut rng);
            for p in rolls.windows(2) {
                cells[p[0]][p[1]] += 1;
            }
        }
        let n: u64 = cells.iter().flatten().sum();
        let e = n as f64 / 16.0;
        let chi: f64 = cells.iter().flatten().map(|&o| (o as f64 - e).powi(2) / e).sum();
        std::println!("pairs {n}: {cells:?}, chi2(15) = {chi:.1}");
        assert!(chi < 37.7, "chi2 {chi:.1}: {cells:?}");
    }

    /// The mean bite is the row's mean, 3.75, within one per cent — at every
    /// place in the chain, not only over all of it (the hash drifted from
    /// 3.74 at the first bite to 4.3 at the sixth).
    #[test]
    fn the_mean_bite_is_the_rows_mean_at_every_place_in_the_chain() {
        let mut sums = [0u64; BITES];
        let mut rng = burngfx::spawn::Rng::game(7);
        for _ in 0..CHAINS {
            for (i, r) in chain(&mut rng).into_iter().enumerate() {
                sums[i] += ROW[r] as u64;
            }
        }
        let means: Vec<f64> = sums.iter().map(|&s| s as f64 / CHAINS as f64).collect();
        let all = sums.iter().sum::<u64>() as f64 / (CHAINS as f64 * BITES as f64);
        std::println!("mean bite by place {means:?}, over all {all:.4}");
        assert!((all - 3.75).abs() < 0.0375, "{all}");
        // Each place has 20,000 bites: a standard error of 0.0105, so 0.06
        // is over five of them.
        for m in means {
            assert!((m - 3.75).abs() < 0.06, "{means:?}", means = sums);
        }
    }
}
