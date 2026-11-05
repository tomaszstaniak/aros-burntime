//! The pub and the restaurant: worth on the table, shared by the group.

use burngfx::diner::{check, serve, worth, Refusal, Service, Share, TABLE};
use burngfx::game::{CharacterNeeds, Companion, ItemBag, Point, MAX_FOOD, MAX_WATER, PACK_CAPACITY};
use burngfx::items::ItemTypes;
use burngfx::trade::Prices;

/// Shaped like the real tables: a snake is sprite 2 at 21 (5.25), a bottle
/// 1.5, a rag exactly 4, a stone nothing. The dish has no sprite and says
/// `value=24` itself, the way Extended's does.
fn types() -> ItemTypes {
    ItemTypes::parse(
        "[item_snake]\nclass=food\nburngfx=2\nfood=7\n\
         [item_bottle]\nclass=bottle\nburngfx=20\n\
         [item_rag]\nclass=material\nburngfx=31\n\
         [item_stone]\nclass=useless\nburngfx=30\n\
         [item_full_canteen]\nclass=water\nburngfx=21\nwater=4\nempty=item_empty_canteen\n\
         [item_empty_canteen]\nclass=bottle\nburngfx=22\n\
         [item_dish]\nclass=useless rare\nimage=gfx/dish.png\nvalue=24\n",
    )
}

fn prices() -> Prices {
    Prices::parse("2 = 21\n20 = 6\n21 = 20\n22 = 8\n30 = 0\n31 = 16\n")
}

fn kind(t: &ItemTypes, name: &str) -> usize {
    t.index_of(name).unwrap_or_else(|| panic!("{name}"))
}

fn pack(t: &ItemTypes, names: &[&str]) -> ItemBag {
    let kinds: Vec<usize> = names.iter().map(|n| kind(t, n)).collect();
    ItemBag::with(PACK_CAPACITY, &kinds)
}

fn needs(food: i32, water: i32) -> CharacterNeeds {
    CharacterNeeds { food, max_food: MAX_FOOD, water, max_water: MAX_WATER, health: 100 }
}

const LEADER_AT: Point = Point { x: 100, y: 100 };

fn member(id: usize, food: i32, water: i32, dx: i32, pack: ItemBag) -> Companion {
    Companion { needs: needs(food, water), at: Point::new(100 + dx, 100), pack, ..Companion::new(id) }
}

// -- worth ---------------------------------------------------------------------

/// **Worth is barter worth, summed in fractions and cut once.** Two bottles
/// are 3, a bottle and a snake 6.75 → 6; the snake's `food=7` counts for
/// nothing here.
#[test]
fn the_table_is_worth_what_a_trader_would_give() {
    let (t, p) = (types(), prices());
    let (b, s) = (kind(&t, "item_bottle"), kind(&t, "item_snake"));
    assert_eq!(worth(&[b, b], &t, &p), 3);
    assert_eq!(worth(&[b, s], &t, &p), 6);
    assert_eq!(worth(&[s], &t, &p), 5, "5.25, and the quarter is not served");
}

/// Extended's added items carry `value=` and no sprite. Read as zero, a dish
/// would buy nothing — and would sell to a trader for nothing too.
#[test]
fn an_item_with_its_own_value_is_worth_that() {
    let (t, p) = (types(), prices());
    assert_eq!(worth(&[kind(&t, "item_dish")], &t, &p), 6, "24 / 4");
    assert_eq!(burngfx::trade::value_of(&[kind(&t, "item_dish")], &t, &p), 6);
}

// -- limited supply: order, reach, remainder -------------------------------------

/// **Too little for everyone, one member out of reach.** Four points to a
/// group at food 7, 3, 4 and — eighty away — 0.
///
/// `Group.Eat` goes a level at a time and, inside a level, in group order.
/// Nobody near is below 3, so the first point is companion 1's (3→4); at
/// level 4 companion 1 again, then companion 2; at level 5 companion 1 takes
/// the last. **Order decides who ends a point ahead**: 6 against 5, though
/// they started one apart the other way. The one at eighty, the hungriest of
/// all, gets nothing and is named as out of reach.
#[test]
fn a_short_meal_goes_level_by_level_in_group_order_and_skips_the_far_one() {
    let (t, p) = (types(), prices());
    let mut leader = needs(7, 5);
    let mut carried = pack(&t, &["item_bottle", "item_rag", "item_snake"]);
    let mut group = vec![
        member(1, 3, 5, 20, ItemBag::new(PACK_CAPACITY)),
        member(2, 4, 5, 30, ItemBag::new(PACK_CAPACITY)),
        member(3, 0, 5, 80, pack(&t, &["item_dish"])),
    ];

    let got = serve(Service::Restaurant, &mut leader, LEADER_AT, &mut carried, &mut group,
                    &[(0, 1)], &t, &p)
        .expect("a rag is worth four");

    assert_eq!((got.worth, got.taken, got.wasted), (4, 4, 0));
    assert_eq!(
        got.shares,
        vec![
            Share { who: 0, from: 7, to: 7, out_of_reach: None },
            Share { who: 1, from: 3, to: 6, out_of_reach: None },
            Share { who: 2, from: 4, to: 5, out_of_reach: None },
            Share { who: 3, from: 0, to: 0, out_of_reach: Some(80) },
        ]
    );
    assert_eq!((leader.food, group[0].needs.food, group[1].needs.food, group[2].needs.food), (7, 6, 5, 0));
    // Only the rag went; the pack closes up around it and the far member's
    // dish is untouched.
    let left: Vec<usize> = carried.iter().collect();
    assert_eq!(left, vec![kind(&t, "item_bottle"), kind(&t, "item_snake")]);
    assert_eq!(group[2].pack.get(0), Some(kind(&t, "item_dish")));
    // A pub serves water and leaves food alone.
    assert!(group.iter().all(|c| c.needs.water == 5));
}

/// **Part full: the surplus is lost, not returned.** Five points of drink to
/// a leader one short and a companion already full.
#[test]
fn what_the_group_has_no_room_for_is_wasted() {
    let (t, p) = (types(), prices());
    let mut leader = needs(9, 4);
    let mut carried = pack(&t, &["item_snake"]);
    let mut group = vec![member(1, 9, 5, 10, ItemBag::new(PACK_CAPACITY))];

    let got = serve(Service::Pub, &mut leader, LEADER_AT, &mut carried, &mut group, &[(0, 0)], &t, &p)
        .expect("worth five");
    assert_eq!((got.worth, got.taken, got.wasted), (5, 1, 4));
    assert_eq!(leader.water, 5);
    assert!(carried.is_empty(), "the snake is gone, all of it");
}

/// **Fully fed: served, and nothing taken.** Upstream clears the table on
/// every path, so the price is paid; the panel's job is to say so first, and
/// the check does.
#[test]
fn a_full_group_pays_and_gets_nothing_and_is_told_beforehand() {
    let (t, p) = (types(), prices());
    let mut leader = needs(9, 5);
    let mut carried = pack(&t, &["item_rag"]);
    let mut group = vec![member(1, 9, 5, 10, ItemBag::new(PACK_CAPACITY))];

    let told = check(Service::Restaurant, &leader, LEADER_AT, &carried, &group, &[(0, 0)], &t, &p).unwrap();
    assert_eq!((told.taken, told.wasted), (0, 4));

    let got = serve(Service::Restaurant, &mut leader, LEADER_AT, &mut carried, &mut group, &[(0, 0)], &t, &p).unwrap();
    assert_eq!(got, told, "what was shown is what happened");
    assert!(carried.is_empty());
    assert_eq!((leader.food, group[0].needs.food), (9, 9));
}

/// A full canteen on a pub's table goes whole: the table takes the item, not
/// its water, and `empty=` is not applied.
#[test]
fn a_full_canteen_is_taken_not_emptied() {
    let (t, p) = (types(), prices());
    let mut leader = needs(9, 0);
    let mut carried = pack(&t, &["item_full_canteen"]);
    let got = serve(Service::Pub, &mut leader, LEADER_AT, &mut carried, &mut [], &[(0, 0)], &t, &p).unwrap();
    assert_eq!(got.worth, 5, "its worth, 20 / 4 — not its water=4");
    assert_eq!(leader.water, 5);
    assert!(carried.is_empty(), "no empty canteen is left behind");
}

/// Companions within reach can pay, from their own packs, together with the
/// leader.
#[test]
fn a_near_companion_can_put_their_things_on_the_table() {
    let (t, p) = (types(), prices());
    let mut leader = needs(0, 5);
    let mut carried = pack(&t, &["item_bottle"]);
    let mut group = vec![member(1, 0, 5, 49, pack(&t, &["item_stone", "item_bottle"]))];
    let got = serve(Service::Restaurant, &mut leader, LEADER_AT, &mut carried, &mut group,
                    &[(0, 0), (1, 1)], &t, &p)
        .unwrap();
    assert_eq!(got.worth, 3);
    assert_eq!((leader.food, group[0].needs.food), (2, 1), "leader first at each level");
    assert!(carried.is_empty());
    assert_eq!(group[0].pack.iter().collect::<Vec<_>>(), vec![kind(&t, "item_stone")]);
}

// -- refusals change nothing ---------------------------------------------------------

fn untouched(before: &(CharacterNeeds, ItemBag, Vec<Companion>), after: &(CharacterNeeds, ItemBag, Vec<Companion>)) {
    assert_eq!(before.0, after.0);
    assert_eq!(before.1.iter().collect::<Vec<_>>(), after.1.iter().collect::<Vec<_>>());
    assert_eq!(before.2, after.2);
}

/// **A companion at fifty has no page, so nothing of theirs can pay.**
#[test]
fn a_far_companions_pack_cannot_pay() {
    let (t, p) = (types(), prices());
    let mut state = (needs(0, 0), pack(&t, &["item_bottle"]),
                     vec![member(1, 0, 0, 50, pack(&t, &["item_rag"]))]);
    let before = state.clone();
    let got = serve(Service::Pub, &mut state.0, LEADER_AT, &mut state.1, &mut state.2,
                    &[(0, 0), (1, 0)], &t, &p);
    assert_eq!(got, Err(Refusal::OutOfReach { companion: 1, distance: 50 }));
    untouched(&before, &state);
}

/// "Does this look like a soup kitchen?" — and nothing is taken, which is
/// where we part from upstream on purpose.
#[test]
fn a_table_worth_nothing_is_refused_and_keeps_its_things() {
    let (t, p) = (types(), prices());
    let mut state = (needs(0, 0), pack(&t, &["item_stone"]), Vec::new());
    let before = state.clone();
    let got = serve(Service::Restaurant, &mut state.0, LEADER_AT, &mut state.1, &mut state.2, &[(0, 0)], &t, &p);
    assert_eq!(got, Err(Refusal::WorthNothing));
    untouched(&before, &state);
}

/// Serving re-checks: a pick that no longer points at anything is refused,
/// not served from whatever now sits in that slot.
#[test]
fn a_stale_pick_is_refused_at_the_moment_of_serving() {
    let (t, p) = (types(), prices());
    let mut state = (needs(0, 0), pack(&t, &["item_rag"]), Vec::new());
    check(Service::Pub, &state.0, LEADER_AT, &state.1, &state.2, &[(0, 0)], &t, &p).expect("fine now");
    state.1.remove(0);
    let before = state.clone();
    let got = serve(Service::Pub, &mut state.0, LEADER_AT, &mut state.1, &mut state.2, &[(0, 0)], &t, &p);
    assert_eq!(got, Err(Refusal::NoSuchSlot));
    untouched(&before, &state);
}

#[test]
fn the_table_holds_four_and_one_thing_once() {
    let (t, p) = (types(), prices());
    let carried = pack(&t, &["item_rag"; 5]);
    let five: Vec<(usize, usize)> = (0..5).map(|s| (0, s)).collect();
    assert_eq!(TABLE, 4);
    assert_eq!(check(Service::Pub, &needs(0, 0), LEADER_AT, &carried, &[], &five, &t, &p),
               Err(Refusal::TooMuchOnTheTable));
    assert_eq!(check(Service::Pub, &needs(0, 0), LEADER_AT, &carried, &[], &[(0, 1), (0, 1)], &t, &p),
               Err(Refusal::SlotNamedTwice));
    assert_eq!(check(Service::Pub, &needs(0, 0), LEADER_AT, &carried, &[], &[], &t, &p),
               Err(Refusal::NothingOnTheTable));
}

#[test]
fn each_door_has_its_own_picture() {
    assert_eq!(Service::Pub.background(14), "bar");
    assert_eq!(Service::Pub.background(23), "pub1");
    assert_eq!(Service::Restaurant.background(22), "wirt");
    assert_eq!(Service::Restaurant.background(24), "koch");
}

// -- v1.2: the table takes, and serves whatever it is worth -------------------

/// **A worthless table is served, and the things on it are gone.** v1.2's
/// `ServiceScene.CommitOffer` (`Scenes/ServiceScene.cs:224-233`) serves
/// `CalculatePubValue(offer)` — 0 here — and `OnButtonAction` clears the
/// table on every path (`:318-326`). Nobody's water changes, nothing is
/// refused, and the stone is not given back.
#[test]
fn v12_a_worthless_table_is_served_and_the_items_are_gone() {
    use burngfx::diner::serve_table;
    let (t, p) = (types(), prices());
    let mut table = pack(&t, &["item_stone"]);
    let (mut leader, mut group) = (needs(3, 1), vec![member(1, 3, 1, 10, pack(&t, &[]))]);

    let got = serve_table(Service::Pub, &mut leader, LEADER_AT, &mut group, Some(LEADER_AT), &mut table, &t, &p);

    assert_eq!((got.worth, got.taken, got.wasted), (0, 0, 0));
    assert!(table.is_empty(), "the stone is gone");
    assert_eq!((leader.water, group[0].needs.water), (1, 1), "nobody drank");
}

/// **Served from the selected character** (`serviceLeader =
/// SelectedCharacter`, `ServiceScene.cs:137, 227, 232`): in single mode with
/// the companion selected, the leader eighty away is passed over and the
/// companion drinks.
#[test]
fn v12_the_table_is_shared_from_the_selected_character() {
    use burngfx::diner::serve_table;
    let (t, p) = (types(), prices());
    let mut table = pack(&t, &["item_rag"]); // 4
    let (mut leader, mut group) = (needs(3, 1), vec![member(1, 3, 1, 80, pack(&t, &[]))]);
    let selected = group[0].at;

    let got = serve_table(Service::Pub, &mut leader, LEADER_AT, &mut group, Some(selected), &mut table, &t, &p);

    assert_eq!(leader.water, 1, "the leader, eighty off, gets nothing");
    assert_eq!(group[0].needs.water, MAX_WATER, "the selected companion drinks his fill");
    assert_eq!((got.worth, got.taken, got.wasted), (4, 4, 0));
    assert_eq!(got.shares[0].out_of_reach, Some(80), "and the leader is named as out of reach");
    assert!(table.is_empty());
}

/// **In group mode nobody is out of reach** (`Group.IgnoreRangeFilter`):
/// the companion eighty away is served too.
#[test]
fn v12_in_group_mode_the_table_serves_everyone() {
    use burngfx::diner::serve_table;
    let (t, p) = (types(), prices());
    let mut table = pack(&t, &["item_rag"]); // 4
    let (mut leader, mut group) = (needs(3, 1), vec![member(1, 3, 1, 80, pack(&t, &[]))]);

    let got = serve_table(Service::Pub, &mut leader, LEADER_AT, &mut group, None, &mut table, &t, &p);

    assert_eq!((leader.water, group[0].needs.water), (3, 3), "a level at a time, both");
    assert!(got.shares.iter().all(|s| s.out_of_reach.is_none()));
}

/// **An empty table is served too**: `lastAmount = 0`, the `+6` line, and
/// nothing changes (`ServiceScene.cs:224-233, 244-245`).
#[test]
fn v12_an_empty_table_serves_nothing() {
    use burngfx::diner::serve_table;
    let (t, p) = (types(), prices());
    let mut table = ItemBag::new(TABLE);
    let (mut leader, mut group) = (needs(3, 1), Vec::new());
    let got = serve_table(Service::Restaurant, &mut leader, LEADER_AT, &mut group, None, &mut table, &t, &p);
    assert_eq!((got.worth, leader.food), (0, 3));
}
