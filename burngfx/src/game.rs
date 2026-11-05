//! Where you are, where you are walking, and what you can enter.
//!
//! Deliberately small and explicit. The remaster carries a reflection-driven
//! object graph that save games and multiplayer both ride on; nothing here
//! needs it yet, and inventing it early would be inventing the wrong one.
//!
//! No rendering and no SDL, so all of this is testable on its own.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::zei::{WalkGrid, MASK_CELL, MASK_GRID};

use crate::map::Map;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    /// Squared distance — comparisons do not need the square root, and this
    /// keeps the whole module in integers.
    pub fn distance_squared(self, other: Point) -> i64 {
        let (dx, dy) = ((self.x - other.x) as i64, (self.y - other.y) as i64);
        dx * dx + dy * dy
    }
}

/// Which of the game's two modes is being played.
///
/// The remaster's own choice, and in the source it is one flag with two
/// consequences — a different item table, and (in v1.1 only) a
/// different settings file. Nothing else in the engine asks about it.
///
/// [`Mode::Original`] is the default because matching the 1993 game is the
/// point; the extended rules are Jakob Harder's additions and are worth
/// having, not worth defaulting to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// The remaster's rendition of the 1993 rules.
    #[default]
    Original,
    /// The remaster's own additions on top of them.
    Extended,
}

impl Mode {
    /// The extended list is the longer one, and it is the file without a
    /// suffix — which is the wrong way round from every other pair here.
    /// Both releases choose it the same way.
    pub fn items_file(&self) -> &'static str {
        match self {
            Mode::Original => "items_original.txt",
            Mode::Extended => "items.txt",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Mode::Original => "original",
            Mode::Extended => "extended",
        }
    }
}

/// Which release's behaviour is being reproduced.
///
/// **A second axis, and not the same one as [`Mode`].** A rule can differ
/// between the two modes, between the two releases, or neither — and reading
/// "older" as "original" is a mistake this project has already made once, in
/// the pump formula below.
///
/// The two axes cross in the settings file, which is why this is not a detail:
///
/// | | v0.9.1 | v1.1 |
/// |---|---|---|
/// | items | by mode | by mode |
/// | settings | **one file for both modes** | by mode |
///
/// v0.9.1 has a single `gamesettings.txt`; splitting it in two is a later
/// change. Feeding v0.9.1's code from v1.1's settings is
/// therefore a thing that can happen without anybody noticing, and did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Engine {
    /// The last tag of the v0.9 series, which set out to match the DOS build.
    #[default]
    V091,
    /// **Three rules taken from v1.1, on top of v0.9.1. Not v1.1.**
    ///
    /// The tag is `v1.1`, commit `ddfad97`, and the three are listed in
    /// [`Engine::SWITCHES`]. Everything else here is still v0.9.1's, so this
    /// is a ruleset of ours that borrows, not a second release reproduced.
    /// It keeps the name `current` on disk because saves carry it.
    Current,
}

impl Engine {
    /// **What this axis actually switches, today.**
    ///
    /// Not "all of v0.9.1" against "all of v1.1" — most of this
    /// project is ported from v0.9.1 and does not branch at all. Naming the
    /// switch is not the same as having two engines, and the list is short
    /// enough to keep honest:
    ///
    /// | | switched by `Engine` |
    /// |---|---|
    /// | settings file | yes — [`Engine::settings_file`] |
    /// | pump boost | yes — [`pump_boost`] |
    /// | production when a location is full | yes — [`production_turn`] |
    /// | item generation | **no**, v0.9.1's `LoadItems` in both |
    /// | the day, travel, springs, rooms | **no**, v0.9.1 in both |
    ///
    /// So [`Engine::Current`] means "v0.9.1 with these three differences",
    /// not v1.1. Anything else it would have to change has not
    /// been read yet, let alone written. Between the two tags the reference
    /// changes 220 C# files; three rules out of them are here.
    pub const SWITCHES: &'static str = "settings file, pump boost, production when full";

    /// Which settings file this release reads for that mode.
    pub fn settings_file(&self, mode: Mode) -> &'static str {
        match (self, mode) {
            // One file, whichever mode: `new GameSettings("gamesettings.txt")`
            // takes no argument about the mode in v0.9.1.
            (Engine::V091, _) => "gamesettings.txt",
            (Engine::Current, Mode::Original) => "gamesettings_original.txt",
            (Engine::Current, Mode::Extended) => "gamesettings_extended.txt",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Engine::V091 => "v091",
            Engine::Current => "current",
        }
    }
}

/// A water pump's contribution to a spring's yield.
///
/// **A difference between releases, not between modes.** `WaterSource.GetBoost`
/// does not consult the game mode at all: v0.9.1 has one formula and `v1.1`
/// has the other, and every mode gets whichever its release carries.
/// An earlier version of this code keyed the two on the mode, which quietly
/// turned a decision of ours into a claim about the remaster.
///
/// Reached through [`WaterSource::begin_turn`], which is `Reserve += Water`
/// with `Water`'s getter folded in, and [`WaterSource::yield_with`].
pub fn pump_boost(engine: Engine, water: i32, pump: Pump) -> i32 {
    let (floor, divisor) = match pump {
        Pump::None => return 0,
        Pump::Hand => (2, 4),
        Pump::Industrial => (5, 2),
    };
    match engine {
        // v0.9.1: `water == 1 ? floor : water / divisor`.
        Engine::V091 => {
            if water == 1 {
                floor
            } else {
                water / divisor
            }
        }
        // v1.1: `max(floor, water / divisor)`. It takes out v0.9.1's cliff,
        // where a spring yielding 2 or 3 got less from a pump than one
        // yielding 1, and a hand pump on 2 or 3 added nothing whatever.
        Engine::Current => floor.max(water / divisor),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pump {
    None,
    Hand,
    Industrial,
}

/// Which of the game's two levels you are standing on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// The desert, `MAT_000`.
    World,
    /// Inside location `n`, whose map is `MAT_<n+1>`.
    Location(usize),
}

/// What a location's own settings say about it.
///
/// `entry_point` is `Option` on purpose. The classic entry points live in
/// `GAMDAT`, which is part of an original installation and which we do not
/// have; only a couple of maps carry one in their `.txt`. Keeping it optional
/// means a fallback stays visibly a fallback instead of quietly becoming data.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LocationData {
    pub entry_point: Option<Point>,
}

/// What one character needs to stay alive: water and food, and health.
///
/// The original's `Character.MaxWater` is a constant 5, and `MaxFood` is 9.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CharacterNeeds {
    pub water: i32,
    pub max_water: i32,
    pub food: i32,
    pub max_food: i32,
    pub health: i32,
}

/// `Character.MaxFood` is 9 and `GameSettings.StartFood` is the same, so a
/// character begins a game with no appetite at all.
pub const MAX_FOOD: i32 = 9;
/// `Character.MaxWater` is 5, and `GameSettings.StartWater` matches it.
pub const MAX_WATER: i32 = 5;

/// `GameSettings.StartHealth` is 100, and `Character` caps health there too.
pub const START_HEALTH: i32 = 100;
pub const MAX_HEALTH: i32 = 100;
/// Health lost per empty need, each day. Food and water are counted
/// separately, so a character with neither loses fifty.
pub const DEPRIVATION: i32 = 25;
/// What heals overnight with no doctor in the group, and the health you must
/// already have for it. Below the floor you do not recover at all — which is
/// what makes a bad day hard to climb out of.
pub const REGEN_WITHOUT_DOCTOR: i32 = 2;
pub const REGEN_FLOOR_WITHOUT_DOCTOR: i32 = 70;

impl CharacterNeeds {
    /// A character as the game starts one: `GameSettings.StartWater` is 5,
    /// which is also `MaxWater`, so a fresh character is not thirsty and would
    /// drink nothing at a spring until travel or a turn has cost them some.
    pub fn at_game_start() -> CharacterNeeds {
        CharacterNeeds { water: 5, max_water: 5, food: MAX_FOOD, max_food: MAX_FOOD,
                         health: START_HEALTH }
    }

    /// Empty of food and water, health untouched.
    ///
    /// This was the game's starting state while there was no time in the
    /// game and drinking at a spring had to be made visible somehow. It is
    /// not that any more — the game starts as the original does — but it is
    /// what most of the tests here want, because a character with nothing
    /// left is where every interesting rule bites.
    pub fn parched_for_demonstration() -> CharacterNeeds {
        CharacterNeeds { water: 0, max_water: 5, food: 0, max_food: MAX_FOOD,
                         health: START_HEALTH }
    }

    /// Eat `food_value` points' worth, and say how much of it went to waste.
    ///
    /// This is `ICharacterCollection.Eat` as `Character` implements it, which
    /// is the one-character case: fill up to `max_food` and hand back the
    /// rest. `Group.Eat` fills a party a level at a time so that nobody is
    /// fed twice while somebody is hungry, and with one person the two agree.
    ///
    /// A return equal to `food_value` means nobody ate — which is what
    /// decides whether the item is used up. See [`eaten`].
    pub fn eat(&mut self, food_value: i32) -> i32 {
        let eaten = (self.max_food - self.food).min(food_value).max(0);
        self.food += eaten;
        food_value - eaten
    }

    /// Drink `water_value` points' worth, and say how much went to waste.
    ///
    /// `ICharacterCollection.Drink` as `Character` implements it, the twin of
    /// [`CharacterNeeds::eat`]. A return equal to `water_value` means nobody
    /// drank, and then the container is not used up — see [`eaten`], which
    /// serves both.
    pub fn drink_value(&mut self, water_value: i32) -> i32 {
        let drunk = (self.max_water - self.water).min(water_value).max(0);
        self.water += drunk;
        water_value - drunk
    }
}

/// Was the item used up?
///
/// `InventoryScene` removes it only when someone actually ate: `left <
/// item.FoodValue`. Anything eaten at all consumes the whole thing — a snake
/// worth seven eaten by someone with one point of room is still a whole
/// snake gone. Nothing partial is kept.
pub fn eaten(food_value: i32, left: i32) -> bool {
    left < food_value
}

impl Default for CharacterNeeds {
    /// The game's own starting state.
    fn default() -> CharacterNeeds {
        CharacterNeeds::at_game_start()
    }
}

/// A location's spring.
///
/// The original names the per-turn yield `Water` and starts `Reserve` at the
/// same number; `refresh` is the clearer name for the same thing. Nob Hill
/// yields 5 a turn and holds at most 6.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct WaterSource {
    pub refresh: i32,
    pub reserve: i32,
    pub capacity: i32,
}

impl WaterSource {
    /// From a location's settings, the way `LocationCreator` builds one:
    /// the reserve starts equal to the refresh rate.
    pub fn new(refresh: i32, capacity: i32) -> WaterSource {
        WaterSource { refresh, reserve: refresh, capacity }
    }

    /// From `gam.dat`, the way `OriginalLocationCreator` builds one — and
    /// **the reserve is not the refresh rate**: it is whatever the 1993 save
    /// says the spring holds on day one. Muarab's is zero, so the first
    /// person to walk in on the first day draws nothing at all. Building it
    /// the other way makes the game open with a free drink.
    pub fn classic(reserve: i32, refresh: i32, capacity: i32) -> WaterSource {
        WaterSource { refresh, reserve, capacity }
    }

    /// Drink your fill, and take it out of the spring.
    ///
    /// This is the original's `Drink` with the caller's assignment folded in:
    /// it takes `min(room left in the character, what the spring has)`, which
    /// is why an empty spring is not an error and a full character is not
    /// either. Returns how much was actually drunk.
    pub fn drink(&mut self, who: &mut CharacterNeeds) -> i32 {
        let room_left = (who.max_water - who.water).max(0);
        let drink = room_left.min(self.reserve.max(0));
        who.water += drink;
        self.reserve -= drink;
        drink
    }

    /// A turn's yield, pump included.
    ///
    /// `BeginTurn` is `Reserve += Water`, and `Water` is not the stored
    /// number: its getter is `water + GetBoost()`. So the boost is not a
    /// separate addition made once — it is part of every reading of the
    /// spring, and this is where it reaches the reserve.
    pub fn begin_turn(&mut self, engine: Engine, pump: Pump) {
        self.reserve += self.yield_with(engine, pump);
    }

    /// What one turn adds, which is what INFO's water line should say.
    pub fn yield_with(&self, engine: Engine, pump: Pump) -> i32 {
        self.refresh + pump_boost(engine, self.refresh, pump)
    }

    /// The reserve is capped at the end of a turn, not when it is filled —
    /// so a spring can briefly hold more than its capacity, which is what the
    /// original does.
    pub fn end_turn(&mut self) {
        if self.reserve > self.capacity {
            self.reserve = self.capacity;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GameState {
    pub place: Place,
    /// Where the character stands, in map pixels.
    pub position: Point,
    /// Where they stood on the world map before entering a location.
    pub return_position: Option<Point>,
    /// Not advanced yet: the rules for time are a separate question.
    pub day: u32,
    pub needs: CharacterNeeds,
    /// What the played character has drawn out of their pack.
    pub kit: crate::combat::Kit,
    /// **Speak or fight.** `Player.FightMode`, set from the right-button
    /// menu, decides what a click on somebody does: talk to them, or hit
    /// them. It belongs to the player rather than to the scene, so it
    /// survives leaving a location — but not entering a city, where
    /// `OnActivateScene` clears it and the menu does not offer it.
    pub fight_mode: bool,
    /// **Single mode**: the leader acts alone and the group stands where it
    /// is. `Player.SingleMode`, set from the menu's single/all entry, is a
    /// plain field of a serialised class — so it survives a save, and so does
    /// ours. It does not stop a nearby companion joining the leader's blow,
    /// nor the leader waiting for the group before an interaction: v0.9.1
    /// checks it in `FellowerMind` and nowhere else that matters here.
    pub single_mode: bool,
    pub mode: Mode,
    pub engine: Engine,
}

impl GameState {
    pub fn new(position: Point) -> GameState {
        GameState {
            place: Place::World,
            position,
            return_position: None,
            day: 0,
            needs: CharacterNeeds::default(),
            kit: crate::combat::Kit::default(),
            fight_mode: false,
            single_mode: false,
            mode: Mode::Original,
            engine: Engine::V091,
        }
    }

    /// Walk into a location, remembering where we came from.
    ///
    /// `spawn` is the caller's decision: a location's real entry point when it
    /// has one, otherwise a stand-in.
    pub fn enter(&mut self, location: usize, spawn: Point) {
        if let Place::World = self.place {
            self.return_position = Some(self.position);
        }
        self.place = Place::Location(location);
        self.position = spawn;
    }

    /// Back to the desert, at the spot we left. Does nothing on the world map.
    pub fn leave(&mut self) {
        if let Place::Location(_) = self.place {
            self.place = Place::World;
            if let Some(p) = self.return_position.take() {
                self.position = p;
            }
        }
    }

    pub fn is_inside(&self) -> bool {
        matches!(self.place, Place::Location(_))
    }
}

/// Every location's spring, kept across visits.
///
/// A source belongs to its location, not to the visit: what you drank has to
/// still be gone after you walk out into the desert and come back. Small and
/// linear because a game has a few dozen locations, not thousands.
#[derive(Default, Debug)]
pub struct Springs {
    sources: Vec<(usize, WaterSource)>,
}

impl Springs {
    pub fn new() -> Springs {
        Springs { sources: Vec::new() }
    }

    /// The spring at `location`, created from `make` the first time it is
    /// asked for and remembered from then on.
    pub fn get_or_insert(
        &mut self,
        location: usize,
        make: impl FnOnce() -> WaterSource,
    ) -> &mut WaterSource {
        if let Some(at) = self.sources.iter().position(|(i, _)| *i == location) {
            return &mut self.sources[at].1;
        }
        self.sources.push((location, make()));
        let last = self.sources.len() - 1;
        &mut self.sources[last].1
    }

    /// Every spring, as `(location, source)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &WaterSource)> + '_ {
        self.sources.iter().map(|(i, s)| (*i, s))
    }

    /// Put one back as it was, for restoring a saved world.
    pub fn set(&mut self, location: usize, source: WaterSource) {
        match self.sources.iter_mut().find(|(i, _)| *i == location) {
            Some((_, existing)) => *existing = source,
            None => self.sources.push((location, source)),
        }
    }

    pub fn get(&self, location: usize) -> Option<&WaterSource> {
        self.sources.iter().find(|(i, _)| *i == location).map(|(_, s)| s)
    }

    /// Every location's spring, out of the 1993 save.
    ///
    /// All of them, not the visited ones: `World.Turn` turns every location,
    /// so a spring fills up whether or not anybody is standing in it. Built
    /// lazily on first visit, a spring would instead be found at day one's
    /// level however long the game had been running.
    pub fn from_gamdat(gam: &crate::gamdat::Gamdat) -> Springs {
        let mut springs = Springs::new();
        for (i, loc) in gam.locations.iter().enumerate() {
            if loc.has_water() {
                springs.sources.push((
                    i,
                    WaterSource::classic(loc.water_reserve, loc.water_refresh, loc.water_capacity),
                ));
            }
        }
        springs
    }

    /// One location's spring, to drink from or fill a container at.
    pub fn at(&mut self, location: usize) -> Option<&mut WaterSource> {
        let at = self.at_index(location)?;
        Some(&mut self.sources[at].1)
    }

    fn at_index(&self, location: usize) -> Option<usize> {
        self.sources.iter().position(|(i, _)| *i == location)
    }

    /// Which locations have a spring at all.
    pub fn locations(&self) -> Vec<usize> {
        self.sources.iter().map(|(i, _)| *i).collect()
    }

    /// A day passes for every spring: each yields, then each is capped.
    ///
    /// `Location.Turn` does `BeginTurn`, then its own business, then
    /// `EndTurn` — so the cap lands after the yield, and a spring is never
    /// left holding more than it can. What happens between the two is filling
    /// bottles left in the room: a v1.2 game does it in its own day
    /// (`Location.cs:457-463`; compliance RULE-1); the legacy day fills a
    /// container when it is stored.
    /// `pump_at` answers for one location, because a pump is installed in a
    /// place and not owned by the world.
    pub fn advance_day(&mut self, engine: Engine, pump_at: impl Fn(usize) -> Pump) {
        for (location, source) in self.sources.iter_mut() {
            source.begin_turn(engine, pump_at(*location));
            source.end_turn();
        }
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

// -- items ------------------------------------------------------------------

/// How much one character carries.
///
/// `GameCreation` sets `Items.MaxCount = 9` for every player and then 6 for a
/// human one, so 6 is what a person plays with; a trader gets another 6 on top
/// and nobody else is bigger. Not a number chosen to fit a panel.
pub const PACK_CAPACITY: usize = 6;

/// A place items sit: a character's pack, or a room's floor.
///
/// It holds *instances*, each one a type index. Two snakes are one type and
/// two entries, and they take two slots — which is why moving one names a
/// slot rather than a type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemBag {
    items: Vec<usize>,
    /// Rounds left in each slot, kept in step with `items`.
    ///
    /// `None` means "whatever that type holds when new", which is what all
    /// but a fired gun has. Only `Item.Use` writes a number here, because
    /// only ammunition is spent an instance at a time: the type says a
    /// loaded rifle holds two rounds, and after one shot *this* rifle holds
    /// one while every other loaded rifle still holds two.
    ///
    /// It matters between the two item tables. The original's rifle carries
    /// `ammo=2` and becomes an unloaded rifle after the second shot; the
    /// extended one carries `ammo=1` and becomes `item_loaded_rifle_1`,
    /// which holds another single round. Two shots either way — but only if
    /// the count is kept per item, because the original has no type for a
    /// rifle with one round left.
    ammo: Vec<Option<i32>>,
    capacity: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveError {
    /// There is nothing in that slot.
    NoSuchSlot,
    /// The destination is full, or takes nothing at all.
    DestinationFull,
}

impl ItemBag {
    pub fn new(capacity: usize) -> ItemBag {
        ItemBag { items: Vec::new(), ammo: Vec::new(), capacity }
    }

    /// A bag holding these types, clamped to the capacity rather than
    /// silently over-filled.
    pub fn with(capacity: usize, types: &[usize]) -> ItemBag {
        let mut bag = ItemBag::new(capacity);
        for &t in types {
            if !bag.push(t) {
                break;
            }
        }
        bag
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_full(&self) -> bool {
        self.items.len() >= self.capacity
    }

    /// The type in one slot.
    pub fn get(&self, slot: usize) -> Option<usize> {
        self.items.get(slot).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.items.iter().copied()
    }

    /// Turn the item in one slot into another kind, in place.
    ///
    /// A bottle drunk becomes an empty bottle and a bottle filled becomes a
    /// full one, and both keep the slot they were in — `MakeEmpty` and
    /// `MakeFull` change an item's type rather than swapping the object, so
    /// nothing moves in the grid the player is looking at.
    pub fn replace(&mut self, slot: usize, item_type: usize) -> Option<usize> {
        let old = *self.items.get(slot)?;
        self.items[slot] = item_type;
        // A new type is a new instance's worth of ammunition: `Item.Use`
        // does `Type = Type.Empty; ammo = Type.AmmoValue` in one breath.
        self.ammo[slot] = None;
        Some(old)
    }

    /// Rounds left in one slot, or `None` for "as many as the type says".
    ///
    /// The caller needs the item table to turn that into a number; this
    /// module has no opinion about item types.
    pub fn rounds_left(&self, slot: usize) -> Option<i32> {
        self.ammo.get(slot).copied().flatten()
    }

    /// Write the rounds left in one slot. `None` puts it back to the
    /// type's own count.
    pub fn set_rounds_left(&mut self, slot: usize, rounds: Option<i32>) {
        if let Some(cell) = self.ammo.get_mut(slot) {
            *cell = rounds;
        }
    }

    /// Take the item out of one slot. The slots after it close up, so a bag
    /// never has a hole in it.
    pub fn remove(&mut self, slot: usize) -> Option<usize> {
        if slot >= self.items.len() {
            return None;
        }
        self.ammo.remove(slot);
        Some(self.items.remove(slot))
    }

    /// Add one, or refuse because there is no room. Never grows past capacity.
    pub fn push(&mut self, item_type: usize) -> bool {
        if self.is_full() {
            return false;
        }
        self.items.push(item_type);
        self.ammo.push(None);
        true
    }
}

/// Move one item from a slot of `from` into `to`.
///
/// Either the item moves or nothing changes at all: both ends are checked
/// before either is touched. A half-done move would either lose the item or
/// make a second one, and both are worse than refusing.
///
/// The only condition is whether the destination has room. That is also the
/// only condition the original applies — `InventoryScene` enables the
/// transfer on `Count < MaxCount` and nothing else. The item classes,
/// `nodrop` included, decide what random generation may produce, not what the
/// player may carry or set down.
pub fn move_item(from: &mut ItemBag, slot: usize, to: &mut ItemBag) -> Result<usize, MoveError> {
    let item = from.get(slot).ok_or(MoveError::NoSuchSlot)?;
    if to.is_full() {
        return Err(MoveError::DestinationFull);
    }
    // Both checks have passed, so neither of these can fail. **The rounds
    // go with the gun**: v0.9.1 moves the `Item` itself, so a half-fired
    // rifle put down is half-fired when it is picked up again.
    let rounds = from.rounds_left(slot);
    from.remove(slot);
    to.push(item);
    let last = to.len() - 1;
    to.set_rounds_left(last, rounds);
    Ok(item)
}

/// Put a pack's slot down on the ground, as it is — rounds and all.
pub fn put_down(bag: &mut ItemBag, slot: usize, location: usize, at: Point) -> Option<crate::spawn::Dropped> {
    let rounds = bag.rounds_left(slot);
    let kind = bag.remove(slot)?;
    Some(crate::spawn::Dropped { location, at, kind, rounds, icon: crate::spawn::PILE_UNDRAWN })
}

/// `LocationScene.PICKUP_DISTANCE`: how far from the selected character a
/// thing on the ground may lie and still be in reach of the inventory.
pub const PICKUP_DISTANCE: i32 = 20;

/// `PickItemList`: the things lying within reach of `at`, as indices into
/// `ground`, **in the ground list's own order**.
///
/// `(position - at).Length <= range`: a circle, not a square, and the edge
/// counts. The order matters because everything that searches it —
/// `Contains(type)`, `Find(type)` — walks it front to back through the
/// indexer, so when two of a kind are in reach it is the earlier one that is
/// taken.
pub fn pick_items(ground: &[crate::spawn::Dropped], location: usize, at: Point) -> Vec<usize> {
    let reach = PICKUP_DISTANCE * PICKUP_DISTANCE;
    ground
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            let (dx, dy) = (d.at.x - at.x, d.at.y - at.y);
            d.location == location && dx * dx + dy * dy <= reach
        })
        .map(|(i, _)| i)
        .collect()
}

/// Pick a thing up off the ground into a pack, as it lies.
pub fn pick_up(ground: &mut Vec<crate::spawn::Dropped>, index: usize, bag: &mut ItemBag) -> Result<usize, MoveError> {
    let d = ground.get(index).ok_or(MoveError::NoSuchSlot)?;
    if bag.is_full() {
        return Err(MoveError::DestinationFull);
    }
    let (kind, rounds) = (d.kind, d.rounds);
    ground.remove(index);
    bag.push(kind);
    let last = bag.len() - 1;
    bag.set_rounds_left(last, rounds);
    Ok(kind)
}

/// `Group.MoveItems` (`Logic/Character/Group.cs:178-182`): each pack in
/// group order — the leader's first — takes from the front of `from` until
/// it is full (`ItemList.Move`), whoever the things came from. Rounds go
/// with each thing. What nobody has room for is left in `from`.
pub fn move_to_group(from: &mut ItemBag, packs: &mut [&mut ItemBag]) {
    for bag in packs.iter_mut() {
        while !from.is_empty() && move_item(from, 0, bag).is_ok() {}
    }
}

/// **Ours, not v0.9.1's:** what is set aside at a trader's
/// (`temporarySpace`) when the scene is left. v0.9.1 clears the space on the
/// next visit, and what was in it is gone. Here each thing goes back to the
/// pack it came out of — `from[i]` names it, an index into `packs` — rounds and
/// all; a thing whose owner's pack has no room is **not** given to somebody
/// else and not squeezed in, but returned to the caller to be put down on the
/// ground, where it can be picked up again.
pub fn return_set_aside(set_aside: &mut ItemBag, from: &mut Vec<usize>,
                        packs: &mut [&mut ItemBag]) -> Vec<(usize, Option<i32>)> {
    let mut left = Vec::new();
    while !set_aside.is_empty() {
        let owner = if from.is_empty() { usize::MAX } else { from.remove(0) };
        match packs.get_mut(owner) {
            Some(bag) if !bag.is_full() => {
                let _ = move_item(set_aside, 0, bag);
            }
            _ => {
                let rounds = set_aside.rounds_left(0);
                if let Some(kind) = set_aside.remove(0) {
                    left.push((kind, rounds));
                }
            }
        }
    }
    left
}

/// What is lying on the floor of each room, kept across visits.
///
/// Keyed by location and room, like [`Springs`] is by location: leaving a room
/// and coming back is a redraw, not a new room. A room that has never been
/// stocked is absent rather than empty, so the caller can tell "nothing was
/// ever put here" from "someone took the last of it".
#[derive(Debug, Default)]
pub struct RoomItems {
    rooms: Vec<((usize, usize), ItemBag)>,
}

impl RoomItems {
    pub fn new() -> RoomItems {
        RoomItems { rooms: Vec::new() }
    }

    fn at(&self, location: usize, room: usize) -> Option<usize> {
        self.rooms.iter().position(|(k, _)| *k == (location, room))
    }

    /// The floor of one room, stocked from `make` the first time it is asked
    /// for. `make` runs once per room for the life of the game, so a room
    /// emptied by the player stays empty on the next visit.
    pub fn get_or_insert(
        &mut self,
        location: usize,
        room: usize,
        make: impl FnOnce() -> ItemBag,
    ) -> &mut ItemBag {
        if let Some(at) = self.at(location, room) {
            return &mut self.rooms[at].1;
        }
        self.rooms.push(((location, room), make()));
        let last = self.rooms.len() - 1;
        &mut self.rooms[last].1
    }

    /// Every room that holds something, as `((location, room), items)`.
    /// Rooms nobody has opened are not in here, and that is the difference
    /// between "empty" and "not visited yet".
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, &ItemBag)> + '_ {
        self.rooms.iter().map(|((l, r), bag)| (*l, *r, bag))
    }

    /// Put a bag back where it was, for restoring a saved world.
    pub fn set(&mut self, location: usize, room: usize, bag: ItemBag) {
        match self.rooms.iter_mut().find(|(k, _)| *k == (location, room)) {
            Some((_, existing)) => *existing = bag,
            None => self.rooms.push(((location, room), bag)),
        }
    }

    pub fn get(&self, location: usize, room: usize) -> Option<&ItemBag> {
        self.at(location, room).map(|at| &self.rooms[at].1)
    }

    /// The same room's floor, to take from. A room nobody has opened yet has
    /// no entry and so no items — `get_or_insert` is for putting things down.
    pub fn get_mut(&mut self, location: usize, room: usize) -> Option<&mut ItemBag> {
        let at = self.at(location, room)?;
        Some(&mut self.rooms[at].1)
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// Every opened room of these locations, by location then room, to
    /// take from together.
    pub fn bags_in(&mut self, locations: &[usize]) -> Vec<&mut ItemBag> {
        let mut bags: Vec<((usize, usize), &mut ItemBag)> = self.rooms.iter_mut().filter(|((l, _), _)| locations.contains(l)).map(|(k, b)| (*k, b)).collect();
        bags.sort_by_key(|(k, _)| *k);
        bags.into_iter().map(|(_, b)| b).collect()
    }

    /// Every opened room of one location, by room number, to take from
    /// together.
    pub fn bags_at(&mut self, location: usize) -> Vec<&mut ItemBag> {
        let mut bags: Vec<(usize, &mut ItemBag)> = self.rooms.iter_mut().filter(|((l, _), _)| *l == location).map(|((_, r), b)| (*r, b)).collect();
        bags.sort_by_key(|(r, _)| *r);
        bags.into_iter().map(|(_, b)| b).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }
}

// -- containers -------------------------------------------------------------

/// What the rules need to know about a kind of item.
///
/// The rules take this rather than an item table, so a test can state four
/// numbers about three made-up items and the real table can answer the same
/// four questions for fifty. [`crate::items::ItemTypes`] implements it.
pub trait ItemFacts {
    /// Points of hunger it fills. Zero means it is not food.
    fn food(&self, kind: usize) -> i32;
    /// Points of thirst it fills. Zero means there is nothing to drink.
    fn water(&self, kind: usize) -> i32;
    /// What it becomes when filled at a spring, if it is an empty container.
    fn full(&self, kind: usize) -> Option<usize>;
    /// What it becomes when used up. `None` means it simply goes.
    fn empty(&self, kind: usize) -> Option<usize>;

    /// The kind with that name, for the one place a rule names an item
    /// rather than pointing at one: a production says what it produces.
    fn kind_named(&self, name: &str) -> Option<usize>;

    /// What that kind is called, for the other direction: a production names
    /// its tool, and a pack holds kinds.
    fn name_of(&self, kind: usize) -> Option<&str>;
    /// Whether the item has that `function=` (v1.2's Extended items).
    fn has_function(&self, _kind: usize, _function: &str) -> bool {
        false
    }

    /// How much of a danger the item keeps off, in percent.
    fn protection(&self, _kind: usize, _danger: crate::gamdat::Danger) -> u32 {
        0
    }
}

/// Use up the item in a slot: transformed if it has an empty form, gone if
/// not. The bottle stays where it was; the snake vanishes.
fn consume(bag: &mut ItemBag, slot: usize, facts: &impl ItemFacts) {
    let Some(kind) = bag.get(slot) else { return };
    match facts.empty(kind) {
        Some(empty) => {
            bag.replace(slot, empty);
        }
        None => {
            bag.remove(slot);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FillError {
    NoSuchSlot,
    /// Not an empty container: it has no filled form, or the filled form
    /// holds no water.
    NotAContainer,
    /// The spring has less than a full container's worth.
    ///
    /// **All or nothing.** `InventoryScene` tests `Reserve >= WaterValue` and
    /// does nothing at all below it — a spring with one point in it does not
    /// half-fill a bottle, and does not lose the point either.
    NotEnoughWater,
}

/// Fill an empty container at a spring, the way the original's right click
/// does it: only in a water-source room, only if the reserve covers the whole
/// container, and in place so the slot does not move.
pub fn fill_container(
    source: &mut WaterSource,
    bag: &mut ItemBag,
    slot: usize,
    facts: &impl ItemFacts,
) -> Result<usize, FillError> {
    let kind = bag.get(slot).ok_or(FillError::NoSuchSlot)?;
    let full = facts.full(kind).ok_or(FillError::NotAContainer)?;
    let worth = facts.water(full);
    if worth == 0 {
        return Err(FillError::NotAContainer);
    }
    if source.reserve < worth {
        return Err(FillError::NotEnoughWater);
    }
    source.reserve -= worth;
    bag.replace(slot, full);
    Ok(full)
}

/// Drink what is in a slot. Returns how much was actually drunk, or `None` if
/// there was nothing to drink there.
///
/// The container is used up only if somebody drank — the same `left < value`
/// rule as food. A full bottle drunk by someone with one point of room is
/// still emptied, and the other point is lost.
pub fn drink_from(
    bag: &mut ItemBag,
    slot: usize,
    needs: &mut CharacterNeeds,
    facts: &impl ItemFacts,
) -> Option<i32> {
    let kind = bag.get(slot)?;
    let worth = facts.water(kind);
    if worth == 0 {
        return None;
    }
    let left = needs.drink_value(worth);
    if !eaten(worth, left) {
        return Some(0);
    }
    consume(bag, slot, facts);
    Some(worth - left)
}

/// Which slot holds the most water, the way `Group.FindWater` picks: the
/// largest value, and the first of equals.
pub fn find_water(bag: &ItemBag, facts: &impl ItemFacts) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for slot in 0..bag.len() {
        let Some(kind) = bag.get(slot) else { continue };
        let value = facts.water(kind);
        if value != 0 && best.is_none_or(|(_, b)| value > b) {
            best = Some((slot, value));
        }
    }
    best.map(|(slot, _)| slot)
}

// -- travel -----------------------------------------------------------------

/// A journey under way: one road, committed until it is finished.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Journey {
    pub from: usize,
    pub to: usize,
    /// What the road costs, in days.
    pub total: u8,
    /// How many of them are left. The arrival happens when this reaches zero,
    /// on a day ending — never on the click that started it.
    pub remaining: u8,
}

impl Journey {
    /// How far along, for drawing: 0.0 at the start, 1.0 on arrival.
    pub fn progress(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        (self.total - self.remaining) as f32 / self.total as f32
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TravelError {
    /// Already on the road. `CanDepart` refuses while `IsTraveling`: a
    /// journey is one committed edge and the destination cannot be changed
    /// half way.
    AlreadyTravelling,
    /// There is no road straight from here to there.
    ///
    /// **The original does not route.** `GetDirectTravelDays` walks the
    /// current location's four neighbours and returns the way length of the
    /// one that matches, or zero — so a place two roads away simply cannot be
    /// travelled to, and the player has to take the legs one at a time.
    NoRoad,
    /// Already there.
    Arrived,
}

/// The cost of the road from `from` to `to`, in days, if there is one.
///
/// Straight out of the 1993 save's own graph: four neighbours a location,
/// each with its own travel time. Not a distance on the map and not a count
/// of steps — the number the file records for that edge.
pub fn road_days(gam: &crate::gamdat::Gamdat, from: usize, to: usize) -> Option<u8> {
    gam.locations
        .get(from)?
        .roads
        .iter()
        .find(|r| r.neighbour == to)
        .map(|r| r.days)
}

/// Set out from `from` to `to`, if there is a road and nobody is on one.
pub fn depart(
    gam: &crate::gamdat::Gamdat,
    travelling: Option<Journey>,
    from: usize,
    to: usize,
) -> Result<Journey, TravelError> {
    if travelling.is_some() {
        return Err(TravelError::AlreadyTravelling);
    }
    if from == to {
        return Err(TravelError::Arrived);
    }
    let days = road_days(gam, from, to).filter(|&d| d > 0).ok_or(TravelError::NoRoad)?;
    Ok(Journey { from, to, total: days, remaining: days })
}

/// A day of walking. Returns where the journey now stands, or `None` on
/// arrival — `Player.Turn` decrements the count and moves the player only
/// when it reaches zero.
pub fn travel_a_day(journey: Journey) -> Option<Journey> {
    let remaining = journey.remaining.saturating_sub(1);
    (remaining > 0).then_some(Journey { remaining, ..journey })
}

// -- a day passing ----------------------------------------------------------

/// Which slot holds the best food in a bag, the way `Group.FindFood` picks.
///
/// The largest `FoodValue`, and the *first* of equals: the comparison is
/// strictly greater, so a later item of the same worth does not displace an
/// earlier one. That is the original's order and it is visible — it decides
/// which of two snakes is eaten.
pub fn find_food(bag: &ItemBag, facts: &impl ItemFacts) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for slot in 0..bag.len() {
        let Some(kind) = bag.get(slot) else { continue };
        let value = facts.food(kind);
        if value != 0 && best.is_none_or(|(_, b)| value > b) {
            best = Some((slot, value));
        }
    }
    best.map(|(slot, _)| slot)
}

/// What one day did to one character.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DayOutcome {
    /// The item type eaten without being asked, when the character woke up
    /// with nothing left and had food on them.
    pub ate: Option<usize>,
    /// The same for water: the fullest container they were carrying.
    pub drank: Option<usize>,
    pub health_before: i32,
    /// Health lost to going without. Food and water count separately.
    pub deprivation: i32,
    /// The location reached, when the last day of a journey ended today.
    pub arrived_at: Option<usize>,
    pub died: bool,
    /// Everyone else who died today. **The leader is not in here** — their
    /// death is [`DayOutcome::died`], because it ends the game rather than
    /// changing it.
    pub deaths: Vec<Death>,
    /// v1.2: the leader died of the place's danger, not of want.
    pub killed_by_danger: bool,
    /// v1.2: what the leader carried when he died, already out of his pack
    /// (`Die` drops it at once) — for the caller to put on the ground.
    pub dropped: Option<ItemBag>,
}

/// Somebody other than the played character dying, and what it costs.
///
/// `Character.Die` does four things and this carries three of them out; the
/// fourth, the respawn, is not modelled — see [`Death::respawns_in`].
///
/// ```csharp
/// public virtual void Die() {
///     Location.Items.DropPosition = Position;   // everything they carried
///     Items.MoveTo(Location.Items);
///     if (Player != null && Player.Character != this) Dismiss();
///     health = 0;
///     classic.World.Respawn.Object.Respawn(this);
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Death {
    /// Who, by the identity they kept while they lived.
    pub id: usize,
    /// **Which character in `gam.dat` this was.** Leaving the group is
    /// right; losing who they were is not. Somebody who came from a record
    /// is still that record's person while they lie there, and it is what a
    /// respawn would need.
    pub origin: Option<usize>,
    /// Where they were camped, or `None` for somebody walking with the
    /// player — a travelling group has no location of its own, and
    /// `Dismiss` puts the body where the player is.
    pub at: Option<usize>,
    /// What they were carrying. **`Die` drops it where they fell**, on the
    /// location's ground rather than into a room, and this hands it back
    /// for the caller to put down: the rules know who died, not where
    /// anybody was standing.
    pub dropped: ItemBag,
    /// They were the last one camped there, so the place is nobody's again.
    /// `Dismiss` does `Location.Player = null` only for the last.
    pub camp_lost: bool,
    /// What they had when they died. **`Revive` restores health and nothing
    /// else**, so these come back with them.
    pub food: i32,
    pub water: i32,
    /// Where he stood: v1.2's `Die` drops at `Position`
    /// (`Location.Items.DropPosition = Position`).
    pub position: Point,
}

/// Somebody scheduled to come back.
///
/// `CharacterRespawn.RespawnObject`: the character, a countdown, and the
/// location they are to re-enter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending {
    /// Which character in `gam.dat`.
    pub record: usize,
    /// Where they will re-enter. **Fixed when they die**, from
    /// `character.Location` at that moment — which `Dismiss` has just set
    /// to the player's location for somebody who fell on the road. It does
    /// not follow the player afterwards.
    pub location: usize,
    /// Days left. Counted down at the end of every world turn, **including
    /// the day of the death** — `Respawn.Object.Turn()` is the last thing
    /// `World.Turn` does, and the entry was added during the group's turn
    /// earlier in the same day.
    pub days: i32,
    /// What they had when they died, because **`Revive` restores health and
    /// nothing else**: `public virtual void Revive() { health = 100; … }`.
    /// Food, water and the pack are not touched — and the pack was emptied
    /// by `Die`, so they come back with nothing.
    pub food: i32,
    pub water: i32,
    /// What `Revive` puts the health back to. **A hundred for a person and
    /// thirty-one for a dog**: `Dog.Revive` overrides the base with
    /// `DogHealth`, and `Mutant.Revive` with `MutantHealth`. It is fixed
    /// when they die, because that is when their class is known.
    pub revive_health: i32,
}

/// Somebody who came back today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Returned {
    pub record: usize,
    pub location: usize,
    /// Health full, food and water as they were. Not a fresh character.
    pub needs: CharacterNeeds,
    /// v1.2's `RefreshIdentity`: an ordinary person comes back as somebody
    /// else. `None` for v0.9.1 and for everybody but the three professions.
    pub renamed: Option<Renamed>,
}

/// `RefreshIdentity`, for the caller to carry out on the people: the name
/// is swapped with `donor`'s (another person still waiting), the face —
/// unless it is face 10, the gas mask — becomes [`respawn_face`], and the
/// body [`respawn_body`], both of `location_turn` (`day + death location`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Renamed {
    pub donor: usize,
    pub location_turn: i32,
}

/// `7 + turn % 21`, skipping face 10 (the gas mask).
pub fn respawn_face(location_turn: i32) -> u8 {
    let face = 7 + location_turn.rem_euclid(21);
    (if face >= 10 { face + 1 } else { face }) as u8
}

/// `GetCharacterBody(GetSetBodyId(class), turn % 3)`: the first frame of
/// the body strip, for the three professions.
pub fn respawn_body(class: crate::gamdat::CharClass, location_turn: i32) -> Option<u16> {
    use crate::gamdat::CharClass::*;
    let body = match class {
        Doctor => 0,
        Mercenary => 1,
        Technician => 2,
        _ => return None,
    };
    Some(((body + 1) * 16 + 48 * location_turn.rem_euclid(3)) as u16)
}

/// v1.2's `CharacterRespawn` settings for a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnCycle {
    pub method: crate::rules::SpawnMethod,
    /// `npc_spawn`: the ordinary people's interval (and, for `timer`,
    /// their countdown).
    pub npc: i32,
    pub trader: i32,
    pub dog: i32,
    pub mutant: i32,
    /// Extended only: `city_spawn_threshold` (0 keeps people where they
    /// died).
    pub city_threshold: i32,
}

impl SpawnCycle {
    /// `Respawn(character)`'s countdown for a class; nought or less is
    /// never.
    pub fn days_for(&self, class: crate::gamdat::CharClass) -> i32 {
        use crate::gamdat::CharClass::*;
        match class {
            Trader => self.trader,
            Dog => self.dog,
            Mutant => self.mutant,
            _ => self.npc,
        }
    }
}

/// `IsOrdinaryNpc`: the three professions.
pub fn is_ordinary(class: crate::gamdat::CharClass) -> bool {
    use crate::gamdat::CharClass::*;
    matches!(class, Mercenary | Technician | Doctor)
}

/// Everybody waiting to come back.
///
/// **The one place a death is not the end.** `Die` schedules; this counts
/// down and hands them back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Respawns {
    waiting: Vec<Pending>,
}

impl Respawns {
    pub fn new() -> Respawns {
        Respawns { waiting: Vec::new() }
    }

    /// `Respawn(character)`: schedule one. `days` comes from the settings
    /// of the game being played — see [`crate::settings::GameSettings`].
    ///
    /// **Nought days is never.** v1.2 says so outright (`if (timeToSpawn <=
    /// 0) return;`, `CharacterRespawn.Respawn`), and v0.9.1 comes to the same
    /// end by its countdown: `remainingTime--` takes a nought to minus one,
    /// and `== 0` is never true again. v1.2 Classic's `dog_respawn = 0` is a
    /// dog that stays dead.
    pub fn schedule(&mut self, one: Pending) {
        if one.days <= 0 {
            return;
        }
        // A record waits once. Scheduling somebody already waiting would
        // make two of them come back.
        if self.waiting.iter().any(|w| w.record == one.record) {
            return;
        }
        self.waiting.push(one);
    }

    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pending> + '_ {
        self.waiting.iter()
    }

    /// Is this record waiting to come back? The answer a hire has to
    /// respect: not yet.
    pub fn waiting_for(&self, record: usize) -> Option<i32> {
        self.waiting.iter().find(|w| w.record == record).map(|w| w.days)
    }

    /// Put one back, for restoring a save.
    pub fn restore(&mut self, one: Pending) {
        self.waiting.push(one);
    }

    /// `CharacterRespawn.Turn`: everybody's countdown, and whoever reached
    /// zero comes back.
    ///
    /// **Zero is the moment, not "past zero"** — upstream tests
    /// `RemainingTime == 0` after decrementing, so an entry that somehow
    /// went below would never fire. Ours treats `<= 0` as due, which is the
    /// same for every sequence that can arise and does not strand anybody.
    pub fn advance_day(&mut self) -> Vec<Returned> {
        let mut back = Vec::new();
        let mut i = 0;
        while i < self.waiting.len() {
            self.waiting[i].days -= 1;
            if self.waiting[i].days > 0 {
                i += 1;
                continue;
            }
            let one = self.waiting.remove(i);
            back.push(Returned {
                record: one.record,
                location: one.location,
                needs: CharacterNeeds {
                    food: one.food,
                    max_food: MAX_FOOD,
                    water: one.water,
                    max_water: MAX_WATER,
                    // `Revive`'s only line — for whichever class this is.
                    health: one.revive_health,
                },
                renamed: None,
            });
        }
        back
    }

    /// v1.2's `CharacterRespawn.Turn`, on `day` (already advanced).
    ///
    /// Traders, dogs and mutants count down and come back at nought where
    /// they died; a dog or a mutant coming back starts the others of its
    /// class waiting there over. The ordinary people do not count down
    /// under a cycle: under `amiga_location_cycle` a location whose turn
    /// (`day + index`) is a multiple of `npc_spawn` lets the first of them
    /// who died there come back — **only by taking the name of another one
    /// still waiting** (`RefreshIdentity`); with nobody to swap with he
    /// stays dead. Under `timer` they count down like the rest.
    ///
    /// `cities` is every city with the number of free ordinary people in
    /// it: in Extended, with a threshold, a person comes back in the
    /// emptiest city below it. `revive` is what `Revive` restores for a
    /// class (95, or a creature's or trader's own health); food 8 and
    /// water 4 for everybody, the pack empty. `player_cycle` is not here:
    /// it belongs to the Dos preset.
    pub fn advance_day_v12(
        &mut self,
        day: u16,
        locations: usize,
        cycle: SpawnCycle,
        class_of: &impl Fn(usize) -> crate::gamdat::CharClass,
        revive: &impl Fn(crate::gamdat::CharClass) -> i32,
        cities: &mut Vec<(usize, usize)>,
    ) -> Vec<Returned> {
        use crate::rules::SpawnMethod;
        let timer = cycle.method == SpawnMethod::Timer;
        let ordinary = |p: &Pending| is_ordinary(class_of(p.record));
        for p in self.waiting.iter_mut() {
            if timer || !is_ordinary(class_of(p.record)) {
                p.days -= 1;
            }
        }
        let mut back = Vec::new();
        if cycle.method == SpawnMethod::AmigaLocationCycle && cycle.npc > 0 {
            for location in 0..locations {
                if (day as i64 + location as i64) % cycle.npc as i64 != 0 {
                    continue;
                }
                if let Some(i) = self.waiting.iter().position(|p| ordinary(p) && p.location == location) {
                    self.spawn_v12(i, day, cycle, class_of, revive, cities, &mut back);
                }
            }
        }
        let mut i = 0;
        while i < self.waiting.len() {
            let p = self.waiting[i];
            if p.days > 0 || (!timer && ordinary(&p)) {
                i += 1;
                continue;
            }
            if !self.spawn_v12(i, day, cycle, class_of, revive, cities, &mut back) {
                i += 1;
            }
        }
        back
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_v12(
        &mut self,
        i: usize,
        day: u16,
        cycle: SpawnCycle,
        class_of: &impl Fn(usize) -> crate::gamdat::CharClass,
        revive: &impl Fn(crate::gamdat::CharClass) -> i32,
        cities: &mut Vec<(usize, usize)>,
        back: &mut Vec<Returned>,
    ) -> bool {
        let p = self.waiting[i];
        let class = class_of(p.record);
        let mut renamed = None;
        if is_ordinary(class) {
            let Some(donor) = self.waiting.iter().enumerate().find(|&(j, d)| j != i && is_ordinary(class_of(d.record)))
            else {
                return false;
            };
            renamed = Some(Renamed { donor: donor.1.record, location_turn: day as i32 + p.location as i32 });
        }
        // `GetSpawnLocation`.
        let mut location = p.location;
        if is_ordinary(class) && cycle.city_threshold > 0 {
            let mut best: Option<usize> = None;
            for (k, &(_, population)) in cities.iter().enumerate() {
                if (population as i32) < cycle.city_threshold && best.is_none_or(|b| population < cities[b].1) {
                    best = Some(k);
                }
            }
            if let Some(k) = best {
                location = cities[k].0;
                cities[k].1 += 1;
            }
        }
        self.waiting.remove(i);
        back.push(Returned {
            record: p.record,
            location,
            needs: CharacterNeeds { food: 8, max_food: MAX_FOOD, water: 4, max_water: MAX_WATER, health: revive(class) },
            renamed,
        });
        // "Camps restore dogs and mutants one at a time."
        let restart = match class {
            crate::gamdat::CharClass::Dog => cycle.dog,
            crate::gamdat::CharClass::Mutant => cycle.mutant,
            _ => 0,
        };
        if restart > 0 {
            for w in self.waiting.iter_mut() {
                if w.location == location && class_of(w.record) == class {
                    w.days = restart;
                }
            }
        }
        true
    }
}

/// One player character's day, in the order `Character.Turn` does it.
///
/// The order is the whole of the rule, and no part of it is interchangeable:
///
/// 1. **Eat, if there is nothing left.** Only at zero, and only from what the
///    character carries — the biggest meal on them. A character with one
///    point of food left does not touch their pack.
/// 2. **Heal.** Two points with no doctor, and only from 70 up. Below that
///    you do not recover at all.
/// 3. **Go without.** Twenty five off for no food, twenty five more for no
///    water — measured *before* the day's ration is taken off, so the day you
///    run out is the day it hurts.
/// 4. **Die**, at nothing left.
/// 5. **Spend the day's food and water**, each floored at zero.
///
/// Step 1 before step 3 is why eating in time matters; step 3 before step 5 is
/// why running out at the end of a day is not punished twice.
///
/// Eating comes before drinking, as it does in `Character.Turn`. Both are
/// used up in place: a snake goes, a bottle becomes an empty bottle in the
/// same slot.
pub fn character_day(
    needs: &mut CharacterNeeds,
    pack: &mut ItemBag,
    facts: &impl ItemFacts,
) -> DayOutcome {
    let mut out = DayOutcome { health_before: needs.health, ..DayOutcome::default() };

    if needs.food == 0 {
        if let Some(slot) = find_food(pack, facts) {
            let kind = pack.get(slot).expect("find_food named a slot it found");
            let worth = facts.food(kind);
            let left = needs.eat(worth);
            // The same rule as eating by hand: used up only if it fed anyone.
            // At zero food it always does, but the rule is the rule.
            if eaten(worth, left) {
                consume(pack, slot, facts);
                out.ate = Some(kind);
            }
        }
    }

    if needs.water == 0 {
        if let Some(slot) = find_water(pack, facts) {
            let kind = pack.get(slot).expect("find_water named a slot it found");
            if drink_from(pack, slot, needs, facts).unwrap_or(0) > 0 {
                out.drank = Some(kind);
            }
        }
    }

    if needs.health >= REGEN_FLOOR_WITHOUT_DOCTOR {
        needs.health = (needs.health + REGEN_WITHOUT_DOCTOR).min(MAX_HEALTH);
    }

    if needs.food == 0 {
        out.deprivation += DEPRIVATION;
    }
    if needs.water == 0 {
        out.deprivation += DEPRIVATION;
    }
    needs.health -= out.deprivation;

    if needs.health <= 0 {
        out.died = true;
        return out;
    }

    needs.food = (needs.food - 1).max(0);
    needs.water = (needs.water - 1).max(0);
    out
}

/// How many days the character can go before they start going without.
///
/// **Not a formula.** It runs the real day, on a copy of the character and a
/// copy of their pack, until one of them ends in deprivation — so it counts
/// the food and water they are carrying, eats and drinks it in the order
/// `Character.Turn` would, and gets the timing of the last day right for the
/// same reason the game does. A second, simpler estimate would be a second
/// set of rules to keep in step with the first, and it was wrong the moment
/// it was written: `min(food, water)` ignores the pack, so a full bottle
/// changed nothing on screen while visibly changing the journey.
///
/// What it cannot know is where the character is going: springs, floors and
/// traders are not in it. It answers "how long can they walk", not "will they
/// survive".
///
/// `limit` caps the simulation, because a well-stocked character would
/// otherwise be counted out to the end of the world.
pub fn days_of_supplies(
    needs: &CharacterNeeds,
    pack: &ItemBag,
    facts: &impl ItemFacts,
    limit: u32,
) -> u32 {
    let mut needs = *needs;
    let mut pack = pack.clone();

    for days in 0..limit {
        let out = character_day(&mut needs, &mut pack, facts);
        if out.deprivation > 0 || out.died {
            return days;
        }
    }
    limit
}

// -- camps ------------------------------------------------------------------

/// One location's camp, as far as production is concerned.
///
/// **A camp is what ownership is.** In the original a location has an owner
/// only while somebody is camped in it: `JoinCamp` sets `Location.Player` and
/// `LeaveCamp` clears it when the last one goes. Nothing else ever writes it —
/// not arriving, not walking through, not leaving a knife on the floor.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Camp {
    /// Which player holds the location, if any.
    pub owner: Option<usize>,
    /// The people camped here. `CampNPC` — the characters at the location
    /// that belong to a player, and the reason the location has an owner at
    /// all. Their number sets the production rate, their appetites eat it,
    /// and **their packs count towards the tools**: `GetFoodProductionRate`
    /// adds `trapsOnNPCs` to what is lying in the rooms.
    pub residents: Vec<Companion>,
    /// Which production is selected. The 1993 save records one for every
    /// location that is not a city, so this is not usually empty.
    pub production: Option<usize>,
    /// Progress towards the next item, in days. Carried over rather than
    /// reset: the original subtracts the interval and keeps the remainder.
    pub progress: f32,
    /// **Leaders who came back here**, by seat. `Revive` and then
    /// `EnterLocation` put a player's own character into
    /// `Location.Characters` — alive and walking with his group he is in no
    /// location's list — and nothing takes him out again: `Die` does not
    /// clear it, and `CampNPC` asks only `Player != null`, not whether he is
    /// alive. From his first return he is one of this place's people for
    /// good: a mouth in `GetFoodProductionRate`, and the reason an ordinary
    /// camper here is never `IsLastInCamp`.
    pub stationed: Vec<usize>,
    /// v1.2: the production was chosen by its player, and is kept while it
    /// produces (`manualProductionSelection`); otherwise it is chosen anew
    /// each day. Written as `manual` on the save's `camp` line.
    pub manual_production: bool,
}

impl Camp {
    /// `CampNPC.Count`: the camped, and any leader who came back here.
    pub fn head_count(&self) -> usize {
        self.residents.len() + self.stationed.len()
    }

    /// Nobody of anybody's here: what `IsLastInCamp` comes to once the one
    /// leaving has gone.
    pub fn is_empty(&self) -> bool {
        self.residents.is_empty() && self.stationed.is_empty()
    }
}

/// One person, and everything that is theirs.
///
/// **Not a counter.** A companion keeps the same identity and the same pack
/// across joining a camp and being taken back out, because the original moves
/// the character itself: `JoinCamp` takes them out of `Player.Group` and puts
/// them at the location; nothing copies or replaces them.
#[derive(Clone, Debug, PartialEq)]
pub struct Companion {
    /// Stable for the life of the game, so that the same person can be
    /// recognised after a spell in a camp.
    pub id: usize,
    /// **Which character in `gam.dat` this is.**
    ///
    /// Being taken out of the group is not the same as ceasing to exist.
    /// `Die` ends by scheduling the character to come back, and coming back
    /// means becoming an ordinary person at a location again — which needs
    /// to know *which* person. A companion handed over by a test scenario
    /// rather than hired has none, and says so.
    pub origin: Option<usize>,
    pub needs: CharacterNeeds,
    /// What they know, which both of their fighting values scale with.
    /// Theirs, not the leader's: `AttackValue` and `DefenseValue` are read
    /// off the character doing the swinging.
    pub experience: i32,
    /// Their own six slots. `Items.MaxCount = 6` for an ordinary character,
    /// the same as the player's.
    pub pack: ItemBag,
    /// The fraction of a danger's damage not yet paid. See
    /// [`crate::danger::Exposure`]: not saved, and worth at most a point.
    pub exposure: crate::danger::Exposure,
    /// What they have drawn out of it. `Character.Weapon` and
    /// `Character.Protection` outlive a fight, and which of two weapons a
    /// man ends up carrying depends on the order he drew them in — see
    /// [`crate::combat::Kit`].
    pub kit: crate::combat::Kit,
    /// Where they stand in the location, in map pixels.
    ///
    /// **Their own**, not the leader's. Before this a group was one point on
    /// the map, so everybody joined every fight and nobody was ever left
    /// behind; `Player.Group.Where(… < 25)` had nothing to filter.
    pub at: Point,
    /// `Path.MoveTo`: where `FellowerMind` last told them to walk.
    pub move_to: Point,
    /// How many numbers their own stream has drawn — see
    /// [`crate::mind::follow`]. A counter rather than a generator, so a save
    /// carries one number.
    pub draws: u32,
    /// The fraction of a pixel they have walked past [`Companion::at`] in a
    /// v1.2 game (`precisePosition`) — see [`crate::gait::Stride`].
    pub stride: crate::gait::Stride,
    /// v1.2's `FellowerMind` state: slot radius, start delays.
    pub fellower: crate::mind::Fellower,
}

impl Companion {
    pub fn new(id: usize) -> Companion {
        Companion {
            id,
            origin: None,
            needs: CharacterNeeds::at_game_start(),
            experience: 0,
            pack: ItemBag::new(PACK_CAPACITY),
            exposure: crate::danger::Exposure::default(),
            kit: crate::combat::Kit::default(),
            at: Point::new(0, 0),
            move_to: Point::new(0, 0),
            draws: 0,
            stride: crate::gait::Stride::default(),
            fellower: crate::mind::Fellower::default(),
        }
    }
}

/// Which pack a group operation reached into: the leader's, or one of the
/// companions' by position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Whose {
    Leader,
    Companion(usize),
}

/// The best food in the whole group, the way `Group.FindFood` finds it.
///
/// **A travelling group shares.** `Character.Turn` asks `GetGroup()`, which
/// returns the player's group for anyone in it, and `Group.FindFood` walks
/// every member's items. So one person's snake feeds whoever is hungry.
///
/// A camped person is a different case entirely: `JoinCamp` takes them out of
/// the group, so `GetGroup()` returns *themselves* and they eat from the
/// location's rooms and their own pack alone. Leaving somebody behind cuts
/// them off from the larder.
pub fn group_find_food(
    leader: &ItemBag,
    companions: &[Companion],
    facts: &impl ItemFacts,
) -> Option<(Whose, usize)> {
    let mut best: Option<(Whose, usize, i32)> = None;
    let consider = |whose: Whose, bag: &ItemBag, best: &mut Option<(Whose, usize, i32)>| {
        for slot in 0..bag.len() {
            let Some(kind) = bag.get(slot) else { continue };
            let value = facts.food(kind);
            if value != 0 && best.is_none_or(|(_, _, b)| value > b) {
                *best = Some((whose, slot, value));
            }
        }
    };
    consider(Whose::Leader, leader, &mut best);
    for (i, who) in companions.iter().enumerate() {
        consider(Whose::Companion(i), &who.pack, &mut best);
    }
    best.map(|(whose, slot, _)| (whose, slot))
}

/// The same for water. `Group.FindWater`, largest value, first of equals.
pub fn group_find_water(
    leader: &ItemBag,
    companions: &[Companion],
    facts: &impl ItemFacts,
) -> Option<(Whose, usize)> {
    let mut best: Option<(Whose, usize, i32)> = None;
    let consider = |whose: Whose, bag: &ItemBag, best: &mut Option<(Whose, usize, i32)>| {
        for slot in 0..bag.len() {
            let Some(kind) = bag.get(slot) else { continue };
            let value = facts.water(kind);
            if value != 0 && best.is_none_or(|(_, _, b)| value > b) {
                *best = Some((whose, slot, value));
            }
        }
    };
    consider(Whose::Leader, leader, &mut best);
    for (i, who) in companions.iter().enumerate() {
        consider(Whose::Companion(i), &who.pack, &mut best);
    }
    best.map(|(whose, slot, _)| (whose, slot))
}

/// `Group.Eat`: a level at a time, so nobody is fed twice while somebody is
/// hungry.
///
/// The loop fills every member sitting at level `step` before moving up, which
/// is why a snake shared between two hungry people evens them out rather than
/// filling the first. Returns what was left over.
pub fn group_eat(
    leader: &mut CharacterNeeds,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    let ceiling = companions
        .iter()
        .map(|c| c.needs.max_food)
        .chain(core::iter::once(leader.max_food))
        .max()
        .unwrap_or(0);

    for step in 0..ceiling {
        if value <= 0 {
            break;
        }
        if leader.food == step && leader.food < leader.max_food {
            value -= 1;
            leader.food += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if who.needs.food == step && who.needs.food < who.needs.max_food {
                value -= 1;
                who.needs.food += 1;
            }
        }
    }
    value
}

/// `Group.Drink`, which is the same shape over five levels of water.
pub fn group_drink(
    leader: &mut CharacterNeeds,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    for step in 0..MAX_WATER {
        if value <= 0 {
            break;
        }
        if leader.water == step && leader.water < leader.max_water {
            value -= 1;
            leader.water += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if who.needs.water == step && who.needs.water < who.needs.max_water {
                value -= 1;
                who.needs.water += 1;
            }
        }
    }
    value
}

/// `Group.RangeFilterValue` for a player's group: fifty pixels.
pub const GROUP_RANGE: i32 = 50;

/// Is this companion near enough to share what the leader eats, drinks or
/// waits for? `(leader.Position - ch.Position).Length >= RangeFilterValue` is
/// skipped, so strictly under fifty.
pub fn in_group_range(leader_at: Point, member_at: Point) -> bool {
    let (dx, dy) = ((member_at.x - leader_at.x) as i64, (member_at.y - leader_at.y) as i64);
    dx * dx + dy * dy < (GROUP_RANGE as i64) * (GROUP_RANGE as i64)
}

/// `Group.Eat(leader, value)` **with** a leader: the same level-by-level
/// sharing as [`group_eat`], but anybody fifty or more from the leader is
/// passed over.
///
/// It is not the day's feeding. `Character.Turn` calls `group.Eat(null, …)`,
/// and a null leader turns the filter off, so a companion who has wandered off
/// is still fed at the end of the day. Only the hand-to-mouth interactions pass
/// a leader: eating or drinking from the inventory or a room's floor, and the
/// spring room watering the group as it is entered.
pub fn group_eat_near(
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    for step in 0..MAX_FOOD {
        if value <= 0 {
            break;
        }
        if leader.food == step && leader.food < leader.max_food {
            value -= 1;
            leader.food += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if !in_group_range(leader_at, who.at) {
                continue;
            }
            if who.needs.food == step && who.needs.food < who.needs.max_food {
                value -= 1;
                who.needs.food += 1;
            }
        }
    }
    value
}

/// `Group.Eat(selected, value)`, measured from `center` — see
/// [`group_drink_around`].
pub fn group_eat_around(
    center: Point,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    value: i32,
) -> i32 {
    group_eat_from(Some(center), leader, leader_at, companions, value)
}

/// `Group.Eat(selected, value)` with the range filter on (`Some(center)`)
/// or off (`None`). v1.2's `Group.IgnoreRangeFilter` (commit `6d5f752`,
/// `Logic/Character/Group.cs:32-45`) turns it off in a v1.2 game's rooms and
/// scenes entered in group mode: `IsRangeFiltered` is then false, and
/// `Eat`'s `continue` (`:92-94`) skips nobody.
pub fn group_eat_from(
    center: Option<Point>,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    let near = |at: Point| center.is_none_or(|c| in_group_range(c, at));
    let leader_near = near(leader_at);
    for step in 0..MAX_FOOD {
        if value <= 0 {
            break;
        }
        if leader_near && leader.food == step && leader.food < leader.max_food {
            value -= 1;
            leader.food += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if !near(who.at) {
                continue;
            }
            if who.needs.food == step && who.needs.food < who.needs.max_food {
                value -= 1;
                who.needs.food += 1;
            }
        }
    }
    value
}

/// `Group.Drink(leader, value)`, the same shape and the same filter.
pub fn group_drink_near(
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    for step in 0..MAX_WATER {
        if value <= 0 {
            break;
        }
        if leader.water == step && leader.water < leader.max_water {
            value -= 1;
            leader.water += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if !in_group_range(leader_at, who.at) {
                continue;
            }
            if who.needs.water == step && who.needs.water < who.needs.max_water {
                value -= 1;
                who.needs.water += 1;
            }
        }
    }
    value
}

/// `Group.Drink(selected, value)` with somebody other than the player's own
/// character selected: the range is measured from **him**, and the player's
/// character is one of the group, filtered like the rest. `leader_at` is where
/// the player's character stands; with him as `center` this is exactly
/// [`group_drink_near`].
pub fn group_drink_around(
    center: Point,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    value: i32,
) -> i32 {
    group_drink_from(Some(center), leader, leader_at, companions, value)
}

/// `Group.Drink(selected, value)` with the range filter on or off — see
/// [`group_eat_from`] (`Group.cs:120-122`).
pub fn group_drink_from(
    center: Option<Point>,
    leader: &mut CharacterNeeds,
    leader_at: Point,
    companions: &mut [Companion],
    mut value: i32,
) -> i32 {
    let near = |at: Point| center.is_none_or(|c| in_group_range(c, at));
    let leader_near = near(leader_at);
    for step in 0..MAX_WATER {
        if value <= 0 {
            break;
        }
        if leader_near && leader.water == step && leader.water < leader.max_water {
            value -= 1;
            leader.water += 1;
        }
        for who in companions.iter_mut() {
            if value <= 0 {
                break;
            }
            if !near(who.at) {
                continue;
            }
            if who.needs.water == step && who.needs.water < who.needs.max_water {
                value -= 1;
                who.needs.water += 1;
            }
        }
    }
    value
}

/// Everyone travelling with the player.
///
/// The leader is the character being played; the rest are the group they
/// picked up. `Player.Group` in the original, where they walk together, eat
/// their own food and can be left behind — which is the only way a location
/// is ever claimed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Party {
    pub companions: Vec<Companion>,
    /// The next identity to hand out.
    next_id: usize,
}

/// Which companions join the leader's attack, by their place in the group.
///
/// `Player.Group.Where(ch => (ch.Position - Position).Length < 25)`, measured
/// from the leader, each member on their own. **One member's distance never
/// decides another's**: a companion at 20 joins while one at 30 beside them
/// stays out, and the one who stays out is not touched by the exchange at all
/// — neither struck nor answered, nor made to draw a weapon.
///
/// Single mode does not enter into it. v0.9.1's `Attack` asks only the
/// distance; the `!Player.SingleMode` in the same line is a later release's.
pub fn attack_party(party: &Party, leader_at: Point) -> Vec<usize> {
    party
        .companions
        .iter()
        .enumerate()
        .filter(|(_, c)| crate::combat::joins_group_attack(leader_at, c.at))
        .map(|(i, _)| i)
        .collect()
}

/// The order of a group, and why it is this one.
///
/// `Player.Character`'s setter does `Group.Insert(0, character)`, so **the
/// leader is always index 0**. `Hire` does `Group.Add`, which appends, and so
/// does `LeaveCamp` — somebody taken back out of a camp rejoins at the back
/// rather than in their old place. So the order is the leader, then everyone
/// else by when they last joined the travelling group.
///
/// It is not a detail. `Group.FindFood` takes the first of equals, so a tie
/// between two identical meals is settled by position; and `Group.Eat` walks
/// the list at each level, so who eats first among the equally hungry is too.
impl Party {
    pub fn new() -> Party {
        Party { companions: Vec::new(), next_id: 1 }
    }

    /// The next identity, so that a save can carry it.
    ///
    /// It is saved rather than worked out from the largest id in use:
    /// somebody dismissed must not have their number handed to the next
    /// person hired.
    pub fn next_id(&self) -> usize {
        self.next_id
    }

    /// A party read back from a save. Takes the counter as well as the
    /// people, for the reason above.
    pub fn restored(companions: Vec<Companion>, next_id: usize) -> Party {
        Party { companions, next_id }
    }

    /// Put somebody back into the party with the identity they already have —
    /// somebody read out of a save, or taken back out of a camp.
    ///
    /// **The counter moves up past them.** An identity is handed out once;
    /// a party rebuilt from a file and then asked for a new person must not
    /// offer a number one of its own people is already using.
    pub fn take_back(&mut self, who: Companion) {
        self.next_id = self.next_id.max(who.id + 1);
        self.companions.push(who);
    }

    /// Move the counter past an identity this party gave out to somebody who
    /// is not travelling with it — a camper, read back from a save.
    pub fn clear_id(&mut self, id: usize) {
        self.next_id = self.next_id.max(id + 1);
    }

    /// Take on one more person, with an identity of their own.
    pub fn recruit(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.companions.push(Companion::new(id));
        id
    }

    pub fn len(&self) -> usize {
        self.companions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.companions.is_empty()
    }

    /// `JoinCamp`: leave a particular one behind. **This is what claims a
    /// location** — `Location.Player = Player` happens here and nowhere else.
    ///
    /// Which one matters once there is more than one: the original leaves the
    /// character the player is acting on, not whoever happens to be last.
    pub fn leave(&mut self, camps: &mut Camps, location: usize, player: usize, who: usize) -> bool {
        if who >= self.companions.len() {
            return false;
        }
        // The person moves, pack and all: the original hands the character
        // itself over rather than copying anything out of it.
        let companion = self.companions.remove(who);
        camps.join(location, player, companion);
        true
    }

    /// **`Dismiss` for somebody walking with the group**: out of it, and the
    /// caller hands them back to the world where the group is. Nothing they
    /// carry is dropped.
    pub fn dismiss(&mut self, who: usize) -> Option<Companion> {
        (who < self.companions.len()).then(|| self.companions.remove(who))
    }

    /// The last of them, for a caller with nobody in particular in mind.
    pub fn leave_one(&mut self, camps: &mut Camps, location: usize, player: usize) -> bool {
        let last = self.companions.len().checked_sub(1);
        match last {
            Some(who) => self.leave(camps, location, player, who),
            None => false,
        }
    }

    /// `LeaveCamp`: take one back. When the last one goes the location has
    /// no owner again, and stops producing.
    ///
    /// They rejoin at the **back** of the group, not where they were:
    /// `LeaveCamp` calls `Player.Group.Add(this)`, which appends. Somebody
    /// taken out of a camp is therefore fed after everyone who stayed with
    /// you, and the order of the group is "the leader, then whoever joined it
    /// most recently".
    /// **Only your own.** `LeaveCamp` is a method on a character of the
    /// active player's, so there is no way to reach somebody else's camper
    /// with it — and reaching one would be worse than a missing feature: it
    /// would empty an enemy camp without a fight and hand you their man.
    /// A camp nobody owns has nobody in it.
    pub fn take_one_back(&mut self, camps: &mut Camps, location: usize, player: usize) -> bool {
        let Some(camp) = camps.at(location) else { return false };
        if camp.owner != Some(player) {
            return false;
        }
        let Some(who) = camp.residents.pop() else { return false };
        if camp.is_empty() {
            camp.owner = None;
        }
        // Through `take_back`, so that a man who has been sitting in a camp
        // since before a load cannot have his number handed out again.
        self.take_back(who);
        true
    }
}

/// The camps of the world, keyed by location.
#[derive(Debug, Default)]
pub struct Camps {
    camps: Vec<(usize, Camp)>,
}

impl Camps {
    pub fn new() -> Camps {
        Camps { camps: Vec::new() }
    }

    /// Every location's camp as the 1993 save has it: nobody owns anything,
    /// and each location outside the cities is set to produce something.
    pub fn from_gamdat(gam: &crate::gamdat::Gamdat) -> Camps {
        let mut camps = Camps::new();
        for (i, loc) in gam.locations.iter().enumerate() {
            camps.camps.push((
                i,
                Camp {
                    owner: (loc.owner != 0).then(|| loc.owner as usize - 1),
                    residents: Vec::new(),
                    production: loc.producing,
                    progress: 0.0,
                    stationed: Vec::new(),
                    manual_production: false,
                },
            ));
        }
        camps
    }

    /// Every camp, as `(location, camp)`.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Camp)> + '_ {
        self.camps.iter().map(|(i, c)| (*i, c))
    }

    /// Put one back as it was, for restoring a saved world.
    pub fn set(&mut self, location: usize, camp: Camp) {
        match self.camps.iter_mut().find(|(i, _)| *i == location) {
            Some((_, existing)) => *existing = camp,
            None => self.camps.push((location, camp)),
        }
    }

    pub fn get(&self, location: usize) -> Option<&Camp> {
        self.camps.iter().find(|(i, _)| *i == location).map(|(_, c)| c)
    }

    /// Leave somebody here. `JoinCamp`: this is what gives a location an
    /// owner, and the only thing that does.
    pub fn join(&mut self, location: usize, player: usize, who: Companion) {
        // A location with no entry gets one. Doing nothing instead was a
        // silent no-op that only worked because the game happens to build an
        // entry for all thirty seven up front.
        if self.at(location).is_none() {
            self.camps.push((location, Camp::default()));
        }
        if let Some(camp) = self.at(location) {
            camp.owner = Some(player);
            camp.residents.push(who);
        }
    }

    /// **`Dismiss` for a camper**: out of the camp. When they were the last
    /// one (`IsLastInCamp`) the place becomes nobody's, and the owner it had
    /// comes back as the second value so the caller can decide whether that
    /// was a takeover (`ClearItemsAfterTakeover`, when the owner is not the
    /// active player).
    pub fn dismiss(&mut self, location: usize, who: usize) -> Option<(Companion, Option<usize>)> {
        let camp = self.at(location)?;
        if who >= camp.residents.len() {
            return None;
        }
        let one = camp.residents.remove(who);
        let freed = if camp.is_empty() { camp.owner.take() } else { None };
        Some((one, freed))
    }

    pub fn at(&mut self, location: usize) -> Option<&mut Camp> {
        let at = self.camps.iter().position(|(i, _)| *i == location)?;
        Some(&mut self.camps[at].1)
    }

    /// The residents of these locations' camps, `(location, resident)`, in
    /// the order the locations are given, to change together.
    pub fn residents_in(&mut self, locations: &[usize]) -> Vec<(usize, &mut Companion)> {
        let mut found: Vec<(usize, usize, &mut Companion)> = Vec::new();
        for (l, camp) in self.camps.iter_mut() {
            if let Some(order) = locations.iter().position(|x| x == l) {
                for (i, r) in camp.residents.iter_mut().enumerate() {
                    found.push((order, i, r));
                }
            }
        }
        found.sort_by_key(|(o, i, _)| (*o, *i));
        found.into_iter().map(|(o, _, r)| (locations[o], r)).collect()
    }

    /// A player's leader has come back here (`EnterLocation`): one of the
    /// place's people from now on. Once only — `CharacterRespawn` sends him
    /// back to the location he is already in, so a second return is here too.
    pub fn station(&mut self, location: usize, seat: usize) {
        if self.at(location).is_none() {
            self.camps.push((location, Camp::default()));
        }
        if let Some(camp) = self.at(location) {
            if !camp.stationed.contains(&seat) {
                camp.stationed.push(seat);
            }
        }
    }

    /// Which locations have a camp entry at all.
    pub fn locations(&self) -> Vec<usize> {
        self.camps.iter().map(|(i, _)| *i).collect()
    }

    pub fn len(&self) -> usize {
        self.camps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.camps.is_empty()
    }
}

/// Where a location keeps what it holds, as much of it as production needs.
///
/// The rule lives here; the storage lives in whatever is playing the game, so
/// that a test can answer these three questions with a `Vec` and the real
/// thing can answer them with rooms and ground.
pub trait Storage {
    /// How many of that kind lie in the location's rooms. The stock limit
    /// counts the produced item across the rooms and nowhere else.
    fn count_in_rooms(&self, location: usize, kind: usize) -> usize;

    /// How many tools for that production the location holds — items in its
    /// rooms, and in the original also items carried by the camped people,
    /// which this cannot see and does not claim to.
    fn tools(&self, location: usize, production: usize) -> usize;

    /// Has any room here space left? Current upstream tests this before
    /// letting production make progress; v0.9.1 does not.
    fn any_room_has_space(&self, location: usize) -> bool;

    /// `Location.StoreItem`: the first room with space, or outside when none
    /// has any. False only when there is nowhere at all.
    fn store(&mut self, location: usize, kind: usize) -> bool;

    /// Which pump, if any, is **installed** at this location's spring.
    ///
    /// `WaterSource.GetBoost` looks in exactly one place: the items of the
    /// first room flagged `IsWaterSource`. A pump carried in somebody's pack
    /// does nothing, and neither does one left in the storeroom next door.
    /// The two do not add up — `GetBoost`'s `else if` lets the industrial one
    /// win outright.
    fn pump_at(&self, location: usize) -> Pump;

    /// Every item in the location's rooms, as `(room, slot, kind)`, rooms in
    /// order. v1.2's camp larder reads it; the default is none.
    fn room_items(&self, _location: usize) -> Vec<(usize, usize, usize)> {
        Vec::new()
    }

    /// Take one item out of a room. `None` when there is nothing there.
    fn take_from_room(&mut self, _location: usize, _room: usize, _slot: usize) -> Option<usize> {
        None
    }

    /// Turn one room item into another kind, in place (`MakeEmpty`).
    fn replace_in_room(&mut self, _location: usize, _room: usize, _slot: usize, _kind: usize) {}

    /// v1.2's `StoreItem(item, preferredRoom: TrapRoom)`: the first room
    /// holding a tool of the production if it has space, else the first room
    /// with space, else outside. The default is [`Storage::store`].
    fn store_near_tool(&mut self, location: usize, kind: usize, _production: usize) -> bool {
        self.store(location, kind)
    }

    /// `ValidProductions`: the production ids this location can be set to.
    /// The default is none.
    fn valid_productions(&self, _location: usize) -> Vec<usize> {
        Vec::new()
    }
}

/// The stock limit: `MAX_STOCK_FOOD`, counted as **items of the produced
/// kind** across the location's rooms. Not food value, and not all food.
pub const MAX_STOCK_FOOD: usize = 6;

/// What one location's day of production did.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Produced {
    /// Food a day the camp itself gets. Spent feeding the people camped
    /// there before any of it becomes an item — a separate mechanism, and
    /// not a claim that anything was made.
    pub food_per_day: i32,
    /// The item kind that dropped today, if one did.
    pub item: Option<usize>,
    /// The camp makes less than it eats.
    pub starving: bool,
    /// Something was made and there was nowhere to put it.
    pub nowhere_to_put_it: bool,
}

/// Whether the player may change what this place produces.
///
/// `MapScene.OnClickEntrance` in info mode:
///
/// ```csharp
/// clickedLocation.Player == player ||
/// (!clickedLocation.IsCity && Number == player.Location.Id &&
///  clickedLocation.Player == null)
/// ```
///
/// So: a place you hold, **or** a place that is not a city, that you are
/// standing in, and that nobody holds. You can set what a wild place will
/// produce before you claim it — but never a city that is not yours, and
/// never somebody else's camp.
pub fn may_change_production(
    camp: &Camp,
    player: usize,
    is_city: bool,
    standing_here: bool,
) -> bool {
    match camp.owner {
        Some(owner) => owner == player,
        None => !is_city && standing_here,
    }
}

/// Step through a location's available productions, as the info screen does.
///
/// `productionID` is a **slot in `AvailableProducts`**, not a production id.
/// `OnProductionLeft` goes forward when the slot after it holds something;
/// `OnProductionRight` goes back while the slot is above zero — so **once
/// something is chosen it can never be set back to nothing**. Both are the
/// original's, odd names included: "left" advances.
///
/// A gap in the list stops forward movement dead, because the test is on the
/// very next slot rather than the next filled one. No location in the 1993
/// save has a gap, so it never bites there — but it is the rule.
pub fn step_production(
    slot: Option<usize>,
    available: &[Option<usize>; 4],
    forward: bool,
) -> Option<usize> {
    match (slot, forward) {
        // From nothing, forward reaches slot 0 if it holds anything.
        (None, true) => available[0].is_some().then_some(0),
        (None, false) => None,
        (Some(at), true) => {
            if at < 3 && available[at + 1].is_some() {
                Some(at + 1)
            } else {
                Some(at)
            }
        }
        (Some(at), false) => Some(at.saturating_sub(1)),
    }
}

/// Which slot of `available` a camp's current production sits in.
pub fn production_slot(camp: &Camp, available: &[Option<usize>; 4]) -> Option<usize> {
    let production = camp.production?;
    available.iter().position(|slot| *slot == Some(production))
}

/// Set what a place produces.
///
/// **The progress is not touched.** `productionState` is written in exactly
/// three places in the whole tree, all of them inside `Location.Turn`;
/// nothing resets it when the production changes. So a camp that was most of
/// the way to a snake carries that progress into making maggots instead, and
/// because the interval is the new item's food value over the same surplus,
/// switching to something cheaper can drop one the very next day.
///
/// It also does not turn the automatic choice off: `Location.Turn` calls
/// `AutoSelectFoodProduction(onlyIfStarving: true)` every turn regardless, so
/// a starving camp will still switch away from what the player picked.
pub fn set_production(camp: &mut Camp, production: Option<usize>) {
    camp.production = production;
}

/// One camped person's day, as `Character.Turn` does it for a stationed NPC.
///
/// The order differs from a traveller's in two ways that matter:
///
/// - **The camp's production feeds them first.** `if (NPCFoodProduction > 0)
///   { NPCFoodProduction--; Food++; }` — one point each, from the day's
///   yield, before anything else is looked at. Taking the mouths off the
///   surplus is not the same thing: that decides how much becomes items, this
///   decides whether the person eats.
/// - **They drink from the location's spring**, which a traveller does not.
///
/// Returns what the day did to them, and how much of the day's production
/// they did not need.
pub fn camp_turn(
    residents: &mut [Companion],
    mut food_pool: i32,
    spring: Option<&mut WaterSource>,
    facts: &impl ItemFacts,
) -> i32 {
    let mut spring = spring;
    for who in residents.iter_mut() {
        if food_pool > 0 {
            food_pool -= 1;
            who.needs.food = (who.needs.food + 1).min(who.needs.max_food);
        }
        if let Some(source) = spring.as_deref_mut() {
            source.drink(&mut who.needs);
        }
        // A camped person is their own group — `GetGroup()` returns `this`
        // once `JoinCamp` has taken them out of the player's — so what they
        // fall back on is their own pack and nothing else.
        character_day(&mut who.needs, &mut who.pack, facts);
    }
    food_pool
}

/// One location's production for a day, in `Location.Turn`'s order.
///
/// Nothing happens without an owner: `GetFoodProductionRate` returns an empty
/// rate when `Player is null`, and the only thing that gives a location an
/// owner is somebody camping in it. A location nobody holds produces nothing
/// however many knives are lying in it.
pub fn production_turn(
    camp: &mut Camp,
    location: usize,
    productions: &crate::production::Productions,
    facts: &impl ItemFacts,
    storage: &mut impl Storage,
    engine: Engine,
) -> Produced {
    let mut out = Produced::default();
    if camp.owner.is_none() {
        return out;
    }
    let Some(index) = camp.production else { return out };
    let Some(production) = productions.get(index) else { return out };
    let Some(produce_kind) = facts.kind_named(&production.produce) else { return out };
    let produce_food = facts.food(produce_kind);

    // `trapsInRooms + trapsOnNPCs`: what is lying about, and what the people
    // camped there are carrying. Leaving somebody a knife in their pack is
    // as good as leaving it on the floor, and taking them away takes it.
    let carried: usize = camp
        .residents
        .iter()
        .flat_map(|who| who.pack.iter())
        .filter(|&kind| facts.name_of(kind).is_some_and(|name| production.worked_by(name)))
        .count();
    let tools = storage.tools(location, index) + carried;
    // `CampNPC.Count`, a leader who came back here included. He carries
    // nothing — `Die` dropped it all and nothing gives him more.
    let rate = production.rate(tools, camp.head_count(), produce_food);
    out.food_per_day = rate.food_per_day;
    out.starving = rate.camp_starving;

    if rate.item_drop_interval <= 0.0 {
        return out;
    }

    // The whole step sits inside the stock test, so progress does not
    // accumulate while the store is full — it resumes from where it stopped
    // once something is taken away.
    if storage.count_in_rooms(location, produce_kind) >= MAX_STOCK_FOOD {
        return out;
    }
    // Current upstream adds a second condition: a location whose rooms are
    // all full makes no progress either, so the day is lost rather than
    // banked. v0.9.1 has only the stock test above, and its progress keeps
    // accumulating until the item can be placed — outside if need be.
    if engine == Engine::Current && !storage.any_room_has_space(location) {
        return out;
    }

    camp.progress += 1.0;
    if camp.progress >= rate.item_drop_interval {
        camp.progress -= rate.item_drop_interval;
        if storage.store(location, produce_kind) {
            out.item = Some(produce_kind);
        } else {
            out.nowhere_to_put_it = true;
        }
    }
    out
}

/// Everything a day happens to, apart from the character being played.
///
/// Bundled because `Location.Turn` needs all of it at once and in one order:
/// the spring yields, the camp produces, the camped people eat and drink, and
/// only then is the spring capped.
pub struct WorldDay<'a, S: Storage> {
    pub springs: &'a mut Springs,
    pub camps: &'a mut Camps,
    pub storage: &'a mut S,
    pub productions: &'a crate::production::Productions,
    pub engine: Engine,
    /// Who died in the camps today, filled in by [`WorldDay::turn`]. The
    /// caller drops what they left where it belongs.
    pub deaths: Vec<Death>,
    /// Everybody waiting to come back, and how long. Ticked **last**, the
    /// way `World.Turn` ends with `Respawn.Object.Turn()`.
    pub respawns: &'a mut Respawns,
    /// Who came back today.
    pub returned: Vec<Returned>,
}

impl<S: Storage> WorldDay<'_, S> {
    /// `World.Turn`'s loop over the locations, in `Location.Turn`'s order.
    pub fn turn(&mut self, facts: &impl ItemFacts) {
        // The springs yield first, everywhere — `Location.Turn` begins with
        // `Source.BeginTurn()` and a location with no camp still has water.
        // (A v1.2 game's day is not this: see the game's own `World.Turn`.)
        for location in self.springs.locations() {
            let pump = self.storage.pump_at(location);
            let engine = self.engine;
            if let Some(spring) = self.springs.at(location) {
                spring.begin_turn(engine, pump);
            }
        }

        for location in self.camps.locations() {
            let produced = match self.camps.at(location) {
                Some(camp) => {
                    let out = production_turn(
                        camp,
                        location,
                        self.productions,
                        facts,
                        self.storage,
                        self.engine,
                    );
                    out.food_per_day
                }
                None => 0,
            };

            // The people in the camp, fed from what it made today.
            if let Some(camp) = self.camps.at(location) {
                if !camp.residents.is_empty() {
                    // Somebody still here who is not a camper: the last
                    // camper to go does not free the place.
                    let others = !camp.stationed.is_empty();
                    let mut residents = core::mem::take(&mut camp.residents);
                    camp_turn(&mut residents, produced, self.springs.at(location), facts);

                    // Anyone who did not survive the day leaves the camp and
                    // drops what they had. `Dismiss` clears the location's
                    // owner only when the last of them goes.
                    let mut fell = Vec::new();
                    let mut i = 0;
                    while i < residents.len() {
                        if residents[i].needs.health > 0 {
                            i += 1;
                            continue;
                        }
                        let mut who = residents.remove(i);
                        who.needs.health = 0;
                        fell.push(Death {
                            id: who.id,
                            origin: who.origin,
                            at: Some(location),
                            dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
                            camp_lost: false,
                            food: who.needs.food,
                            water: who.needs.water,
                            position: who.at,
                        });
                    }
                    let emptied = residents.is_empty() && !fell.is_empty() && !others;
                    if let Some(last) = fell.last_mut() {
                        last.camp_lost = emptied;
                    }
                    if let Some(camp) = self.camps.at(location) {
                        camp.residents = residents;
                        if emptied {
                            // `if (IsLastInCamp) Location.Player = null;`
                            camp.owner = None;
                        }
                    }
                    self.deaths.extend(fell);
                }
            }

            // Bottles left in a water-source room are filled here in
            // `Location.Turn` (`Location.cs:457-463`). A v1.2 game does it in
            // its own day (`v12_day::refill_spring_room`, compliance RULE-1);
            // this legacy day fills a container when it is stored.
        }

        // And capped last, after anyone camped there has drunk.
        for location in self.springs.locations() {
            if let Some(spring) = self.springs.at(location) {
                spring.end_turn();
            }
        }
    }
}

/// The whole world's day, in `World.Turn`'s order: locations first, then the
/// people in them.
///
/// One call is one day. Nothing here counts frames, watches a clock or cares
/// which panel is open — the caller decides when a day ends and this decides
/// what a day is. Two calls are two days; a call that happens to be made
/// while a room is open is the same day as one made on the desert.
///
/// `World.Turn` also turns traders, NPCs and respawns, and produces food at
/// camps. None of that exists yet. What is here is the part that a lone
/// character on foot can actually feel.
pub fn advance_day<S: Storage>(
    day: &mut u16,
    world: &mut WorldDay<'_, S>,
    needs: &mut CharacterNeeds,
    party: &mut Party,
    pack: &mut ItemBag,
    facts: &impl ItemFacts,
    journey: &mut Option<Journey>,
) -> DayOutcome {
    world_turn(day, world, facts);
    let out = player_day(needs, party, pack, facts, journey);

    // **Last of all**, the way `World.Turn` ends with
    // `Respawn.Object.Turn()`. It matters that this is after the group's
    // turn and not before: somebody scheduled by a death earlier today is
    // already in the list, so the day of the death counts towards the
    // wait.
    world.returned = world.respawns.advance_day();
    out
}

/// The first half of [`advance_day`]: the day moves on and every location
/// has its turn. A caller that has to act between the locations' turn and
/// the players' — `World.Turn`'s check for a player whose character died —
/// calls the two halves itself and then `respawns.advance_day()`.
pub fn world_turn<S: Storage>(day: &mut u16, world: &mut WorldDay<'_, S>, facts: &impl ItemFacts) {
    *day += 1;
    // Locations before people, so a spring has filled before anyone drinks.
    world.turn(facts);
}

/// The second half: one player's road and their group's day.
pub fn player_day(
    needs: &mut CharacterNeeds,
    party: &mut Party,
    pack: &mut ItemBag,
    facts: &impl ItemFacts,
    journey: &mut Option<Journey>,
) -> DayOutcome {
    // A day of walking is a day like any other: the character still eats,
    // still drinks, still goes without. `Player.Turn` takes the day off the
    // journey and only then moves the player, which is why arriving is
    // something a day does rather than something a click does.
    let mut out = DayOutcome::default();
    if let Some(on_the_road) = *journey {
        out.arrived_at = match travel_a_day(on_the_road) {
            Some(still_going) => {
                *journey = Some(still_going);
                None
            }
            None => {
                *journey = None;
                Some(on_the_road.to)
            }
        };
    }

    let day_out = party_day(needs, pack, party, facts);
    DayOutcome { arrived_at: out.arrived_at, ..day_out }
}

/// A travelling group's day, sharing one larder.
///
/// Each member takes their turn in order, and each of them reaches into
/// **everybody's** packs when they run out: `Character.Turn` asks
/// `GetGroup()`, which for anyone travelling is the player's whole group, and
/// `Group.FindFood` walks every member's items. One person's snake feeds
/// whoever is hungry, and `Group.Eat` spreads it a level at a time so nobody
/// is filled while somebody else is still empty.
///
/// A camped person is the other case entirely and is not here: `JoinCamp`
/// takes them out of the group, so they eat from the location and their own
/// pack alone. See [`camp_turn`].
///
/// The outcome returned is the leader's — the character being played.
pub fn party_day(
    leader: &mut CharacterNeeds,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
) -> DayOutcome {
    let mut leader_out = DayOutcome::default();
    let mut deaths = Vec::new();

    // Index 0 is the leader, `Group[0]`; the rest follow in order.
    //
    // **Somebody who dies leaves the group at once**, before the members
    // after them take their turn — `Die` calls `Dismiss` in the middle of
    // the day, so what they were carrying is on the ground and no longer in
    // the group's larder. Collecting the dead at the end instead would let
    // a corpse feed the living.
    let mut member = 0;
    while member <= party.companions.len() {
        let out = one_members_day(member, leader, leader_pack, party, facts);
        if member == 0 {
            leader_out = out;
            member += 1;
            continue;
        }
        if party.companions[member - 1].needs.health > 0 {
            member += 1;
            continue;
        }
        let mut who = party.companions.remove(member - 1);
        // `health = 0` is `Die`'s own line, and it is why a dead man's
        // health never runs past zero into the negative.
        who.needs.health = 0;
        deaths.push(Death {
            id: who.id,
            origin: who.origin,
            at: None,
            dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
            camp_lost: false,
            food: who.needs.food,
            water: who.needs.water,
            position: who.at,
        });
        // The next member has shifted into this index.
    }

    leader_out.deaths = deaths;
    leader_out
}

/// `Character.Die` for a companion who falls where they stand — in a
/// fight, rather than at the end of a day without food.
///
/// **The same death**, and deliberately the same code path as the one
/// [`party_day`] takes: out of the group, health at nought, and everything
/// they carried handed back for the caller to put on the ground. Only the
/// occasion differs.
///
/// `None` when there is no such companion or they are still standing, so a
/// caller cannot bury somebody twice.
/// `Location.entryOffsets`: where `Player.Travel` puts the members of a
/// group at its destination, the leader (0) on the entry point.
pub const ENTRY_OFFSETS: [(i32, i32); 5] = [(0, 0), (0, -8), (8, 0), (-8, 0), (0, 8)];

/// `Location.GetEntryPosition(i)` for party member `i` (the leader is 0). A
/// group holds at most five (`Group.MAX_PEOPLE`); past that, the last offset.
pub fn entry_position(entry: Point, member: usize) -> Point {
    let (dx, dy) = ENTRY_OFFSETS[member.min(ENTRY_OFFSETS.len() - 1)];
    Point::new(entry.x + dx, entry.y + dy)
}

/// Where the human's group stands on entering a place, the leader being
/// put at `spawn` by the caller. A member whose place `keeps` (v1.2: his
/// `Position` set by `Travel`, still standable) stays; any other goes on
/// the entry point (`EnterLocation`; the legacy game, every one). When only
/// the leader was moved — `/petko`'s Travel, `TravelToLocation` with
/// `_debugNoTravel` setting `player.Character.Position` alone
/// (`MapScene.cs:1203-1207`) — nobody else is touched: the members keep
/// the coordinates of the place they left.
pub fn arrive(party: &mut Party, spawn: Point, leader_only: bool, keeps: impl Fn(Point) -> bool) {
    if leader_only {
        return;
    }
    for friend in party.companions.iter_mut() {
        if !keeps(friend.at) {
            friend.at = spawn;
            friend.move_to = spawn;
        }
    }
}

/// `Player.Travel`'s placing of a machine group: the i-th companion (party
/// member i + 1) at its entry position.
pub fn place_at_entry(party: &mut Party, entry: Point) {
    for (i, c) in party.companions.iter_mut().enumerate() {
        c.at = entry_position(entry, i + 1);
        c.move_to = c.at;
    }
}

pub fn companion_fell(party: &mut Party, who: usize) -> Option<Death> {
    if party.companions.get(who)?.needs.health > 0 {
        return None;
    }
    let mut who = party.companions.remove(who);
    who.needs.health = 0;
    Some(Death {
        id: who.id,
        origin: who.origin,
        at: None,
        dropped: core::mem::replace(&mut who.pack, ItemBag::new(0)),
        camp_lost: false,
        food: who.needs.food,
        water: who.needs.water,
        position: who.at,
    })
}

/// What one member of a travelling group does on a day.
fn one_members_day(
    member: usize,
    leader: &mut CharacterNeeds,
    leader_pack: &mut ItemBag,
    party: &mut Party,
    facts: &impl ItemFacts,
) -> DayOutcome {
    let read = |leader: &CharacterNeeds, party: &Party| -> CharacterNeeds {
        if member == 0 {
            *leader
        } else {
            party.companions[member - 1].needs
        }
    };

    let mut out = DayOutcome { health_before: read(leader, party).health, ..DayOutcome::default() };

    // Hungry: the best food anywhere in the group, spread across the group.
    if read(leader, party).food == 0 {
        if let Some((whose, slot)) = group_find_food(leader_pack, &party.companions, facts) {
            let kind = pack_slot(leader_pack, party, whose, slot);
            if let Some(kind) = kind {
                let worth = facts.food(kind);
                let left = group_eat(leader, &mut party.companions, worth);
                if eaten(worth, left) {
                    consume_in(leader_pack, party, whose, slot, facts);
                    out.ate = Some(kind);
                }
            }
        }
    }

    // Thirsty: the same, over water.
    if read(leader, party).water == 0 {
        if let Some((whose, slot)) = group_find_water(leader_pack, &party.companions, facts) {
            let kind = pack_slot(leader_pack, party, whose, slot);
            if let Some(kind) = kind {
                let worth = facts.water(kind);
                let left = group_drink(leader, &mut party.companions, worth);
                if eaten(worth, left) {
                    consume_in(leader_pack, party, whose, slot, facts);
                    out.drank = Some(kind);
                }
            }
        }
    }

    // And then this member's own day, exactly as a lone character's.
    let who: &mut CharacterNeeds = if member == 0 {
        leader
    } else {
        &mut party.companions[member - 1].needs
    };

    if who.health >= REGEN_FLOOR_WITHOUT_DOCTOR {
        who.health = (who.health + REGEN_WITHOUT_DOCTOR).min(MAX_HEALTH);
    }
    if who.food == 0 {
        out.deprivation += DEPRIVATION;
    }
    if who.water == 0 {
        out.deprivation += DEPRIVATION;
    }
    who.health -= out.deprivation;
    if who.health <= 0 {
        out.died = true;
        return out;
    }
    who.food = (who.food - 1).max(0);
    who.water = (who.water - 1).max(0);
    out
}

/// What is in that slot of that member's pack.
fn pack_slot(
    leader_pack: &ItemBag,
    party: &Party,
    whose: Whose,
    slot: usize,
) -> Option<usize> {
    match whose {
        Whose::Leader => leader_pack.get(slot),
        Whose::Companion(i) => party.companions.get(i)?.pack.get(slot),
    }
}

/// Use up that slot, in whoever's pack it is.
fn consume_in(
    leader_pack: &mut ItemBag,
    party: &mut Party,
    whose: Whose,
    slot: usize,
    facts: &impl ItemFacts,
) {
    match whose {
        Whose::Leader => consume(leader_pack, slot, facts),
        Whose::Companion(i) => {
            if let Some(who) = party.companions.get_mut(i) {
                consume(&mut who.pack, slot, facts);
            }
        }
    }
}

/// How far a character moves per step. Movement is free — there is no collision
/// yet, because the walkability masks for the classic tiles live in the
/// player's own `zei_<section>.raw`.
pub const WALK_SPEED: i32 = 2;

/// Move `from` towards `to` by at most `speed`, staying inside `bounds`.
///
/// Returns the new position; when it equals `to`, the walk is over.
pub fn step_towards(from: Point, to: Point, speed: i32, bounds: (u32, u32)) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx == 0 && dy == 0 {
        return from;
    }

    // Integer approximation of a straight line: move the long axis by the full
    // step and the short axis in proportion. Good enough without pathfinding,
    // and it never overshoots.
    let (adx, ady) = (dx.abs(), dy.abs());
    let long = adx.max(ady);
    let (sx, sy) = if long <= speed {
        (dx, dy)
    } else {
        (dx * speed / long, dy * speed / long)
    };

    clamp(Point::new(from.x + sx, from.y + sy), bounds)
}

/// Move `from` towards `to`, stopping at whatever is in the way.
///
/// Without pathfinding, "stop" is the honest behaviour: try the full step,
/// then try each axis alone so a character sliding along a wall keeps moving,
/// and otherwise stand still. `passable` is asked about the destination pixel.
pub fn step_towards_passable(
    from: Point,
    to: Point,
    speed: i32,
    bounds: (u32, u32),
    passable: impl Fn(Point) -> bool,
) -> Point {
    let full = step_towards(from, to, speed, bounds);
    if full == from || passable(full) {
        return full;
    }
    // Sliding: keep whichever single axis is allowed, so a character brushing
    // a wall keeps moving instead of sticking to it.
    for candidate in [Point::new(full.x, from.y), Point::new(from.x, full.y)] {
        if candidate != from && passable(candidate) {
            return candidate;
        }
    }
    // Last resort, for a character already standing somewhere blocked: move
    // anyway. Every step out of a wall thicker than one step is still inside
    // it, so without this a change in the data would strand them for good.
    //
    // It is an attempt, not a guarantee: nothing here knows which way is out,
    // so a character can walk further in. It only fires when they are already
    // somewhere they should not be, and only when no better step exists.
    if !passable(from) {
        return full;
    }
    from
}

/// Keep a position on the map.
pub fn clamp(p: Point, bounds: (u32, u32)) -> Point {
    Point::new(p.x.clamp(0, bounds.0 as i32 - 1), p.y.clamp(0, bounds.1 as i32 - 1))
}

/// How close you have to get to a door for it to open.
///
/// The original's `EntryCondition.MaxDistanceOnMap`, which
/// `OriginalLocationCreator` sets to 15 for an ordinary room. It is a distance
/// rather than "standing inside the rectangle" for a good reason: the tile
/// under a doorway is part of the building and is not walkable, so a character
/// can never stand in the rectangle. They stop in front of it and go in.
pub const ENTRY_REACH: i32 = 15;

/// Squared distance from a point to a rectangle; zero inside it.
///
/// Squared, so the comparison stays in integers — the original's
/// `Rect.Distance` takes the square root and compares against 15.
fn distance_squared_to(e: &crate::map::Entrance, at: Point) -> i64 {
    let dx = (e.left - at.x).max(0).max(at.x - (e.left + e.width - 1)) as i64;
    let dy = (e.top - at.y).max(0).max(at.y - (e.top + e.height - 1)) as i64;
    dx * dx + dy * dy
}

/// The entrance whose rectangle contains this point, if any.
///
/// Distinct from [`entrance_in_reach`], and the distinction matters at the
/// moment of a click: a rope room is *usable* from 75 pixels away, which is
/// nowhere near a claim that 75 pixels of map belong to it. What a click hits
/// is the doorway drawn on the screen.
pub fn entrance_at(map: &Map, at: Point) -> Option<usize> {
    map.entrances.iter().position(|e| distance_squared_to(e, at) == 0)
}

/// The entrance the character is close enough to use, if any.
///
/// Clicking a door from across the map still does nothing: the character has to
/// walk there first, and `reach` is how near counts as arrived.
pub fn entrance_in_reach(map: &Map, at: Point, reach: i32) -> Option<usize> {
    let limit = (reach as i64) * (reach as i64);
    map.entrances
        .iter()
        .enumerate()
        .filter(|(_, e)| distance_squared_to(e, at) <= limit)
        .min_by_key(|(_, e)| distance_squared_to(e, at))
        .map(|(i, _)| i)
}

/// Is the character close enough to *this* entrance?
///
/// Not "is this the nearest one" — two doors can both be in reach, and the one
/// you asked for should open even when another is closer. Asking about the
/// nearest was a bug: standing between two huts opened the wrong one.
/// The squared distance from a point to an entrance's rectangle (nought
/// inside it), as `Rect.Distance` measures before its root.
pub fn entrance_distance_squared(map: &Map, index: usize, at: Point) -> Option<i64> {
    map.entrances.get(index).map(|e| distance_squared_to(e, at))
}

pub fn entrance_reached(map: &Map, index: usize, at: Point, reach: i32) -> bool {
    match map.entrances.get(index) {
        Some(e) => in_reach_of(e, at, reach),
        None => false,
    }
}

/// Is `at` close enough to this doorway to use it?
///
/// The one rule, so that walking to a door and arriving at one cannot come to
/// disagree: [`path_to_entrance`] searches for a cell this answers yes for.
pub fn in_reach_of(e: &crate::map::Entrance, at: Point, reach: i32) -> bool {
    distance_squared_to(e, at) <= (reach as i64) * (reach as i64)
}

/// The entrance the character is standing exactly inside, if any.
pub fn entrance_under(map: &Map, at: Point) -> Option<usize> {
    entrance_in_reach(map, at, 0)
}

/// The middle of a map: the stand-in for a missing entry point.
pub fn map_centre(map: &Map) -> Point {
    let (w, h) = map.pixel_size();
    Point::new(w as i32 / 2, h as i32 / 2)
}

/// Where to put the character on arriving at a location.
///
/// The real value comes from `GAMDAT`. Until that exists, the centre of the map
/// stands in — and because [`LocationData::entry_point`] is an `Option`, it is
/// always clear which of the two happened.
pub fn spawn_point(location: &LocationData, map: &Map) -> Point {
    location.entry_point.unwrap_or_else(|| map_centre(map))
}

/// Somewhere the character could stand to use this entrance, and how to get
/// there.
///
/// A door is not a destination. Its rectangle covers the doorway itself, which
/// `BurnmapProcessor` blocks along with the rest of the building, so walking
/// *to* a door means walking to a cell that is walkable, connected to where
/// the character is, and within the entrance's reach — 15 pixels for most
/// rooms and 75 for one entered by rope.
///
/// Breadth-first over the walk grid, so the first cell found is the fewest
/// steps away. The grid is at most a few thousand cells and this runs on a
/// click, not a frame.
///
/// `None` means there is no such cell: either the doorway is walled off, or
/// nothing that can reach it is reachable from here. That is a different
/// answer from "the character stopped at a wall", and the difference is worth
/// having — one of them is a missing algorithm and the other is bad data.
pub fn path_to_entrance(
    grid: &WalkGrid,
    from: Point,
    entrance: &crate::map::Entrance,
    reach: i32,
) -> Option<Vec<Point>> {
    // Measured from the cell's centre, because that is where the walker
    // stands. Measuring from the nearest corner instead would find cells up to
    // five pixels closer — a third of an ordinary reach — and then the walk
    // would end somewhere `entrance_reached` says is too far, because that
    // asks about the character's actual position. Search and arrival must
    // answer the same question or the route ends at a door that will not open.
    route(grid, from, |p| in_reach_of(entrance, p, reach))
}

/// The same search, aimed at a point rather than a doorway.
pub fn path_to(grid: &WalkGrid, from: Point, to: Point) -> Option<Vec<Point>> {
    let goal = cell_of(to);
    route(grid, from, move |p| cell_of(p) == goal)
}

/// The centre of the mask cell a point falls in — where the walker aims, since
/// walkability is decided per cell rather than per pixel.
fn cell_of(p: Point) -> (i32, i32) {
    (p.x.div_euclid(MASK_CELL as i32), p.y.div_euclid(MASK_CELL as i32))
}

fn centre_of(cx: i32, cy: i32) -> Point {
    let half = MASK_CELL as i32 / 2;
    Point::new(cx * MASK_CELL as i32 + half, cy * MASK_CELL as i32 + half)
}

/// Breadth-first from `from` to the nearest cell satisfying `done`.
///
/// The returned path is cell centres, ending at the goal cell and excluding
/// the starting one. A caller already standing on a goal cell gets an empty
/// path rather than `None`: it has arrived.
fn route(grid: &WalkGrid, from: Point, done: impl Fn(Point) -> bool) -> Option<Vec<Point>> {
    let (w, h) = (grid.width as i32, grid.height as i32);
    let start = cell_of(from);

    // Where the search starts is checked first, and `done` is not consulted
    // until it passes. The other order says yes to a character standing off
    // the map who happens to be near the door — a position it could not have
    // walked to and cannot act from.
    //
    // The bounds check comes before the cell index because a negative
    // coordinate cast to `usize` is not a small number, it is an enormous one.
    if start.0 < 0 || start.1 < 0 || start.0 >= w || start.1 >= h {
        return None;
    }
    if !grid.cell(start.0 as usize, start.1 as usize) {
        return None;
    }
    if done(from) {
        return Some(Vec::new());
    }

    let index = |c: (i32, i32)| (c.1 * w + c.0) as usize;
    let mut came: Vec<i32> = alloc::vec![-1; (w * h) as usize];
    let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
    came[index(start)] = index(start) as i32;
    queue.push_back(start);

    while let Some(c) = queue.pop_front() {
        // Four-way, because the walker moves along axes when it slides and a
        // diagonal step through a corner is not one it could take.
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (c.0 + dx, c.1 + dy);
            if next.0 < 0 || next.1 < 0 || next.0 >= w || next.1 >= h {
                continue;
            }
            if !grid.cell(next.0 as usize, next.1 as usize) {
                continue;
            }
            let at = index(next);
            if came[at] != -1 {
                continue;
            }
            came[at] = index(c) as i32;

            if done(centre_of(next.0, next.1)) {
                // Walk the trail back, then turn it round.
                let mut path = alloc::vec![centre_of(next.0, next.1)];
                let mut here = at;
                while came[here] as usize != here {
                    here = came[here] as usize;
                    let (cx, cy) = ((here as i32) % w, (here as i32) / w);
                    path.push(centre_of(cx, cy));
                }
                path.pop(); // the cell already stood on
                path.reverse();
                return Some(path);
            }
            queue.push_back(next);
        }
    }
    None
}

/// What a click means: which door, if any, and how to get to it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Intent {
    /// Go there. Nothing happens on arrival.
    Walk,
    /// Go to where this entrance can be used, and use it.
    Enter(usize),
}

/// A decided click: everything the caller needs to carry it out.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Plan {
    pub intent: Intent,
    /// Where the click landed, in map pixels. Only somewhere to aim when there
    /// is no collision data; the route is the real answer.
    pub target: Point,
    /// Waypoints, in order, excluding where the character already stands.
    pub path: Vec<Point>,
    /// Nothing to walk: the character can act where it is.
    ///
    /// An empty `path` alone does not say this — a walk of no steps and an
    /// arrival are different things, and treating them alike made the game
    /// fall back to the clicked pixel and march into the building it had just
    /// been asked to enter.
    pub act_now: bool,
}

/// Turn a click into a plan.
///
/// The whole of the decision, in one place: which doorway was hit, whether the
/// character is already close enough to use it, and the route to somewhere it
/// can be used from. Callers step along `path` and act when they run out.
///
/// `reach_of` gives an entrance's own reach — 75 for a rope room, 15 for the
/// rest. `slack` is how far a click may miss a doorway and still mean it, and
/// is deliberately *not* the reach: how far away a room can be used from is no
/// claim on that much of the map.
///
/// `None` means there is no way to reach what was asked for.
pub fn plan_click(
    map: &Map,
    grid: Option<&WalkGrid>,
    from: Point,
    at: Point,
    reach_of: impl Fn(usize) -> i32,
    slack: i32,
) -> Option<Plan> {
    // The doorway clicked, else the nearest one a hair away. By distance, so
    // two ordinary doors are told apart by which was clicked rather than by
    // which came first in the file.
    let intent = entrance_at(map, at)
        .or_else(|| entrance_in_reach(map, at, slack))
        .map_or(Intent::Walk, Intent::Enter);

    let Some(grid) = grid else {
        // No collision data is not free movement by choice; there is simply
        // nothing to route around.
        let act_now = matches!(&intent, Intent::Enter(i) if entrance_reached(map, *i, from, reach_of(*i)));
        return Some(Plan { intent, target: at, path: Vec::new(), act_now });
    };

    let path = match &intent {
        Intent::Enter(i) => path_to_entrance(grid, from, map.entrances.get(*i)?, reach_of(*i))?,
        Intent::Walk => path_to(grid, from, at)?,
    };
    // Already in reach, and it is a door: act. Already at the spot, and it is
    // a walk: there is nothing to do either way.
    let act_now = path.is_empty() && matches!(intent, Intent::Enter(_));
    Some(Plan { intent, target: at, path, act_now })
}

/// How far [`nearest_standing`] will move an arrival: one tile.
pub const NUDGE_LIMIT: i32 = MASK_GRID as i32;

/// The nearest place the character could actually stand.
///
/// `gam.dat` puts Hard Mans Death's arrival on a cell the tile masks block. Left
/// alone that is not a cosmetic problem: every search starts from where the
/// character is, refuses to start from somewhere unwalkable, and so nothing in
/// that location works at all. Nudging the arrival to the nearest open cell
/// costs a few pixels of fidelity and buys a location.
///
/// The search is deliberately short — one tile, [`NUDGE_LIMIT`] cells — so
/// this can only correct a point that is *just* inside something. Searching
/// further would find open ground on the other side of a building and put the
/// character somewhere the file plainly did not mean.
///
/// `None` when there is nothing to stand on that near. That is not a state to
/// paper over: every search starts from the character's position, so a caller
/// that falls back to the blocked point has a location where nothing works and
/// no message saying why.
pub fn nearest_standing(grid: &WalkGrid, from: Point) -> Option<Point> {
    const _: () = assert!(NUDGE_LIMIT == MASK_GRID as i32);
    if grid.walkable_at(from.x, from.y) {
        return Some(from);
    }
    let (w, h) = (grid.width as i32, grid.height as i32);
    let (sx, sy) = cell_of(from);
    // Rings outwards, nearest first.
    for r in 1..=NUDGE_LIMIT {
        let mut best: Option<(i64, Point)> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                // The ring only, so nearer rings are not searched twice.
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (cx, cy) = (sx + dx, sy + dy);
                if cx < 0 || cy < 0 || cx >= w || cy >= h || !grid.cell(cx as usize, cy as usize) {
                    continue;
                }
                let p = centre_of(cx, cy);
                let d = p.distance_squared(from);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, p));
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }
    None
}

/// A walkable point next to something that may not be walkable itself.
///
/// **Somebody standing about need not be standing on walkable ground.** The
/// save's characters have their own positions and our walk mask is built
/// from the location's tiles, so the two do not always agree — and a click
/// on a figure that walks *onto* them fails to find a path at all. This
/// answers the question the click actually asks: where can I stand to be
/// within reach of them?
///
/// The nearest such point, searched outwards a cell at a time, so the
/// character stops beside the person rather than at the far edge of reach.
/// `None` means nowhere within `reach` can be stood on.
pub fn approach(walk: &crate::zei::WalkGrid, target: Point, reach: i32) -> Option<Point> {
    if walk.walkable_at(target.x, target.y) {
        return Some(target);
    }
    let cell = crate::zei::MASK_CELL as i32;
    let mut ring = cell;
    while ring <= reach {
        let mut best: Option<(i32, Point)> = None;
        let mut y = target.y - ring;
        while y <= target.y + ring {
            let mut x = target.x - ring;
            while x <= target.x + ring {
                // Only the edge of the ring: the inside was searched already.
                let on_edge = (x - target.x).abs() == ring || (y - target.y).abs() == ring;
                if on_edge && x >= 0 && y >= 0 && walk.walkable_at(x, y) {
                    let d = (x - target.x).pow(2) + (y - target.y).pow(2);
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, Point::new(x, y)));
                    }
                }
                x += cell;
            }
            y += cell;
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
        ring += cell;
    }
    None
}
