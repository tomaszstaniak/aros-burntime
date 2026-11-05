//! The turn order of v0.9.1's `GameServer`.

use burngfx::turns::{Next, Seat, Turns};

#[test]
fn two_humans_and_two_machines_take_turns_in_order_then_the_day_ends() {
    let mut t = Turns::new(vec![Seat::Human, Seat::Human, Seat::Machine, Seat::Machine]);
    t.begin_day(|_| (false, false));
    assert_eq!(t.next(), Next::Human(0));
    t.finish();
    assert_eq!(t.next(), Next::Human(1));
    t.finish();
    assert_eq!(t.next(), Next::Machine(2));
    t.finish();
    assert_eq!(t.next(), Next::Machine(3));
    t.finish();
    assert_eq!(t.next(), Next::DayEnds);
    t.day_ended();
    assert!(!t.is_finished(0));
}

#[test]
fn the_dead_and_the_travelling_are_not_asked() {
    let mut t = Turns::new(vec![Seat::Human, Seat::Machine]);
    // The human is on the road: the machine plays, and the day ends by itself.
    t.begin_day(|i| (false, i == 0));
    assert_eq!(t.next(), Next::Machine(1));
    t.finish();
    assert_eq!(t.next(), Next::DayEnds);
    t.day_ended();
    t.begin_day(|i| (i == 1, false));
    assert_eq!(t.next(), Next::Human(0));
}

#[test]
fn a_game_resumed_mid_day_skips_who_already_played() {
    let mut t = Turns::new(vec![Seat::Human, Seat::Human]);
    t.begin_day(|_| (false, false));
    assert_eq!(t.next(), Next::Human(0));
    t.finish();
    // A load rebuilds the queue: the finished player stays finished.
    t.begin_day(|_| (false, false));
    assert_eq!(t.next(), Next::Human(1));
}

#[test]
fn a_save_in_the_second_humans_turn_resumes_in_it() {
    let mut t = Turns::new(vec![Seat::Human, Seat::Human, Seat::Machine, Seat::Machine]);
    t.begin_day(|_| (false, false));
    assert_eq!(t.next(), Next::Human(0));
    t.finish();
    assert_eq!(t.next(), Next::Human(1));
    // Written down in player 1's turn, and read back into a new scheduler.
    let saved = t.finished_seats();
    assert_eq!(saved, [0]);
    let mut again = Turns::new(vec![Seat::Human, Seat::Human, Seat::Machine, Seat::Machine]);
    again.restore_finished(&saved);
    again.begin_day(|_| (false, false));
    assert_eq!(again.next(), Next::Human(1), "not player 0 again");
    again.finish();
    assert_eq!(again.next(), Next::Machine(2));
}

#[test]
fn a_machine_in_seat_zero_plays_before_the_human_in_seat_one() {
    // Only the second start-screen slot used: the human is player 1.
    let mut t = Turns::new(vec![Seat::Machine, Seat::Human, Seat::Machine, Seat::Machine]);
    t.begin_day(|_| (false, false));
    assert_eq!(t.next(), Next::Machine(0));
    t.finish();
    assert_eq!(t.next(), Next::Human(1));
}

/// `WaitScene.OnUpdate` (`Scenes/WaitScene.cs:69-83`) counts `app.Clients`,
/// which are the humans only (`GameCreation.cs:211-254`): with every human
/// gone the game is over, whoever else is alive.
#[test]
fn the_game_is_over_when_every_human_is_gone() {
    use burngfx::turns::every_human_gone;
    assert!(every_human_gone([true]));
    assert!(every_human_gone([true, true]));
    assert!(!every_human_gone([true, false]));
    assert!(!every_human_gone([false]));
}
