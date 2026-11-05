//! v1.2's `ItemSpawner` (`initial_items = remaster_spawning`): where a
//! stored and a dropped item end up. `Logic/Location.cs:169-221`.
use std::path::PathBuf;

use burngfx::gamdat::{self, Danger, Gamdat};
use burngfx::game::Springs;
use burngfx::items::ItemTypes;
use burngfx::settings::{Filter, GameSettings, Rate, Spawn};
use burngfx::spawn::{stock_world_v12, Place, Stock};
use burngfx::zei::{WalkGrid, MASK_CELL};

fn gam() -> Option<Gamdat> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/burntime.pak");
    let bytes = Box::leak(std::fs::read(&path).ok()?.into_boxed_slice());
    let pak = burngfx::pak::ClassicPak::open(bytes).ok()?;
    gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()
}

fn types() -> ItemTypes {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/items_original.txt");
    ItemTypes::parse(&std::fs::read_to_string(path).unwrap())
}

/// One snake, and nothing else, of the given spawn.
fn one(rate: u32) -> Spawn {
    Spawn { filter: Filter::parse("item_snake"), rate: Rate { min: rate, max: rate } }
}

/// A place that is neither a city nor dangerous, so only the passes asked
/// for reach it.
fn quiet(g: &Gamdat) -> usize {
    (0..g.locations.len()).find(|&l| !g.locations[l].is_city && g.locations[l].danger == Danger::None).unwrap()
}

/// One world, from the game generator as it stands: a run of worlds from
/// one generator, rather than one world each from neighbouring seeds —
/// .NET's first draws from neighbouring seeds are nearly in step.
fn lay(g: &Gamdat, cfg: &GameSettings, place: &dyn Fn(usize) -> Place<'static>, rng: &mut burngfx::spawn::Rng) -> Stock {
    let mut springs = Springs::from_gamdat(g);
    stock_world_v12(g, cfg, &types(), place, &mut springs, rng, &[], &[])
}

/// `StoreItem(item, randomRoom: true)`: `rooms[Next(0, rooms.Count)]` of
/// the rooms with space (`Location.cs:213`) — **any** of them. v0.9.1's
/// `Next(0, Count - 1)` never chose the last; v1.2 does. Three open rooms,
/// one item stored into one of them, over 3,000 worlds: each about a third.
#[test]
fn store_at_random_chooses_any_open_room() {
    let Some(g) = gam() else { return };
    let here = quiet(&g);
    let rooms: &'static [u8] = &[0, 0, 0];
    let entry = g.locations[here].entry_point;
    let place = move |l: usize| if l == here { Place { room_ids: rooms, walk: None, entry } } else { Place::default() };
    let cfg = GameSettings { room: one(1), ..Default::default() };
    let mut chosen = [0u32; 3];
    let mut rng = burngfx::spawn::Rng::game(1993);
    for _ in 0..3000 {
        let stock = lay(&g, &cfg, &place, &mut rng);
        for r in 0..3 {
            if stock.get(here, r).is_some_and(|b| b.len() == 1) {
                chosen[r] += 1;
            }
        }
    }
    println!("rooms chosen {chosen:?}");
    assert_eq!(chosen.iter().sum::<u32>(), 3000);
    for c in chosen {
        assert!((850..1150).contains(&c), "{chosen:?}");
    }
}

/// `DropItemRandom`: ten tries at a random mask cell, then the location's
/// entry point (`Location.cs:169-184`). On a mask with one cell in twenty
/// walkable, a drop finds none in ten tries 0.95^10 = 59.9 % of the time and
/// lies at the entry — never lost, never placed by a longer search.
#[test]
fn a_drop_that_finds_no_ground_in_ten_tries_lies_at_the_entry() {
    let Some(g) = gam() else { return };
    let here = quiet(&g);
    let entry = g.locations[here].entry_point;
    // 5 x 4 tiles, one of them walkable: 16 of 320 cells.
    let mut grid = WalkGrid::new(5, 4);
    grid.fill_tile(2, 1, true);
    let walk: &'static WalkGrid = Box::leak(Box::new(grid));
    assert!(!walk.cell(entry.x as usize / MASK_CELL, entry.y as usize / MASK_CELL) || entry.x as usize % MASK_CELL != 0);
    let place = move |l: usize| if l == here { Place { room_ids: &[], walk: Some(walk), entry } } else { Place::default() };
    let cfg = GameSettings { ground: one(1), ..Default::default() };
    let (mut at_entry, mut on_ground) = (0u32, 0u32);
    let mut rng = burngfx::spawn::Rng::game(7);
    for _ in 0..4000 {
        let stock = lay(&g, &cfg, &place, &mut rng);
        for d in stock.ground.iter().filter(|d| d.location == here) {
            if d.at == entry {
                at_entry += 1;
            } else {
                assert!(walk.cell(d.at.x as usize / MASK_CELL, d.at.y as usize / MASK_CELL));
                on_ground += 1;
            }
        }
        // Never lost: the ground pass puts every item somewhere.
        assert_eq!(stock.unplaced, 0);
    }
    let share = at_entry as f64 / (at_entry + on_ground) as f64;
    println!("at the entry {at_entry}, on walkable ground {on_ground}: {share:.3}");
    assert_eq!(at_entry + on_ground, 4000);
    // 4,000 drops: a standard error of 0.008 around 0.599.
    assert!((share - 0.5987).abs() < 0.04, "{share}");
}
