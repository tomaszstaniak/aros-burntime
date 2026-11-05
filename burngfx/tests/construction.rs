//! Building an item out of parts, against v0.9.1's own `construction.txt`.
//!
//! The file is the remaster's, not the 1993 game's data, so it sits in
//! `testdata` like `production.txt` and `items_original.txt` do.

use burngfx::construction::{Constructions, From};
use std::path::PathBuf;

fn table() -> Option<Constructions> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join("construction.txt");
    Some(Constructions::parse(&std::fs::read_to_string(path).ok()?))
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_classic_file_has_eight_recipes() {
    let Some(table) = table() else { return };
    assert_eq!(table.len(), 8);
    assert_eq!(table.default_dialog, 503);
    let made: Vec<&str> = table.iter().map(|r| r.result.as_str()).collect();
    assert_eq!(
        made,
        [
            "item_rat_trap",
            "item_trap",
            "item_hand_pump",
            "item_industrial_pump",
            "item_two_way_radio",
            "item_protective_suit",
            "item_loaded_rifle",
            "item_loaded_pistol",
        ]
    );
}

/// The one thing in this file that looks like a typo and is the release's
/// behaviour: the radio's main item is written `dialog_items=`, which nothing
/// reads, so no right click reaches the recipe. `v1.1` writes `main_items=`.
#[test]
fn the_two_way_radio_cannot_be_built_at_v091() {
    let Some(table) = table() else { return };
    let radio = table.iter().find(|r| r.result == "item_two_way_radio").unwrap();
    assert!(radio.main_items.is_empty());

    let carried = names(&[
        "item_defective_two_way_radio",
        "item_electrical_odds_and_ends",
        "item_batteries",
    ]);
    assert_eq!(table.find("technician", "item_defective_two_way_radio", &carried, &[]), None);
}

#[test]
fn only_a_technician_builds_a_pump_and_anybody_reloads_a_rifle() {
    let Some(table) = table() else { return };
    let parts = names(&["item_broken_pump", "item_rags", "item_hose"]);
    assert!(table.find("technician", "item_broken_pump", &parts, &[]).is_some());
    assert_eq!(table.find("doctor", "item_broken_pump", &parts, &[]), None);

    let gun = names(&["item_unloaded_rifle", "item_ammunition"]);
    for class in ["mercenary", "boss", "technician", "doctor"] {
        assert!(table.find(class, "item_unloaded_rifle", &gun, &[]).is_some(), "{class}");
    }
}

/// The second collection is the pile you are standing in, not another
/// person's pack — and `CanConstruct` is happy for the parts to be split
/// across the two.
#[test]
fn parts_may_lie_on_the_ground_beside_the_builder() {
    let Some(table) = table() else { return };
    let carried = names(&["item_broken_pump"]);
    let nearby = names(&["item_rags", "item_hose"]);

    let offer = table.find("technician", "item_broken_pump", &carried, &nearby).unwrap();
    assert!(offer.can_build);
    assert_eq!(offer.dialog, 508);

    // Nothing beside them, and the recipe is still offered — with no Yes.
    let offer = table.find("technician", "item_broken_pump", &carried, &[]).unwrap();
    assert!(!offer.can_build);
    assert_eq!(offer.dialog, 508);
}

/// `Construct` empties the builder's own pack first and only then the pile,
/// and it puts the result where the **main item** was.
#[test]
fn the_builder_pays_first_and_keeps_what_they_started_from() {
    let Some(table) = table() else { return };
    let carried = names(&["item_broken_pump", "item_rags"]);
    let nearby = names(&["item_hose"]);
    let offer = table.find("technician", "item_broken_pump", &carried, &nearby).unwrap();

    let build = table.build(&offer, "item_broken_pump", &carried).unwrap();
    assert_eq!(build.result, "item_hand_pump");
    assert_eq!(build.into, From::Carried);
    assert_eq!(
        build.take,
        [
            ("item_broken_pump".to_string(), From::Carried),
            ("item_rags".to_string(), From::Carried),
            ("item_hose".to_string(), From::Nearby),
        ]
    );
}

#[test]
fn a_pump_built_off_the_ground_stays_on_the_ground() {
    let Some(table) = table() else { return };
    let nearby = names(&["item_broken_pump", "item_rags", "item_hose"]);
    let offer = table.find("technician", "item_broken_pump", &[], &nearby).unwrap();

    let build = table.build(&offer, "item_broken_pump", &[]).unwrap();
    assert_eq!(build.into, From::Nearby);
    assert!(build.take.iter().all(|(_, from)| *from == From::Nearby));
}

/// The main item is itself one of `items=`, so it is spent too.
#[test]
fn the_main_item_is_consumed_with_the_rest() {
    let Some(table) = table() else { return };
    let carried = names(&["item_unloaded_rifle", "item_ammunition"]);
    let offer = table.find("boss", "item_unloaded_rifle", &carried, &[]).unwrap();
    let build = table.build(&offer, "item_unloaded_rifle", &carried).unwrap();

    assert!(build.take.iter().any(|(name, _)| name == "item_unloaded_rifle"));
    assert_eq!(build.result, "item_loaded_rifle");
}

/// Four parts, and `items=`'s order is the file's. It decides nothing about
/// the rules and everything about which three the dialog line names.
#[test]
fn the_protective_suit_needs_all_four_pieces() {
    let Some(table) = table() else { return };
    let suit = table.iter().find(|r| r.result == "item_protective_suit").unwrap();
    assert_eq!(
        suit.items,
        names(&["item_gas_mask", "item_gloves", "item_protective_overall", "item_boots"])
    );
    assert_eq!(suit.main_items, names(&["item_protective_overall"]));

    let three = names(&["item_protective_overall", "item_gas_mask", "item_gloves"]);
    assert!(!table.find("technician", "item_protective_overall", &three, &[]).unwrap().can_build);

    let mut four = three.clone();
    four.push("item_boots".to_string());
    assert!(table.find("technician", "item_protective_overall", &four, &[]).unwrap().can_build);
}

// -- the ground: `PickItemList` ------------------------------------------------

use burngfx::game::{pick_items, Point, PICKUP_DISTANCE};
use burngfx::spawn::Dropped;

fn lying(location: usize, x: i32, y: i32, kind: usize) -> Dropped {
    Dropped { location, at: Point::new(x, y), kind, rounds: None, icon: burngfx::spawn::PILE_UNDRAWN }
}

/// A circle of twenty, edge included, around the character — not a square,
/// and not around wherever the thing was clicked.
#[test]
fn in_reach_is_a_circle_of_twenty_with_its_edge() {
    assert_eq!(PICKUP_DISTANCE, 20);
    let ground = vec![
        lying(3, 120, 100, 1), // 20 across: in
        lying(3, 100, 121, 2), // 21 down: out
        lying(3, 114, 114, 3), // 14,14 is 19.8: in
        lying(3, 115, 115, 4), // 15,15 is 21.2: out, though a square of 20 would take it
        lying(4, 100, 100, 5), // right spot, another location
    ];
    assert_eq!(pick_items(&ground, 3, Point::new(100, 100)), vec![0, 2]);
}

/// `Find(type)` walks the list front to back, so of two alike in reach the
/// earlier is the one used.
#[test]
fn the_ground_keeps_its_own_order() {
    let ground = vec![lying(3, 90, 100, 7), lying(3, 300, 300, 7), lying(3, 110, 100, 7)];
    assert_eq!(pick_items(&ground, 3, Point::new(100, 100)), vec![0, 2]);
}

/// With the ammunition just out of reach there is still an answer — the
/// recipe's sentence, without the Yes line.
#[test]
fn a_part_out_of_reach_is_a_refusal_not_silence() {
    let Some(table) = table() else { return };
    let carried = names(&["item_unloaded_rifle"]);
    match table.answer("boss", "item_unloaded_rifle", None, &carried, &[]) {
        burngfx::construction::Answer::Recipe(offer) => {
            assert!(!offer.can_build);
            assert_eq!(offer.dialog, 512);
        }
        other => panic!("{other:?}"),
    }
    match table.answer("boss", "item_unloaded_rifle", None, &carried, &names(&["item_ammunition"])) {
        burngfx::construction::Answer::Recipe(offer) => assert!(offer.can_build),
        other => panic!("{other:?}"),
    }
}

/// Nothing to make: the item's own fluff if it has any, else 503.
#[test]
fn nothing_to_make_still_gets_an_answer() {
    use burngfx::construction::Answer;
    let Some(table) = table() else { return };
    assert_eq!(table.answer("boss", "item_rope", Some("@newburn?33"), &[], &[]), Answer::Fluff("@newburn?33".into()));
    assert_eq!(table.answer("boss", "item_knife", None, &[], &[]), Answer::Default(503));
    // A pump part in a Boss's hands: no recipe for his class, so 503.
    assert_eq!(table.answer("boss", "item_broken_pump", None, &names(&["item_broken_pump", "item_rags", "item_hose"]), &[]), Answer::Default(503));
}

/// **Where the source crashes, the port answers.** From the inventory opened
/// on the world map there is no second pile (`InventoryRoom` and `PickItems`
/// are both null), and `CanConstruct` calls `secondary.Contains` on it as
/// soon as the builder lacks a part — a `NullReferenceException` in v0.9.1
/// and in `ddb88b2`. Here the pile is empty: the recipe's sentence without
/// its Yes line, and nothing is taken.
#[test]
fn with_no_pile_a_missing_part_is_a_refusal_not_a_crash() {
    let Some(table) = table() else { return };
    let carried = names(&["item_unloaded_rifle"]);
    match table.answer("boss", "item_unloaded_rifle", None, &carried, &[]) {
        burngfx::construction::Answer::Recipe(offer) => assert!(!offer.can_build),
        other => panic!("{other:?}"),
    }
    // Everything in the pack: that builds, as it does in the source.
    let both = names(&["item_unloaded_rifle", "item_ammunition"]);
    match table.answer("boss", "item_unloaded_rifle", None, &both, &[]) {
        burngfx::construction::Answer::Recipe(offer) => assert!(offer.can_build),
        other => panic!("{other:?}"),
    }
}

// ---- v1.2 ------------------------------------------------------------------
//
// A v1.2 game reads v1.2's own `resources/game/classic/construction.txt`
// (`GameCreation.cs:114`), and `Constructions.IsAvailable` gates a recipe by
// the preset's item table (`Logic/Data/Constructions.cs:99-121`).

fn v12_file(rel: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join("v1.2").join(rel);
    std::fs::read_to_string(path).ok()
}

/// The table and the preset's items, as a game of that preset sees them.
fn v12_table(items_dir: &str) -> Option<Constructions> {
    let types = burngfx::items::ItemTypes::parse_v12(&v12_file(&format!("rules/{items_dir}/items.txt"))?).ok()?;
    Some(Constructions::parse(&v12_file("construction.txt")?).for_items(&types))
}

/// The radio's recipe is a `main_items` one at v1.2 and needs no function,
/// so the defective radio is repaired in both presets (Classic reads DOS's
/// item table, `rules::RuleSet::files`).
#[test]
fn v12_the_two_way_radio_is_built_in_classic_and_extended() {
    let parts = names(&["item_defective_two_way_radio", "item_electrical_odds_and_ends", "item_batteries"]);
    for dir in ["dos", "extended"] {
        let Some(table) = v12_table(dir) else { return };
        let offer = table.find("technician", "item_defective_two_way_radio", &parts, &[]).expect(dir);
        assert!(offer.can_build, "{dir}");
        assert_eq!(table.get(offer.recipe).unwrap().result, "item_two_way_radio");
    }
}

/// `[item_mine_detector]` has `requires_function=metal_detection`, which
/// only Extended's `item_mine_detector` has (`rules/extended/items.txt:128`).
/// Same recipe as the upstream test "metal detector recipe uses repair
/// electronics without LCD" (`ConstructionFeedbackTests.cs`).
#[test]
fn v12_the_mine_detector_is_built_in_extended_only() {
    let parts = names(&["item_defective_mine_detector", "item_electrical_odds_and_ends", "item_batteries"]);
    let (Some(extended), Some(classic)) = (v12_table("extended"), v12_table("dos")) else { return };
    let recipe = extended.iter().find(|r| r.result == "item_mine_detector").unwrap();
    assert_eq!(recipe.requires_function.as_deref(), Some("metal_detection"));
    assert_eq!(recipe.items, parts);
    let offer = extended.find("technician", "item_defective_mine_detector", &parts, &[]).unwrap();
    assert!(offer.can_build);
    assert_eq!(classic.find("technician", "item_defective_mine_detector", &parts, &[]), None);
}

/// `CanConstruct` asks for every tool as for every part, and `Construct`
/// removes only the `Items` (`Constructions.cs:123-140`, `:225-250`).
#[test]
fn v12_tools_are_needed_but_not_consumed() {
    let table = Constructions::parse(
        "dialog=503\n\n[item_trap]\nitems=item_spring item_tin\ntools=item_pliers\ndialog=505\nmain_items=item_spring\nclass=technician\n",
    );
    let recipe = table.get(0).unwrap();
    assert_eq!(recipe.tools, names(&["item_pliers"]));
    let parts = names(&["item_spring", "item_tin"]);
    let without = table.find("technician", "item_spring", &parts, &[]).unwrap();
    assert!(!without.can_build, "the tool is missing");
    let with = names(&["item_spring", "item_tin", "item_pliers"]);
    let offer = table.find("technician", "item_spring", &with, &[]).unwrap();
    assert!(offer.can_build);
    let build = table.build(&offer, "item_spring", &with).unwrap();
    let taken: Vec<&str> = build.take.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(taken, ["item_spring", "item_tin"], "the pliers stay");
}

/// v1.2's file, byte for byte (sha256 `4459837e…`, 1339 bytes): nine
/// recipes, the protective suit's parts reordered.
#[test]
fn v12_file_has_nine_recipes() {
    let Some(text) = v12_file("construction.txt") else { return };
    assert_eq!(text.len(), 1339);
    let table = Constructions::parse(&text);
    assert_eq!(table.len(), 9);
    let suit = table.iter().find(|r| r.result == "item_protective_suit").unwrap();
    assert_eq!(suit.items[0], "item_protective_overall");
}

/// The developer's pack carries v1.2's file for v1.2 games, byte for byte
/// (sha256 `4459837e…` is the file's), and v0.9.1's for the legacy game.
#[test]
fn v12_the_pack_carries_v12s_recipes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
    let (Ok(pack), Some(file)) = (std::fs::read(dir.join("burntime.pack")), v12_file("construction.txt")) else { return };
    let pack = burngfx::pack::Pack::open(&pack).unwrap();
    assert_eq!(pack.get("v12/constructions"), Some(file.as_bytes()));
    let legacy = std::fs::read(dir.join("construction.txt")).unwrap();
    assert_eq!(pack.get("rules/constructions"), Some(&legacy[..]));
}

/// F23: `GetConstructionInspector` → `SelectInspector`
/// (`InventoryScene.cs:854-858`, `Constructions.cs:102-115`): a doctor
/// looking at a broken pump has it judged by the group's technician; with
/// no technician, or for an item no recipe starts from, by himself.
#[test]
fn the_inspector_is_the_viewer_or_the_first_of_the_group_who_may_build() {
    let Some(table) = table() else { return };
    assert_eq!(table.inspector_class("doctor", &["boss", "technician", "mercenary"], "item_broken_pump"), "technician");
    assert_eq!(table.inspector_class("technician", &["boss", "technician"], "item_broken_pump"), "technician");
    assert_eq!(table.inspector_class("doctor", &["boss", "mercenary"], "item_broken_pump"), "doctor");
    assert_eq!(table.inspector_class("doctor", &["technician"], "item_rags"), "doctor", "not a main item");
    assert_eq!(table.inspector_class("doctor", &["technician"], "item_unloaded_rifle"), "doctor", "anybody reloads");
    assert_eq!(table.select_inspector("doctor", &["boss", "technician"], "item_broken_pump"), Some(1), "whose face asks");
}
