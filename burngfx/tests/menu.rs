//! The start screen: what the two toggles decide, and what they do not.

use burngfx::game::{Engine, Mode};
use burngfx::menu::{Choice, Menu, Row, Start, Waiting, DIFFICULTIES};

fn waiting() -> Waiting {
    Waiting { day: 12, mode: Mode::Extended, difficulty: 2, usable: true }
}

/// Pressing Enter at once starts the game that used to start with no
/// arguments at all: Original, v0.9.1, and `gam.dat`'s own difficulty.
#[test]
fn the_first_row_starts_the_default_game() {
    let mut menu = Menu::new(Mode::Original, 0, None);
    assert_eq!(menu.at(), Row::NewGame);
    assert_eq!(
        menu.take(),
        Some(Choice::Start(Start { mode: Mode::Original, engine: Engine::V091, difficulty: 0, opponent: true, v12: None }))
    );
}

/// The mode row is the game's two rulesets and nothing else. The engine axis
/// and the contested combat rules are not on this screen — they would read as
/// two more modes.
#[test]
fn the_mode_row_turns_between_the_two_rulesets() {
    let mut menu = Menu::new(Mode::Original, 0, None);
    menu.down();
    assert_eq!(menu.at(), Row::Mode);
    menu.turn(true);
    assert_eq!(menu.mode, Mode::Extended);
    menu.turn(true);
    assert_eq!(menu.mode, Mode::Original, "two states, and back");

    // Taking a setting row turns it rather than starting anything.
    assert_eq!(menu.take(), None);
    assert_eq!(menu.mode, Mode::Extended);

    // And whatever it says, a new game is v0.9.1.
    menu.up();
    let Some(Choice::Start(start)) = menu.take() else { panic!("the first row starts") };
    assert_eq!(start.engine, Engine::V091);
    assert_eq!(start.mode, Mode::Extended);
}

/// Three sections in `gamesettings.txt`, and the row cycles through them in
/// both directions.
#[test]
fn the_difficulty_row_cycles_three_ways() {
    let mut menu = Menu::new(Mode::Original, 0, None);
    menu.down();
    menu.down();
    assert_eq!(menu.at(), Row::Difficulty);
    for expected in [1, 2, 0] {
        menu.turn(true);
        assert_eq!(menu.difficulty, expected);
    }
    menu.turn(false);
    assert_eq!(menu.difficulty, DIFFICULTIES - 1, "backwards from the first is the last");

    menu.up();
    menu.up();
    let Some(Choice::Start(start)) = menu.take() else { panic!("the first row starts") };
    assert_eq!(start.difficulty, DIFFICULTIES - 1);
}

/// A difficulty out of range — an old `gam.dat`, a hand-edited file — is
/// brought back into range rather than carried.
#[test]
fn an_impossible_difficulty_is_clamped() {
    assert_eq!(Menu::new(Mode::Original, 9, None).difficulty, DIFFICULTIES - 1);
}

/// Walk the cursor down to a row. Counting key presses instead would make
/// every one of these tests break the day a row is added, which says nothing
/// about the row they are each actually about.
fn go_to(menu: &mut Menu, row: Row) {
    for _ in 0..burngfx::menu::ROWS.len() {
        if menu.at() == row {
            return;
        }
        menu.down();
    }
    panic!("no such row on this screen: {row:?}");
}

/// **Load is offered only when there is a file**, and choosing it says
/// nothing about the rules: those come out of the save's own header.
#[test]
fn load_is_offered_only_when_something_is_waiting() {
    let mut without = Menu::new(Mode::Original, 0, None);
    go_to(&mut without, Row::Load);
    assert_eq!(without.take(), None, "nothing to take up");

    let mut with = Menu::new(Mode::Original, 0, Some(waiting()));
    go_to(&mut with, Row::Load);
    assert_eq!(with.take(), Some(Choice::Load));
}

/// **A file this program cannot use is still offered.** The refusal belongs
/// to the load, which can say why; a row that vanishes cannot.
#[test]
fn an_unusable_save_is_still_offered() {
    let mut menu = Menu::new(Mode::Original, 0, Some(Waiting { usable: false, ..waiting() }));
    go_to(&mut menu, Row::Load);
    assert_eq!(menu.take(), Some(Choice::Load));
}

#[test]
fn the_rows_wrap_both_ways() {
    let mut menu = Menu::new(Mode::Original, 0, None);
    menu.up();
    assert_eq!(menu.at(), Row::Quit);
    assert_eq!(menu.take(), Some(Choice::Quit));
    menu.down();
    assert_eq!(menu.at(), Row::NewGame);
}

/// **An ordinary new game has somebody on the other side.** Upstream's screen
/// has a toggle for whether the other players are played by the machine;
/// with one opponent that is what is left of it, and it opens on. A game with
/// nobody trying to take the world from you has no victory to race for, so
/// making that the default would hide the ordinary game behind a setting.
#[test]
fn a_new_game_comes_with_an_opponent_unless_you_say_otherwise() {
    let mut menu = Menu::new(Mode::Original, 0, None);
    assert!(menu.opponent, "on when the screen opens");
    assert_eq!(
        menu.take(),
        Some(Choice::Start(Start {
            mode: Mode::Original,
            engine: Engine::V091,
            difficulty: 0,
            opponent: true,
            v12: None,
        })),
        "and pressing enter at once starts that game"
    );

    go_to(&mut menu, Row::Opponent);
    assert_eq!(menu.take(), None, "a setting row turns rather than starting");
    assert!(!menu.opponent);
    go_to(&mut menu, Row::NewGame);
    match menu.take() {
        Some(Choice::Start(start)) => assert!(!start.opponent, "the world to yourself"),
        other => panic!("expected a start, got {other:?}"),
    }
}
