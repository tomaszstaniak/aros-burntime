//! v1.2's Amiga opponent (`AI/Profiles/Amiga/AmigaAiState.cs`, on
//! `Original/OriginalAiState.cs`): a computer player recovered from the
//! Amiga executable, offered by v1.2's start screen with either rule set.
//!
//! The decision loop is here, in the source's order, and the world is asked
//! for and changed only through [`AmigaWorld`]. Its random draws — the
//! expansion margin, the route slot, the strength of a blow — come from a
//! stream the caller gives it, so a game repeats itself; they are not the
//! sequence of v1.2's one `Random`.
//!
//! Upstream's comments cite the executable (`0x7286..0x729c` and so on);
//! they are kept where they explain an odd choice.

use alloc::string::String;
use alloc::vec::Vec;

use crate::spawn::Rng;

/// `OriginalAiState.Mode`. The Amiga loop only ever sets `WaitInterval`
/// (after a contested fight) and `LookForNextCamp` (on setting out); the
/// value is kept because the save keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    None,
    LookForNextCamp,
    HireNpc,
    WaitInterval,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::None => "none",
            Mode::LookForNextCamp => "look",
            Mode::HireNpc => "hire",
            Mode::WaitInterval => "wait",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        [Mode::None, Mode::LookForNextCamp, Mode::HireNpc, Mode::WaitInterval].into_iter().find(|m| m.name() == s)
    }
}

/// One seat's Amiga state: everything that is saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmigaAi {
    /// `AiSettings.Difficulty`, 0 to 2.
    pub difficulty: i32,
    pub mode: Mode,
    /// `headedLocation`.
    pub headed: Option<usize>,
    pub wait: i32,
    /// The day of the last completed trip (the route filter relaxes after
    /// three stationary days).
    pub last_arrival_day: i32,
    /// Local work still to do where it stands.
    pub maintenance_pending: bool,
}

impl AmigaAi {
    /// `InitInstance`: looking for the next camp, and — the field's own
    /// initializer — maintenance pending.
    pub fn new(difficulty: i32, day: i32) -> AmigaAi {
        AmigaAi {
            difficulty: difficulty.clamp(0, 2),
            mode: Mode::LookForNextCamp,
            headed: None,
            wait: 0,
            last_arrival_day: day,
            maintenance_pending: true,
        }
    }

    /// **`InitAfterLoad`** (`AI/Profiles/Amiga/AmigaAiState.cs:54-59`): the
    /// base's does nothing (`Original/OriginalAiState.cs:73-77`) and every
    /// field is serialized, so all a load changes is a negative
    /// `lastArrivalDay`, which becomes the load's day.
    pub fn after_load(&mut self, day: i32) {
        if self.last_arrival_day < 0 {
            self.last_arrival_day = day;
        }
    }

    fn conflict_budget(&self) -> i32 {
        [3, 6, 10][self.difficulty as usize]
    }
    fn garrison_target(&self) -> usize {
        [2, 3, 4][self.difficulty as usize]
    }
    /// `MaximumGroupSize`, the leader included.
    pub fn maximum_group(&self) -> usize {
        if self.difficulty == 0 { 3 } else { 4 }
    }
    fn normal_pair_counter(&self) -> i32 {
        [3, 4, 6][self.difficulty as usize]
    }
    fn margin(&self) -> u32 {
        [3, 7, 15][self.difficulty as usize]
    }
}

/// `NaturalHealingThreshold`: its people heal from 50 health, not 70.
pub const HEALING_THRESHOLD: i32 = 50;

/// `CanRecruit`: a mercenary, technician or doctor whose experience the
/// boss's reaches less four.
pub fn can_recruit(class: crate::gamdat::CharClass, recruit_xp: i32, boss_xp: i32) -> bool {
    use crate::gamdat::CharClass::*;
    matches!(class, Mercenary | Technician | Doctor) && boss_xp >= recruit_xp - 4
}

/// `GetProgressBenchmark`'s answer: whether the AI is held back, and by how
/// many camps its strongest rival holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Benchmark {
    pub restricts: bool,
    pub strongest: usize,
}

/// A fighter in the strategic duel, as `StrategicDamage` sees him.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duelist {
    pub health: i32,
    pub experience: i32,
    pub mercenary: bool,
    /// `SelectOriginalWeapon(allowUnloadedRifle: true)?.ID`.
    pub weapon: Option<String>,
    /// `IsWithBoss`: one of a player's travelling group.
    pub with_boss: bool,
}

/// `StrategicDamage`: a weapon's basis plus experience, at least a
/// day-dependent minimum for a group member, and a random 0..3 (0..7 with a
/// pitchfork, or for a group member from day 200).
pub fn strategic_damage(d: &Duelist, day: i32, rng: &mut Rng) -> i32 {
    let basis = match d.weapon.as_deref() {
        Some("item_knife") => 9,
        Some("item_axe") => 12,
        Some("item_pitchfork") => 14,
        Some("item_loaded_rifle") => 20,
        Some("item_unloaded_rifle") => 6,
        _ => 5,
    };
    let mut mask = if d.weapon.as_deref() == Some("item_pitchfork") { 7 } else { 3 };
    let experience = if d.mercenary { d.experience } else { d.experience / 2 };
    let mut damage = basis + basis * experience / 100;
    if d.with_boss {
        let minimum = match day {
            ..100 => 0,
            100..200 => 12,
            200..300 => 18,
            _ => 25,
        };
        damage = damage.max(minimum);
        if day >= 200 {
            mask = 7;
        }
    }
    damage + rng.below(mask + 1) as i32
}

/// `ProductionCapacity`: how many a camp's production feeds, by production
/// id (0 knife … 3 trap) and tools in it.
pub fn production_capacity(production: Option<usize>, tools: usize) -> usize {
    match (production, tools) {
        (None, _) => 0,
        (Some(0), 0) => 1,
        (Some(0), _) => 3,
        (Some(1), 0) => 0,
        (Some(1), 1) => 3,
        (Some(1), _) => 4,
        (Some(2), 0) => 0,
        (Some(2), 1) => 4,
        (Some(2), _) => 5,
        (Some(3), 0) => 0,
        (Some(3), 1) => 5,
        (Some(3), _) => 7,
        _ => 0,
    }
}

/// `ApplyArrivalRecovery`'s numbers: `(food, health)` added to each of the
/// party. Food only where nobody else holds the place.
pub fn arrival_recovery(arrived_in_city: bool, left_hostile_camp: bool) -> (i32, i32) {
    let hostile = arrived_in_city && left_hostile_camp;
    let food = if hostile { 3 } else { 9 };
    let health = if hostile { 20 } else if arrived_in_city { 30 } else { 0 };
    (food, health)
}

/// What the loop needs of the world, and what it may do to it.
pub trait AmigaWorld {
    fn day(&self) -> i32;
    fn me(&self) -> usize;
    /// `Player.Location`.
    fn here(&self) -> usize;
    fn travelling(&self) -> bool;
    fn dead(&self) -> bool;
    fn is_city(&self, location: usize) -> bool;
    /// `Location.Player`.
    fn owner(&self, location: usize) -> Option<usize>;
    /// `Neighbors`, in their own order.
    fn neighbors(&self, location: usize) -> Vec<usize>;
    /// `Player.CanTravel(from, to)`.
    fn can_travel(&self, from: usize, to: usize) -> bool;
    /// `PreviousLocation`.
    fn previous(&self) -> Option<usize>;
    /// `Location.Production`, as its id.
    fn production(&self, location: usize) -> Option<usize>;
    /// `ValidProductions` contains this id.
    fn production_valid(&self, location: usize, production: usize) -> bool;
    /// `GetProductionToolCount(Production)`.
    fn production_tools(&self, location: usize) -> usize;
    /// `Source.Water`: the spring's output by `water_output`.
    fn water_output(&self, location: usize) -> i32;
    /// `Danger.Type`: `"gas"`, `"radiation"` or none.
    fn danger(&self, location: usize) -> Option<&'static str>;
    /// Living campers of this player here (`CampNPC` of its own).
    fn stationed(&self, location: usize) -> usize;
    /// Places this player holds.
    fn owned_camps(&self) -> usize;
    /// `GetProgressBenchmark`.
    fn benchmark(&self) -> Benchmark;
    /// `Player.Party.Count`, the leader included.
    fn group_size(&self) -> usize;
    /// The leader's food.
    fn leader_food(&self) -> i32;

    /// `OffsetDailyWaterConsumption`: every one of the party to full water.
    fn fill_water(&mut self);
    /// `RecalculateExperience`: the boss's experience by `boss_experience`.
    fn recalculate_experience(&mut self);
    /// `ApplyArrivalRecovery`, then `OriginalItemRecords.CleanupAmiga`.
    fn recover_group(&mut self);
    /// The first person here who is not dead, hired, a creature or a trader
    /// and passes [`can_recruit`], hired without payment or supplies
    /// (`HireWithoutInitialization`). Whether anyone was.
    fn recruit_one(&mut self) -> bool;
    /// Enemy defenders here (`GetCampDefenders`): their campers and every
    /// enemy party standing here, living. Opaque ids.
    fn defenders(&self) -> Vec<usize>;
    /// This player's party, the leader first. Opaque ids.
    fn attackers(&self) -> Vec<usize>;
    /// The leader's id among [`AmigaWorld::attackers`].
    fn leader(&self) -> usize;
    fn duelist(&self, id: usize) -> Option<Duelist>;
    /// `SelectOriginalWeapon(allowUnloadedRifle: true)`: the weapon
    /// [`Duelist::weapon`] names becomes the fighter's chosen one
    /// (`Character.cs:700-704`), which the next look starts from.
    fn select_weapon(&mut self, id: usize);
    /// `Health -= amount`. The world settles the death, and frees a camp
    /// whose last camper this blow killed at once (`Die` → `Dismiss`).
    fn hurt(&mut self, id: usize, amount: i32);
    /// `current.Player = null`.
    fn free_here(&mut self);
    /// `JoinCamp` for the first living companion, with a protective item
    /// made for him and chosen (`Protection = item`) when named. Whether one
    /// was stationed.
    fn station_first(&mut self, protection: Option<&str>) -> bool;
    /// `TryCreateCampItem`: an item made out of nothing into the first room
    /// here with space (the spring's room preferred when asked), and whether
    /// there was one.
    fn create_camp_item(&mut self, item: &str, water_room: bool) -> bool;
    /// `Location.Production = Productions[id]`.
    fn set_production(&mut self, production: usize);
    /// `Player.Travel`.
    fn travel(&mut self, to: usize);
    /// A line for the console (`AiTelemetry.Report`).
    fn report(&mut self, what: String);
}

impl AmigaAi {
    /// `ExpansionTarget`: unrestricted without a rival to measure by;
    /// otherwise the strongest rival's camps plus a random margin.
    fn expansion_target(&self, w: &impl AmigaWorld, rng: &mut Rng) -> usize {
        let b = w.benchmark();
        if !b.restricts {
            return usize::MAX;
        }
        b.strongest + rng.below(self.margin() + 1) as usize
    }

    /// `AmigaAiState.Turn`.
    pub fn turn(&mut self, w: &mut impl AmigaWorld, rng: &mut Rng) {
        if w.dead() {
            return;
        }
        w.fill_water();
        w.recalculate_experience();
        if w.travelling() {
            return;
        }
        let here = w.here();
        let arrived = self.headed == Some(here);
        if arrived {
            self.headed = None;
            self.maintenance_pending = true;
        }
        if self.maintenance_pending {
            w.recover_group();
            self.last_arrival_day = w.day();
            // 0x7286..0x729c: only this branch does local work and clears the
            // pending state.
            if self.expansion_target(w, rng) > w.owned_camps() {
                self.recruit_one(w);
                self.resolve_opposition(w, rng);
                self.claim_or_reinforce(w);
                self.maintenance_pending = false;
            }
        }
        let expansion = self.expansion_target(w, rng) > w.owned_camps();
        if let Some(next) = self.choose_next_step(w, expansion, rng) {
            self.headed = Some(next);
            self.mode = Mode::LookForNextCamp;
            w.travel(next);
            let mut line = String::from("travels toward location ");
            push_num(&mut line, next as u32);
            line.push_str(" using original amiga AI");
            w.report(line);
        }
    }

    fn recruit_one(&mut self, w: &mut impl AmigaWorld) -> bool {
        // `HasRecruitmentCapacity`: `Party.Count < MaximumGroupSize`.
        if w.group_size() >= self.maximum_group() {
            return false;
        }
        let hired = w.recruit_one();
        if hired {
            w.report(String::from("recruited somebody for free"));
        }
        hired
    }

    /// `ChooseNextStep`.
    fn choose_next_step(&self, w: &impl AmigaWorld, mut expansion: bool, rng: &mut Rng) -> Option<usize> {
        let current = w.here();
        let me = w.me();
        let owner = w.owner(current);
        // From an enemy camp: back the way it came, or nowhere.
        if owner.is_some_and(|o| o != me) {
            return w.previous().filter(|&p| w.can_travel(current, p));
        }
        // A free producing camp first looks for an adjacent own camp; with
        // none, maintenance routing.
        if !w.is_city(current) && owner.is_none() && w.production(current).is_some() {
            if let Some(back) = w.neighbors(current).into_iter().find(|&l| w.owner(l) == Some(me) && w.can_travel(current, l)) {
                return Some(back);
            }
            expansion = false;
        }
        // One pseudo-random route slot of four.
        let neighbors = w.neighbors(current);
        if neighbors.is_empty() {
            return None;
        }
        let mut slot = rng.below(4) as usize;
        if slot >= neighbors.len() {
            slot &= 1;
        }
        if slot >= neighbors.len() {
            slot = 0;
        }
        let candidate = neighbors[slot];
        if !w.can_travel(current, candidate) {
            return None;
        }
        let stationary = w.day() - self.last_arrival_day;
        if stationary == 0 {
            return None;
        }
        if stationary >= 3 {
            return Some(candidate);
        }
        if expansion {
            return match w.owner(candidate) {
                None => Some(candidate),
                Some(o) if o == me => {
                    let capacity = production_capacity(w.production(candidate), w.production_tools(candidate)) as i32;
                    (capacity - w.stationed(candidate) as i32 > 1).then_some(candidate)
                }
                Some(_) => (w.leader_food() >= 8).then_some(candidate),
            };
        }
        (w.is_city(candidate) || w.owner(candidate) == Some(me)).then_some(candidate)
    }

    /// `TryClaimOrReinforceCurrentCamp`.
    fn claim_or_reinforce(&mut self, w: &mut impl AmigaWorld) -> bool {
        let current = w.here();
        let me = w.me();
        let owner = w.owner(current);
        if w.is_city(current) || owner.is_some_and(|o| o != me) {
            return false;
        }
        let stationed = w.stationed(current);
        if owner == Some(me) {
            let capacity = production_capacity(w.production(current), w.production_tools(current)) as i32;
            if capacity - stationed as i32 <= 1 {
                return self.upgrade_production(w);
            }
            if w.water_output(current) - stationed as i32 <= 1 {
                return self.install_pump(w);
            }
        }
        if stationed >= self.garrison_target() {
            return false;
        }
        let protection = match w.danger(current) {
            Some("gas") => {
                if stationed >= 2 || w.day() < 80 {
                    return false;
                }
                Some("item_gas_mask")
            }
            Some(_) => {
                if stationed >= 1 || w.day() < 200 {
                    return false;
                }
                Some("item_protective_suit")
            }
            None => None,
        };
        let claimed = owner.is_none();
        if !w.station_first(protection) {
            return false;
        }
        if claimed && w.create_camp_item("item_knife", false) {
            w.set_production(0);
        }
        let mut line = String::from(if claimed { "claimed location " } else { "stationed a man at location " });
        push_num(&mut line, current as u32);
        w.report(line);
        true
    }

    fn install_pump(&mut self, w: &mut impl AmigaWorld) -> bool {
        let item = if w.day() >= 300 {
            "item_industrial_pump"
        } else if w.day() >= 100 {
            "item_hand_pump"
        } else {
            return false;
        };
        if !w.create_camp_item(item, true) {
            return false;
        }
        let mut line = String::from("installed ");
        line.push_str(item);
        w.report(line);
        true
    }

    fn upgrade_production(&mut self, w: &mut impl AmigaWorld) -> bool {
        let here = w.here();
        let Some(current) = w.production(here) else { return false };
        if w.day() < 30 || current >= 3 {
            return false;
        }
        let next = current + 1;
        if !w.production_valid(here, next) {
            return false;
        }
        let item = match next {
            1 => "item_rat_trap",
            2 => "item_snake_trap",
            _ => "item_trap",
        };
        if !w.create_camp_item(item, false) {
            return false;
        }
        w.set_production(next);
        let mut line = String::from("upgraded with ");
        line.push_str(item);
        w.report(line);
        true
    }

    /// `ResolveCurrentOpposition`. Whether the place stays contested.
    fn resolve_opposition(&mut self, w: &mut impl AmigaWorld, rng: &mut Rng) -> bool {
        let current = w.here();
        let me = w.me();
        if w.owner(current).is_none_or(|o| o == me) {
            return false;
        }
        let defenders = w.defenders();
        if defenders.is_empty() {
            w.free_here();
            return false;
        }
        let leader = w.leader();
        let day = w.day();
        let alive = |w: &dyn Fn(usize) -> Option<Duelist>, id: usize| w(id).is_some_and(|d| d.health > 0);
        for attacker in w.attackers() {
            let mut next_attacker = false;
            for &defender in &defenders {
                if w.duelist(defender).is_none_or(|d| d.health <= 0) {
                    continue;
                }
                // "The original rolls each fighter's strength once per
                // pairing", each choosing his weapon as he does.
                w.select_weapon(attacker);
                let Some(a) = w.duelist(attacker) else { continue };
                let attack = strategic_damage(&a, day, rng);
                w.select_weapon(defender);
                let Some(d) = w.duelist(defender) else { continue };
                let retaliation = strategic_damage(&d, day, rng);
                let mut exchanges = if attack > retaliation {
                    self.normal_pair_counter() + 1
                } else if a.health >= 80 {
                    3
                } else {
                    0
                };
                if exchanges == 0 {
                    continue;
                }
                let mut remaining = self.conflict_budget();
                let mut killed = false;
                while exchanges > 0 && remaining > 0 {
                    exchanges -= 1;
                    let get = |id: usize| w.duelist(id);
                    if !alive(&get, attacker) || !alive(&get, defender) {
                        break;
                    }
                    // The boss withdraws from a pairing at 65.
                    if attacker == leader && w.duelist(attacker).is_some_and(|x| x.health <= 65) {
                        break;
                    }
                    w.hurt(defender, attack);
                    if w.duelist(defender).is_none_or(|x| x.health <= 0) {
                        killed = true;
                        break;
                    }
                    w.hurt(attacker, retaliation);
                    remaining -= 1;
                }
                // An exhausted budget ends the whole pass; a surviving
                // defender sends the next attacker in; only a kill lets the
                // same one go on.
                if remaining == 0 {
                    return self.contested(w, current);
                }
                if !killed {
                    next_attacker = true;
                    break;
                }
            }
            if next_attacker {
                continue;
            }
        }
        if defenders.iter().all(|&d| w.duelist(d).is_none_or(|x| x.health <= 0)) {
            w.free_here();
            let mut line = String::from("defeated the opposition at location ");
            push_num(&mut line, current as u32);
            w.report(line);
            return false;
        }
        self.contested(w, current)
    }

    fn contested(&mut self, w: &mut impl AmigaWorld, current: usize) -> bool {
        self.mode = Mode::WaitInterval;
        self.wait = 1;
        let mut line = String::from("remains contested at location ");
        push_num(&mut line, current as u32);
        w.report(line);
        true
    }
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

/// `ItemTypes.GetOriginalTitleId`: `0x33 +` the item's place in the classic
/// sprite list, which is its `burngfx`; an item with none is `0x32`.
pub fn title_id(sprite: Option<u8>) -> i32 {
    0x33 + sprite.map_or(-1, |s| s as i32)
}

/// The production tool the cleanup spares, by production id.
pub fn production_tool_title(production: Option<usize>) -> i32 {
    match production {
        Some(0) => 0x3d,
        Some(1) => 0x43,
        Some(2) => 0x45,
        Some(3) => 0x44,
        _ => 0,
    }
}

/// What the cleanup does to one record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sweep {
    /// Into that party member's pack.
    Take { record: usize, member: usize },
    /// Out of the world.
    Destroy { record: usize },
}

/// `OriginalItemRecords.CleanupAmiga`: [`sweep`] over the ground's records,
/// then — `rooms` given, in a free or own place — over the rooms', each pass
/// with its own "already threw something away" (`discardedOther` is local
/// to `SweepRecords`, `Logic/Rules/OriginalItemRecords.cs:50-54`) and with
/// the free slots counted again from the packs (`6 - Items.Count`): what the
/// ground pass took fills a slot, what it threw away does not.
///
/// `free` is each member's free slots before the cleanup.
pub fn cleanup(ground: &[i32], rooms: Option<&[i32]>, free: &[usize], production_tool: i32) -> (Vec<Sweep>, Vec<Sweep>) {
    let mut left = free.to_vec();
    let first = sweep(ground, &mut left, production_tool);
    let Some(rooms) = rooms else { return (first, Vec::new()) };
    let mut left = free.to_vec();
    for s in &first {
        if let Sweep::Take { member, .. } = *s {
            left[member] -= 1;
        }
    }
    let second = sweep(rooms, &mut left, production_tool);
    (first, second)
}

/// `OriginalItemRecords.SweepRecords` over one run of records (the ground's,
/// then — in a free or own place — the rooms'): each party member in turn,
/// while he has a free slot. Food is thrown away without spending a slot;
/// the place's production tool is left; a weapon is taken (or left, if the
/// pack is full); a trap is thrown away; and the first other thing is thrown
/// away — after any record of this run is dealt with, no further "other"
/// thing is.
///
/// `titles` are the records' title ids, in order; `free` each member's free
/// slots (`6 - Items.Count`), spent as records are dealt with.
pub fn sweep(titles: &[i32], free: &mut [usize], production_tool: i32) -> Vec<Sweep> {
    let mut out = Vec::new();
    let mut cursor = 0;
    let mut discarded = false;
    for member in 0..free.len() {
        while free[member] > 0 && cursor < titles.len() {
            let record = cursor;
            let id = titles[cursor];
            cursor += 1;
            if (0x33..0x37).contains(&id) {
                out.push(Sweep::Destroy { record });
                continue;
            }
            if id == production_tool {
                continue;
            }
            if (0x3d..0x42).contains(&id) {
                // `character.Items.Add` refuses a full pack; the slot count
                // says it is not full here.
                out.push(Sweep::Take { record, member });
            } else if (0x43..=0x45).contains(&id) || !discarded {
                out.push(Sweep::Destroy { record });
            } else {
                continue;
            }
            discarded = true;
            free[member] -= 1;
        }
    }
    out
}
