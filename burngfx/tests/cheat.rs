//! v1.2's `/petko` dialog, against `Scenes/MapScene.cs` and the shipped
//! `settings.txt`.

use burngfx::cheat::*;
use burngfx::game::CharacterNeeds;

fn typed(c: &mut Command, word: &str) -> Vec<Typed> {
    word.chars().map(|ch| c.key(ch)).collect()
}

/// `MapScene.OnKeyPress` (`MapScene.cs:320-357`): `/` starts the command,
/// each letter goes on, and the sixth character opens the dialog.
#[test]
fn the_word_opens_the_dialog() {
    let mut c = Command::default();
    assert!(!c.is_typing());
    let steps = typed(&mut c, "/petko");
    assert_eq!(steps[..5], [Typed::Taken; 5]);
    assert_eq!(steps[5], Typed::Open);
    // `_cheatCommand = string.Empty` once it is shown.
    assert!(!c.is_typing());
}

/// `key = char.ToLowerInvariant(key)` before anything else (`:325`): a
/// shifted letter is the same letter.
#[test]
fn the_word_in_capitals_opens_it_too() {
    let mut c = Command::default();
    assert_eq!(typed(&mut c, "/PeTkO").last(), Some(&Typed::Open));
}

/// Without a `/` first nothing is the command's: `base.OnKeyPress` (`:356`).
#[test]
fn letters_alone_are_not_taken() {
    let mut c = Command::default();
    assert_eq!(typed(&mut c, "petko"), [Typed::Passed; 5]);
    assert!(!c.is_typing());
}

/// `"/petko".StartsWith(_cheatCommand)` (`:351-352`): a wrong letter drops
/// the command — that letter is still taken — and the next one is the
/// map's again.
#[test]
fn a_wrong_letter_drops_the_command() {
    let mut c = Command::default();
    assert_eq!(typed(&mut c, "/px"), [Typed::Taken; 3]);
    assert!(!c.is_typing());
    assert_eq!(c.key('e'), Typed::Passed);
    assert_eq!(typed(&mut c, "/petk").last(), Some(&Typed::Taken));
    assert_eq!(c.typed(), "/petk");
}

/// A `/` always starts again (`:327-331`), and backspace takes one back
/// (`:335-341`) — the last one leaves nothing typed.
#[test]
fn slash_restarts_and_backspace_takes_one_back() {
    let mut c = Command::default();
    typed(&mut c, "/pet");
    assert_eq!(c.key('/'), Typed::Taken);
    assert_eq!(c.typed(), "/");
    typed(&mut c, "pe");
    assert_eq!(c.key('\u{8}'), Typed::Taken);
    assert_eq!(c.typed(), "/p");
    assert_eq!(typed(&mut c, "etko").last(), Some(&Typed::Open));
    c.key('/');
    assert_eq!(c.key('\u{8}'), Typed::Taken);
    assert!(!c.is_typing());
    // With nothing typed, backspace is the map's.
    assert_eq!(c.key('\u{8}'), Typed::Passed);
}

/// `TryGetInputAction` (`:21-32`): Escape while typing drops the command.
#[test]
fn escape_drops_the_command() {
    let mut c = Command::default();
    typed(&mut c, "/pet");
    assert!(c.is_typing());
    c.escape();
    assert!(!c.is_typing());
    assert_eq!(typed(&mut c, "ko"), [Typed::Passed; 2]);
}

/// `OnDialogHidden`'s switch (`:289-300`): the three lines in order.
#[test]
fn the_answers_are_travel_refill_items() {
    assert_eq!(CHOICES, ["Travel", "Refill", "Items"]);
    assert_eq!(Choice::of_line(0), Some(Choice::Travel));
    assert_eq!(Choice::of_line(1), Some(Choice::Refill));
    assert_eq!(Choice::of_line(2), Some(Choice::Items));
    assert_eq!(Choice::of_line(3), None);
}

/// `RefillPlayer` (`:377-382`): the leader's food 9, water 5, health 100.
#[test]
fn refill_fills_the_leader() {
    let mut leader = CharacterNeeds { food: 1, max_food: 9, water: 0, max_water: 5, health: 12 };
    refill(&mut leader);
    assert_eq!(leader, CharacterNeeds { food: 9, max_food: 9, water: 5, max_water: 5, health: 100 });
}

/// `SpawnCheatItems` (`:384-395`) reads `[debug] insert_items_1..3` with
/// `ConfigSection.GetStrings`. v1.2's shipped `settings.txt` lines, tabs and
/// comments as they are there.
#[test]
fn the_items_are_the_debug_sections_three_lists() {
    let settings = "[game]\r\nroundtime          = 240\r\n\r\n[debug]\r\nenable_cheats   = no\r\n\
                    show_routes \t= no\r\n\
                    insert_items_1\t= item_meat item_full_canteen\r\n\
                    insert_items_2\t= item_rope item_gas_mask\r\n\
                    insert_items_3\t= item_unloaded_rifle item_unloaded_pistol item_ammunition item_ammunition item_two_way_radio item_bible item_skull item_bones\r\n\
                    godmode         = no ' recover human player main character health to 100 when below 1\r\n";
    assert_eq!(
        items(settings),
        [
            "item_meat", "item_full_canteen", "item_rope", "item_gas_mask", "item_unloaded_rifle",
            "item_unloaded_pistol", "item_ammunition", "item_ammunition", "item_two_way_radio",
            "item_bible", "item_skull", "item_bones",
        ]
    );
    // An empty value, a missing key, commas and a section in other case.
    let other = "[DEBUG]\nINSERT_ITEMS_1 = a,b ; c\ninsert_items_2 =\n";
    assert_eq!(items(other), ["a", "b"]);
    assert!(items("[game]\ninsert_items_1 = x\n").is_empty());
}

/// `OnFastTravel` (`:1131-1139`): `syst.raw?4-7`, not progressive.
#[test]
fn the_fast_travel_cursor_is_syst_4_to_7() {
    assert_eq!(fast_travel_cursor_frame(0), 4);
    assert_eq!(fast_travel_cursor_frame(200), 5);
    assert_eq!(fast_travel_cursor_frame(799), 7);
    assert_eq!(fast_travel_cursor_frame(800), 4);
}

/// `StatisticsScene` (`:91`, `:207`): `{cheats}` in `burn?571` is
/// `newburn?52` once the game is marked, and nothing before.
#[test]
fn the_statistics_say_so_once_marked() {
    assert_eq!(statistics_line("{cheats}", false, "- Cheats active -"), "");
    assert_eq!(statistics_line("{cheats}", true, "- Cheats active -"), "- Cheats active -");
    assert_eq!(NEWBURN_ACTIVE, 52);
}
