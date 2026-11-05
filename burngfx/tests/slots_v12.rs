//! v1.2's Saves page arithmetic: the file names it makes, the names it
//! shows and the time it shows them with.

use burngfx::save::Stamp;
use burngfx::slots::*;

#[test]
fn a_new_save_is_numbered_past_the_players_highest() {
    let existing: Vec<String> = ["gregory-save001", "gregory-save007", "anna-save009", "game"].iter().map(|s| s.to_string()).collect();
    assert_eq!(next_save_name("Gregory", &existing), "gregory-save008");
    assert_eq!(next_save_name("Anna", &existing), "anna-save010");
    assert_eq!(next_save_name("Bob", &existing), "bob-save001");
    assert_eq!(next_save_name("Max Power!", &[]), "max-power-save001");
    assert_eq!(next_save_name("???", &[]), "player-save001");
}

#[test]
fn slugs_keep_letters_and_digits() {
    assert_eq!(slug("  Max  Power 2 "), "max-power-2");
    assert_eq!(slug("ÄÖÜ"), "player");
}

#[test]
fn the_shown_name_drops_the_number_it_was_given() {
    assert_eq!(display_name("gregory-save008"), "gregory");
    assert_eq!(display_name("game"), "game");
    assert_eq!(display_name("-save001"), "-save001");
    assert_eq!(display_name("x-savex"), "x-savex");
}

#[test]
fn every_save_is_listed() {
    let names = (0..14).map(|i| format!("s{i:02}.sav")).chain(["notes.txt".to_string()]).collect::<Vec<_>>();
    assert_eq!(every_save(names.iter().map(|s| s.as_str())).len(), 14);
}

#[test]
fn stamps_read_as_month_day_and_time() {
    // 2026-10-09 00:12 UTC is Unix 1_791_504_720.
    let t = stamp_of_unix(1_791_504_720);
    assert_eq!(stamp_text(t), "Oct-09 00:12");
    // dos.library's day 0 is 1 January 1978.
    assert_eq!(stamp_text(Stamp { days: 0, minute: 23 * 60 + 59 }), "Jan-01 23:59");
    assert_eq!(stamp_text(Stamp { days: 59, minute: 0 }), "Mar-01 00:00");
}
