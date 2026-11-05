//! `gam.dat` — the original game's starting state.
//!
//! The synthetic cases build records byte by byte. The rest open the real file
//! out of `burntime.pak` when it is there, and skip otherwise.

use burngfx::game::Point;
use burngfx::gamdat::{self, Danger, GamdatError, LOCATION_COUNT};
use burngfx::pak::ClassicPak;
use burngfx::text::{MapSettings, TextDb, TextRef};
use std::path::PathBuf;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

/// A file with `LOCATION_COUNT` records, the first of which is `first`.
fn build(first: &[u8; 50]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"BURN");
    out.extend_from_slice(&[0u8; 18]);
    out.extend_from_slice(first);
    out.extend(std::iter::repeat_n(0u8, (LOCATION_COUNT - 1) * 50));
    out
}

fn record(fields: &[(usize, &[u8])]) -> [u8; 50] {
    let mut r = [0u8; 50];
    for (at, bytes) in fields {
        r[*at..*at + bytes.len()].copy_from_slice(bytes);
    }
    r
}

#[test]
fn reads_a_record_at_the_offsets_savegame_uses() {
    let r = record(&[
        (0, &7u16.to_le_bytes()),    // title id
        (12, &3u16.to_le_bytes()),   // water reserve
        (14, &8u16.to_le_bytes()),   // water capacity
        (16, &5u16.to_le_bytes()),   // water refresh
        (18, &352u16.to_le_bytes()), // entry x
        (20, &224u16.to_le_bytes()), // entry y
        (22, &1u16.to_le_bytes()),   // city flag
        (45, &[3]),                  // danger: radiation
        (46, &45u16.to_le_bytes()),  // danger amount
        (48, &25u16.to_le_bytes()),  // life reduction
    ]);

    let gam = gamdat::parse(&build(&r)).unwrap();
    assert_eq!(gam.locations.len(), LOCATION_COUNT);

    let l = &gam.locations[0];
    assert_eq!(l.title_id, 7);
    assert_eq!(l.entry_point, Point::new(352, 224));
    assert!(l.is_city);
    assert_eq!((l.water_reserve, l.water_capacity, l.water_refresh), (3, 8, 5));
    assert_eq!(l.danger, Danger::Radiation);
    assert_eq!((l.danger_amount, l.life_reduction), (45, 25));
    assert!(l.has_water());
}

/// The file counts identifiers from one and writes zero for "none", so the
/// graph has to come out as indices and `Option`, not as sentinel numbers.
#[test]
fn zeroes_in_the_road_graph_become_absence() {
    let r = record(&[
        (32, &[2, 5, 0, 0]), // neighbours: 1 and 4, then nothing
        (36, &[2, 3, 9, 9]), // travel days
        (40, &[15, 0, 7, 7]), // ways: the second road has no drawn path
    ]);

    let gam = gamdat::parse(&build(&r)).unwrap();
    let roads = &gam.locations[0].roads;

    assert_eq!(roads.len(), 2, "a zero neighbour ends the list");
    assert_eq!(roads[0].neighbour, 1, "stored as 2, meaning location 1");
    assert_eq!(roads[0].days, 2);
    assert_eq!(roads[0].way, Some(14), "stored as 15, meaning way 14");
    assert_eq!(roads[1].neighbour, 4);
    assert_eq!(roads[1].way, None, "no drawn path for this one");
}

#[test]
fn danger_is_radiation_only_for_three() {
    for (byte, expected) in [(0u8, Danger::None), (3, Danger::Radiation), (1, Danger::Gas), (2, Danger::Gas)] {
        let gam = gamdat::parse(&build(&record(&[(45, &[byte])]))).unwrap();
        assert_eq!(gam.locations[0].danger, expected, "danger byte {byte}");
    }
}

#[test]
fn refuses_anything_that_is_not_a_save() {
    assert_eq!(gamdat::parse(&[]).unwrap_err(), GamdatError::NotAGamdat);
    assert_eq!(gamdat::parse(b"NOPE").unwrap_err(), GamdatError::NotAGamdat);

    let mut short = build(&[0; 50]);
    short.truncate(short.len() - 1);
    assert_eq!(gamdat::parse(&short).unwrap_err(), GamdatError::Truncated);
}

fn real_gamdat() -> Option<Vec<u8>> {
    let bytes = std::fs::read(testdata("burntime.pak")).ok()?;
    let pak = ClassicPak::open(&bytes).ok()?;
    pak.get("burn_gfx/gam.dat").map(<[u8]>::to_vec)
}

#[test]
fn reads_nob_hill_out_of_the_real_file() {
    let Some(bytes) = real_gamdat() else {
        eprintln!("skipping: no burntime.pak — see testdata/README.md");
        return;
    };
    let gam = gamdat::parse(&bytes).expect("the real gam.dat should parse");
    assert_eq!(gam.locations.len(), 37, "the classic game has 37; the 38th is the remaster's");
    assert_eq!(gam.day, 1);

    // World entrance 4 is Nob Hill, and so is record 4.
    let nob = &gam.locations[4];
    assert_eq!(nob.entry_point, Point::new(352, 224));
    assert_eq!((nob.water_reserve, nob.water_capacity, nob.water_refresh), (0, 6, 5));
    assert_eq!(nob.danger, Danger::Radiation);
    assert_eq!(nob.danger_amount, 45);
    assert_eq!(nob.roads.len(), 2);

    // Every entry point has to be somewhere sensible.
    for (i, l) in gam.locations.iter().enumerate() {
        assert!(l.entry_point.x > 0 && l.entry_point.y > 0, "location {i} has no entry point");
        assert!(l.entry_point.x < 2048 && l.entry_point.y < 2048, "location {i} is off the map");
    }
}

/// Every road leads somewhere that exists, and the graph is symmetric — if A
/// has a road to B then B has one back.
#[test]
fn the_road_graph_is_consistent() {
    let Some(bytes) = real_gamdat() else { return };
    let gam = gamdat::parse(&bytes).unwrap();

    let mut roads = 0;
    for (i, l) in gam.locations.iter().enumerate() {
        for road in &l.roads {
            roads += 1;
            assert!(road.neighbour < gam.locations.len(), "location {i} leads off the map");
            assert_ne!(road.neighbour, i, "location {i} has a road to itself");
            assert!(road.days > 0, "location {i} has a road that takes no time");
            assert!(
                gam.locations[road.neighbour].roads.iter().any(|back| back.neighbour == i),
                "location {i} -> {} has no road back",
                road.neighbour
            );
        }
    }
    assert!(roads > 60, "only {roads} roads in the whole world");
}

/// `title_id` is one-based, and the location it names is the one the world map
/// names at the same index. That is what lets a gam.dat record and a world
/// entrance be the same location without a lookup table.
#[test]
fn record_index_matches_the_world_maps_entrance_index() {
    let (Some(bytes), Ok(world), Ok(names)) = (
        real_gamdat(),
        std::fs::read_to_string(testdata("maps/MAT_000.txt")),
        std::fs::read_to_string(testdata("lang/BURN.TXT")),
    ) else {
        eprintln!("skipping: needs burntime.pak, MAT_000.txt and BURN.TXT");
        return;
    };
    let gam = gamdat::parse(&bytes).unwrap();
    let world = MapSettings::parse(&world);
    let db = TextDb::parse(&names);

    for (i, l) in gam.locations.iter().enumerate() {
        assert_eq!(l.title_id as usize, i + 1, "record {i}: title_id is one-based");

        let from_world = world
            .room(i)
            .and_then(|r| TextRef::parse(&r.title))
            .and_then(|r| db.get(r.index))
            .unwrap_or_else(|| panic!("world map has no name for entrance {i}"));
        let from_gamdat = db.get(l.title_id as usize - 1).unwrap();
        assert_eq!(from_world, from_gamdat, "record {i} and world entrance {i} disagree");
    }

    assert_eq!(db.get(gam.locations[4].title_id as usize - 1), Some("Nob Hill"));
}

/// The two independent sources of a location's name must agree.
///
/// `gam.dat` stores `title_id` counted from one; `MAT_000.txt` gives each of
/// the world map's rooms a `title = burn?<index>`, and those rooms *are* the
/// locations. So the same string has to come out of both, and it does only if
/// `title_id - 1` is used for the first.
///
/// The test goes through `LocationRecord::title_ref`, which is the same
/// function the game calls. Comparing two parsers would not have caught what
/// actually happened: the game did its own arithmetic and got it wrong while
/// every test still passed.
///
/// Worth a test of its own because getting it wrong is quiet: every location
/// takes its neighbour's name, and the result still reads like this game.
/// Reading `title_id` directly named Nob Hill "Muarab" and looked plausible.
#[test]
fn gamdat_and_the_world_map_name_the_same_places() {
    let Some(bytes) = std::fs::read(testdata("burntime.pak")).ok() else {
        eprintln!("skipping: no burntime.pak — see testdata/README.md");
        return;
    };
    let pak = ClassicPak::open(&bytes).unwrap();
    let gam = gamdat::parse(pak.get("burn_gfx/gam.dat").unwrap()).unwrap();

    let Ok(names) = std::fs::read_to_string(testdata("lang/BURN.TXT")) else {
        eprintln!("skipping: no lang/BURN.TXT");
        return;
    };
    let db = TextDb::parse(&names);

    let Ok(world) = std::fs::read_to_string(testdata("maps/MAT_000.txt")) else {
        eprintln!("skipping: no maps/MAT_000.txt");
        return;
    };
    let settings = MapSettings::parse(&world);

    let mut checked = 0;
    for (i, location) in gam.locations.iter().enumerate() {
        assert_eq!(location.title_id as usize, i + 1, "location {i} is numbered from one");

        let reference = location.title_ref().expect("every location names something");
        assert_eq!(reference.database, "burn");
        let from_gamdat = db.get(reference.index);
        let from_world = settings
            .room(i)
            .and_then(|r| TextRef::parse(&r.title))
            .and_then(|r| db.get(r.index));

        assert_eq!(from_gamdat, from_world, "location {i}");
        assert!(from_gamdat.is_some_and(|n| !n.is_empty()), "location {i} has no name");
        checked += 1;
    }
    assert_eq!(checked, 37);

    // The two the rest of this project talks about, spelled out.
    let name = |i: usize| db.get(gam.locations[i].title_ref().unwrap().index);
    assert_eq!(name(4), Some("Nob Hill"));
    assert_eq!(name(0), Some("New Sandez"));

    // And a record that names nothing has no reference at all, rather than
    // wrapping round to the last line of the table.
    let mut blank = gam.locations[0].clone();
    blank.title_id = 0;
    assert!(blank.title_ref().is_none());
}

/// The item table (`SaveGame.cs`, `Save/Item.cs`): 1199 records, and the
/// four player slots' packs are there — v1.2 Classic's `dos_gamdat` start and
/// the Amiga opponent's start come from them.
#[test]
fn the_item_table_and_the_player_packs() {
    use burngfx::gamdat::ItemOwner;
    let Some(bytes) = real_gamdat() else { return };
    let g = gamdat::parse(&bytes).unwrap();
    assert_eq!(g.items.len(), gamdat::ITEM_COUNT);
    let packs: Vec<Vec<u8>> = (0..4)
        .map(|slot| g.items.iter().filter(|i| i.owner() == ItemOwner::Character(slot)).map(|i| i.sprite_id).collect())
        .collect();
    let rooms = g.items.iter().filter(|i| matches!(i.owner(), ItemOwner::Room { .. })).count();
    let ground = g.items.iter().filter(|i| matches!(i.owner(), ItemOwner::Dropped { .. })).count();
    let pool = g.items.iter().filter(|i| i.owner() == ItemOwner::Pool).count();
    eprintln!("packs {packs:?} rooms {rooms} ground {ground} pool {pool}");
    for (slot, pack) in packs.iter().enumerate() {
        assert!(!pack.is_empty(), "player slot {slot} starts with something");
    }
    for i in &g.items {
        if let ItemOwner::Room { location, .. } | ItemOwner::Dropped { location, .. } = i.owner() {
            assert!(location < gamdat::LOCATION_COUNT, "{i:?}");
        }
    }
}

/// **Dogs stand in cities** — so v1.2's city rule for creatures is not
/// moot (the register's RULE-38). v1.2's creature mind does not attack in a
/// city (`canAttack = !Owner.Location.IsCity`, `AI/CreatureMind.cs:31,
/// 39-44,70`). A creature is placed by its record (`GameCreation.cs:
/// 438-476`) and comes back where it died (`CharacterRespawn.cs:158-161`;
/// only mercenaries, technicians and doctors are sent to a city, `:266-288`),
/// and the file puts five dogs in three cities.
#[test]
fn five_dogs_stand_in_cities() {
    use burngfx::gamdat::{CharClass, Placed};
    let Some(bytes) = real_gamdat() else { return };
    let g = gamdat::parse(&bytes).unwrap();
    let mut in_cities: Vec<(CharClass, usize)> = g
        .characters
        .iter()
        .filter(|c| matches!(c.class, CharClass::Dog | CharClass::Mutant))
        .filter_map(|c| match c.placed {
            Placed::At(l) if g.locations[l].is_city => Some((c.class, l)),
            _ => None,
        })
        .collect();
    in_cities.sort_by_key(|&(_, l)| l);
    assert_eq!(
        in_cities,
        [(CharClass::Dog, 8), (CharClass::Dog, 8), (CharClass::Dog, 8), (CharClass::Dog, 13), (CharClass::Dog, 21)]
    );
}
