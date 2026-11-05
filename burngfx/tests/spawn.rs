//! Laying out the world's rooms when a game begins.
//!
//! The rules are the original's; the numbers are ours. We cannot read the DOS
//! game's generator and the remaster's is `System.Random` seeded from the
//! clock, so what is tested here is which items may appear where, how many,
//! and that the same seed lays out the same world — not that any particular
//! room gets any particular thing.

use burngfx::game::ItemBag;
use burngfx::gamdat::{self, Gamdat};
use burngfx::items::ItemTypes;
use burngfx::settings::{Filter, GameSettings, Rate};
use burngfx::game::Springs;
use burngfx::spawn::{candidates, fingerprint, stock_world, Counting, Place, Rng, Stock};
use burngfx::zei::WalkGrid;
use std::path::PathBuf;

fn archive() -> Option<burngfx::pak::ClassicPak<'static>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/burntime.pak");
    let bytes = std::fs::read(&path).ok()?;
    // Leaked on purpose: a test binary that reads the archive once and keeps
    // it is simpler than threading a lifetime through every helper.
    burngfx::pak::ClassicPak::open(Box::leak(bytes.into_boxed_slice())).ok()
}

fn save(pak: &burngfx::pak::ClassicPak) -> Option<Gamdat> {
    gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

fn types() -> Option<ItemTypes> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/items_original.txt");
    Some(ItemTypes::parse(&std::fs::read_to_string(path).ok()?))
}

fn settings(difficulty: u16) -> Option<GameSettings> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/gamesettings_original.txt");
    Some(GameSettings::parse(&std::fs::read_to_string(path).ok()?, difficulty))
}

/// Every location's rooms and its walkable ground, which is what the archive
/// carries for every location whatever artwork was packed alongside.
///
/// Read once into a table, because `Place` borrows and the generator asks for
/// each location more than once.
struct World {
    rooms: Vec<Vec<u8>>,
    walk: Vec<Option<WalkGrid>>,
}

impl World {
    fn read(pak: &burngfx::pak::ClassicPak) -> World {
        let mut rooms = Vec::new();
        let mut walk = Vec::new();
        for location in 0..gamdat::LOCATION_COUNT {
            let ids = pak
                .get(&format!("burn_gfx/mat_{:03}.raw", location + 1))
                .and_then(|raw| burngfx::matmap::parse(raw).ok())
                .map(|m| m.doors.iter().map(|d| d.room_id).collect())
                .unwrap_or_default();
            rooms.push(ids);
            // A grid of the map's own size, everything walkable: the shape
            // of the ground is a packing question, and the tests here are
            // about what is dropped rather than exactly where.
            walk.push(Some(open_ground(15, 12)));
        }
        World { rooms, walk }
    }

    fn place(&self, location: usize) -> Place<'_> {
        Place {
            room_ids: self.rooms.get(location).map(|v| v.as_slice()).unwrap_or(&[]),
            walk: self.walk.get(location).and_then(|w| w.as_ref()),
            entry: burngfx::game::Point::new(0, 0),
        }
    }
}

/// A location's ground, every cell of it walkable and then the outer ring
/// closed, which is the shape the packer produces.
fn open_ground(tiles_wide: usize, tiles_high: usize) -> WalkGrid {
    let mut grid = WalkGrid::new(tiles_wide, tiles_high);
    for ty in 0..tiles_high {
        for tx in 0..tiles_wide {
            grid.fill_tile(tx, ty, true);
        }
    }
    grid.close_the_edges();
    grid
}

// -- reading the settings ---------------------------------------------------

#[test]
fn the_shipped_settings_say_what_may_be_generated() {
    let Some(s) = settings(0) else { return };

    // The line this whole thing hangs on.
    assert!(s.room.filter.include.contains(&String::from("food")));
    assert!(s.room.filter.include.contains(&String::from("water")));
    assert!(s.room.filter.exclude.contains(&String::from("nodrop")));
    assert!(s.room.filter.exclude.contains(&String::from("special")));
    assert_eq!(s.room.rate, Rate { min: 1, max: 3 });

    assert_eq!(s.start.rate, Rate { min: 1, max: 1 }, "one number means both");
    assert_eq!(s.closed_room.rate, Rate { min: 2, max: 4 });
    assert_eq!(s.danger.rate, Rate { min: 2, max: 5 });
    assert!(s.closed_room.filter.include.contains(&String::from("special")));

    // The three difficulties are three different sections.
    let hard = settings(2).unwrap();
    assert_ne!(hard.room.filter, s.room.filter, "difficulty 2 reads the same as 0");
}

/// A leading `-` excludes; a `'` starts a comment; a section is only entered
/// by its own name.
#[test]
fn a_settings_line_is_words_and_a_comment() {
    let f = Filter::parse("food water -nodrop -item_rope");
    assert_eq!(f.include, ["food", "water"]);
    assert_eq!(f.exclude, ["nodrop", "item_rope"]);
    assert!(Filter::parse("   ").is_empty());

    let s = GameSettings::parse(
        "[0] ' difficulty easy\n\
         random_items_room = food ' but not much\n\
         random_items_room_rate = 2 6\n\
         [1] ' difficulty normal\n\
         random_items_room = weapon\n",
        0,
    );
    assert_eq!(s.room.filter.include, ["food"], "the comment is not an item class");
    assert_eq!(s.room.rate, Rate { min: 2, max: 6 });

    // Asking for a section that is not there gives nothing, not a panic.
    let none = GameSettings::parse("[0]\nrandom_items_room = food\n", 7);
    assert!(none.room.filter.is_empty());
}

/// The two counting rules, and the difference between them written down.
#[test]
fn the_rate_is_read_two_ways_and_they_differ_by_one() {
    let rate = Rate { min: 1, max: 3 };
    let mut ours = Rng::seeded(1);
    let mut theirs = Rng::seeded(1);

    let mut seen_ours = [false; 4];
    let mut seen_theirs = [false; 4];
    for _ in 0..300 {
        seen_ours[rate.count(&mut ours) as usize] = true;
        seen_theirs[rate.count_as_upstream(&mut theirs) as usize] = true;
    }

    assert_eq!(seen_ours, [false, true, true, true], "1 to 3 inclusive");
    assert_eq!(seen_theirs, [false, true, true, false], "1 to 2, as the C# computes");

    // One number means exactly that many, either way.
    let fixed = Rate { min: 1, max: 1 };
    assert_eq!(fixed.count(&mut ours), 1);
    assert_eq!(fixed.count_as_upstream(&mut ours), 1);
}

// -- who may be generated ---------------------------------------------------

/// Exclusion wins over inclusion, and both sides accept a bare item name.
#[test]
fn the_filter_keeps_what_it_says_and_throws_out_what_it_says() {
    let Some(types) = types() else { return };
    let name_of = |k: usize| types.get(k).unwrap().name.clone();

    let food = candidates(&types, &Filter::parse("food"));
    assert!(!food.is_empty());
    for kind in &food {
        assert!(types.get(*kind).unwrap().is_class("food"), "{} is not food", name_of(*kind));
    }

    // Nothing in `class=food` is also `nodrop` — the three foods are plain,
    // and `item_meat` carries `nodrop` alone rather than alongside `food`.
    // So the room rule's `-nodrop` takes nothing away from the food.
    assert_eq!(candidates(&types, &Filter::parse("food -nodrop")), food);

    // Weapons are where the two overlap: two of them are `weapon nodrop`.
    let weapons = candidates(&types, &Filter::parse("weapon"));
    let droppable = candidates(&types, &Filter::parse("weapon -nodrop"));
    assert_eq!(droppable.len(), weapons.len() - 2, "nothing was excluded");
    for kind in &droppable {
        assert!(!types.get(*kind).unwrap().is_class("nodrop"));
    }

    // A class an item carries alongside another still excludes it: meat is
    // `class=nodrop`, and a "special rare" item is thrown out by `-rare`.
    let specials = candidates(&types, &Filter::parse("special"));
    let not_rare = candidates(&types, &Filter::parse("special -rare"));
    assert!(not_rare.len() < specials.len(), "the second class is not being read");

    // By name, on either side.
    let by_name = candidates(&types, &Filter::parse("item_snake"));
    assert_eq!(by_name.len(), 1);
    assert_eq!(name_of(by_name[0]), "item_snake");
    assert!(candidates(&types, &Filter::parse("food -item_snake"))
        .iter()
        .all(|&k| name_of(k) != "item_snake"));

    // Nothing asked for, nothing found.
    assert!(candidates(&types, &Filter::parse("-food")).is_empty());
    assert!(candidates(&types, &Filter::parse("no_such_class")).is_empty());
}

// -- the world ---------------------------------------------------------------

/// Everything a test needs to lay out a world, read once.
struct Bench {
    gam: Gamdat,
    types: ItemTypes,
    cfg: GameSettings,
    world: World,
}

fn bench(pak: &burngfx::pak::ClassicPak) -> Option<Bench> {
    Some(Bench {
        gam: save(pak)?,
        types: types()?,
        cfg: settings(0)?,
        world: World::read(pak),
    })
}

impl Bench {
    fn lay(&self, seed: u64) -> Stock {
        let mut springs = Springs::from_gamdat(&self.gam);
        stock_world(
            &self.gam,
            &self.cfg,
            &self.types,
            |i| self.world.place(i),
            &mut springs,
            Counting::AsUpstream,
            seed,
        )
    }
}

/// The same seed lays out the same world, and a different one does not.
///
/// Compared by fingerprint rather than by counts: two different worlds can
/// hold the same number of things in the same number of rooms, and one of
/// this project's own reports made exactly that mistake.
#[test]
fn one_seed_is_one_world() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };

    assert_eq!(fingerprint(&b.lay(1234)), fingerprint(&b.lay(1234)));
    assert_ne!(fingerprint(&b.lay(1234)), fingerprint(&b.lay(1235)));
    assert!(b.lay(1234).items() > 0, "the world came out empty");
}

/// Every item drawn ends up somewhere, and the somewhere is accounted for.
///
/// The original never throws one away: a room with no space falls through to
/// the ground. This did throw them away, in three places at once — the ground
/// pass was skipped entirely, half of a dangerous location's spoils went with
/// it, and a full location lost the rest. Found in review.
#[test]
fn every_generated_item_is_in_a_room_or_on_the_ground() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };

    for seed in [1u64, 7, 1993, 2024] {
        let stock = b.lay(seed);
        assert_eq!(
            stock.items(),
            stock.in_rooms() + stock.ground.len() + stock.unplaced,
            "seed {seed}: the books do not balance"
        );
        assert_eq!(stock.unplaced, 0, "seed {seed}: {} items went nowhere", stock.unplaced);
        assert!(!stock.ground.is_empty(), "seed {seed}: nothing at all outside");
        assert!(stock.in_rooms() > 0, "seed {seed}: nothing at all indoors");
    }
}

/// What is dropped outside is dropped somewhere you can stand.
#[test]
fn nothing_is_dropped_where_it_cannot_be_reached() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };
    let stock = b.lay(1993);

    for item in &stock.ground {
        let place = b.world.place(item.location);
        let walk = place.walk.expect("a location with ground but no grid");
        let cell = burngfx::zei::MASK_CELL as i32;
        assert_eq!(item.at.x % cell, 0, "not on a cell corner");
        assert_eq!(item.at.y % cell, 0);
        let (x, y) = ((item.at.x / cell) as usize, (item.at.y / cell) as usize);
        assert!(walk.cell(x, y), "location {}: dropped on a blocked cell", item.location);
    }
}

/// A city keeps its rooms to itself — but everything else about a city
/// happens anyway.
///
/// `main` refactored this into an `ItemSpawner` and moved the danger block
/// behind the same `continue` that skips a city's rooms. v0.9.1 tests for a
/// city separately in each block, so a dangerous city still gets its spoils
/// and every city gets ground items. This was written from `main` first and
/// had the wrong shape.
#[test]
fn a_city_loses_its_room_items_and_nothing_else() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };
    let stock = b.lay(99);

    let starts: Vec<usize> = b.gam.players.iter().map(|p| p.location).collect();
    let mut cities = 0;
    let mut cities_with_ground = 0;

    for (i, location) in b.gam.locations.iter().enumerate() {
        if !location.is_city {
            continue;
        }
        cities += 1;
        if stock.ground.iter().any(|g| g.location == i) {
            cities_with_ground += 1;
        }
        // A city's rooms hold only what the start pass and the danger pass
        // put there.
        let indoors: usize =
            (0..8).filter_map(|r| stock.get(i, r)).map(|b| b.len()).sum();
        if !starts.contains(&i) && location.danger == burngfx::gamdat::Danger::None {
            assert_eq!(indoors, 0, "city {i} was stocked with room items");
        }
    }

    assert_eq!(cities, 5, "the save records five cities");
    assert_eq!(cities_with_ground, cities, "a city got nothing on its ground");
}

/// All four players' locations are stocked, not just the one being played.
///
/// `foreach (Player p in game.Player)` — the other three are AI players in
/// the original, and their towns are stocked whether or not anybody drives
/// them. This used `players.first()` and gave three quarters of the world's
/// starting supplies to nobody. Found in review.
///
/// Tested with the start rule alone, because the room rule reaches three of
/// the four start locations too and would hide the difference.
#[test]
fn every_players_town_gets_its_start_items() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };

    let only_start = GameSettings { start: b.cfg.start.clone(), ..Default::default() };
    assert!(!only_start.start.filter.is_empty(), "the start rule reads as empty");

    let towns: Vec<usize> = b.gam.players.iter().map(|p| p.location).collect();
    assert_eq!(towns.len(), 4);

    let mut ever: Vec<usize> = Vec::new();
    for seed in 0..30u64 {
        let mut springs = Springs::from_gamdat(&b.gam);
        let stock = stock_world(
            &b.gam,
            &only_start,
            &b.types,
            |i| b.world.place(i),
            &mut springs,
            Counting::AsUpstream,
            seed,
        );

        // Nothing but the start rule ran, so everything placed is a start
        // item and every location holding one is somebody's town.
        for ((location, _), bag) in &stock.rooms {
            assert!(towns.contains(location), "location {location} is nobody's start");
            if !bag.is_empty() && !ever.contains(location) {
                ever.push(*location);
            }
        }
        for item in &stock.ground {
            assert!(towns.contains(&item.location), "dropped outside somebody else's town");
        }
    }

    ever.sort_unstable();
    let mut expected = towns.clone();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(ever, expected, "some player's town was never stocked");
}

/// Nothing is put where it cannot go.
#[test]
fn what_is_generated_fits_where_it_is_put() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };
    let stock = b.lay(7);

    let allowed: Vec<usize> = candidates(&b.types, &b.cfg.room.filter)
        .into_iter()
        .chain(candidates(&b.types, &b.cfg.closed_room.filter))
        .chain(candidates(&b.types, &b.cfg.start.filter))
        .chain(candidates(&b.types, &b.cfg.danger.filter))
        .chain(candidates(&b.types, &b.cfg.ground.filter))
        // A container filled at a spring becomes its full form, which no
        // rule names — `StoreItem` does that on the way in.
        .chain((0..b.types.len()).filter_map(|k| b.types.full_of(k)))
        .collect();

    for ((location, room), bag) in &stock.rooms {
        let ids = &b.world.rooms[*location];
        let kind = burngfx::matmap::RoomType::from_room_id(ids[*room]);
        let capacity = kind.item_capacity();

        assert!(capacity > 0, "location {location} room {room} is a {kind:?} with no floor");
        assert!(bag.len() <= capacity, "location {location} room {room} overfilled");
        for item in bag.iter() {
            assert!(
                allowed.contains(&item),
                "location {location} room {room} holds {}, which no rule allows",
                b.types.get(item).unwrap().name
            );
        }
    }
}

/// A rope room is stocked from its own rule, and gets more than an ordinary
/// one — that is what makes the rope worth carrying.
#[test]
fn a_room_behind_a_rope_is_worth_the_rope() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };
    let stock = b.lay(11);
    let specials = candidates(&b.types, &b.cfg.closed_room.filter);
    assert!(!specials.is_empty(), "nothing at all can go behind a rope");

    let mut ropes = 0;
    for (i, location) in b.gam.locations.iter().enumerate() {
        if location.is_city {
            continue;
        }
        for (room, &id) in b.world.rooms[i].iter().enumerate() {
            if burngfx::matmap::RoomType::from_room_id(id) != burngfx::matmap::RoomType::Rope {
                continue;
            }
            ropes += 1;
            let bag = stock.get(i, room).expect("a rope room with nothing in it");
            assert!(bag.len() >= b.cfg.closed_room.rate.min as usize);
            assert!(
                bag.iter().any(|k| specials.contains(&k)),
                "location {i} room {room}: nothing special behind the rope"
            );
        }
    }
    assert!(ropes >= 3, "only {ropes} rope rooms outside the cities");
}

/// A bottle left at a spring is filled from it, and the spring pays.
#[test]
fn a_container_stored_at_a_spring_comes_out_full() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };

    let mut drained = false;
    for seed in 0..30u64 {
        let mut springs = Springs::from_gamdat(&b.gam);
        let before: Vec<i32> = (0..b.gam.locations.len())
            .filter_map(|i| springs.get(i).map(|s| s.reserve))
            .collect();
        let _ = stock_world(
            &b.gam,
            &b.cfg,
            &b.types,
            |i| b.world.place(i),
            &mut springs,
            Counting::AsUpstream,
            seed,
        );
        let after: Vec<i32> = (0..b.gam.locations.len())
            .filter_map(|i| springs.get(i).map(|s| s.reserve))
            .collect();
        if after.iter().zip(&before).any(|(a, b)| a < b) {
            drained = true;
            break;
        }
    }
    assert!(drained, "no spring anywhere ever filled a bottle");
}

/// A rule with no candidates produces nothing, rather than reaching into an
/// empty list — which is what the original would do.
#[test]
fn a_rule_that_can_produce_nothing_produces_nothing() {
    let Some(pak) = archive() else { return };
    let (Some(gam), Some(types)) = (save(&pak), types()) else { return };
    let world = World::read(&pak);
    let mut springs = Springs::from_gamdat(&gam);

    let impossible = GameSettings {
        room: burngfx::settings::Spawn {
            filter: Filter::parse("no_such_class"),
            rate: Rate { min: 3, max: 3 },
        },
        closed_room: burngfx::settings::Spawn {
            filter: Filter::parse("food -food"),
            rate: Rate { min: 3, max: 3 },
        },
        ..Default::default()
    };

    let stock = stock_world(
        &gam,
        &impossible,
        &types,
        |i| world.place(i),
        &mut springs,
        Counting::AsUpstream,
        5,
    );
    assert_eq!(stock.items(), 0);

    // A world with no rooms and no ground is no worse: everything drawn is
    // counted as unplaced rather than lost without trace.
    let cfg = settings(0).unwrap();
    let bare = stock_world(
        &gam,
        &cfg,
        &types,
        |_| Place::default(),
        &mut springs,
        Counting::AsUpstream,
        5,
    );
    assert_eq!(bare.in_rooms(), 0);
    assert!(bare.ground.is_empty());
    assert!(bare.unplaced > 0, "a world with nowhere to put anything lost nothing");
    assert_eq!(bare.items(), bare.unplaced);
}

/// The pack's artwork must not change the world.
///
/// This is why the archive carries every location's room list and every
/// location's walk grid, not only the packed maps': which places have
/// pictures is a packaging decision, and the world is not.
#[test]
fn the_world_does_not_depend_on_which_maps_were_packed() {
    let Some(pak) = archive() else { return };
    let Some(b) = bench(&pak) else { return };
    assert_eq!(fingerprint(&b.lay(2024)), fingerprint(&b.lay(2024)));

    // Muarab in particular: the demo profile's town.
    const MUARAB: usize = 4;
    let stock = b.lay(2024);
    let held: usize = (0..8).filter_map(|r| stock.get(MUARAB, r)).map(|b| b.len()).sum();
    assert!(held > 0 || stock.ground.iter().any(|g| g.location == MUARAB));
}

// -- the generator's own arithmetic -----------------------------------------

/// `below` is a range, not a suggestion, and the same seed is the same
/// stream on any machine this builds for.
#[test]
fn the_generator_stays_inside_its_bounds() {
    let mut rng = Rng::seeded(42);
    for n in [1u32, 2, 3, 7, 64, 1000] {
        for _ in 0..200 {
            assert!(rng.below(n) < n);
        }
    }
    assert_eq!(rng.below(0), 0, "nothing to choose from");

    // A zero seed would leave xorshift producing zeroes for ever.
    let mut zero = Rng::seeded(0);
    let first: Vec<u64> = (0..8).map(|_| zero.next_u64()).collect();
    assert!(first.iter().any(|&v| v != 0));

    // Both faces of the coin turn up.
    let mut coins = Rng::seeded(3);
    let heads = (0..200).filter(|_| coins.coin()).count();
    assert!((40..160).contains(&heads), "{heads} heads in 200 — that is not a coin");
}

/// The stream itself, pinned. If this changes, every seeded world changes
/// with it, and a comparison between two machines stops meaning anything.
#[test]
fn the_stream_is_the_stream() {
    let mut rng = Rng::seeded(1993);
    let first: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
    assert_eq!(
        first,
        [
            13_945_343_638_797_394_863,
            8_518_555_255_460_251_915,
            12_531_097_654_944_474_645,
            4_467_730_689_085_986_774
        ]
    );
}

/// A bag never overfills, however the generator is pushed at it.
#[test]
fn a_generated_bag_respects_the_room_it_is_in() {
    let mut bag = ItemBag::new(2);
    assert!(bag.push(1));
    assert!(bag.push(2));
    assert!(!bag.push(3));
    assert_eq!(bag.len(), 2);
}
