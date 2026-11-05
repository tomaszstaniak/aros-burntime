//! New Village (`MAT_038`): the location the remaster adds after the 37 of
//! `gam.dat` (`LocationCreator.Create` since v1.2; docs/compliance-v1.3.md
//! D2), checked against the real `gam.dat` and v1.3's `MAT_038.txt`.

mod common;

use burngfx::gamdat::{self, add_remaster_location, Danger, Gamdat, RemasterLocation, NEW_VILLAGE};
use burngfx::rivals::{holdings_wanted, Map};

/// A missing input fails the test (`common`), or skips it with
/// `BURNTIME_ALLOW_MISSING_TESTDATA`.
fn testdata(rel: &str) -> Option<Vec<u8>> {
    common::read(rel)
}

fn world() -> Option<(Gamdat, RemasterLocation)> {
    let pak = testdata("burntime.pak")?;
    let pak = burngfx::pak::ClassicPak::open(&pak).ok()?;
    let g = gamdat::parse(pak.get("burn_gfx/gam.dat")?).ok()?;
    let text = String::from_utf8(testdata("v1.3/maps/MAT_038.txt")?).ok()?;
    Some((g, RemasterLocation::parse(&text)))
}

struct W<'a>(&'a Gamdat);
impl Map for W<'_> {
    fn count(&self) -> usize { self.0.locations.len() }
    fn is_city(&self, l: usize) -> bool { self.0.locations[l].is_city }
    fn neighbours(&self, l: usize) -> Vec<usize> { self.0.locations[l].roads.iter().map(|r| r.neighbour).collect() }
}

#[test]
fn the_38th_location_is_built_from_its_settings() {
    let Some((mut g, nv)) = world() else { return };
    assert_eq!(g.locations.len(), 37);
    assert!(add_remaster_location(&mut g, &nv));
    assert!(!add_remaster_location(&mut g, &nv), "once only");
    let l = &g.locations[NEW_VILLAGE];
    let title = l.title_ref().unwrap();
    assert_eq!((title.database, title.index), ("burn", 37), "Location.Title = burn?Id");
    assert_eq!(l.entry_point, burngfx::game::Point::new(252, 229));
    assert_eq!((l.water_refresh, l.water_reserve, l.water_capacity), (3, 3, 6), "WaterReserve = water");
    assert!(!l.is_city);
    assert_eq!(l.danger, Danger::None);
    assert_eq!(l.available_products, [Some(1), None, None, None], "the rat trap only");
    assert_eq!(l.producing, None);
    assert_eq!(l.owner, 0);
}

/// `ways = 8`, `way_lengths = 3`: a road of three days to location 8, and
/// `updateBurngfxLocation` adds the way back; no drawn path either way.
#[test]
fn the_road_goes_both_ways() {
    let Some((mut g, nv)) = world() else { return };
    let before = g.locations[8].roads.len();
    add_remaster_location(&mut g, &nv);
    let out: Vec<_> = g.locations[NEW_VILLAGE].roads.iter().map(|r| (r.neighbour, r.days, r.way)).collect();
    assert_eq!(out, [(8, 3, None)]);
    let back: Vec<_> = g.locations[8].roads.iter().filter(|r| r.neighbour == NEW_VILLAGE).map(|r| (r.days, r.way)).collect();
    assert_eq!(back, [(3, None)]);
    assert_eq!(g.locations[8].roads.len(), before + 1);
    // No other place gains a road.
    for (i, l) in g.locations.iter().enumerate().filter(|&(i, _)| i != 8 && i != NEW_VILLAGE) {
        assert!(l.roads.iter().all(|r| r.neighbour != NEW_VILLAGE), "{i}");
    }
}

/// Location 8 is a city, so New Village neighbours a city and must be held
/// to win (`VictoryCondition`): one more holding than the 37-location world.
#[test]
fn new_village_counts_for_victory() {
    let Some((mut g, nv)) = world() else { return };
    assert!(g.locations[8].is_city);
    let without = holdings_wanted(&W(&g));
    add_remaster_location(&mut g, &nv);
    let with = holdings_wanted(&W(&g));
    assert!(!without.contains(&NEW_VILLAGE));
    assert!(with.contains(&NEW_VILLAGE));
    assert_eq!(with.len(), without.len() + 1);
}
