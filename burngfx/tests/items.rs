//! Items: what kinds there are, and moving one between a room and a pack.
//!
//! The rule this file exists to pin is that a move is all or nothing. A
//! half-done one either loses the item or makes a second, and neither is
//! visible in a screenshot — the room simply has one fewer thing in it than
//! the player remembers, several minutes later.

use burngfx::game::{move_item, ItemBag, MoveError, RoomItems, PACK_CAPACITY};
use burngfx::items::ItemTypes;
use burngfx::matmap::RoomType;
use std::path::PathBuf;

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

fn shipped() -> Option<ItemTypes> {
    let text = std::fs::read_to_string(testdata("items_original.txt")).ok()?;
    Some(ItemTypes::parse(&text))
}

// -- the type table ---------------------------------------------------------

#[test]
fn a_section_becomes_a_type_and_a_stray_line_does_not() {
    // The real file opens with a bare `image=` line belonging to no section.
    let types = ItemTypes::parse(
        "image=gfx/dummy.png\n\
         \n\
         [item_snake]\n\
         class=food\n\
         burngfx=2\n\
         food=7\n\
         \n\
         [item_new_thing]\n\
         class=useless\n\
         \n\
         [not_an_item]\n\
         class=food\n\
         burngfx=9\n",
    );

    assert_eq!(types.len(), 2, "only [item_*] sections are types");

    let snake = types.get(types.index_of("item_snake").unwrap()).unwrap();
    assert!(snake.is_class("food"));
    assert_eq!(snake.sprite, Some(2));

    // A class line is a list. `class=special rare` is two of them, and the
    // spawn filters test them one at a time — `-rare` throws out an item
    // `special` would have kept.
    let two = ItemTypes::parse("[item_gadget]\nclass=special rare\n");
    let gadget = two.get(0).unwrap();
    assert_eq!(gadget.class, ["special", "rare"]);
    assert!(gadget.is_class("special") && gadget.is_class("rare"));
    assert!(!gadget.is_class("food"));

    // An item the remaster added has no frame in the 1993 archive, so it
    // cannot be drawn from it — which is different from frame 0.
    let new = types.get(types.index_of("item_new_thing").unwrap()).unwrap();
    assert_eq!(new.sprite, None);
    assert!(new.name_ref().is_none(), "and it has no classic name either");

    assert_eq!(types.index_of("item_nothing_like_it"), None);
}

/// The three things an item's `burngfx` number is used for are three offsets
/// into one string table, and `ItemTypesData` writes them out as
/// `gst.raw?N`, `burn?50+N` and `burn?110+N`.
#[test]
fn the_sprite_number_addresses_the_name_and_the_description() {
    let types = ItemTypes::parse("[item_snake]\nburngfx=2\n");
    let snake = types.get(0).unwrap();
    let name = snake.name_ref().unwrap();
    let text = snake.description_ref().unwrap();
    assert_eq!((name.database, name.index), ("burn", 52));
    assert_eq!((text.database, text.index), ("burn", 112));
}

/// The shipped file, so a change to it is noticed here rather than in a room.
#[test]
fn the_shipped_items_parse() {
    let Some(types) = shipped() else { return };
    assert!(types.len() > 40, "only {} item types", types.len());

    // Every item the 1993 game had draws from gst.raw, which has 58 frames.
    for t in types.iter() {
        if let Some(n) = t.sprite {
            assert!(n < 58, "{}: sprite {n} is past the end of gst.raw", t.name);
        }
        assert!(t.name.starts_with("item_"));
    }

    // The item this project's demo puts on a floor has to be a real one.
    let snake = types.index_of("item_snake").expect("item_snake is in the file");
    assert!(types.get(snake).unwrap().is_class("food"));
    assert_eq!(types.get(snake).unwrap().sprite, Some(2));
}

/// `nodrop` is a generation filter and nothing else.
///
/// The settings read `random_items_room = food water ... -nodrop`, which means
/// "never spawn one at random". Cooked meat and a filled canteen are the
/// classic examples: you can only get one by making it, and once you have it
/// you may put it down like anything else. There is no code here to refuse
/// that, and there is none in the original either — `InventoryScene` enables a
/// transfer on `Count < MaxCount` and on nothing else.
#[test]
fn a_nodrop_item_can_still_be_put_down() {
    let Some(types) = shipped() else { return };
    let meat = types.index_of("item_meat").expect("item_meat is in the file");
    assert!(types.get(meat).unwrap().is_class("nodrop"), "the premise of this test");

    let mut pack = ItemBag::with(PACK_CAPACITY, &[meat]);
    let mut floor = ItemBag::new(32);
    assert_eq!(move_item(&mut pack, 0, &mut floor), Ok(meat));
    assert_eq!(floor.len(), 1);
}

// -- moving one -------------------------------------------------------------

#[test]
fn an_item_moves_out_of_the_slot_it_was_named_by() {
    // Two of a kind: one type, two instances, two slots. Taking slot 1 must
    // take that one and leave the other.
    let mut floor = ItemBag::with(32, &[7, 7, 9]);
    let mut pack = ItemBag::new(PACK_CAPACITY);

    assert_eq!(move_item(&mut floor, 1, &mut pack), Ok(7));
    assert_eq!(floor.iter().collect::<Vec<_>>(), [7, 9], "the slots close up");
    assert_eq!(pack.iter().collect::<Vec<_>>(), [7]);

    // And back the other way, into the room.
    assert_eq!(move_item(&mut pack, 0, &mut floor), Ok(7));
    assert_eq!(pack.len(), 0);
    assert_eq!(floor.iter().collect::<Vec<_>>(), [7, 9, 7]);
}

/// A refused move changes nothing at either end. This is the whole point.
#[test]
fn a_refused_move_leaves_both_sides_exactly_as_they_were() {
    let full_pack = ItemBag::with(PACK_CAPACITY, &[1, 2, 3, 4, 5, 6]);
    assert!(full_pack.is_full(), "six is the pack, and the premise here");

    // Destination full.
    let mut floor = ItemBag::with(32, &[9]);
    let mut pack = full_pack.clone();
    assert_eq!(move_item(&mut floor, 0, &mut pack), Err(MoveError::DestinationFull));
    assert_eq!(floor.iter().collect::<Vec<_>>(), [9], "the item stays on the floor");
    assert_eq!(pack, full_pack, "and nothing in the pack moved");

    // Nothing in that slot: one past the end, and an empty bag.
    let mut floor = ItemBag::with(32, &[9]);
    let mut pack = ItemBag::new(PACK_CAPACITY);
    assert_eq!(move_item(&mut floor, 1, &mut pack), Err(MoveError::NoSuchSlot));
    assert_eq!(move_item(&mut pack, 0, &mut floor), Err(MoveError::NoSuchSlot));
    assert_eq!(floor.iter().collect::<Vec<_>>(), [9]);
    assert!(pack.is_empty());

    // A room that takes nothing — a doctor or a pub, capacity 0 — refuses
    // rather than growing a floor it does not have.
    let mut nowhere = ItemBag::new(RoomType::Doctor.item_capacity());
    let mut pack = ItemBag::with(PACK_CAPACITY, &[9]);
    assert_eq!(move_item(&mut pack, 0, &mut nowhere), Err(MoveError::DestinationFull));
    assert_eq!(pack.len(), 1);
    assert!(nowhere.is_empty());
}

/// The capacities come from the original, and this is where they are written
/// down: 6 in a person's pack, 32 on an ordinary floor, 8 at a water source.
#[test]
fn the_capacities_are_the_ones_the_original_uses() {
    assert_eq!(PACK_CAPACITY, 6);
    assert_eq!(RoomType::Normal.item_capacity(), 32);
    assert_eq!(RoomType::Rope.item_capacity(), 32);
    assert_eq!(RoomType::WaterSource.item_capacity(), 8);
    for kind in [RoomType::Doctor, RoomType::Pub, RoomType::Trader, RoomType::Scene] {
        assert_eq!(kind.item_capacity(), 0, "{kind:?} has no floor");
    }

    // A bag never grows past its capacity, however it is filled.
    let bag = ItemBag::with(2, &[1, 2, 3, 4]);
    assert_eq!(bag.len(), 2);
    let mut bag = ItemBag::new(1);
    assert!(bag.push(5));
    assert!(!bag.push(6));
    assert_eq!(bag.len(), 1);
}

// -- across a visit ---------------------------------------------------------

/// The flow, end to end: take it, leave, come back to an empty floor, put it
/// back, leave, come back to exactly one.
///
/// "Leaving" is what the game does — it drops the room and rebuilds it on the
/// next visit — so the test asks the store for the room afresh each time,
/// with the same stocking function it would use on a first visit. If that
/// function ran again the floor would refill and the count would be wrong.
#[test]
fn one_item_survives_being_taken_out_and_put_back() {
    const LOCATION: usize = 5;
    const ROOM: usize = 0;
    const SNAKE: usize = 3;

    let mut rooms = RoomItems::new();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let stock = || ItemBag::with(RoomType::Normal.item_capacity(), &[SNAKE]);

    // First visit: one snake on the floor, and it goes in the pack.
    let floor = rooms.get_or_insert(LOCATION, ROOM, stock);
    assert_eq!(floor.len(), 1);
    assert_eq!(move_item(floor, 0, &mut pack), Ok(SNAKE));

    // Leave, come back. The floor is empty and stays empty: the room was
    // stocked once, when the game reached it, not once per visit.
    let floor = rooms.get_or_insert(LOCATION, ROOM, stock);
    assert!(floor.is_empty(), "the room restocked itself");
    assert_eq!(pack.iter().collect::<Vec<_>>(), [SNAKE]);

    // Put it back, leave, come back. Exactly one — not none, not two.
    assert_eq!(move_item(&mut pack, 0, floor), Ok(SNAKE));
    let floor = rooms.get_or_insert(LOCATION, ROOM, stock);
    assert_eq!(floor.iter().collect::<Vec<_>>(), [SNAKE]);
    assert!(pack.is_empty());

    // A different room of the same location is a different floor, and an
    // untouched one is absent rather than empty.
    assert!(rooms.get(LOCATION, ROOM + 1).is_none());
    assert!(rooms.get(LOCATION + 1, ROOM).is_none());
    assert_eq!(rooms.len(), 1);
}

/// Nothing is created or destroyed by any sequence of moves.
#[test]
fn moving_items_about_never_changes_how_many_there_are() {
    let mut floor = ItemBag::with(8, &[1, 2, 3, 4, 5, 6, 7, 8]);
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut all: Vec<usize> = floor.iter().chain(pack.iter()).collect();
    all.sort_unstable();

    // Take until the pack is full, put back until it is empty, twice over.
    for _ in 0..2 {
        while move_item(&mut floor, 0, &mut pack).is_ok() {}
        assert!(pack.is_full());
        while move_item(&mut pack, 0, &mut floor).is_ok() {}
        assert!(pack.is_empty());
    }

    let mut after: Vec<usize> = floor.iter().chain(pack.iter()).collect();
    after.sort_unstable();
    assert_eq!(after, all, "items were lost or duplicated");
}

// -- eating one -------------------------------------------------------------

use burngfx::game::{eaten, CharacterNeeds, MAX_FOOD};

/// A snake is `food=7 heal=28`, and those are two different uses of it.
///
/// `InventoryScene` reads `FoodValue` when someone eats; `HealValue` is read
/// in `DoctorScene`, where a doctor treats you in exchange for the item. So
/// eating restores no health at all, and this test says so about the real
/// numbers rather than about a fixture.
#[test]
fn food_and_heal_are_two_different_uses_of_the_same_item() {
    let Some(types) = shipped() else { return };
    let snake = types.get(types.index_of("item_snake").unwrap()).unwrap();
    assert_eq!(snake.food, 7);
    assert_eq!(snake.heal, 28, "what a doctor charges, not what a meal gives");

    // And something that is not food at all has no food value, which is the
    // test `InventoryScene` makes before it offers to eat one.
    let bottle = types.get(types.index_of("item_bottle").unwrap()).unwrap();
    assert_eq!(bottle.food, 0);
}

/// A hungry character eats the lot, and the item goes.
#[test]
fn a_hungry_character_eats_the_whole_snake() {
    let mut needs = CharacterNeeds::parched_for_demonstration();
    assert_eq!(needs.food, 0);

    let left = needs.eat(7);
    assert_eq!(left, 0, "all seven went in");
    assert_eq!(needs.food, 7);
    assert!(eaten(7, left), "so the snake is gone");
}

/// One point of room, and the whole snake is still used up. The remaining six
/// are not kept as anything — there is no partial item in this game.
#[test]
fn a_nearly_full_character_still_uses_the_whole_snake() {
    let mut needs = CharacterNeeds { food: 8, max_food: MAX_FOOD, ..Default::default() };

    let left = needs.eat(7);
    assert_eq!(needs.food, 9, "filled to the top and no further");
    assert_eq!(left, 6, "six wasted");
    assert!(eaten(7, left), "and the snake is gone all the same");
}

/// A character with no appetite eats nothing and keeps the item.
#[test]
fn a_full_character_eats_nothing_and_keeps_the_snake() {
    let mut needs = CharacterNeeds { food: MAX_FOOD, max_food: MAX_FOOD, ..Default::default() };

    let left = needs.eat(7);
    assert_eq!(needs.food, MAX_FOOD, "unchanged");
    assert_eq!(left, 7, "none of it was eaten");
    assert!(!eaten(7, left), "so the item stays");
}

/// Eating moves food and nothing else.
///
/// There is no health in this model yet, so a test that health is unchanged
/// would be vacuous. What can be said, and is worth saying, is that eating
/// touches exactly one number: compare the whole of the character before and
/// after, so water and the capacities cannot drift either.
#[test]
fn eating_changes_food_and_nothing_else() {
    for (start, value) in [(0, 7), (8, 7), (MAX_FOOD, 7), (3, 0)] {
        let before = CharacterNeeds { food: start, max_food: MAX_FOOD, ..Default::default() };
        let mut after = before;
        after.eat(value);

        assert_eq!(after.water, before.water, "water moved");
        assert_eq!(after.max_water, before.max_water);
        assert_eq!(after.max_food, before.max_food);
        assert!(after.food >= before.food && after.food <= MAX_FOOD);
        assert_eq!(CharacterNeeds { food: before.food, ..after }, before, "something else moved");
    }
}

/// The whole slice: a hungry character in a room with a snake on the floor.
///
/// Take it, eat it, and it is gone from the pack; leave the room, come back,
/// and neither the floor nor the pack has produced another one. That last
/// step is the one that catches a room being restocked on entry, which would
/// make food infinite.
#[test]
fn an_eaten_snake_does_not_come_back() {
    const LOCATION: usize = 4;
    const ROOM: usize = 0;
    const SNAKE: usize = 3;

    let mut rooms = RoomItems::new();
    let mut pack = ItemBag::new(PACK_CAPACITY);
    let mut needs = CharacterNeeds { food: 0, max_food: MAX_FOOD, ..Default::default() };
    let stock = || ItemBag::with(RoomType::Normal.item_capacity(), &[SNAKE]);

    let floor = rooms.get_or_insert(LOCATION, ROOM, stock);
    assert_eq!(move_item(floor, 0, &mut pack), Ok(SNAKE));

    // Eat it: something was eaten, so the item goes.
    let left = needs.eat(7);
    assert!(eaten(7, left));
    assert_eq!(pack.remove(0), Some(SNAKE));
    assert_eq!(needs.food, 7);

    // Leave and come back, twice. Nothing reappears anywhere.
    for _ in 0..2 {
        let floor = rooms.get_or_insert(LOCATION, ROOM, stock);
        assert!(floor.is_empty(), "the floor grew a second snake");
        assert!(pack.is_empty(), "the pack grew one");
    }
    assert_eq!(needs.food, 7, "and the meal stayed eaten");
}

/// Eating from an empty slot, or eating something that is not food, does
/// nothing at all rather than consuming a slot or a point.
#[test]
fn there_is_nothing_to_eat_in_an_empty_slot() {
    let Some(types) = shipped() else { return };
    let bottle = types.index_of("item_bottle").unwrap();

    let mut needs = CharacterNeeds { food: 0, max_food: MAX_FOOD, ..Default::default() };
    let mut pack = ItemBag::with(PACK_CAPACITY, &[bottle]);

    // Not food: the caller checks `food != 0` and offers nothing, so the
    // model is never asked. Asking it with zero must still be harmless.
    let left = needs.eat(types.get(bottle).unwrap().food);
    assert_eq!(left, 0);
    assert!(!eaten(0, left), "nothing was eaten, so nothing is used up");
    assert_eq!(needs.food, 0);
    assert_eq!(pack.len(), 1);

    assert_eq!(pack.remove(1), None, "no such slot");
    assert_eq!(pack.len(), 1);
}
