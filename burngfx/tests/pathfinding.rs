//! Walking to a door you cannot stand in.
//!
//! A door's rectangle is the doorway, and `BurnmapProcessor` blocks it along
//! with the rest of the building. So "walk to the door" has to mean "walk to
//! somewhere within reach of it", and the search has to be told that — aiming
//! at the rectangle itself finds nothing and reports the room unreachable.

use burngfx::game::{self, LocationData, Point, ENTRY_REACH};
use burngfx::map::Entrance;
use burngfx::zei::{WalkGrid, WalkMask, MASK_CELL};
use std::collections::BTreeSet;

/// A grid from a picture: `#` blocked, `.` open. One character per mask cell,
/// so one character is 8x8 map pixels.
fn grid(rows: &[&str]) -> WalkGrid {
    let (w, h) = (rows[0].len(), rows.len());
    assert!(w % 4 == 0 && h % 4 == 0, "the grid is built a tile at a time");
    let mut g = WalkGrid::new(w / 4, h / 4);
    for (ty, chunk) in rows.chunks(4).enumerate() {
        for tx in 0..w / 4 {
            g.fill_tile(tx, ty, true);
            let mut mask = 0u16;
            for (y, row) in chunk.iter().enumerate() {
                for x in 0..4 {
                    if row.as_bytes()[tx * 4 + x] == b'#' {
                        mask |= 0x8000 >> (y * 4 + x);
                    }
                }
            }
            g.and_tile(tx, ty, WalkMask(mask));
        }
    }
    g
}

/// The centre of cell (cx, cy) in map pixels.
fn cell(cx: i32, cy: i32) -> Point {
    let half = MASK_CELL as i32 / 2;
    Point::new(cx * MASK_CELL as i32 + half, cy * MASK_CELL as i32 + half)
}

/// A door on the far side of a building: reachable, but not in a straight line.
/// This is precisely the shape that defeated the old walker.
const AROUND_A_BUILDING: &[&str] = &[
    "............",
    "....####....",
    "....####....",
    "....####....",
    "....####....",
    "............",
    "............",
    "............",
];

#[test]
fn a_door_behind_a_building_is_reached_by_going_around_it() {
    let g = grid(AROUND_A_BUILDING);
    // The doorway is the building's own top-left cell — blocked, as doorways
    // are, and unreachable if taken literally.
    let door = Entrance { left: 4 * 8, top: 8, width: 8, height: 8 };
    assert!(!g.cell(4, 1), "the doorway itself is blocked");

    let from = cell(0, 6);
    let path = game::path_to_entrance(&g, from, &door, ENTRY_REACH)
        .expect("there is a way round");

    // Every step is walkable, adjacent to the last, and the last is in reach.
    let mut prev = from;
    for &p in &path {
        assert!(g.walkable_at(p.x, p.y), "stepped onto a blocked cell at {p:?}");
        let steps = (p.x - prev.x).abs() / MASK_CELL as i32
            + (p.y - prev.y).abs() / MASK_CELL as i32;
        assert_eq!(steps, 1, "path jumps from {prev:?} to {p:?}");
        prev = p;
    }
    assert!(game::in_reach_of(&door, prev, ENTRY_REACH), "ends out of reach");

    // And it really did go around rather than through: a straight line from
    // the start would cross the building.
    assert!(path.len() >= 6, "suspiciously short: {} steps", path.len());
}

/// The distinction the report has to be able to make.
#[test]
fn a_walled_in_door_is_reported_unreachable_rather_than_far() {
    // The wall is three cells thick. Two would not do: a cell on the near side
    // would be 16 pixels from the door and reach is 15, so the character would
    // be "in reach" through a solid wall. That is a real trap and the reason
    // this fixture looks over-built.
    let g = grid(&[
        "............",
        "############",
        "############",
        "############",
        "............",
        "............",
        "............",
        "............",
    ]);
    // A door in the corridor along the top, sealed off from everything below.
    let door = Entrance { left: 5 * 8, top: 0, width: 8, height: 8 };
    assert!(g.cell(5, 0), "the corridor itself is open");

    assert!(
        game::path_to_entrance(&g, cell(5, 6), &door, ENTRY_REACH).is_none(),
        "nothing below can reach the corridor"
    );
    // From inside, it is already arrived.
    assert_eq!(game::path_to_entrance(&g, cell(5, 0), &door, ENTRY_REACH), Some(Vec::new()));
}

/// Reach is a property of the room, and a rope room's 75 pixels is nine cells
/// — far enough to change the answer, not merely the distance.
#[test]
fn a_rope_room_is_reachable_from_where_an_ordinary_one_is_not() {
    let g = grid(&[
        "............",
        "############",
        "############",
        "............",
        "............",
        "............",
        "............",
        "............",
    ]);
    // A door up above the wall, with no way through it.
    let door = Entrance { left: 5 * 8, top: 0, width: 8, height: 8 };

    let from = cell(5, 4);
    assert!(
        game::path_to_entrance(&g, from, &door, ENTRY_REACH).is_none(),
        "15 pixels cannot cross two blocked rows"
    );
    let by_rope = game::path_to_entrance(&g, from, &door, 75).expect("75 pixels can");
    // Nothing had to move: the character is already within a rope's throw.
    assert!(by_rope.is_empty(), "already in reach, so no walking: {by_rope:?}");
}

#[test]
fn walking_to_an_ordinary_point_uses_the_same_search() {
    let g = grid(AROUND_A_BUILDING);
    let path = game::path_to(&g, cell(0, 6), cell(11, 0)).expect("the top right is open");
    assert_eq!(path.last().copied(), Some(cell(11, 0)));

    // A point inside the building has no path to it at all.
    assert!(game::path_to(&g, cell(0, 6), cell(5, 2)).is_none());
    // And standing still is a path of no steps rather than no path.
    assert_eq!(game::path_to(&g, cell(3, 3), cell(3, 3)), Some(Vec::new()));
}

#[test]
fn a_search_from_somewhere_impossible_gives_up_instead_of_guessing() {
    let g = grid(AROUND_A_BUILDING);
    let door = Entrance { left: 0, top: 0, width: 8, height: 8 };
    // Inside the building, and off the map entirely.
    assert!(game::path_to_entrance(&g, cell(5, 2), &door, ENTRY_REACH).is_none());
    assert!(game::path_to_entrance(&g, Point::new(-100, -100), &door, ENTRY_REACH).is_none());
}

/// Everything reachable in whatever archive happens to be built.
///
/// Deliberately makes **no claim about coverage**: it reports what it looked
/// at and finds nothing wrong with it. The default archive holds two maps, so
/// a green result here means two maps are sound and says nothing at all about
/// the other thirty six. The claim about the whole game is
/// [`the_whole_world_can_be_walked`], which refuses to run on a partial
/// archive rather than passing on one.
#[test]
fn whatever_is_packed_can_be_walked() {
    let Some(archive) = Archive::open() else { return };
    let (checked, problems) = archive.audit();
    assert!(
        problems.iter().all(|p| KNOWN.contains(&p.as_str())),
        "unexplained problems in {} maps: {:#?}",
        archive.maps.len(),
        problems.iter().filter(|p| !KNOWN.contains(&p.as_str())).collect::<Vec<_>>()
    );
    eprintln!(
        "checked {checked} doors across {} maps — this says nothing about the rest",
        archive.maps.len()
    );
}

/// The claim: every door in the game can be walked to.
///
/// Ignored by default, because it needs the whole archive and the default one
/// holds two maps. An ignored test reports itself as not run; a test that
/// quietly checks two maps reports itself as green, and that difference is the
/// entire point of this being separate.
///
/// ```sh
/// scripts/pack.sh $(seq -w 0 37 | sed 's/^/MAT_0/')
/// cargo test -p burngfx --test pathfinding -- --ignored --nocapture
/// scripts/pack.sh   # put the small archive back
/// ```
///
/// Last run 2026-09-05: 188 doors across 38 maps, the two known problems and
/// nothing else.
#[test]
#[ignore = "needs the whole archive; see the doc comment for the two commands"]
fn the_whole_world_can_be_walked() {
    let archive = Archive::open().expect("the archive must be built for this test");

    // The world and all thirty seven classic locations, named. Checking "every
    // map present" would pass on any subset, which is how the earlier version
    // of this reported success for two maps.
    let expected: Vec<String> = (0..=37).map(|i| format!("MAT_{i:03}")).collect();
    assert_eq!(archive.maps, expected, "the archive is not the whole world");
    let gam = archive.gam.as_ref().expect("the whole world needs gam.dat");
    assert_eq!(gam.locations.len(), 37, "gam.dat should hold 37 locations");

    // Every classic map must carry a room id for every door it has, or the
    // reaches below are guesses rather than the game's own answer. Outback has
    // no rooms at all, which is why this counts doors rather than maps.
    let mut with_rooms = 0;
    for name in archive.maps.iter().filter(|n| *n != "MAT_000") {
        let ids = archive.room_ids(name);
        let doors = archive.doors_of(name);
        assert_eq!(ids.len(), doors, "{name}: {} room ids for {doors} doors", ids.len());
        if !ids.is_empty() {
            with_rooms += 1;
        }
    }
    assert_eq!(with_rooms, 36, "only {with_rooms} locations have rooms; Outback has none");

    let (checked, problems) = archive.audit();

    // Both directions. `unexpected` alone lets a known problem disappear along
    // with the map that had it, and calls that a pass.
    let got: BTreeSet<&str> = problems.iter().map(|s| s.as_str()).collect();
    let want: BTreeSet<&str> = KNOWN.iter().copied().collect();
    assert_eq!(
        got,
        want,
        "\n  new: {:?}\n  gone: {:?}",
        got.difference(&want).collect::<Vec<_>>(),
        want.difference(&got).collect::<Vec<_>>()
    );

    assert!(checked >= 185, "only {checked} doors — the archive is thinner than it looks");
    eprintln!("the whole world: {checked} doors across {} maps", archive.maps.len());
}

/// Known, and each one a statement about the archive rather than about the
/// search. Listed rather than tolerated: a new entry has to be looked at and
/// explained, and one that stops happening has to be removed.
const KNOWN: &[&str] = &[
    // The Factory's second room. Its four in-reach cells are a pocket sealed by
    // the tile masks of ordinary classic tiles — no custom sheet, no
    // hand-written correction — so this is what ZEI says, read the way the
    // rest of the archive is read. Note also that reachability here is decided
    // through cell centres, not every pixel of the map: a character allowed to
    // stand anywhere in a cell might just reach it. So this is a case to
    // compare against the original, not a proven fault in the data.
    "MAT_023 door 1: 4 walkable cells in reach, none connected to Point { x: 32, y: 64 }",
    // Hard Mans Death: gam.dat puts the arrival on a blocked cell, so the
    // character is nudged to the nearest one it can stand on. Without that
    // every search in the location refuses to start and nothing works.
    "MAT_032: spawn Point { x: 110, y: 66 } is not walkable, nudged to Point { x: 108, y: 60 }",
];

/// The built archive, and the walk over it that both tests above share.
struct Archive {
    bytes: Vec<u8>,
    maps: Vec<String>,
    gam: Option<burngfx::gamdat::Gamdat>,
}

impl Archive {
    fn open() -> Option<Archive> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("burntime.pack");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipping: no burntime.pack — run `scripts/pack.sh`");
            return None;
        };
        let mut me = Archive { bytes, maps: Vec::new(), gam: None };
        let pack = burngfx::pack::Pack::open(&me.bytes).unwrap();
        me.gam = pack.get("classic/gam.dat").and_then(|b| burngfx::gamdat::parse(b).ok());
        for i in 0..pack.len() {
            if let Some(stem) = pack.name(i).strip_prefix(b"map/".as_slice()) {
                if let Some(stem) = stem.strip_suffix(b".burnmap".as_slice()) {
                    me.maps.push(core::str::from_utf8(stem).unwrap().to_string());
                }
            }
        }
        me.maps.sort();
        assert!(!me.maps.is_empty(), "the archive holds no maps at all");
        Some(me)
    }

    fn pack(&self) -> burngfx::pack::Pack<'_> {
        burngfx::pack::Pack::open(&self.bytes).unwrap()
    }

    fn entry<'a>(&'a self, pack: &burngfx::pack::Pack<'a>, kind: u8, name: &str) -> Option<&'a [u8]> {
        let mut key = [0u8; burngfx::pack::NAME_LEN];
        let n = match kind {
            b'm' => burngfx::pack::names::map(&mut key, name),
            b'w' => burngfx::pack::names::walk(&mut key, name),
            _ => burngfx::pack::names::rooms(&mut key, name),
        }?;
        pack.get(core::str::from_utf8(&key[..n]).unwrap())
    }

    /// How many doors this map's `.burnmap` has.
    fn doors_of(&self, name: &str) -> usize {
        let pack = self.pack();
        burngfx::map::parse(self.entry(&pack, b'm', name).unwrap()).unwrap().entrances.len()
    }

    fn room_ids(&self, name: &str) -> Vec<u8> {
        let pack = self.pack();
        self.entry(&pack, b'r', name).map(|b| b.to_vec()).unwrap_or_default()
    }

    /// Walk every door of every map in the archive. Returns how many doors were
    /// looked at, and every problem found — all of them, rather than the first.
    fn audit(&self) -> (usize, Vec<String>) {
        let pack = self.pack();
        let mut doors = 0;
        let mut problems = Vec::new();

        for stem in &self.maps {
            let map = burngfx::map::parse(self.entry(&pack, b'm', stem).unwrap()).unwrap();
            let packed = self
                .entry(&pack, b'w', stem)
                .unwrap_or_else(|| panic!("{stem} has no walk grid — the packer should write one"));
            let g = WalkGrid::from_packed(packed).expect("the packer wrote it");
            let room_ids = self.room_ids(stem);

            // The spawn the game uses: gam.dat's entry point where there is
            // one, the middle of the map otherwise. MAT_000 is the world and
            // has no record of its own, so it takes the centre like any other.
            let index: usize =
                stem.trim_start_matches("MAT_").trim_start_matches('0').parse().unwrap_or(0);
            let entry = index
                .checked_sub(1)
                .and_then(|i| self.gam.as_ref()?.locations.get(i))
                .map(|l| l.entry_point);
            let named = game::spawn_point(&LocationData { entry_point: entry }, &map);
            let Some(from) = game::nearest_standing(&g, named) else {
                problems.push(format!("{stem}: nowhere to stand near spawn {named:?}"));
                continue;
            };
            if from != named {
                problems.push(format!(
                    "{stem}: spawn {named:?} is not walkable, nudged to {from:?}"
                ));
            }

            for (k, e) in map.entrances.iter().enumerate() {
                doors += 1;
                let reach = room_ids
                    .get(k)
                    .map(|&id| burngfx::matmap::RoomType::from_room_id(id).entry_reach())
                    .unwrap_or(ENTRY_REACH);
                if game::path_to_entrance(&g, from, e, reach).is_some() {
                    continue;
                }
                // Which of the two it is. A zone that exists but cannot be
                // walked to is the collision data enclosing the doorway; no
                // zone at all would be the search or the reach being wrong.
                let zone = (0..g.height)
                    .flat_map(|cy| (0..g.width).map(move |cx| (cx, cy)))
                    .filter(|&(cx, cy)| g.cell(cx, cy))
                    .filter(|&(cx, cy)| {
                        let half = MASK_CELL as i32 / 2;
                        let p = Point::new(
                            cx as i32 * MASK_CELL as i32 + half,
                            cy as i32 * MASK_CELL as i32 + half,
                        );
                        game::in_reach_of(e, p, reach)
                    })
                    .count();
                problems.push(if zone == 0 {
                    format!("{stem} door {k}: nothing within {reach} of {e:?} is walkable at all")
                } else {
                    format!(
                        "{stem} door {k}: {zone} walkable cells in reach, none connected to {from:?}"
                    )
                });
            }
        }
        (doors, problems)
    }
}

/// Where the search starts is checked before whether it has arrived.
///
/// The other order says yes to a character standing off the map who happens to
/// be within reach of a door — a position it could not have walked to and
/// cannot act from — and hands back an empty path meaning "already there".
#[test]
fn a_start_outside_the_map_is_refused_even_next_to_the_door() {
    let g = grid(AROUND_A_BUILDING);
    let door = Entrance { left: 0, top: 0, width: 8, height: 8 };

    // Four pixels off the left edge, and well inside the door's reach.
    let outside = Point::new(-4, 4);
    assert!(game::in_reach_of(&door, outside, ENTRY_REACH), "the fixture must be in reach");
    assert!(game::path_to_entrance(&g, outside, &door, ENTRY_REACH).is_none());

    // The same point on the map is not refused, which is what makes the case
    // above about the start rather than about the door.
    assert_eq!(
        game::path_to_entrance(&g, Point::new(4, 4), &door, ENTRY_REACH),
        Some(Vec::new())
    );
}

/// A click hits the doorway that was drawn, not whatever is nearest.
#[test]
fn a_click_inside_a_doorway_picks_that_doorway() {
    use burngfx::map::Map;

    // Two doors side by side, both with the ordinary reach, so nothing but the
    // distance can tell them apart. Picking by reach makes this a tie, and a
    // tie is settled by the order they happen to sit in the file.
    let map = Map {
        width: 12,
        height: 8,
        tile_size: 32,
        set_names: Vec::new(),
        tiles: Vec::new(),
        entrances: vec![
            Entrance { left: 0, top: 0, width: 16, height: 16 },
            Entrance { left: 20, top: 0, width: 16, height: 16 },
        ],
    };

    assert_eq!(game::entrance_at(&map, Point::new(25, 8)), Some(1), "clicked the second");
    assert_eq!(game::entrance_at(&map, Point::new(8, 8)), Some(0));
    assert_eq!(game::entrance_at(&map, Point::new(18, 8)), None, "between them is neither");

    // The nearest-within-tolerance fallback is by distance, so a click just
    // outside the second door still means the second door.
    assert_eq!(game::entrance_in_reach(&map, Point::new(18, 8), 4), Some(1));
    assert_eq!(game::entrance_in_reach(&map, Point::new(17, 8), 4), Some(0));
}

/// The path a click actually takes: hit a doorway, decide the intent, plan the
/// route, arrive. Testing the search alone left this whole seam uncovered, and
/// it is where the mistakes were.
mod clicking {
    use super::*;
    use burngfx::game::{Intent, Plan};
    use burngfx::map::Map;

    const ORDINARY: fn(usize) -> i32 = |_| ENTRY_REACH;

    fn town() -> (Map, WalkGrid) {
        let g = grid(AROUND_A_BUILDING);
        // A door on the building's near side and another on its far side, both
        // ordinary, so nothing but the click can tell them apart.
        let map = Map {
            width: 12,
            height: 8,
            tile_size: 32,
            set_names: Vec::new(),
            tiles: Vec::new(),
            entrances: vec![
                Entrance { left: 4 * 8, top: 4 * 8, width: 8, height: 8 },
                Entrance { left: 6 * 8, top: 8, width: 8, height: 8 },
            ],
        };
        (map, g)
    }

    /// Standing in reach and clicking the door must open it where you stand.
    ///
    /// The bug this pins: an empty route was read as "no waypoints", the code
    /// fell back to the clicked pixel, and the character set off walking into
    /// the building it had just been asked to enter.
    #[test]
    fn standing_in_reach_acts_without_walking() {
        let (map, g) = town();
        // One cell below the near door — inside its 15 pixels.
        let from = cell(4, 5);
        assert!(game::entrance_reached(&map, 0, from, ENTRY_REACH));

        let plan = game::plan_click(&map, Some(&g), from, Point::new(4 * 8 + 4, 4 * 8 + 4), ORDINARY, 4)
            .expect("the door is right there");
        assert_eq!(plan.intent, Intent::Enter(0));
        assert!(plan.path.is_empty(), "nothing to walk: {:?}", plan.path);
        assert!(plan.act_now, "must act where it stands, not walk to the click");
    }

    /// The same shape for a plain walk: no steps is not an interaction.
    #[test]
    fn standing_on_the_spot_is_not_an_interaction() {
        let (map, g) = town();
        let here = cell(1, 6);
        let plan = game::plan_click(&map, Some(&g), here, here, ORDINARY, 4).unwrap();
        assert_eq!(plan.intent, Intent::Walk);
        assert!(plan.path.is_empty());
        assert!(!plan.act_now, "arriving where you already are opens nothing");
    }

    /// The door that was clicked, not the one that happens to be first.
    #[test]
    fn the_clicked_door_is_the_one_that_opens() {
        let (map, g) = town();
        let from = cell(1, 6);

        let far = game::plan_click(&map, Some(&g), from, Point::new(6 * 8 + 4, 12), ORDINARY, 4)
            .expect("reachable the long way round");
        assert_eq!(far.intent, Intent::Enter(1), "clicked the far door");
        assert!(!far.path.is_empty(), "and it is a walk to get there");
        assert!(!far.act_now);

        let near = game::plan_click(&map, Some(&g), from, Point::new(4 * 8 + 4, 4 * 8 + 4), ORDINARY, 4)
            .unwrap();
        assert_eq!(near.intent, Intent::Enter(0));
    }

    /// A click in open desert is a walk, and a click a hair off a door is not.
    #[test]
    fn slack_is_a_fingertip_not_a_reach() {
        let (map, g) = town();
        let from = cell(1, 6);

        // Two pixels below the near door: still that door.
        let just_off = Point::new(4 * 8 + 4, 5 * 8);
        assert_eq!(
            game::plan_click(&map, Some(&g), from, just_off, ORDINARY, 4).unwrap().intent,
            Intent::Enter(0)
        );

        // Ten pixels away is well inside the *reach* of 15, and is still a
        // walk — because reach is how far a door works from, not how much of
        // the map belongs to it.
        let further = Point::new(4 * 8 + 4, 5 * 8 + 8);
        assert!(game::in_reach_of(&map.entrances[0], further, ENTRY_REACH));
        assert_eq!(
            game::plan_click(&map, Some(&g), from, further, ORDINARY, 4).unwrap().intent,
            Intent::Walk
        );
    }

    /// A rope room is used from 75 pixels, and that changes the plan rather
    /// than merely the distance walked.
    #[test]
    fn a_rope_rooms_reach_is_used_for_its_route() {
        let (map, g) = town();
        let by_rope: fn(usize) -> i32 = |i| if i == 1 { 75 } else { ENTRY_REACH };
        let from = cell(1, 6);

        let ordinary = game::plan_click(&map, Some(&g), from, Point::new(6 * 8 + 4, 12), ORDINARY, 4)
            .unwrap();
        let roped = game::plan_click(&map, Some(&g), from, Point::new(6 * 8 + 4, 12), by_rope, 4)
            .unwrap();
        assert_eq!(roped.intent, ordinary.intent);
        assert!(
            roped.path.len() < ordinary.path.len(),
            "a rope is thrown from further off: {} steps against {}",
            roped.path.len(),
            ordinary.path.len()
        );
    }

    /// Somewhere with no way in is refused, rather than planned and then
    /// abandoned partway.
    #[test]
    fn an_unreachable_door_has_no_plan_at_all() {
        let g = grid(&[
            "............",
            "############",
            "############",
            "############",
            "............",
            "............",
            "............",
            "............",
        ]);
        let map = Map {
            width: 12,
            height: 8,
            tile_size: 32,
            set_names: Vec::new(),
            tiles: Vec::new(),
            entrances: vec![Entrance { left: 5 * 8, top: 0, width: 8, height: 8 }],
        };
        assert!(game::plan_click(&map, Some(&g), cell(5, 6), Point::new(5 * 8 + 4, 4), ORDINARY, 4)
            .is_none());
    }

    /// Without collision data there is nothing to route around, and the plan
    /// still has to say whether to act or to walk.
    #[test]
    fn no_walk_grid_still_produces_a_usable_plan() {
        let (map, _) = town();
        let near = Point::new(4 * 8 + 4, 4 * 8 + 4);

        let standing = game::plan_click(&map, None, cell(4, 5), near, ORDINARY, 4).unwrap();
        assert!(standing.act_now, "in reach already");

        let far = game::plan_click(&map, None, cell(1, 6), near, ORDINARY, 4).unwrap();
        assert_eq!(far.intent, Intent::Enter(0));
        assert!(!far.act_now);
        assert_eq!(far.target, near, "with nothing to route around, aim at the click");
        let _: Plan = far;
    }
}

/// Nudging an arrival is a correction, and a short one on purpose.
mod arrival {
    use super::*;
    use burngfx::game::NUDGE_LIMIT;

    #[test]
    fn a_point_that_is_already_fine_is_left_exactly_alone() {
        let g = grid(AROUND_A_BUILDING);
        let p = Point::new(3, 51); // not a cell centre; must come back unchanged
        assert!(g.walkable_at(p.x, p.y));
        assert_eq!(game::nearest_standing(&g, p), Some(p));
    }

    #[test]
    fn a_point_just_inside_a_wall_moves_to_the_nearest_cell_outside_it() {
        let g = grid(AROUND_A_BUILDING);
        // The building's bottom-left cell, one step from open ground below it.
        let inside = cell(4, 4);
        assert!(!g.walkable_at(inside.x, inside.y));
        let out = game::nearest_standing(&g, inside).expect("open ground is one cell away");
        assert!(g.walkable_at(out.x, out.y));
        assert!(
            (out.x - inside.x).abs() + (out.y - inside.y).abs() <= MASK_CELL as i32,
            "moved further than one cell: {inside:?} -> {out:?}"
        );
    }

    /// The limit is what stops a nudge from becoming teleportation. A point
    /// walled in by more than a tile is reported as having nowhere to stand,
    /// so a caller can say so instead of quietly using the blocked point.
    #[test]
    fn it_will_not_carry_the_character_across_a_building() {
        assert_eq!(NUDGE_LIMIT, 4, "one tile");
        // A solid block wider than the limit in every direction.
        let mut rows = Vec::new();
        for y in 0..16 {
            let mut row = String::new();
            for x in 0..16 {
                row.push(if (2..14).contains(&x) && (2..14).contains(&y) { '#' } else { '.' });
            }
            rows.push(row);
        }
        let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
        let g = grid(&refs);

        // Dead centre of the block: open ground is six cells away, and six is
        // more than one tile.
        assert_eq!(game::nearest_standing(&g, cell(8, 8)), None);
        // Just inside its edge, it is within reach and does move.
        assert!(game::nearest_standing(&g, cell(2, 8)).is_some());
    }
}
