use burngfx::slots::*;

#[test]
fn every_sav_in_the_directory_is_a_slot_whoever_wrote_it() {
    let listing = ["game.sav", "MAX.SAV", "index", "alpha.sav.bak", "alpha.sav.new", "beta.sav", "notes.txt"];
    assert_eq!(from_listing(listing), vec!["beta", "game", "max"]);
}

#[test]
fn at_most_ten_and_each_once() {
    let names: Vec<String> = (0..12).map(|n| format!("s{n:02}.sav")).collect();
    let shown = from_listing(names.iter().map(|s| s.as_str()));
    assert_eq!(shown.len(), 10);
    assert_eq!(shown[0], "s00");
    assert_eq!(from_listing(["a.sav", "A.SAV"]), vec!["a"]);
}

#[test]
fn a_slot_is_its_own_file() {
    assert_eq!(file_of("saves/", "ALPHA"), "saves/alpha.sav");
    assert_ne!(file_of("saves/", "one"), file_of("saves/", "two"));
}
