//! The start screen behaves as `MenuScene` does, click for click: v0.9.1's
//! controls, v1.2's choices of rules and opponent.

use burngfx::rules::{AiProfile, RuleSet};
use burngfx::player_colors::PlayerColor;
use burngfx::start::{at, Outcome, StartScreen, Toggle, tooltip};

fn click(s: &mut StartScreen, r: burngfx::start::Rect) -> Option<Outcome> {
    s.click(r.x + 1, r.y + 1, false)
}

#[test]
fn it_opens_as_on_activate_scene_leaves_it() {
    let s = StartScreen::new();
    assert!(s.slots.iter().all(|slot| !slot.used && slot.face == -1 && slot.name.is_empty()));
    assert_eq!((s.difficulty, s.mode_state, s.ai_state), (0, 0, 0));
    // State 0 of the mode toggle is the remake: extended rules.
    assert_eq!(tooltip(Toggle::Mode, &s), 110, "v1.2 newburn: Extended rules");
    assert_eq!(s.slots[0].slide_frame(), 6, "covers start closed");
}

#[test]
fn start_needs_a_face() {
    let mut s = StartScreen::new();
    assert_eq!(click(&mut s, at::START), None);
}

#[test]
fn opening_a_slot_gives_it_face_zero_and_focus_and_starts_as_max() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[0]);
    assert!(s.slots[0].used && s.slots[0].focused);
    assert_eq!(s.slots[0].face, 0);
    s.tick(1000);
    assert_eq!(s.slots[0].slide_frame(), 0, "the cover runs back to open");
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => {
            let one = g.players[0].as_ref().expect("seat 0 is human");
            assert!(g.players[1].is_none(), "seat 1 is the machine's");
            assert_eq!(one.name, "Max");
            assert_eq!(one.face, 0);
            assert_eq!(one.color, PlayerColor::Green, "player one starts green");
            assert_eq!((g.rules, g.opponent), (RuleSet::Extended, AiProfile::Modern));
            assert_eq!(g.difficulty, 0);
        }
        other => panic!("{other:?}"),
    }
}

/// Typing goes into the focused slot, up to ten. **After the automatic
/// name**: opening the slot set one (`SetAutomaticName`, `MenuScene.cs:466`;
/// "Max" with no `names.txt`), and the focused `NameWindow` appends what is
/// typed to whatever it holds (`Name += Key`, `NameWindow.cs:96-125`) — the
/// scene's own `SetAutomaticName("")` is only for typing at a slot not yet
/// open (`MenuScene.cs:576-588`). The name is then a typed one.
#[test]
fn a_name_is_typed_into_the_focused_slot_up_to_ten() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[1]);
    assert_eq!((s.slots[1].name.as_str(), s.slots[1].manual), ("Max", false));
    for c in "Bogus the Great".chars() {
        s.type_char(c, true, &[]);
    }
    assert_eq!(s.slots[1].name, "MaxBogus t");
    assert!(s.slots[1].manual);
    s.type_char('\u{8}', false, &[]);
    assert_eq!(s.slots[1].label(), "[ MaxBogus  ]");
    // Rubbed out to nothing, it is no longer a typed name.
    for _ in 0..9 {
        s.type_char('\u{8}', false, &[]);
    }
    assert_eq!((s.slots[1].name.as_str(), s.slots[1].manual), ("", false));
}

/// `GetRandomName` (`MenuScene.cs:926-940`) over `names.txt`: a slot opened
/// with no typed name is given one, never the other slot's, each of the
/// others about equally often; START gives one to a slot left empty.
#[test]
fn an_opened_slot_is_given_a_name_from_the_list() {
    let mut s = StartScreen::new();
    s.names = burngfx::start::parse_names("Max\nAsh\n\n  Axel \nash\nBran\n");
    assert_eq!(s.names, ["Max", "Ash", "Axel", "Bran"]);
    s.rng = burngfx::spawn::Rng::game(1993);
    click(&mut s, at::NAME[0]);
    let first = s.slots[0].name.clone();
    assert!(s.names.contains(&first));
    let mut seen = std::collections::BTreeMap::new();
    for _ in 0..3000 {
        click(&mut s, at::NAME[1]);
        assert!(s.slots[1].used);
        let second = s.slots[1].name.clone();
        assert_ne!(second, first);
        *seen.entry(second).or_insert(0u32) += 1;
        click(&mut s, at::NAME[1]);
        assert!(!s.slots[1].used && s.slots[1].name.is_empty());
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
    for n in seen.values() {
        // The other's name is skipped to the next one along, so the one
        // after it comes up twice as often: a quarter, a quarter, a half.
        assert!((600..1700).contains(n), "{seen:?}");
    }
    // A slot rubbed out to nothing gets one at START, not "Max".
    for _ in 0..10 {
        s.type_char('\u{8}', false, &[]);
    }
    assert!(s.slots[0].name.is_empty());
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => {
            let name = &g.players[0].as_ref().unwrap().name;
            assert!(s.names.contains(name), "{name}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn faces_skip_the_other_players_and_stop_at_the_ends() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[0]);
    click(&mut s, at::NAME[1]);
    // The second slot took face 0 first, so it was stepped to 1.
    assert_eq!((s.slots[0].face, s.slots[1].face), (0, 1));
    let f = at::FACE[0];
    s.click(f.x + 1, f.y + 1, false);
    assert_eq!(s.slots[0].face, 2, "1 is taken, so 2");
    s.click(f.x + 1, f.y + 1, true);
    assert_eq!(s.slots[0].face, 0, "back past 1 to 0");
    s.click(f.x + 1, f.y + 1, true);
    assert_eq!(s.slots[0].face, 0);
}

#[test]
fn closing_one_slot_hands_focus_to_the_other() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[0]);
    click(&mut s, at::NAME[1]);
    assert!(!s.slots[0].focused && s.slots[1].focused);
    click(&mut s, at::NAME[1]);
    assert!(!s.slots[1].used && s.slots[1].face == -1);
    assert!(s.slots[0].focused, "OnPlayerTwoUp puts focus back on one");
}

/// Two slots, two human seats, in opposite colours.
#[test]
fn two_slots_are_two_players() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[0]);
    click(&mut s, at::NAME[1]);
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => {
            let (a, b) = (g.players[0].as_ref().unwrap(), g.players[1].as_ref().unwrap());
            assert_eq!((a.face, b.face), (0, 1));
            assert_eq!((a.color, b.color), (PlayerColor::Green, PlayerColor::Red));
            assert_eq!(g.humans().count(), 2);
        }
        other => panic!("{other:?}"),
    }
}

/// `NameOne` and `NameTwo` are separate: the second slot alone makes player 1
/// the human, and player 0 the machine's.
#[test]
fn the_second_slot_alone_is_player_one() {
    let mut s = StartScreen::new();
    click(&mut s, at::NAME[1]);
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => {
            assert!(g.players[0].is_none());
            let two = g.players[1].as_ref().unwrap();
            assert_eq!(two.color, PlayerColor::Red, "the second colour");
            assert_eq!(g.humans().map(|(i, _)| i).collect::<Vec<_>>(), [1]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn toggles_cycle_and_the_colour_swaps() {
    let mut s = StartScreen::new();
    click(&mut s, at::DIFFICULTY);
    click(&mut s, at::DIFFICULTY);
    click(&mut s, at::MODE);
    click(&mut s, at::AI);
    click(&mut s, at::COLOUR[0]);
    click(&mut s, at::NAME[0]);
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => {
            assert_eq!(g.difficulty, 2);
            assert_eq!((g.rules, g.opponent), (RuleSet::Classic, AiProfile::Amiga));
            // Green's next is red, the other's: the two swap.
            assert_eq!(g.players[0].as_ref().unwrap().color, PlayerColor::Red);
            assert_eq!(g.colors, [PlayerColor::Red, PlayerColor::Green]);
        }
        other => panic!("{other:?}"),
    }
    click(&mut s, at::DIFFICULTY);
    assert_eq!(s.difficulty, 0);
}

#[test]
fn a_face_behind_a_closed_cover_still_counts() {
    // The original's quirk, kept: the cover lets the click through.
    let mut s = StartScreen::new();
    let f = at::FACE[1];
    s.click(f.x + 1, f.y + 1, false);
    assert!(!s.slots[1].used);
    assert!(matches!(click(&mut s, at::START), Some(Outcome::Start(_))));
}

/// `AiPlayers`: Modern, Amiga, None, round again — and an opponent this
/// build does not offer is passed over, the toggle starting on the first
/// that is.
#[test]
fn the_opponent_cycles_through_what_is_offered() {
    let mut s = StartScreen::new();
    let mut seen = vec![s.ai_state];
    for _ in 0..3 {
        click(&mut s, at::AI);
        seen.push(s.ai_state);
    }
    assert_eq!(seen, [0, 1, 2, 0]);
    assert_eq!([0, 1, 2].map(|n| { s.ai_state = n; tooltip(Toggle::Ai, &s) }), [109, 108, 13]);
    let mut s = StartScreen::new();
    s.offer([false, true, true]);
    assert_eq!(s.ai_state, 1, "Modern not offered: the toggle starts on Amiga");
    click(&mut s, at::AI);
    click(&mut s, at::AI);
    assert_eq!(s.ai_state, 1, "Amiga, None, Amiga");
    click(&mut s, at::NAME[0]);
    match click(&mut s, at::START) {
        Some(Outcome::Start(g)) => assert_eq!(g.opponent, AiProfile::Amiga),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_setup_selection_moves_as_v12s() {
    use burngfx::start::{at, Setup, SetupKey, StartScreen};
    let mut s = StartScreen::new();
    assert_eq!((s.setup, s.current), (Setup::Player, 0));
    // The first Enter only shows the selection.
    assert_eq!(s.key(SetupKey::Primary), None);
    // Enter on the first slot is a click on its name field.
    assert_eq!(s.key(SetupKey::Primary), Some((at::NAME[0], false)));
    // Up to START, LOAD; down back; right from LOAD goes to the second slot.
    s.key(SetupKey::Up);
    assert_eq!(s.setup, Setup::Start);
    s.key(SetupKey::Up);
    assert_eq!(s.setup, Setup::Load);
    s.key(SetupKey::Right);
    assert_eq!((s.setup, s.current), (Setup::Player, 1));
    // Down from the second slot is the AI toggle; left along the row to the
    // mode, the difficulty and NOTES, no further (`MoveSetupSelectionHorizontal`,
    // MenuScene.cs:869-893).
    s.key(SetupKey::Down);
    assert_eq!(s.setup, Setup::AiPlayers);
    s.key(SetupKey::Left);
    s.key(SetupKey::Left);
    assert_eq!(s.setup, Setup::Difficulty);
    assert_eq!(s.key(SetupKey::Primary), Some((at::DIFFICULTY, false)));
    s.key(SetupKey::Left);
    s.key(SetupKey::Left);
    assert_eq!(s.setup, Setup::Notes);
    assert_eq!(s.key(SetupKey::Primary), Some((at::NOTES, false)));
    s.key(SetupKey::Right);
    assert_eq!(s.setup, Setup::Difficulty);
    // Up from the difficulty: the first slot. Tab: the other.
    s.key(SetupKey::Up);
    assert_eq!((s.setup, s.current), (Setup::Player, 0));
    s.key(SetupKey::Tab);
    assert_eq!(s.current, 1);
    // A face and the colours only for a slot in use (`CurrentPlayerEnabled`,
    // `MenuScene.cs:503-505`).
    assert_eq!(s.key(SetupKey::FaceUp), None);
    assert_eq!(s.key(SetupKey::Colours), None);
    assert_eq!(s.key(SetupKey::Back), Some((at::LOAD, false)));
}

/// v1.2's NOTES button (`MenuScene.cs:192-204`) at (13, 163), 36 × 18:
/// a click asks for the setup notes; Up from it is the first slot.
#[test]
fn notes_opens_the_setup_notes() {
    use burngfx::start::{at, Hover, Outcome, Setup, SetupKey, StartScreen};
    let mut s = StartScreen::new();
    assert_eq!(at::NOTES, burngfx::start::Rect::new(13, 163, 36, 18));
    s.point(14, 164);
    assert_eq!(s.hover, Some(Hover::Notes));
    assert_eq!(s.click(48, 180, false), Some(Outcome::Notes));
    s.setup = Setup::Notes;
    s.key(SetupKey::Up);
    assert_eq!((s.setup, s.current), (Setup::Player, 0));
}

/// The toggles' tooltip windows (`MenuScene.cs:705-737`): difficulty
/// `tooltip?s9..s11`, mode s4 (Extended) / s5 (Classic), opponent s0
/// (Modern), s1 (Amiga), s2 (None).
#[test]
fn each_toggle_state_has_its_tooltip_section() {
    use burngfx::start::{tooltip_section, StartScreen, Toggle};
    let mut s = StartScreen::new();
    assert_eq!([Toggle::Difficulty, Toggle::Mode, Toggle::Ai].map(|t| tooltip_section(t, &s)), [9, 4, 0]);
    s.difficulty = 2;
    s.mode_state = 1;
    s.ai_state = 1;
    assert_eq!([Toggle::Difficulty, Toggle::Mode, Toggle::Ai].map(|t| tooltip_section(t, &s)), [11, 5, 1]);
    s.ai_state = 2;
    assert_eq!(tooltip_section(Toggle::Ai, &s), 2);
}

/// v1.3's flags (`13e12ef`): each player's cycles green, red, blue, gray;
/// taking the other's colour hands him this one's (`SelectPlayerColor`); a
/// right click does nothing (`Toggle.OnMouseClick`).
#[test]
fn each_flag_cycles_four_colours_and_swaps_on_a_clash() {
    use PlayerColor::*;
    let mut s = StartScreen::new();
    assert_eq!(s.colors, [Green, Red]);
    click(&mut s, at::COLOUR[1]);
    assert_eq!(s.colors, [Green, Blue]);
    click(&mut s, at::COLOUR[1]);
    assert_eq!(s.colors, [Green, Black]);
    click(&mut s, at::COLOUR[1]);
    assert_eq!(s.colors, [Black, Green], "green was the other's: he takes gray");
    click(&mut s, at::COLOUR[0]);
    assert_eq!(s.colors, [Green, Black], "gray's next is green, the other's again");
    let rect = at::COLOUR[0];
    s.click(rect.x + 1, rect.y + 1, true);
    assert_eq!(s.colors, [Green, Black], "the right button does nothing");
}

