//! Conversations as v0.9.1's `Dialog` chooses them.

use burngfx::dialog::*;

fn mercenary() -> Speaker {
    Speaker { record: 7, men_file: 3, is_trader: false, can_work: true, others: false, by_leader: true }
}

#[test]
fn the_first_meeting_is_a_talk_and_then_greetings_alternate() {
    let mut m = Memory::default();
    let s = mercenary();
    let c = conversation(&mut m, &s, Kind::Greeting, 1, || unreachable!());
    assert_eq!((c.file, c.section), (3, 0), "met for the first time: section 0");
    assert_eq!(c.choices[1].action, Action::HireRequirements);
    assert_eq!(c.choices[2], Choice { burn: Some(490), action: Action::Exit });
    let c = conversation(&mut m, &s, Kind::Greeting, 1, || unreachable!());
    assert_eq!(c.section, 3, "the last dialog was this one: hello again");
    let other = Speaker { record: 8, ..s };
    conversation(&mut m, &other, Kind::Greeting, 1, || unreachable!());
    let c = conversation(&mut m, &s, Kind::Greeting, 1, || unreachable!());
    assert_eq!(c.section, 2, "somebody else was spoken to in between");
}

#[test]
fn a_full_group_is_not_offered_work_and_a_trader_trades() {
    let mut m = Memory { talked: vec![7, 9], last: None };
    let c = conversation(&mut m, &mercenary(), Kind::Greeting, 5, || unreachable!());
    assert_eq!(c.choices[0].action, Action::None);
    let trader = Speaker { record: 9, men_file: 12, is_trader: true, can_work: true, others: false, by_leader: true };
    let c = conversation(&mut m, &trader, Kind::Greeting, 1, || unreachable!());
    assert_eq!(c.choices[0], Choice { burn: Some(500), action: Action::Trade });
}

#[test]
fn somebody_elses_man_speaks_from_510_and_offers_nothing() {
    let mut m = Memory { talked: vec![7], last: None };
    let s = Speaker { others: true, ..mercenary() };
    let c = conversation(&mut m, &s, Kind::Greeting, 1, || unreachable!());
    assert_eq!(c.file, 510);
    assert_eq!(c.choices[0].action, Action::None);
}

#[test]
fn asking_about_work_picks_the_section_by_the_answer() {
    let mut m = Memory { talked: vec![7], last: None };
    let s = mercenary();
    let c = conversation(&mut m, &s, Kind::Hire, 1, || HireAnswer::Accepts(4));
    assert_eq!((c.section, c.item), (4, Some(4)));
    assert_eq!(c.choices[1], Choice { burn: Some(498), action: Action::Hire });
    let c = conversation(&mut m, &s, Kind::Hire, 1, || HireAnswer::WantsSomething(2));
    assert_eq!((c.section, c.item), (5, Some(2)));
    assert_eq!(c.choices[1].action, Action::HireRequirements);
    let c = conversation(&mut m, &s, Kind::Hire, 1, || HireAnswer::TooInexperienced);
    assert_eq!(c.section, 7);
    let c = conversation(&mut m, &s, Kind::Profession, 1, || HireAnswer::Accepts(4));
    assert_eq!(c.section, 6);
}

#[test]
fn sections_are_counted_by_their_markers_and_keep_empty_lines() {
    let text = "a}\nb}\n}#\nc}\n}\nd}\n}#\n}#\n";
    assert_eq!(section(text, 0), vec!["a", "b"]);
    assert_eq!(section(text, 1), vec!["c", "", "d"]);
    assert!(section(text, 2).is_empty());
    assert!(section(text, 9).is_empty());
}

#[test]
fn pages_are_two_lines_and_three_show_at_once_only_after_a_choice() {
    let mut p = Pages::open(3);
    assert!(!p.choosing);
    p.more(3);
    assert!(p.choosing);
    assert!(Pages::after_choice(3).choosing);
    let mut p = Pages::open(6);
    p.more(6);
    assert!(!p.choosing);
    p.more(6);
    assert!(p.choosing);
}

#[test]
fn a_companion_who_speaks_is_offered_only_talk() {
    use burngfx::dialog::{conversation, Action, HireAnswer, Kind, Memory};
    let mut m = Memory::default();
    m.talked.push(7);
    m.talked.push(9);
    let asked = Speaker { by_leader: false, ..mercenary() };
    let c = conversation(&mut m, &asked, Kind::Greeting, 2, || HireAnswer::TooInexperienced);
    assert_eq!(c.choices.map(|c| c.action), [Action::None, Action::Talk, Action::Exit]);
    let trader = Speaker { record: 9, men_file: 12, is_trader: true, can_work: true, others: false, by_leader: false };
    let c = conversation(&mut m, &trader, Kind::Greeting, 2, || HireAnswer::TooInexperienced);
    assert_eq!(c.choices.map(|c| c.action), [Action::None, Action::Talk, Action::Exit]);
}

/// `GetCaptureConversation` (v1.2 `Logic/Interaction/Dialog.cs:80-99`, the
/// same at v0.9.1): `men_500?s3`, two empty lines and one answer, `burn?490`,
/// an Exit. STAY THERE on another player's ground opens it
/// (`LocationScene.OnMenuMakeCamp`, :1164-1180) and nothing else happens.
#[test]
fn the_capture_conversation_is_men_500_section_3_with_one_exit() {
    let c = capture();
    assert_eq!((c.file, c.section), (500, 3));
    assert_eq!(c.choices[0], Choice { burn: None, action: Action::None });
    assert_eq!(c.choices[1], Choice { burn: None, action: Action::None });
    assert_eq!(c.choices[2], Choice { burn: Some(490), action: Action::Exit });
    assert!(!c.substitutes);
}
