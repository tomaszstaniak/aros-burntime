//! `Character.Dismiss` and what a takeover puts back — the pieces a player's
//! defeat is made of.

use burngfx::game::{Camps, Companion, ItemBag, Party, Point, PACK_CAPACITY};
use burngfx::items::ItemTypes;
use burngfx::spawn::Rng;

/// Three foods, a bottle, two rare things (one `nodrop`), a trap and some
/// junk. Their order is the table's, which is what `GenerateClass` picks by.
const TABLE: &str = "
[item_maggots]
class=food
food=3
[item_rats]
class=food
food=5
[item_snake]
class=food
food=7
[item_bottle]
class=bottle
[item_gold]
class=rare
[item_idol]
class=rare,nodrop
[item_rat_trap]
class=trap
[item_wire]
class=material
[item_junk]
class=useless
";

fn types() -> ItemTypes {
    ItemTypes::parse(TABLE)
}

fn one_with(id: usize, pack: &[usize]) -> Companion {
    let mut c = Companion::new(id);
    c.origin = Some(100 + id);
    c.pack = ItemBag::with(PACK_CAPACITY, pack);
    c.at = Point::new(40 + id as i32, 50);
    c
}

#[test]
fn a_dismissed_companion_leaves_the_group_with_his_things() {
    let mut party = Party::restored(vec![one_with(1, &[3]), one_with(2, &[4, 5])], 3);
    let gone = party.dismiss(0).expect("somebody there");
    assert_eq!(gone.id, 1);
    assert_eq!(gone.pack.len(), 1, "nothing dropped");
    assert_eq!(party.len(), 1);
    assert!(party.dismiss(5).is_none());
}

#[test]
fn only_the_last_camper_out_frees_the_place() {
    let mut camps = Camps::new();
    camps.join(7, 1, one_with(1, &[]));
    camps.join(7, 1, one_with(2, &[]));
    let (first, freed) = camps.dismiss(7, 0).expect("a camper");
    assert_eq!(first.id, 1);
    assert_eq!(freed, None, "one is still there: still player 1's");
    assert_eq!(camps.get(7).and_then(|c| c.owner), Some(1));
    let (_, freed) = camps.dismiss(7, 0).expect("the last one");
    assert_eq!(freed, Some(1), "the owner it had, for the takeover rule");
    assert_eq!(camps.get(7).and_then(|c| c.owner), None);
}

#[test]
fn nought_to_two_is_none_and_the_last_type_is_never_drawn() {
    let t = types();
    let gold = t.index_of("item_gold");
    let snake = t.index_of("item_snake");
    let junk = t.index_of("item_junk");
    let wire = t.index_of("item_wire");
    for seed in 0..400u64 {
        // Hard: only the first tier, and its "nought to two" gives nothing.
        let got = burngfx::rivals::takeover_restock(&t, 2, &mut Rng::seeded(seed));
        assert!(!got.is_empty(), "one food always");
        assert!(!got.contains(&snake.unwrap()), "the last food is never picked");
        assert!(!got.iter().any(|k| Some(*k) == junk || Some(*k) == wire), "min 0 gives none");
        assert!(got.len() <= 3);
        // Easy: "one to two" is one, and the last of material/useless (junk)
        // is never the one.
        let easy = burngfx::rivals::takeover_restock(&t, 0, &mut Rng::seeded(seed));
        assert_eq!(easy.iter().filter(|k| Some(**k) == wire).count(), 1);
        assert!(!easy.contains(&junk.unwrap()));
    }
    // And the rare pick at every level excludes `nodrop`, leaving one
    // candidate, which `Next(0)` still yields.
    let some_rare = (0..400u64)
        .any(|s| burngfx::rivals::takeover_restock(&t, 2, &mut Rng::seeded(s)).contains(&gold.unwrap()));
    assert!(some_rare);
}

#[test]
fn a_dismissed_man_stands_where_he_was_let_go_across_a_save() {
    use burngfx::hire::People;
    // The real character table, when the game's own files are here.
    let Ok(bytes) = std::fs::read("testdata/burntime.pak") else { return };
    let Ok(pak) = burngfx::pak::ClassicPak::open(&bytes) else { return };
    let Some(raw) = pak.get("burn_gfx/gam.dat") else { return };
    let Ok(gam) = burngfx::gamdat::parse(raw) else { return };
    let t = types();
    let mut people = People::from_records(&gam.characters, &t, &Default::default());
    let who = (0..people.len()).find(|&i| people.get(i).is_some_and(|p| p.class == burngfx::gamdat::CharClass::Mercenary)).expect("a mercenary");
    let record = people.get(who).unwrap().record;
    let home = people.get(who).unwrap().location;
    let elsewhere = if home == 3 { 4 } else { 3 };
    let mut c = one_with(9, &[1, 3]);
    c.origin = Some(record);
    c.experience = 51;
    assert!(people.dismissed(record, elsewhere, &c));
    let line = people.standing().find(|s| s.record == record).expect("written down");
    assert_eq!(line.location, elsewhere);
    assert_eq!(line.pack.len(), 2);
    assert_eq!(line.experience, 51);
    let mut again = People::from_records(&gam.characters, &t, &Default::default());
    assert!(again.restore_standing(line));
    let p = again.get_by_record(record).unwrap();
    assert_eq!((p.location, p.pack.len(), p.experience, p.employer, p.hired), (elsewhere, 2, 51, None, false));
    assert!(again.at(elsewhere).any(|i| i == who), "standing there to be hired again");
}

/// In a legacy game, v0.9.1's `JoinCamp` gives a camper a
/// `PlayerControlledMind`: he stands where he was left. (A v1.2 camper
/// roams — `aros-game`'s `v12_motion` tests.)
#[test]
fn a_camper_stands_where_he_was_left() {
    use burngfx::hire::People;
    let Ok(bytes) = std::fs::read("testdata/burntime.pak") else { return };
    let Ok(pak) = burngfx::pak::ClassicPak::open(&bytes) else { return };
    let Some(raw) = pak.get("burn_gfx/gam.dat") else { return };
    let Ok(gam) = burngfx::gamdat::parse(raw) else { return };
    let mut people = People::from_records(&gam.characters, &types(), &Default::default());
    let who = (0..people.len())
        .find(|&i| people.get(i).is_some_and(|p| p.class == burngfx::gamdat::CharClass::Mercenary))
        .expect("a mercenary");
    let (record, home) = (people.get(who).unwrap().record, people.get(who).unwrap().location);
    let spot = people.get(who).unwrap().at;
    assert!(people.guarding(record, 1, home, spot));
    for step in 0..2000u64 {
        people.think(home, &mut burngfx::mind::Draws::Own(7 + step), None, &|_| true);
        let p = people.get(who).unwrap();
        assert_eq!((p.at, p.move_to), (spot, spot), "a camper does not wander");
    }
}

#[test]
fn a_selected_companion_waters_those_near_him() {
    use burngfx::game::{group_drink_around, group_drink_near, CharacterNeeds};
    let thirsty = || CharacterNeeds { water: 0, ..CharacterNeeds::default() };
    let mut near = one_with(1, &[]);
    near.needs = thirsty();
    near.at = Point::new(300, 300);
    let mut far = one_with(2, &[]);
    far.needs = thirsty();
    far.at = Point::new(40, 50);
    let (mut leader, leader_at) = (thirsty(), Point::new(40, 50));
    // Measured from the selected man at (300, 300): he and nobody else.
    let mut group = vec![near.clone(), far.clone()];
    let left = group_drink_around(Point::new(300, 300), &mut leader, leader_at, &mut group, 100);
    assert_eq!((leader.water, group[0].needs.water, group[1].needs.water), (0, group[0].needs.max_water, 0));
    assert!(left < 100);
    // Measured from the leader it is exactly the old rule.
    let (mut l1, mut l2) = (thirsty(), thirsty());
    let (mut g1, mut g2) = (vec![near.clone(), far.clone()], vec![near, far]);
    let a = group_drink_around(leader_at, &mut l1, leader_at, &mut g1, 7);
    let b = group_drink_near(&mut l2, leader_at, &mut g2, 7);
    assert_eq!((a, l1, g1), (b, l2, g2));
}
