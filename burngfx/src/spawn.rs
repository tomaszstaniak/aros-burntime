//! Filling the world when a game begins.
//!
//! The original rolls a location's contents once, at new-game time, and never
//! again — a room the player empties stays empty. That is the whole of what
//! this does: initial contents. Production, respawning and traders restocking
//! are separate mechanisms in `Location.Turn` and are not here.
//!
//! **Ported from `GameCreation.LoadItems` as v0.9.1 has it.** That matters:
//! `main` moved this into an `ItemSpawner` class and changed it on the way,
//! putting a dangerous location's spoils behind the test that skips cities.
//! v0.9.1 tests for a city separately in each block, so a dangerous city
//! still gets its danger items. This was first written from `main` and was
//! wrong in exactly that way.
//!
//! **This reproduces the rules, not the 1993 sequence.** The DOS game's
//! generator is not something we can read, and the remaster's is .NET's
//! `System.Random` seeded from the clock. So the seed is ours and the numbers
//! are ours; what is faithful is which items may appear where, how many, and
//! that it happens once.
//!
//! The passes, in order:
//!
//! 1. `random_items_start` into **every** player's location — all four, not
//!    just the one being played. The other three are AI players in the
//!    original; their towns are stocked whether or not anybody is driving
//!    them.
//! 2. `random_items_ground` outside, in **every** location including the
//!    cities.
//! 3. `random_items_room` into the rooms of every location that is not a
//!    city.
//! 4. `random_items_danger_location` wherever there is a danger — city or
//!    not — each item going to a room or the ground on a coin flip.
//! 5. `random_items_closed_room` into every room that needs an item to enter,
//!    which in the classic maps means the rope rooms, and again only outside
//!    the cities.
//!
//! Nothing is thrown away. `StoreItem` drops an item outside when no room
//! will take it, and so does this.

use alloc::vec::Vec;

use crate::game::ItemBag;
use crate::gamdat::Gamdat;
use crate::items::ItemTypes;
use crate::matmap::RoomType;
use crate::settings::{Filter, GameSettings, Spawn};

/// A small, explicit generator — or the game's own.
///
/// **Two generators behind one type.** A legacy game draws from streams of
/// its own, each `xorshift64*` — three shifts and a multiply, no tables, no
/// `std` — built per purpose from the seed (`seeded`, and
/// [`crate::trader::rng_for`]): the same seed gives the same world on the
/// host and on AROS. A v1.2 game has **one** generator for everything, as
/// upstream has (`Platform.Math.Random`), and it is .NET's own algorithm
/// ([`crate::net_random`]): [`Rng::game`]. Like upstream's it belongs to the
/// process and is not saved: a load continues whatever it has reached.
///
/// The methods are named for the .NET call each stands for. On the game's
/// generator they are that call exactly; on a legacy stream they are what
/// the port drew there before, so a legacy game is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Xorshift(u64),
    // Boxed: 232 bytes against 8, and the legacy streams are made by the
    // dozen while the game's generator is one.
    Net(alloc::boxed::Box<crate::net_random::NetRandom>),
}

impl Rng {
    /// A legacy stream. A seed of zero would make xorshift produce nothing
    /// but zero, so it is nudged rather than refused.
    pub fn seeded(seed: u64) -> Rng {
        Rng { kind: Kind::Xorshift(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed }) }
    }

    /// A v1.2 game's one generator, `new Random(seed)`: the low 32 bits of
    /// the game's seed, as upstream's `(int)DateTime.Now.Ticks` keeps the
    /// low 32 bits of the clock.
    pub fn game(seed: u64) -> Rng {
        Rng::from_net(crate::net_random::NetRandom::new(seed as u32 as i32))
    }

    /// The game's generator, from a given state.
    pub fn from_net(net: crate::net_random::NetRandom) -> Rng {
        Rng { kind: Kind::Net(alloc::boxed::Box::new(net)) }
    }

    /// The game's generator's state; `None` for a legacy stream.
    pub fn net(&self) -> Option<&crate::net_random::NetRandom> {
        match &self.kind {
            Kind::Net(n) => Some(&**n),
            Kind::Xorshift(_) => None,
        }
    }

    /// 64 bits from a legacy stream. On the game's generator this is
    /// `Next()`, 31 bits: nothing in v1.2 asks for more.
    pub fn next_u64(&mut self) -> u64 {
        match &mut self.kind {
            Kind::Xorshift(state) => {
                let mut x = *state;
                x ^= x >> 12;
                x ^= x << 25;
                x ^= x >> 27;
                *state = x;
                x.wrapping_mul(0x2545_F491_4F6C_DD1D)
            }
            Kind::Net(n) => n.next() as u64,
        }
    }

    /// `Next(n)`: a number below `n`. Zero for `n == 0`, so a caller that
    /// found no candidates does not have to special-case the draw — on the
    /// game's generator after spending a number, as .NET does.
    pub fn below(&mut self, n: u32) -> u32 {
        match &mut self.kind {
            Kind::Xorshift(_) => {
                if n == 0 {
                    return 0;
                }
                // The high bits are the well-mixed ones in xorshift64*.
                ((self.next_u64() >> 32) % n as u64) as u32
            }
            Kind::Net(r) => r.next_max(n.min(i32::MAX as u32) as i32) as u32,
        }
    }

    /// `Next() % 2 == 0`.
    pub fn coin(&mut self) -> bool {
        match &mut self.kind {
            Kind::Xorshift(_) => self.next_u64() & 1 == 0,
            Kind::Net(r) => r.next() % 2 == 0,
        }
    }

    /// `Next()`: `[0, int.MaxValue)`.
    pub fn next_int(&mut self) -> i32 {
        match &mut self.kind {
            Kind::Xorshift(_) => (self.next_u64() >> 33) as i32,
            Kind::Net(r) => r.next(),
        }
    }

    /// `Next() % k`, which is how upstream writes most of its rolls; nought
    /// for `k == 0` without a draw on a legacy stream. A legacy stream gives
    /// what [`Rng::below`] gives, as the port drew there before.
    pub fn next_mod(&mut self, k: u32) -> u32 {
        match &mut self.kind {
            Kind::Xorshift(_) => self.below(k),
            Kind::Net(r) => {
                let n = r.next() as u32;
                if k == 0 { 0 } else { n % k }
            }
        }
    }

    /// `Next(min, max)`: `[min, max)`.
    pub fn range(&mut self, min: i32, max: i32) -> i32 {
        match &mut self.kind {
            Kind::Xorshift(_) => min + self.below(max.saturating_sub(min).max(0) as u32) as i32,
            Kind::Net(r) => r.next_range(min, max),
        }
    }

    /// `NextSingle()`: `[0, 1)`. A legacy stream's is 24 bits over 2^24,
    /// as the port drew it before.
    pub fn single(&mut self) -> f32 {
        match &mut self.kind {
            Kind::Xorshift(_) => self.below(1 << 24) as f32 / (1u32 << 24) as f32,
            Kind::Net(r) => r.next_single(),
        }
    }
}

/// Which counting rule to use for a spawn's `rate`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Counting {
    /// `min..max`, which is what `RandomCount` computes. **The default**,
    /// because the goal is the classic and v0.9.1 is the reference: `1 3`
    /// gives one item or two.
    #[default]
    AsUpstream,
    /// `min..=max`, the range the settings file reads as. An explicit
    /// departure — see [`crate::settings::Rate`].
    Inclusive,
}

/// One location, as much of it as the generator needs to know.
///
/// Deliberately not a loaded map. The world is stocked whether or not its
/// artwork was packed, so a pack with four maps and a pack with thirty eight
/// must lay out the same world: what this needs is the room ids and the
/// walkability grid, and the archive carries both for every location however
/// few of them have pictures.
#[derive(Default)]
pub struct Place<'a> {
    pub room_ids: &'a [u8],
    /// Where an item may be dropped outside. Without it nothing lands on the
    /// ground of that location — which is a gap, not a rule.
    pub walk: Option<&'a crate::zei::WalkGrid>,
    /// `Location.EntryPoint`, in map pixels: where v1.2's `DropItemRandom`
    /// puts an item after ten tries found no ground for it.
    pub entry: crate::game::Point,
}

impl Place<'_> {
    fn capacity(&self, room: usize) -> usize {
        self.room_ids
            .get(room)
            .map_or(0, |&id| RoomType::from_room_id(id).item_capacity())
    }

    fn is_rope(&self, room: usize) -> bool {
        self.room_ids.get(room).map(|&id| RoomType::from_room_id(id)) == Some(RoomType::Rope)
    }

    fn is_water(&self, room: usize) -> bool {
        self.room_ids.get(room).map(|&id| RoomType::from_room_id(id))
            == Some(RoomType::WaterSource)
    }
}

/// The candidate item types for a filter, in table order.
///
/// `GetTypesWithClass`: exclusion first and it wins, then inclusion, and both
/// match either a class or the item's own name.
pub fn candidates(types: &ItemTypes, filter: &Filter) -> Vec<usize> {
    let matches = |kind: usize, names: &[alloc::string::String]| {
        let Some(t) = types.get(kind) else { return false };
        names.iter().any(|n| t.is_class(n) || t.name == *n)
    };

    (0..types.len())
        .filter(|&kind| !matches(kind, &filter.exclude) && matches(kind, &filter.include))
        .collect()
}

/// How many of a spawn to produce, and from what.
struct Pool {
    kinds: Vec<usize>,
}

impl Pool {
    fn of(types: &ItemTypes, spawn: &Spawn) -> Pool {
        Pool { kinds: candidates(types, &spawn.filter) }
    }

    /// `itemTypes[Random.Next() % itemTypes.Length]`.
    fn pick(&self, rng: &mut Rng) -> Option<usize> {
        if self.kinds.is_empty() {
            // The original would divide by zero here. Nothing to give.
            return None;
        }
        Some(self.kinds[rng.below(self.kinds.len() as u32) as usize])
    }
}

fn how_many(spawn: &Spawn, counting: Counting, rng: &mut Rng) -> u32 {
    match counting {
        Counting::AsUpstream => spawn.rate.count_as_upstream(rng),
        Counting::Inclusive => spawn.rate.count(rng),
    }
}

/// One item lying outside, where it was dropped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Dropped {
    pub location: usize,
    /// In map pixels: the walk cell's corner, `cell * MASK_CELL`, which is
    /// what `Items.DropAt(item, pos * Map.Mask.Resolution)` computes.
    pub at: crate::game::Point,
    pub kind: usize,
    /// Rounds left, for a gun put down part-fired; `None` is as new. The
    /// item is one object in v0.9.1 wherever it lies.
    pub rounds: Option<i32>,
    /// v1.2's `DroppedItem.Icon`: which of the two pile pictures
    /// (`munt.raw` 3 or 4) it lies as, drawn `Random.Next() % 2` when it is
    /// put down and kept. [`PILE_UNDRAWN`] until it is drawn, and for good
    /// in a legacy game, which draws no picture for a pile.
    pub icon: u8,
}

/// A pile whose picture has not been drawn yet. See [`draw_pile_icons`].
pub const PILE_UNDRAWN: u8 = u8::MAX;

/// `DroppedItem.InitInstance`: `Icon = Random.Next() % 2`, for everything
/// lying on the ground that has not drawn one, in the order it was put down.
///
/// **Upstream draws it inside `DropAt`**, at every one of the dozen places
/// something is put on the ground. The world's first items draw it there
/// ([`stock_world_v12`]); everything later is put down as
/// [`PILE_UNDRAWN`] and drawn by this, which the game calls at the edges of
/// every action that can put something down — every event, every world
/// step, the day. So the picture is drawn from the game generator after the
/// action that dropped it and before anything else draws, in both a run and
/// the same run resumed from a save.
pub fn draw_pile_icons(ground: &mut [Dropped], rng: &mut Rng) {
    for d in ground.iter_mut().filter(|d| d.icon == PILE_UNDRAWN) {
        d.icon = rng.next_mod(2) as u8;
    }
}

/// Everything the world holds when the game begins.
pub struct Stock {
    /// What is on the floor of each room.
    pub rooms: Vec<((usize, usize), ItemBag)>,
    /// What is lying outside, in the order it was dropped.
    pub ground: Vec<Dropped>,
    /// Items that could not be placed at all, because the location had no
    /// walkable ground packed and no room with space. Zero for a complete
    /// archive; not zero is a fact worth seeing rather than a silent loss.
    pub unplaced: usize,
}

impl Stock {
    pub fn get(&self, location: usize, room: usize) -> Option<&ItemBag> {
        self.rooms.iter().find(|(k, _)| *k == (location, room)).map(|(_, b)| b)
    }

    /// Everything, wherever it ended up. Every drawn item is in exactly one
    /// of these three.
    pub fn items(&self) -> usize {
        self.in_rooms() + self.ground.len() + self.unplaced
    }

    pub fn in_rooms(&self) -> usize {
        self.rooms.iter().map(|(_, b)| b.len()).sum()
    }

    fn bag(&mut self, location: usize, room: usize, capacity: usize) -> &mut ItemBag {
        if let Some(at) = self.rooms.iter().position(|(k, _)| *k == (location, room)) {
            return &mut self.rooms[at].1;
        }
        self.rooms.push(((location, room), ItemBag::new(capacity)));
        let last = self.rooms.len() - 1;
        &mut self.rooms[last].1
    }

    /// `Location.DropItemRandom`: a walk cell picked at random and rejected
    /// until it is one you can stand on, then the item goes at its corner.
    ///
    /// The original's loop has no bound; a location with nothing walkable in
    /// it would hang. This gives up after enough tries to make that
    /// vanishingly unlikely on a real map, and says so by counting the item
    /// as unplaced.
    fn drop_random(&mut self, place: &Place, location: usize, kind: usize, rng: &mut Rng) -> bool {
        let Some(walk) = place.walk else { return false };
        let Some(at) = walkable_spot(walk, rng) else { return false };
        self.ground.push(Dropped { location, at, kind, rounds: None, icon: PILE_UNDRAWN });
        true
    }

    /// v1.2's `DropItemRandom`: ten tries at `(Next(Width), Next(Height))`
    /// on the mask, the item at the first walkable cell's corner — else at
    /// the entry point. Then `DropAt` draws the pile's picture. Nothing is
    /// ever lost. Without a packed mask nothing can be tried, and the item
    /// goes straight to the entry.
    fn drop_random_v12(&mut self, place: &Place, location: usize, kind: usize, rng: &mut Rng) {
        let mut at = place.entry;
        if let Some(walk) = place.walk {
            for _ in 0..10 {
                let x = rng.below(walk.width as u32) as usize;
                let y = rng.below(walk.height as u32) as usize;
                if walk.cell(x, y) {
                    let cell = crate::zei::MASK_CELL as i32;
                    at = crate::game::Point::new(x as i32 * cell, y as i32 * cell);
                    break;
                }
            }
        }
        let icon = rng.next_mod(2) as u8;
        self.ground.push(Dropped { location, at, kind, rounds: None, icon });
    }

    /// v1.2's `StoreItem(item, randomRoom: true)`: `rooms[Next(0,
    /// rooms.Count)]` of the rooms with space — **any of them**, where
    /// v0.9.1's `Next(0, Count - 1)` never took the last — and
    /// [`Stock::drop_random_v12`] when none has. An empty container stored
    /// at a spring comes out full, at the spring's expense.
    fn store_random_v12(
        &mut self,
        place: &Place,
        location: usize,
        kind: usize,
        types: &ItemTypes,
        spring: Option<&mut crate::game::WaterSource>,
        rng: &mut Rng,
    ) {
        let open: Vec<usize> = (0..place.room_ids.len())
            .filter(|&room| {
                let capacity = place.capacity(room);
                capacity > 0 && self.get(location, room).is_none_or(|b| b.len() < capacity)
            })
            .collect();
        if open.is_empty() {
            self.drop_random_v12(place, location, kind, rng);
            return;
        }
        let room = open[rng.range(0, open.len() as i32) as usize];
        let capacity = place.capacity(room);
        let mut kind = kind;
        if place.is_water(room) {
            if let (Some(spring), Some(full)) = (spring, types.full_of(kind)) {
                let worth = types.water_of(full);
                if worth > 0 && spring.reserve >= worth {
                    spring.reserve -= worth;
                    kind = full;
                }
            }
        }
        self.bag(location, room, capacity).push(kind);
    }

    /// `Location.StoreItem(randomRoom: true)`.
    ///
    /// Into a room that has space, chosen at random; and when no room will
    /// take it, **outside** rather than nowhere — the original falls through
    /// to `DropItemRandom` and so does this. An empty container stored at a
    /// spring comes out full, at the spring's expense.
    fn store_random(
        &mut self,
        place: &Place,
        location: usize,
        kind: usize,
        types: &ItemTypes,
        spring: Option<&mut crate::game::WaterSource>,
        rng: &mut Rng,
    ) -> bool {
        let open: Vec<usize> = (0..place.room_ids.len())
            .filter(|&room| {
                let capacity = place.capacity(room);
                capacity > 0 && self.get(location, room).is_none_or(|b| b.len() < capacity)
            })
            .collect();
        if open.is_empty() {
            return self.drop_random(place, location, kind, rng);
        }

        // `Next(0, rooms.Count - 1)` — the last room with space is never
        // chosen unless it is the only one: v0.9.1's rule, for the legacy
        // game. A v1.2 game draws over every room (`store_random_v12`,
        // `Location.cs:213`).
        let last = open.len().saturating_sub(1).max(1) as u32;
        let room = open[rng.below(last) as usize];
        let capacity = place.capacity(room);

        // Bottles left at a spring are filled from it.
        let mut kind = kind;
        if place.is_water(room) {
            if let (Some(spring), Some(full)) = (spring, types.full_of(kind)) {
                let worth = types.water_of(full);
                if worth > 0 && spring.reserve >= worth {
                    spring.reserve -= worth;
                    kind = full;
                }
            }
        }

        self.bag(location, room, capacity).push(kind)
    }
}

/// Stock the whole world, once.
///
/// `place_of` gives a location's rooms and its walkable ground; a location it
/// knows nothing about takes nothing and drops nothing. The order of players,
/// locations, rooms and draws is fixed, so the same seed lays out the same
/// world — whatever the pack happens to carry artwork for.
///
/// Springs are drawn on: a container stored at a water source is filled from
/// it, which is `StoreItem`'s doing and costs the location its reserve before
/// the player ever arrives.
pub fn stock_world<'a>(
    gam: &Gamdat,
    settings: &GameSettings,
    types: &ItemTypes,
    place_of: impl Fn(usize) -> Place<'a>,
    springs: &mut crate::game::Springs,
    counting: Counting,
    seed: u64,
) -> Stock {
    let starts: Vec<usize> = gam.players.iter().map(|p| p.location).collect();
    stock(gam, settings, types, place_of, springs, counting, seed, &starts)
}

/// v1.2's `ItemSpawner` (`initial_items = remaster_spawning`,
/// `Logic/Generation/ItemSpawner.cs`), every draw from the game's
/// generator `rng`, in `ItemSpawner`'s order:
///
/// 1. `SpawnAtPlayerLocation`: for each of the four players, where they
///    were **drawn** to start, the start items — all of them generated
///    first (`GenerateAll`: the count, then each item), then stored.
/// 2. `SpawnInAllLocations`, location by location: the ground items,
///    generated then dropped; then, **unless it is a city** (`continue`),
///    the room items, the danger items (each a coin, `Next() % 2 == 0`:
///    stored, else dropped) and the closed rooms' items.
/// 3. `SpawnRegionItems`: each `[region_item_N]`, generated, then each item
///    into `locations[Next(0, Length)]` (`StoreItemRandom`). `regions` are
///    those sections: the spawn and the locations, counted from nought.
///
/// Storing picks any room with space ([`Stock::store_random_v12`]) and
/// dropping makes ten tries and then uses the entry point
/// ([`Stock::drop_random_v12`]), so nothing is unplaced unless a location
/// was not packed at all. A dropped item draws its pile's picture.
#[allow(clippy::too_many_arguments)]
pub fn stock_world_v12<'a>(
    gam: &Gamdat,
    settings: &GameSettings,
    types: &ItemTypes,
    place_of: impl Fn(usize) -> Place<'a>,
    springs: &mut crate::game::Springs,
    rng: &mut Rng,
    starts: &[usize],
    regions: &[(Spawn, Vec<usize>)],
) -> Stock {
    let mut stock = Stock { rooms: Vec::new(), ground: Vec::new(), unplaced: 0 };
    // `GenerateAll`: the count, then every item, before any is placed.
    let generate = |spawn: &Spawn, rng: &mut Rng| -> Vec<usize> {
        let pool = Pool::of(types, spawn);
        let count = spawn.rate.count_as_upstream(rng);
        let mut drawn = Vec::new();
        for _ in 0..count {
            // `itemTypes[Next(0, Length)]`; an empty list would throw
            // upstream, and gives nothing here.
            let Some(kind) = pool.pick(rng) else { break };
            drawn.push(kind);
        }
        drawn
    };

    for &at in starts {
        if at >= gam.locations.len() {
            continue;
        }
        let place = place_of(at);
        for kind in generate(&settings.start, rng) {
            stock.store_random_v12(&place, at, kind, types, springs.at(at), rng);
        }
    }

    for (i, location) in gam.locations.iter().enumerate() {
        let place = place_of(i);
        for kind in generate(&settings.ground, rng) {
            stock.drop_random_v12(&place, i, kind, rng);
        }
        if location.is_city {
            continue;
        }
        for kind in generate(&settings.room, rng) {
            stock.store_random_v12(&place, i, kind, types, springs.at(i), rng);
        }
        if location.danger != crate::gamdat::Danger::None {
            for kind in generate(&settings.danger, rng) {
                if rng.coin() {
                    stock.store_random_v12(&place, i, kind, types, springs.at(i), rng);
                } else {
                    stock.drop_random_v12(&place, i, kind, rng);
                }
            }
        }
        for r in 0..place.room_ids.len() {
            if !place.is_rope(r) {
                continue;
            }
            let capacity = place.capacity(r);
            for kind in generate(&settings.closed_room, rng) {
                if !stock.bag(i, r, capacity).push(kind) {
                    stock.unplaced += 1;
                }
            }
        }
    }

    for (spawn, locations) in regions {
        if locations.is_empty() {
            continue;
        }
        for kind in generate(spawn, rng) {
            let at = locations[rng.range(0, locations.len() as i32) as usize];
            if at >= gam.locations.len() {
                continue;
            }
            let place = place_of(at);
            stock.store_random_v12(&place, at, kind, types, springs.at(at), rng);
        }
    }

    stock
}

/// v1.2's `PopulateInitialItems` under `initial_items = dos_gamdat`: every
/// item of `gam.dat`'s table where the table puts it — into its room, on the
/// ground at its own position, or to the character holding it. The pool and
/// the four player records are skipped (a human's own come with his start,
/// an opponent's from its template). Returns the stock and, for each
/// character record above the players, what it holds, in table order.
///
/// **The room number is used as it stands, from nought** —
/// `location.Rooms[info.RoomId]`, skipped when it is not below the room
/// count — although `gam.dat` numbers rooms from one (no item names room
/// 0). So each room's items land in the next door along, the last door's
/// are lost, and so is anything landing on a door that holds nothing (a
/// surgery). Classic's file loses 49 of its 157 this way, in v1.2 as here;
/// they are counted as unplaced.
pub fn stock_from_gamdat<'a>(
    gam: &Gamdat,
    types: &ItemTypes,
    place_of: impl Fn(usize) -> Place<'a>,
) -> (Stock, Vec<(usize, usize)>) {
    use crate::gamdat::ItemOwner;
    let mut stock = Stock { rooms: Vec::new(), ground: Vec::new(), unplaced: 0 };
    let mut held = Vec::new();
    for info in &gam.items {
        let owner = info.owner();
        if matches!(owner, ItemOwner::Pool | ItemOwner::Unknown) {
            continue;
        }
        let Some(kind) = crate::v12_economy::kind_of_sprite(types, info.sprite_id) else {
            stock.unplaced += 1;
            continue;
        };
        match owner {
            ItemOwner::Character(r) if r < 4 => {}
            ItemOwner::Character(r) => {
                if r < gam.characters.len() {
                    held.push((r, kind));
                }
            }
            ItemOwner::Room { location, room } => {
                let place = place_of(location);
                let capacity = place.capacity(room);
                if location >= gam.locations.len() || room >= place.room_ids.len() || !stock.bag(location, room, capacity).push(kind) {
                    stock.unplaced += 1;
                }
            }
            ItemOwner::Dropped { location, x, y } => {
                if location < gam.locations.len() {
                    stock.ground.push(Dropped {
                        location,
                        at: crate::game::Point::new(x as i32, y as i32),
                        kind,
                        rounds: None,
                        icon: PILE_UNDRAWN,
                    });
                } else {
                    stock.unplaced += 1;
                }
            }
            ItemOwner::Pool | ItemOwner::Unknown => {}
        }
    }
    (stock, held)
}

#[allow(clippy::too_many_arguments)]
fn stock<'a>(
    gam: &Gamdat,
    settings: &GameSettings,
    types: &ItemTypes,
    place_of: impl Fn(usize) -> Place<'a>,
    springs: &mut crate::game::Springs,
    counting: Counting,
    seed: u64,
    starts: &[usize],
) -> Stock {
    let mut rng = Rng::seeded(seed);
    let mut stock = Stock { rooms: Vec::new(), ground: Vec::new(), unplaced: 0 };

    let start = Pool::of(types, &settings.start);
    let ground = Pool::of(types, &settings.ground);
    let room = Pool::of(types, &settings.room);
    let danger = Pool::of(types, &settings.danger);
    let closed = Pool::of(types, &settings.closed_room);

    // 1. Every player's own location, all four of them.
    for &at in starts {
        if at >= gam.locations.len() {
            continue;
        }
        let count = how_many(&settings.start, counting, &mut rng);
        for _ in 0..count {
            let Some(kind) = start.pick(&mut rng) else { break };
            let place = place_of(at);
            if !stock.store_random(
                &place,
                at,
                kind,
                types,
                springs.at(at),
                &mut rng,
            ) {
                stock.unplaced += 1;
            }
        }
    }

    for (i, location) in gam.locations.iter().enumerate() {
        let place = place_of(i);

        // 2. On the ground, in every location — cities included.
        let count = how_many(&settings.ground, counting, &mut rng);
        for _ in 0..count {
            let Some(kind) = ground.pick(&mut rng) else { break };
            if !stock.drop_random(&place, i, kind, &mut rng) {
                stock.unplaced += 1;
            }
        }

        // 3. In the rooms, but not a city's.
        if !location.is_city {
            let count = how_many(&settings.room, counting, &mut rng);
            for _ in 0..count {
                let Some(kind) = room.pick(&mut rng) else { break };
                if !stock.store_random(&place, i, kind, types, springs.at(i), &mut rng) {
                    stock.unplaced += 1;
                }
            }
        }

        // 4. Somewhere dangerous keeps some of what it kills you for —
        // **whether or not it is a city**, which is where `main` differs.
        if location.danger != crate::gamdat::Danger::None {
            let count = how_many(&settings.danger, counting, &mut rng);
            for _ in 0..count {
                let Some(kind) = danger.pick(&mut rng) else { break };
                let placed = if rng.coin() {
                    stock.store_random(&place, i, kind, types, springs.at(i), &mut rng)
                } else {
                    stock.drop_random(&place, i, kind, &mut rng)
                };
                if !placed {
                    stock.unplaced += 1;
                }
            }
        }

        // 5. A room you need a rope to reach is worth the rope. Not in a
        // city either, and straight into that room rather than a random one.
        if !location.is_city {
            for r in 0..place.room_ids.len() {
                if !place.is_rope(r) {
                    continue;
                }
                let capacity = place.capacity(r);
                let count = how_many(&settings.closed_room, counting, &mut rng);
                for _ in 0..count {
                    let Some(kind) = closed.pick(&mut rng) else { break };
                    if !stock.bag(i, r, capacity).push(kind) {
                        stock.unplaced += 1;
                    }
                }
            }
        }
    }

    stock
}

/// A cell you can stand on, picked at random, as `DropItemRandom` picks one.
///
/// The original's rejection loop is unbounded and would spin for ever on a
/// location with nothing walkable in it; this gives up instead, which is a
/// departure and the reason it can return `None`.
pub fn walkable_spot(walk: &crate::zei::WalkGrid, rng: &mut Rng) -> Option<crate::game::Point> {
    if walk.width == 0 || walk.height == 0 {
        return None;
    }
    for _ in 0..10_000 {
        let x = rng.below(walk.width as u32) as usize;
        let y = rng.below(walk.height as u32) as usize;
        if walk.cell(x, y) {
            let cell = crate::zei::MASK_CELL as i32;
            return Some(crate::game::Point::new(x as i32 * cell, y as i32 * cell));
        }
    }
    None
}

/// Which rules the layout was produced by, for the record.
///
/// A fingerprint means nothing without it: the same seed through different
/// rules is a different world, and "the same number on both machines" is only
/// evidence if both were running this.
pub const RULES: &str = "v0.9.1 LoadItems, xorshift64*";
/// [`RULES`] for [`stock_world_v12`].
pub const RULES_V12: &str = "v1.2 ItemSpawner, System.Random";
/// [`RULES`] for [`stock_from_gamdat`], which draws nothing.
pub const RULES_GAMDAT: &str = "v1.2 gam.dat items";

/// A canonical listing of everything the world holds, for comparing one
/// machine against another.
///
/// Sorted, so the order the generator happened to visit things in does not
/// enter into it, and hashed with FNV-1a — no dependency, and the same number
/// on every target this builds for. `83 items in 58 rooms` is a summary and
/// two different worlds can share it; this cannot be shared by accident.
pub fn fingerprint(stock: &Stock) -> u64 {
    hash_of(stock)
}

/// FNV-1a over any bytes, for the pack's own fingerprint. Same function, so
/// one line can carry both numbers and they mean the same kind of thing.
pub fn digest(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn hash_of(stock: &Stock) -> u64 {
    let mut rows: Vec<(usize, usize, usize, usize, usize)> = Vec::new();
    for ((location, room), bag) in &stock.rooms {
        for (slot, kind) in bag.iter().enumerate() {
            // 0 marks a room, 1 marks the ground.
            rows.push((*location, 0, *room, slot, kind));
        }
    }
    for item in &stock.ground {
        rows.push((item.location, 1, item.at.x as usize, item.at.y as usize, item.kind));
    }
    rows.sort_unstable();

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u64| {
        for byte in v.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    };
    for (a, b, c, d, e) in rows {
        eat(a as u64);
        eat(b as u64);
        eat(c as u64);
        eat(d as u64);
        eat(e as u64);
    }
    eat(stock.unplaced as u64);
    hash
}

/// v1.2's `Location.DropItemRandom` for one item outside the world's laying
/// out (v1.3's `LocationPopulationCreator`): ten tries at
/// `(Next(Width), Next(Height))` on the mask, the first walkable cell's
/// corner, else the entry; then the pile's picture (`Next(2)`).
pub fn drop_item_random(walk: Option<&crate::zei::WalkGrid>, entry: crate::game::Point, location: usize, kind: usize,
                        rng: &mut Rng) -> Dropped {
    let mut at = entry;
    if let Some(walk) = walk {
        for _ in 0..10 {
            let x = rng.below(walk.width as u32) as usize;
            let y = rng.below(walk.height as u32) as usize;
            if walk.cell(x, y) {
                let cell = crate::zei::MASK_CELL as i32;
                at = crate::game::Point::new(x as i32 * cell, y as i32 * cell);
                break;
            }
        }
    }
    let icon = rng.next_mod(2) as u8;
    Dropped { location, at, kind, rounds: None, icon }
}
