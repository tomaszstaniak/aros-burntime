//! The world map's arithmetic, against v0.9.1's own.

use burngfx::mapscene::*;

#[test]
fn distribution_raises_the_worst_off_first() {
    // Two people with 1 and 3 days, four carried: 1→2→3, then 3,3 → 4,3 … the
    // lowest ends at 4.
    assert_eq!(lowest_after_distribution(&[1, 3], 4), 4);
    assert_eq!(lowest_after_distribution(&[2], 0), 2);
    assert_eq!(lowest_after_distribution(&[0, 5], 1), 1);
    assert_eq!(lowest_after_distribution(&[], 5), 0);
}

#[test]
fn the_status_line_follows_the_original_chain() {
    let calm = Hazard::None;
    assert_eq!(status(2, 5, 3, calm), Some(Status::TooLittleWater));
    assert_eq!(status(4, 2, 3, calm), Some(Status::TooLittleFood));
    assert_eq!(status(0, 5, 0, calm), Some(Status::TooLittleWater));
    assert_eq!(status(3, 0, 0, calm), Some(Status::TooLittleFood));
    assert_eq!(status(3, 3, 3, calm), None);
}

/// `MainUiOriginalWindow`'s six branches in their order. The danger comes in
/// twice: as a warning between the trip's shortfall and the empty stores when
/// somebody in the group is not fully protected (`IsInDanger`), and as the
/// quiet last line when everybody is.
#[test]
fn a_danger_sits_between_the_trip_and_the_empty_stores() {
    let hurting = Hazard::Unprotected;
    let shielded = Hazard::Protected;
    // The trip still wins.
    assert_eq!(status(2, 5, 3, hurting), Some(Status::TooLittleWater));
    assert_eq!(status(4, 2, 3, hurting), Some(Status::TooLittleFood));
    // Then the danger, over an empty water skin.
    assert_eq!(status(0, 5, 0, hurting), Some(Status::InDanger));
    assert_eq!(status(3, 3, 0, hurting), Some(Status::InDanger));
    // Protected: the stores come first, the danger only when nothing else.
    assert_eq!(status(0, 5, 0, shielded), Some(Status::TooLittleWater));
    assert_eq!(status(3, 0, 0, shielded), Some(Status::TooLittleFood));
    assert_eq!(status(3, 3, 0, shielded), Some(Status::DangerHere));
}

#[test]
fn the_turn_clock_runs_down_in_roundtime() {
    let mut c = TurnClock::new(240);
    assert_eq!(c.bar(), 60);
    assert!(!c.run(120_000));
    assert_eq!(c.bar(), 30);
    assert!(c.run(200_000));
    assert_eq!(c.bar(), 0);
    c.reset();
    assert_eq!(c.bar(), 60);
}

#[test]
fn the_menu_opens_centred_and_inside_and_answers_over_text_only() {
    let mut m = Menu::for_map(false);
    assert_eq!(m.height(), 65);
    m.show(10, 10, (16, 0, 650, 280));
    assert_eq!(m.at, Some((16, 0)), "pushed inside the view");
    // Line 4 (NEXT) is at top + 4 + 44.
    m.point(16 + 34, 0 + 4 + 44 + 3, |_| 20);
    assert_eq!(m.hover, Some(4));
    m.point(16 + 2, 0 + 4 + 44 + 3, |_| 20);
    assert_eq!(m.hover, None, "beside the text is not the line");
    m.point(16 + 34, 51, |_| 20);
    assert_eq!(m.click(true), Some(Command::Next));
    assert!(!m.is_open());
    assert_eq!(Menu::for_map(true).lines[0], Command::Travel);
}

#[test]
fn roads_and_markers_sit_where_the_reference_puts_them() {
    assert_eq!(way_position(2, (100, 50), (40, 80)), Some((60, 53)));
    assert_eq!(way_position(47, (0, 0), (0, 0)), None);
    assert_eq!(marker_position((10, 20, 30, 10), None, 1), (13, 28));
    assert_eq!(marker_position((0, 0, 0, 0), Some(((0, 0), (40, 20), 1, 4)), 0), (30, 15));
}

#[test]
fn scrolling_is_clamped_and_small_maps_centred() {
    assert_eq!(clamp_scroll((10, -900), (600, 280), (960, 768)), (0, -488));
    assert_eq!(clamp_scroll((-50, 0), (600, 280), (400, 300)), (100, 0));
    assert_eq!(edge_push(2, 100, (600, 280)), (2, 0));
    assert_eq!(edge_push(595, 275, (600, 280)), (-1, -1));
}

#[test]
fn the_location_menu_is_built_from_the_situation() {
    let m = Menu::for_location(true, false, 1, false, burngfx::mapscene::Selected::Leader);
    assert_eq!(m.lines, vec![Command::MainMap, Command::Inventory, Command::Next]);
    let m = Menu::for_location(false, true, 3, true, burngfx::mapscene::Selected::Leader);
    assert_eq!(m.lines, vec![Command::Info, Command::Speak, Command::All, Command::MainMap,
                             Command::Inventory, Command::Next]);
}

#[test]
fn roundtime_is_read_from_the_game_section() {
    assert_eq!(round_seconds("[system]\nroundtime = 5\n[game]\npathfinding_cycles = 50\t\t' x\nroundtime          = 240\n"), 240);
    assert_eq!(round_seconds("[game]\nroundtime = 90 ' short\n"), 90);
    assert_eq!(round_seconds("[game]\nroundtime = 0\n"), 240);
    assert_eq!(round_seconds(""), 240);
}

#[test]
fn the_selected_character_adds_its_lines() {
    use burngfx::mapscene::{Command, Selected};
    let group = Menu::for_location(false, false, 3, true, Selected::InGroup);
    assert_eq!(group.lines, [Command::Info, Command::Fight, Command::All, Command::Dismiss,
                             Command::StayThere, Command::MainMap, Command::Inventory, Command::Next]);
    let camper = Menu::for_location(true, false, 1, true, Selected::Camper);
    assert_eq!(camper.lines, [Command::Dismiss, Command::Rejoin, Command::MainMap,
                              Command::Inventory, Command::Next]);
    assert_eq!((Command::Dismiss.burn(), Command::StayThere.burn(), Command::Rejoin.burn()), (363, 364, 365));
}

#[test]
fn the_v12_location_menu_has_v12_order() {
    use burngfx::mapscene::{Command, Selected};
    // `ShowActionsMenu` opened by mouse, outside a city: the mode first;
    // OPTIONS, MANUAL (`LocationScene.cs:990-991`), NEXT last.
    let m = Menu::for_location_v12(false, false, 3, false, Selected::InGroup);
    assert_eq!(m.lines, [Command::Fight, Command::Info, Command::Inventory, Command::MainMap, Command::Single,
                         Command::Dismiss, Command::StayThere, Command::Options, Command::Manual, Command::Next]);
    let m = Menu::for_location_v12(false, true, 1, false, Selected::Leader);
    assert_eq!(m.lines, [Command::Speak, Command::Info, Command::Inventory, Command::MainMap, Command::Options, Command::Manual, Command::Next]);
    // A city: no mode, no INFO, and the map after the group's lines — which
    // since v1.3 (`d9a1221`) are SINGLE/ALL alone: no REJOIN or DISMISS for
    // a camper there (v1.2 had them).
    let m = Menu::for_location_v12(true, false, 2, true, Selected::Camper);
    assert_eq!(m.lines, [Command::Inventory, Command::All, Command::MainMap,
                         Command::Options, Command::Manual, Command::Next]);
    let m = Menu::for_location_v12(true, false, 5, false, Selected::Camper);
    assert_eq!(m.lines, [Command::Inventory, Command::Single, Command::MainMap, Command::Options, Command::Manual, Command::Next]);
    // Outside a city a camper's REJOIN comes before DISMISS, and only while
    // the group has room.
    let m = Menu::for_location_v12(false, false, 2, true, Selected::Camper);
    assert_eq!(m.lines, [Command::Fight, Command::Info, Command::Inventory, Command::MainMap, Command::All,
                         Command::Rejoin, Command::Dismiss, Command::Options, Command::Manual, Command::Next]);
}

/// v1.2's world map menu has MANUAL after OPTIONS and before NEXT, by
/// mouse and by key (`MapScene.cs:243-266`); v0.9.1's has none.
#[test]
fn the_v12_map_menu_has_manual_before_next() {
    use burngfx::mapscene::{Command, Menu};
    assert_eq!(Menu::for_map_v12(false).lines,
               [Command::Info, Command::Inventory, Command::Statistics, Command::Options, Command::Manual, Command::Next]);
    assert_eq!(Menu::for_map_v12(true).lines[0], Command::Travel);
    assert!(!Menu::for_map(false).lines.contains(&Command::Manual));
    assert_eq!(Command::Manual.manualui(), Some(5));
}

#[test]
fn the_menu_by_keys() {
    use burngfx::mapscene::{Command, Menu};
    let mut m = Menu::for_location_v12_keys(false);
    assert_eq!(m.lines, [Command::MainMap, Command::Inventory, Command::Info, Command::Options, Command::Manual, Command::Next]);
    assert_eq!(Menu::for_location_v12_keys(true).lines, [Command::MainMap, Command::Inventory, Command::Options, Command::Manual, Command::Next]);
    assert_eq!(Menu::for_map_keys().lines, [Command::Inventory, Command::Statistics, Command::Options, Command::Manual, Command::Next]);
    m.show(100, 100, (0, 0, 300, 200));
    // The first Primary only focuses the first line.
    assert_eq!(m.primary(), None);
    assert!(m.is_open());
    // Down twice, up once, round the top: INVENTORY.
    m.focus_step(true);
    m.focus_step(true);
    m.focus_step(false);
    assert_eq!(m.primary(), Some(Command::Inventory));
    assert!(!m.is_open());
    // Up from nothing goes round to the last (NEXT, the sixth line with
    // MANUAL); Back closes and runs nothing.
    m.show(100, 100, (0, 0, 300, 200));
    m.focus_step(false);
    assert_eq!(m.hover, Some(5));
    m.close();
    assert!(!m.is_open());
}

/// v1.2's order (`MainUiOriginalWindow` at v1.2): an empty water skin or
/// food bag first, the trip's shortfall next, the danger after both.
#[test]
fn v12_puts_the_empty_stores_before_the_danger() {
    let hurting = Hazard::Unprotected;
    let shielded = Hazard::Protected;
    let calm = Hazard::None;
    // Where v0.9.1 warned of the danger, v1.2 says what is empty.
    assert_eq!(status(0, 5, 0, hurting), Some(Status::InDanger));
    assert_eq!(status_v12(0, 5, 0, hurting), Some(Status::TooLittleWater));
    assert_eq!(status_v12(3, 0, 0, hurting), Some(Status::TooLittleFood));
    // Empty food before short water for the trip.
    assert_eq!(status(2, 0, 3, calm), Some(Status::TooLittleWater));
    assert_eq!(status_v12(2, 0, 3, calm), Some(Status::TooLittleFood));
    // The trip, then the danger, then the quiet line.
    assert_eq!(status_v12(2, 5, 3, hurting), Some(Status::TooLittleWater));
    assert_eq!(status_v12(4, 2, 3, hurting), Some(Status::TooLittleFood));
    assert_eq!(status_v12(3, 3, 0, hurting), Some(Status::InDanger));
    assert_eq!(status_v12(3, 3, 0, shielded), Some(Status::DangerHere));
    assert_eq!(status_v12(3, 3, 3, calm), None);
}

/// `ShowGroupMenu` (v1.2 `Scenes/LocationScene.cs:997-1034`), Secondary in a
/// place: SINGLE or ALL with a group of more than one; for anybody but the
/// leader REJOIN (a camper, while the group has room), DISMISS, and STAY
/// THERE (a member of the group). Nothing at all when there are no lines
/// (`HasGroupMenuCommands`: the leader alone).
#[test]
fn the_group_menu_has_v12s_group_lines() {
    use burngfx::mapscene::{Command, Menu, Selected};
    assert_eq!(Menu::for_group(false, 2, false, Selected::InGroup).map(|m| m.lines),
               Some(vec![Command::Single, Command::Dismiss, Command::StayThere]));
    assert_eq!(Menu::for_group(false, 3, true, Selected::Leader).map(|m| m.lines), Some(vec![Command::All]));
    assert_eq!(Menu::for_group(false, 1, true, Selected::Camper).map(|m| m.lines),
               Some(vec![Command::Rejoin, Command::Dismiss]));
    assert_eq!(Menu::for_group(false, 5, false, Selected::Camper).map(|m| m.lines),
               Some(vec![Command::Single, Command::Dismiss]));
    assert_eq!(Menu::for_group(false, 1, false, Selected::Leader).map(|m| m.lines), None);
}
