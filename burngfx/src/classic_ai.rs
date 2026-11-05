//! Classic's computer player: v0.9.1's `ClassicAiState`, `AiItemPool` and
//! `WaterGoal`, as they are.
//!
//! Ported from `source/Burntime.Remaster/AI/AiState.cs`, `AiItemPool.cs` and
//! `WaterGoal.cs` at `v0.9.1`. **Its privileges are part of it and are kept**:
//!
//! * an item pool that is never spent and grows on a schedule of days;
//! * every hire paid for with an item made out of nothing, with none of the
//!   player's refusals, and armed out of the pool;
//! * every camp given a trap out of the pool;
//! * food, water and health topped up, and meat and a full wineskin handed
//!   over, whenever it stands in its own camp or in a city;
//! * a free hand with the map: it knows what every place produces, who holds
//!   it and what danger is there, without going to look;
//! * everything lying on the ground where it stands is taken into the pool
//!   and gone from the world.
//!
//! So are the source's own slips, because they are what the game does:
//!
//! * `"item_protection_suit"` names no item (`item_protective_suit` does), so
//!   it is a placeholder type — and having one in the pool is what lets it
//!   camp in radiation, and in gas too;
//! * `NearestCity` takes no one-day road and, failing a city next door,
//!   heads for the nearest *camp site* instead, by calling the other search;
//! * `WaterGoal` counts every item with a trade value as an empty water
//!   container, and so carries nearly everything the group has into the
//!   water room.
//!
//! What the port does **not** reproduce is a crash, and there is none on this
//! path.
//!
//! The rules live here; the world is the caller's, reached through
//! [`AiWorld`]. Every effect on the world goes through that trait, so this
//! file says *what* happens and in *which order*, and the caller says how an
//! item is made, a person hired or a camp set up.
//!
//! # Randomness
//!
//! The source draws from one `System.Random`, seeded from the clock and shared
//! by the whole game, so no two runs of it agree. This port draws at the
//! **same points** — the interval when the state is made and after each camp,
//! the room a trap is put in, the spot it is dropped on when every room is
//! full — from a seeded stream of its own, which the caller supplies. The
//! stream is not `System.Random`'s; that is listed among the departures.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::gamdat::Danger;
use crate::settings::GameSettings;
use crate::spawn::Rng;

/// `AiItemPool.ItemTypeFilter`: what the pool takes. Anything else offered to
/// it is ignored.
pub const FILTER: [&str; 11] = [
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

/// `GetBestTrap` / `HasTrap`'s order.
const FOOD_PRODUCTIONS: [&str; 4] = ["item_trap", "item_snake_trap", "item_rat_trap", "item_knife"];
/// `GetBestWeapon`'s order.
const BEST_WEAPON: [&str; 5] =
    ["item_loaded_rifle", "item_loaded_pistol", "item_pitchfork", "item_axe", "item_knife"];
/// `HasWeapon`'s order — the same set, so only the membership matters.
const ANY_WEAPON: [&str; 5] =
    ["item_knife", "item_axe", "item_pitchfork", "item_loaded_pistol", "item_loaded_rifle"];
/// `HasGasMask`.
const GAS_MASK: [&str; 3] = ["item_gas_mask", "item_protection_suit", "item_paper_helmet"];
/// `HasProtectionSuit`.
const PROTECTION_SUIT: [&str; 2] = ["item_protection_suit", "item_paper_helmet"];

/// The item names the pool can hold that may not exist in an item table: the
/// source asks for them by name and gets a placeholder type back
/// (`ItemTypes[string]` makes one). `item_protection_suit` never exists;
/// Original's table has no pistol and no paper helmet.
pub const MAY_BE_PLACEHOLDERS: [&str; 3] =
    ["item_protection_suit", "item_loaded_pistol", "item_paper_helmet"];

/// `AiItemPool`: item types and how many of each have gone in.
///
/// **Nothing is ever taken out.** `Get*` generate a new item from the type
/// and leave the count alone, so one knife in the pool arms every man the
/// player ever hires. The counts are kept because a save carries them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    /// In the order types first went in.
    pub items: Vec<(String, u32)>,
}

impl Pool {
    /// `Insert`: counted if the filter lets it in, ignored otherwise.
    pub fn insert(&mut self, id: &str) {
        if !FILTER.contains(&id) {
            return;
        }
        match self.items.iter_mut().find(|(t, _)| t == id) {
            Some((_, n)) => *n += 1,
            None => self.items.push((id.to_string(), 1)),
        }
    }

    /// `FindPoolItem(params string[])`: the first of `ids`, in that order,
    /// that is in the pool.
    fn find(&self, ids: &[&'static str]) -> Option<&'static str> {
        ids.iter().copied().find(|id| self.items.iter().any(|(t, _)| t == id))
    }

    pub fn has_weapon(&self) -> bool {
        self.find(&ANY_WEAPON).is_some()
    }

    /// `GetBestWeapon`: rifle, pistol, pitchfork, axe, knife.
    pub fn best_weapon(&self) -> Option<&'static str> {
        self.find(&BEST_WEAPON)
    }

    pub fn has_gas_mask(&self) -> bool {
        self.find(&GAS_MASK).is_some()
    }

    pub fn has_protection_suit(&self) -> bool {
        self.find(&PROTECTION_SUIT).is_some()
    }

    /// `GetBestTrap(availableProducts)`: trap, snake trap, rat trap, knife —
    /// the first the pool holds **whose production makes one of the
    /// products** the place offers. `produce_of` is `Type.Production.Produce`.
    pub fn best_trap(
        &self,
        products: &[String],
        produce_of: &dyn Fn(&str) -> Option<String>,
    ) -> Option<&'static str> {
        for tool in FOOD_PRODUCTIONS {
            if !self.items.iter().any(|(t, _)| t == tool) {
                continue;
            }
            if let Some(made) = produce_of(tool) {
                if products.iter().any(|p| *p == made) {
                    return Some(tool);
                }
            }
        }
        None
    }

    pub fn has_trap(&self, products: &[String], produce_of: &dyn Fn(&str) -> Option<String>) -> bool {
        self.best_trap(products, produce_of).is_some()
    }
}

/// `ClassicAiState.Mode`.
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

    pub fn from_name(name: &str) -> Option<Mode> {
        Some(match name {
            "none" => Mode::None,
            "look" => Mode::LookForNextCamp,
            "hire" => Mode::HireNpc,
            "wait" => Mode::WaitInterval,
            _ => return None,
        })
    }
}

/// Everything `ClassicAiState` keeps between turns, which is what a save
/// carries. `settings` is not here: the source copies it in at creation and
/// it never changes, so the caller's settings of the game being played are
/// the same numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassicAi {
    pub mode: Mode,
    pub wait: i32,
    /// `headedLocation`: written, never read. Kept because it is saved.
    pub headed: Option<usize>,
    pub pool: Pool,
}

impl ClassicAi {
    /// `InitInstance`: waiting, with an interval already drawn.
    pub fn new(settings: &GameSettings, rng: &mut Rng) -> ClassicAi {
        ClassicAi { mode: Mode::WaitInterval, wait: draw_interval(settings, rng), headed: None, pool: Pool::default() }
    }
}

/// `Random.Next(MinInterval, MaxInterval)`: from `min`, below `max`. With the
/// two equal the source's `Next` returns `min`.
pub fn draw_interval(settings: &GameSettings, rng: &mut Rng) -> i32 {
    let (min, max) = settings.ai_camp_interval;
    if max <= min {
        return min;
    }
    min + rng.below((max - min) as u32) as i32
}

/// Which character goes to hold a camp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Camper {
    /// The one just hired where it stands (`GetNpcForCamp` hires first).
    JustHired,
    /// `Group[1]`: the first companion, not the last.
    First,
}

/// The world as `ClassicAiState` sees and changes it. The questions are the
/// source's; the effects are named after the source's methods and must do
/// what those do.
pub trait AiWorld {
    /// `World.Day`.
    fn day(&self) -> u32;
    /// `Player.Location`: where it is, which is where it set out from until
    /// it arrives.
    fn here(&self) -> usize;
    /// `previousLocation`, which `CanTravel` reads.
    fn previous(&self) -> Option<usize>;
    /// Which player this is.
    fn me(&self) -> usize;
    fn is_city(&self, location: usize) -> bool;
    /// `Location.Player`.
    fn owner(&self, location: usize) -> Option<usize>;
    /// `Neighbors` and `WayLengths`, in their own order.
    fn ways(&self, location: usize) -> Vec<(usize, u8)>;
    /// `GetAvailableProducts`: the item names this place can be set to make.
    fn products(&self, location: usize) -> Vec<String>;
    fn danger(&self, location: usize) -> Danger;
    /// `ItemTypes[tool].Production.Produce.ID`.
    fn produce_of(&self, tool: &str) -> Option<String>;
    /// `CampCount`: places it holds.
    fn camp_count(&self) -> usize;
    /// `MaxHumanCampCount`: the most places any player *without* an AI state
    /// holds — dead or alive.
    fn max_human_camp_count(&self) -> usize;
    /// `Player.Group.Count`, the leader included.
    fn group_count(&self) -> usize;
    /// `CanHireNpc`: somebody here who is not dead, not hired, human and not
    /// a trader.
    fn can_hire(&self) -> bool;

    /// `CurrentLocation.Items`, taken off the ground: their type names, and
    /// the items are gone.
    fn clear_ground(&mut self) -> Vec<String>;
    /// `HireNpc`: the first hireable person here, paid with their last hire
    /// item made out of nothing (the leader's pack emptied first), their own
    /// items destroyed, and armed with `weapon` — a new item of that type,
    /// chosen as their weapon.
    fn hire(&mut self, weapon: Option<&str>);
    /// `npc.JoinCamp()`, then the trap: into a random room when the place has
    /// rooms (`StoreItemRandom`), otherwise into the camper's own pack.
    fn join_camp(&mut self, who: Camper, trap: Option<&str>, rng: &mut Rng);
    /// `Player.Travel`: the whole group sets out down that road.
    fn travel(&mut self, to: usize);
    /// `RefreshGroupReserves` then `RefreshGroupAttributes`.
    fn refresh(&mut self);
    /// `WaterGoal.AlwaysDo`.
    fn water_goal(&mut self);
}

/// What the turn did, for the caller to report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Done {
    pub took: Vec<String>,
    pub hired: u32,
    pub camped: Option<usize>,
    pub set_out: Option<usize>,
    pub refreshed: bool,
}

/// `ClassicAiState.Turn`, in the source's order.
pub fn turn(ai: &mut ClassicAi, w: &mut impl AiWorld, settings: &GameSettings, rng: &mut Rng) -> Done {
    let mut done = Done::default();
    let day = w.day();

    // `FirstTurn`.
    if day == 1 {
        for id in ["item_knife", "item_knife", "item_knife", "item_rat_trap", "item_snake_trap"] {
            ai.pool.insert(id);
        }
    }

    // `UpdateItems`: the ground here, then the day's unlocks — in the
    // source's order, the knife twice on a day divisible by three.
    let took = w.clear_ground();
    for id in &took {
        ai.pool.insert(id);
    }
    done.took = took;
    let d = day;
    let unlocks: [(u32, &str); 11] = [
        (3, "item_knife"),
        (10, "item_axe"),
        (15, "item_pitchfork"),
        (20, "item_loaded_rifle"),
        (17, "item_loaded_pistol"),
        (3, "item_knife"),
        (7, "item_rat_trap"),
        (11, "item_snake_trap"),
        (14, "item_trap"),
        (25, "item_gas_mask"),
        (50, "item_protection_suit"),
    ];
    for (every, id) in unlocks {
        if d % every == 0 {
            ai.pool.insert(id);
        }
    }

    if w.group_count() == 1 {
        ai.mode = Mode::HireNpc;
    }

    loop {
        let busy = match ai.mode {
            Mode::None => {
                ai.mode = Mode::LookForNextCamp;
                true
            }
            Mode::LookForNextCamp => look_for_next_camp(ai, w, settings, rng, &mut done),
            Mode::HireNpc => hire_npc(ai, w, &mut done),
            Mode::WaitInterval => wait_interval(ai, w, settings),
        };
        if !busy {
            break;
        }
    }

    let here = w.here();
    if w.owner(here) == Some(w.me()) || w.is_city(here) {
        w.refresh();
        done.refreshed = true;
    }
    w.water_goal();
    done
}

fn below_cap(w: &impl AiWorld, settings: &GameSettings) -> bool {
    (w.max_human_camp_count() as i64 + settings.ai_camp_max_advance as i64) > w.camp_count() as i64
}

fn look_for_next_camp(
    ai: &mut ClassicAi,
    w: &mut impl AiWorld,
    settings: &GameSettings,
    rng: &mut Rng,
    done: &mut Done,
) -> bool {
    if !below_cap(w, settings) {
        ai.mode = Mode::None;
        return false;
    }
    let here = w.here();
    if can_create_camp(ai, w, here) {
        // `GetNpcForCamp`: hire somebody here if anybody will do, else the
        // first companion, else nobody.
        let camper = if w.can_hire() {
            w.hire(ai.pool.best_weapon().filter(|_| ai.pool.has_weapon()));
            done.hired += 1;
            Some(Camper::JustHired)
        } else if w.group_count() > 1 {
            Some(Camper::First)
        } else {
            None
        };
        if let Some(who) = camper {
            let products = w.products(here);
            let trap = ai.pool.best_trap(&products, &|t| w.produce_of(t));
            w.join_camp(who, trap, rng);
            done.camped = Some(here);
        }
        if w.group_count() == 1 {
            ai.mode = Mode::HireNpc;
            return true;
        }
        ai.mode = Mode::WaitInterval;
        ai.wait = draw_interval(settings, rng);
        false
    } else {
        ai.headed = nearest_free_camp(ai, w);
        if let Some(to) = ai.headed {
            w.travel(to);
            done.set_out = Some(to);
        }
        false
    }
}

fn hire_npc(ai: &mut ClassicAi, w: &mut impl AiWorld, done: &mut Done) -> bool {
    if w.can_hire() {
        w.hire(ai.pool.best_weapon().filter(|_| ai.pool.has_weapon()));
        done.hired += 1;
        ai.mode = Mode::LookForNextCamp;
        return true;
    }
    ai.headed = nearest_city(ai, w);
    if let Some(to) = ai.headed {
        w.travel(to);
        done.set_out = Some(to);
    }
    false
}

fn wait_interval(ai: &mut ClassicAi, w: &impl AiWorld, settings: &GameSettings) -> bool {
    ai.wait -= 1;
    if ai.wait <= 0 && below_cap(w, settings) {
        ai.mode = Mode::LookForNextCamp;
        return true;
    }
    false
}

/// `CanCreateCamp`.
pub fn can_create_camp(ai: &ClassicAi, w: &impl AiWorld, location: usize) -> bool {
    if w.is_city(location) || w.owner(location).is_some() {
        return false;
    }
    if !ai.pool.has_trap(&w.products(location), &|t| w.produce_of(t)) {
        return false;
    }
    match w.danger(location) {
        Danger::Gas => ai.pool.has_gas_mask(),
        Danger::Radiation => ai.pool.has_protection_suit(),
        Danger::None => true,
    }
}

/// `Player.CanTravel(from, to)`, with the player's real previous location.
fn can_travel(w: &impl AiWorld, from: usize, to: usize) -> bool {
    match w.owner(from) {
        Some(p) if p != w.me() => w.previous() == Some(to),
        _ => true,
    }
}

/// `NearestFreeCamp()`: the neighbour to set out for.
pub fn nearest_free_camp(ai: &ClassicAi, w: &impl AiWorld) -> Option<usize> {
    let mut visited = Vec::new();
    let (found, _, next) = nfc(ai, w, w.here(), &mut visited);
    found.and(next)
}

/// The recursive `NearestFreeCamp(current, out days, ref list, out next)`.
///
/// Depth first with **one visited list for the whole search**, so what is
/// found depends on the order the roads are listed in, and it is not always
/// the shortest way. That is the source's, and so this.
fn nfc(ai: &ClassicAi, w: &impl AiWorld, current: usize, visited: &mut Vec<usize>) -> (Option<usize>, i32, Option<usize>) {
    if visited.contains(&current) {
        return (None, 0, None);
    }
    visited.push(current);

    let ways = w.ways(current);
    let mut shortest = 9999i32;
    let mut nearest = None;
    for &(to, len) in &ways {
        let len = len as i32;
        if len > 0 && len < shortest && can_create_camp(ai, w, to) && can_travel(w, current, to) {
            shortest = len;
            nearest = Some(to);
        }
    }
    if nearest.is_some() {
        return (nearest, shortest, nearest);
    }

    let mut shortest = -1i32;
    let mut next = None;
    for &(to, len) in &ways {
        let len = len as i32;
        if len == 0 || !can_travel(w, current, to) {
            continue;
        }
        let (found, days, _) = nfc(ai, w, to, visited);
        if found.is_some() && (shortest == -1 || len + days < shortest) {
            shortest = len + days;
            nearest = found;
            next = Some(to);
        }
    }
    if nearest.is_some() {
        return (nearest, shortest, next);
    }
    (None, 0, None)
}

/// `NearestCity()`.
pub fn nearest_city(ai: &ClassicAi, w: &impl AiWorld) -> Option<usize> {
    let here = w.here();
    let mut visited = Vec::new();
    // The outer call: `NearestCity(CurrentLocation, …)`.
    if visited.contains(&here) {
        return None;
    }
    visited.push(here);

    let ways = w.ways(here);
    let mut shortest = 9999i32;
    let mut nearest = None;
    for &(to, len) in &ways {
        let len = len as i32;
        // `> 1`, not `> 0`: a city one day away is never taken here. And no
        // `CanTravel` — this is how it walks out of somebody else's camp in
        // any direction.
        if len > 1 && len < shortest && w.is_city(to) {
            shortest = len;
            nearest = Some(to);
        }
    }
    if nearest.is_some() {
        return nearest;
    }

    let mut shortest = -1i32;
    let mut next = None;
    for &(to, len) in &ways {
        let len = len as i32;
        if len == 0 {
            continue;
        }
        if matches!(w.owner(to), Some(p) if p != w.me()) {
            continue;
        }
        // **The source calls the other search here**: a camp site, not a
        // city. Kept.
        let (found, days, _) = nfc(ai, w, to, &mut visited);
        if let Some(loc) = found {
            let theirs = matches!(w.owner(loc), Some(p) if p != w.me());
            if !theirs && (shortest == -1 || len + days < shortest) {
                shortest = len + days;
                next = Some(to);
            }
        }
    }
    next
}

// -- the effects ----------------------------------------------------------
//
// What the source's methods do to the group and the place, as functions over
// the port's own types. The game calls these from its `AiWorld`.

use crate::game::{CharacterNeeds, Companion, ItemBag, Party, Point, WaterSource};
use crate::items::ItemTypes;

/// `RefreshGroupReserves`: meat if the leader has none; a full wineskin if he
/// has no wineskin at all; every empty wineskin he has, filled. Out of
/// nothing. A full pack simply refuses the new item, as `Items.Add` does.
pub fn refresh_reserves(pack: &mut ItemBag, types: &ItemTypes) {
    let meat = types.index_of("item_meat");
    let empty = types.index_of("item_empty_wineskin");
    let full = types.index_of("item_full_wineskin");
    let holds = |pack: &ItemBag, k: Option<usize>| k.is_some_and(|k| pack.iter().any(|x| x == k));
    if let Some(meat) = meat {
        if !holds(pack, Some(meat)) {
            pack.push(meat);
        }
    }
    if !holds(pack, empty) && !holds(pack, full) {
        if let Some(full) = full {
            pack.push(full);
        }
    }
    if let (Some(empty), Some(full)) = (empty, full) {
        for slot in 0..pack.len() {
            if pack.get(slot) == Some(empty) {
                pack.replace(slot, full);
            }
        }
    }
}

/// `Group.Heal(null, value)`: a level at a time from nought, the leader
/// first, as `Eat` and `Drink` share.
pub fn group_heal(leader: &mut CharacterNeeds, companions: &mut [Companion], mut value: i32) {
    for step in 0..100 {
        if value <= 0 {
            break;
        }
        if leader.health == step {
            value -= 1;
            leader.health += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if who.needs.health == step {
                value -= 1;
                who.needs.health += 1;
            }
        }
    }
}

/// `RefreshGroupAttributes`: `Drink(null, 10)`, `Eat(null, 10)`,
/// `Heal(null, 100)` — for everybody in the group, near or not.
pub fn refresh_attributes(leader: &mut CharacterNeeds, companions: &mut [Companion]) {
    crate::game::group_drink(leader, companions, 10);
    crate::game::group_eat(leader, companions, 10);
    group_heal(leader, companions, 100);
}

/// Where an item in the group is: the leader's pack, or a companion's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whose {
    Leader,
    Companion(usize),
}

/// `Group.GetEmptyWaterItems()`: every item whose type has a `DrinkValue`,
/// and `DrinkValue` is the trade value over four — so **every item that is
/// worth anything**, not only empty bottles. In the group's order, the leader
/// first, each pack front to back.
pub fn water_items(
    leader_pack: &ItemBag,
    companions: &[Companion],
    worth: &dyn Fn(usize) -> f32,
) -> Vec<(Whose, usize)> {
    let mut out = Vec::new();
    for slot in 0..leader_pack.len() {
        if leader_pack.get(slot).is_some_and(|k| worth(k) != 0.0) {
            out.push((Whose::Leader, slot));
        }
    }
    for (i, c) in companions.iter().enumerate() {
        for slot in 0..c.pack.len() {
            if c.pack.get(slot).is_some_and(|k| worth(k) != 0.0) {
                out.push((Whose::Companion(i), slot));
            }
        }
    }
    out
}

fn bag_of<'a>(leader_pack: &'a mut ItemBag, companions: &'a mut [Companion], w: Whose) -> Option<&'a mut ItemBag> {
    match w {
        Whose::Leader => Some(leader_pack),
        Whose::Companion(i) => companions.get_mut(i).map(|c| &mut c.pack),
    }
}

/// `WaterGoal.AlwaysDo`, everywhere it stands.
///
/// 1. The group drinks from the spring, **with the leader passed**, so only
///    members within fifty of him drink; what is left is the reserve.
/// 2. Every "empty water item" is offered to the spring: an empty container
///    whose full form holds water is filled if the reserve covers it all.
/// 3. With a water room here, the same list — worked out again — is carried
///    into it until it is full, and each thing leaves its owner's pack.
///
/// Returns how many things went into the room.
#[allow(clippy::too_many_arguments)]
pub fn water_goal(
    leader: &mut CharacterNeeds,
    leader_at: Point,
    leader_pack: &mut ItemBag,
    companions: &mut [Companion],
    spring: Option<&mut WaterSource>,
    water_room: Option<&mut ItemBag>,
    types: &ItemTypes,
    worth: &dyn Fn(usize) -> f32,
) -> usize {
    let Some(spring) = spring else { return 0 };
    spring.reserve = crate::game::group_drink_near(leader, leader_at, companions, spring.reserve);

    for (whose, slot) in water_items(leader_pack, companions, worth) {
        let Some(bag) = bag_of(leader_pack, companions, whose) else { continue };
        let Some(kind) = bag.get(slot) else { continue };
        let Some(full) = types.full_of(kind) else { continue };
        let water = types.water_of(full);
        if water != 0 && spring.reserve >= water {
            spring.reserve -= water;
            bag.replace(slot, full);
        }
    }

    let Some(room) = water_room else { return 0 };
    let list = water_items(leader_pack, companions, worth);
    let mut moved: Vec<(Whose, usize)> = Vec::new();
    for (whose, slot) in list {
        if room.is_full() {
            break;
        }
        let Some(bag) = bag_of(leader_pack, companions, whose) else { continue };
        let (Some(kind), rounds) = (bag.get(slot), bag.rounds_left(slot)) else { continue };
        if room.push(kind) {
            let last = room.len() - 1;
            room.set_rounds_left(last, rounds);
            moved.push((whose, slot));
        }
    }
    // Out of the packs, the highest slot first so the rest keep their place,
    // and each companion's drawn weapon and armour follow their items.
    let before: Vec<Vec<usize>> = companions.iter().map(|c| c.pack.iter().collect()).collect();
    moved.sort_by(|a, b| b.1.cmp(&a.1));
    for (whose, slot) in &moved {
        if let Some(bag) = bag_of(leader_pack, companions, *whose) {
            bag.remove(*slot);
        }
    }
    for (c, was) in companions.iter_mut().zip(before.iter()) {
        let now: Vec<usize> = c.pack.iter().collect();
        c.kit.follow(was, &now);
    }
    moved.len()
}

/// `HireNpc`'s bargain, after the person is chosen: the leader's pack is
/// **emptied**, the hire item — the last the person accepts — is made out of
/// nothing and handed over (`Hire` takes the first of their accepted items it
/// finds, and it is the only thing there), the person's own things are
/// destroyed, and they are armed with `weapon` out of the pool.
///
/// Returns the hire item's kind, for the food and water it brings.
pub fn ai_pays(leader_pack: &mut ItemBag, accepts: &[usize]) -> Option<usize> {
    let hire_item = *accepts.last()?;
    while !leader_pack.is_empty() {
        leader_pack.remove(0);
    }
    // Made, then taken straight back by `Hire`: the pack ends empty.
    Some(hire_item)
}

/// The hired person's needs, as `Character.Hire` leaves them: the hire item
/// eaten or drunk if it is either, and neither below `2 - difficulty`.
pub fn hired_needs(mut needs: CharacterNeeds, hire_item: usize, types: &ItemTypes, difficulty: i32) -> CharacterNeeds {
    let food = types.food_of(hire_item);
    if food != 0 {
        needs.food = (needs.food + food).min(needs.max_food);
    }
    let water = types.water_of(hire_item);
    if water != 0 {
        needs.water = (needs.water + water).min(needs.max_water);
    }
    let floor = 2 - difficulty;
    needs.food = needs.food.max(floor);
    needs.water = needs.water.max(floor);
    needs
}

/// A new companion out of a hire: nothing of their own, one weapon from the
/// pool — chosen with `SelectItem`, which takes it only if it **is** a
/// weapon. Original's table has no pistol, so a "pistol" out of the pool is a
/// placeholder, carried and never drawn.
pub fn armed_recruit(party: &mut Party, needs: CharacterNeeds, experience: i32, origin: usize,
                     stood: Point, weapon: Option<(usize, bool)>) -> usize {
    let id = party.recruit();
    let c = party.companions.last_mut().expect("recruit pushed one");
    c.origin = Some(origin);
    c.needs = needs;
    c.experience = experience;
    c.at = stood;
    c.move_to = stood;
    if let Some((w, is_weapon)) = weapon {
        if c.pack.push(w) && is_weapon {
            c.kit.weapon = Some(c.pack.len() - 1);
        }
    }
    id
}
