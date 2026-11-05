//! `burngfx::input`: v1.2's bindings (`KeyboardBindings.cs`,
//! `GamepadBindings.cs`) and how `settings.txt` changes them.

use burngfx::input::{Action, Bindings, Key, Pad};

/// v1.2's own `[keyboard]` and `[gamepad]` (`resources/game/classic/settings.txt`
/// at `v1.2`), with its alignment and comment style.
const V12: &str = "[system]\nmusic_volume = 80\n\n[keyboard]\nmove_up           = up\nmove_down         = down\nmove_left         = left\nmove_right        = right\npan_up            = w\npan_down          = s\npan_left          = a\npan_right         = d\naccept             = space enter\nback               = escape\nsecondary          = f\naction             = q\noptions            = o\ninventory          = e i\nworld_map          = v m\nstatistics         = h\ninfo               = r\nnext_turn          = t\ntoggle_interaction = c\nshow_entrances     = alt\n\n[gamepad]\naccept         = a\nback           = b\nsecondary      = x\naction         = y\noptions        = menu\nworld_map      = view\nleft_area      = left_shoulder\nright_area     = right_shoulder\ncycle_target   = right_trigger\ninventory      = dpad_up\nstatistics     = dpad_left\ninfo           = dpad_right\nnext_turn      = dpad_down\nshow_entrances = left_trigger\n\n[debug]\nenable_cheats   = no\n";

#[test]
fn the_shipped_settings_are_the_defaults() {
    assert_eq!(Bindings::from_settings(V12), Bindings::defaults());
    let b = Bindings::defaults();
    for (key, action) in [
        (Key::Up, Action::MoveUp),
        (Key::Char('w'), Action::PanCameraUp),
        (Key::Char(' '), Action::Primary),
        (Key::Enter, Action::Primary),
        (Key::Escape, Action::Back),
        (Key::Char('f'), Action::Secondary),
        (Key::Char('q'), Action::Action),
        (Key::Char('e'), Action::Inventory),
        (Key::Char('i'), Action::Inventory),
        (Key::Char('m'), Action::WorldMap),
        (Key::Char('t'), Action::NextTurn),
        (Key::Char('c'), Action::ToggleInteractionMode),
        (Key::Alt, Action::ShowEntrances),
        (Key::Ctrl, Action::SetupNotes),
    ] {
        assert_eq!(b.key_action(key, false), Some(action), "{key:?}");
    }
    assert_eq!(b.key_action(Key::Char('Q'), false), Some(Action::Action), "letters are matched in lower case");
    assert_eq!(b.key_action(Key::Char('n'), false), None);
    assert_eq!(b.pad_action(Pad::A), Some(Action::Primary));
    assert_eq!(b.pad_action(Pad::RightTrigger), Some(Action::NextTarget));
    assert_eq!(b.pad_action(Pad::DPadDown), Some(Action::NextTurn));
    assert_eq!(b.pad_action(Pad::LeftStick), None);
}

/// The file itself, as the packer packs it for v1.2 games (`v12/system`):
/// its bindings are the defaults the port fell back to while it packed
/// v0.9.1's `settings.txt`, which has no `[keyboard]` or `[gamepad]`.
#[test]
fn v12s_settings_file_binds_the_defaults() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/v1.2/settings.txt");
    let Ok(text) = std::fs::read_to_string(path) else { return };
    assert_eq!(Bindings::from_settings(&text), Bindings::defaults());
}

#[test]
fn the_chords_cannot_be_rebound() {
    let b = Bindings::from_settings("[keyboard]\nmove_left = tab\n");
    assert_eq!(b.key_action(Key::Tab, false), Some(Action::NextTarget));
    assert_eq!(b.key_action(Key::Tab, true), Some(Action::PreviousTarget));
    assert_eq!(b.key_action(Key::Left, true), Some(Action::LeftArea));
    assert_eq!(b.key_action(Key::Right, true), Some(Action::RightArea));
}

#[test]
fn settings_rebind_unbind_and_fall_back() {
    // Two keys, by space or comma; an empty value unbinds; `global_action`
    // stands in for a missing `action`; a key named twice does what the
    // later setting says (`next_turn` comes after `inventory`).
    let b = Bindings::from_settings("[keyboard]\nnext_turn = n, e\nback =\nglobal_action = x\n[gamepad]\naccept = a b\n");
    assert_eq!(b.key_action(Key::Char('n'), false), Some(Action::NextTurn));
    assert_eq!(b.key_action(Key::Char('e'), false), Some(Action::NextTurn));
    assert_eq!(b.key_action(Key::Char('i'), false), Some(Action::Inventory));
    assert_eq!(b.key_action(Key::Char('t'), false), None);
    assert_eq!(b.key_action(Key::Escape, false), None);
    assert_eq!(b.key_action(Key::Char('x'), false), Some(Action::Action));
    assert_eq!(b.key_action(Key::Char('q'), false), None);
    // A gamepad setting is one control: `a b` names none.
    assert_eq!(b.pad_action(Pad::A), None);
    assert_eq!(b.keys_for(Action::NextTurn), [Key::Char('n'), Key::Char('e')]);
}

#[test]
fn party_cycling_goes_as_v12s_does() {
    use burngfx::input::cycle_party;
    use burngfx::save::Selection::{Camper, Companion, Leader};
    let two = [7, 9];
    // Group mode: right the first follower, left the leader, alone either way.
    assert_eq!(cycle_party(Leader, false, &two, true), Some((Companion(7), true)));
    assert_eq!(cycle_party(Leader, false, &two, false), Some((Leader, true)));
    // Single mode goes round, both ways.
    assert_eq!(cycle_party(Companion(7), true, &two, true), Some((Companion(9), true)));
    assert_eq!(cycle_party(Companion(9), true, &two, true), Some((Leader, true)));
    assert_eq!(cycle_party(Leader, true, &two, false), Some((Companion(9), true)));
    // A camper: back to the leader, even with nobody in the group.
    assert_eq!(cycle_party(Camper(61), true, &two, true), Some((Leader, true)));
    assert_eq!(cycle_party(Camper(61), true, &[], false), Some((Leader, true)));
    // The leader alone: nothing to cycle.
    assert_eq!(cycle_party(Leader, true, &[], true), None);
}

/// The world map's `CyclePartyCharacter` (v1.2 `Scenes/MapScene.cs:522-563`):
/// from the selection now, as in a place, except that a group of one does
/// nothing at all and a selection outside the group counts as the leader's
/// place in it (`currentIndex = 0`) rather than going back to him.
#[test]
fn the_map_cycles_from_the_selection_now() {
    use burngfx::input::cycle_party_on_map;
    use burngfx::save::Selection::{Camper, Companion, Leader};
    let two = [7, 9];
    // Group mode: right the first follower, left the leader.
    assert_eq!(cycle_party_on_map(Leader, false, &two, true), Some((Companion(7), true)));
    assert_eq!(cycle_party_on_map(Leader, false, &two, false), Some((Leader, true)));
    // Single mode goes round from whoever is selected, past the first.
    assert_eq!(cycle_party_on_map(Companion(7), true, &two, true), Some((Companion(9), true)));
    assert_eq!(cycle_party_on_map(Companion(9), true, &two, true), Some((Leader, true)));
    assert_eq!(cycle_party_on_map(Companion(9), true, &two, false), Some((Companion(7), true)));
    // A camper selected: counted from the leader's place.
    assert_eq!(cycle_party_on_map(Camper(61), true, &two, true), Some((Companion(7), true)));
    assert_eq!(cycle_party_on_map(Camper(61), true, &two, false), Some((Companion(9), true)));
    // A group of one: nothing, camper or not (`party.Count <= 1`).
    assert_eq!(cycle_party_on_map(Camper(61), true, &[], false), None);
    assert_eq!(cycle_party_on_map(Leader, true, &[], true), None);
}

#[test]
fn acos_is_stds() {
    for i in -1000..=1000 {
        let x = i as f64 / 1000.0;
        assert!((burngfx::input::acos(x) - x.acos()).abs() < 1e-12, "{x}");
    }
}

#[test]
fn an_arrow_selects_the_place_that_way() {
    use burngfx::input::select_in_direction;
    // A cross of places around (100, 100), and one far up the right.
    let places = [(1, (100.0, 50.0)), (2, (100.0, 160.0)), (3, (40.0, 100.0)), (4, (170.0, 100.0)), (5, (190.0, 20.0))];
    let at = (100.0, 100.0);
    assert_eq!(select_in_direction(at, &places, (0, -1)), Some(1));
    assert_eq!(select_in_direction(at, &places, (0, 1)), Some(2));
    assert_eq!(select_in_direction(at, &places, (-1, 0)), Some(3));
    assert_eq!(select_in_direction(at, &places, (1, 0)), Some(4));
    // Up-right: 5 lines up with it best and is the direction's own, but its
    // score (~121) is more than 5 % worse than the best ranked, 1 (87.5, at
    // 45°), so the arrow takes 1 — the rule's fallback.
    assert_eq!(select_in_direction(at, &places, (1, -1)), Some(1));
    // A place squarely up-right and near is taken.
    let mut more = places.to_vec();
    more.push((6, (130.0, 70.0)));
    assert_eq!(select_in_direction(at, &more, (1, -1)), Some(6));
    // From the far left, nothing lies to the left.
    assert_eq!(select_in_direction((0.0, 100.0), &places, (-1, 0)), None);
}
