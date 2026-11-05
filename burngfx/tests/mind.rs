//! What everybody who is not the player does with their time.

use burngfx::game::Point;
use burngfx::mind::{Draws, Mind, CHASE, SECOND, SIGHT};

/// Everywhere is walkable, which is the ordinary case in the middle of a map.
fn open(_: Point) -> bool {
    true
}

/// Nowhere is: the edge of the world, where `SimpleMind` turns round.
fn wall(_: Point) -> bool {
    false
}

const HERE: Point = Point { x: 200, y: 200 };

/// Run a mind until it does something, so a test does not have to know how
/// many frames a decision takes.
fn wander_until_moving(mind: &mut Mind, seed: u64, record: usize) -> Point {
    let mut move_to = HERE;
    for _ in 0..200 {
        mind.wander(&mut Draws::Own(seed), record, HERE, &mut move_to, &open);
        if move_to != HERE {
            return move_to;
        }
    }
    panic!("a mind that never decided anything");
}

// -- wandering --------------------------------------------------------------

/// **Somewhere nearby, and never nowhere.** `25 + rnd % 75` in each axis,
/// with a sign of its own: between 25 and 99 pixels each way, so a step is
/// always a real journey and never a twitch.
#[test]
fn a_wander_is_between_25_and_99_in_each_axis() {
    for record in 0..40 {
        let mut mind = Mind::default();
        let to = wander_until_moving(&mut mind, 1993, record);
        let (dx, dy) = ((to.x - HERE.x).abs(), (to.y - HERE.y).abs());
        assert!((25..=99).contains(&dx), "dx {dx}");
        assert!((25..=99).contains(&dy), "dy {dy}");
    }
}

/// Both signs come up, so people do not all drift one way.
#[test]
fn they_go_in_every_direction() {
    let mut seen = [false; 4];
    for record in 0..60 {
        let mut mind = Mind::default();
        let to = wander_until_moving(&mut mind, 7, record);
        let quadrant = ((to.x > HERE.x) as usize) * 2 + (to.y > HERE.y) as usize;
        seen[quadrant] = true;
    }
    assert!(seen.iter().all(|&s| s), "only some directions were ever chosen: {seen:?}");
}

/// **A person at the edge turns round**: `SimpleMind` reverses both axes
/// when the spot it picked is off the mask. It is the one thing that keeps
/// them off the border, and it is why the check is in the person's mind and
/// not in the walking.
#[test]
fn a_person_turns_round_at_the_edge() {
    let mut open_mind = Mind::default();
    let mut walled_mind = Mind::default();
    let mut open_to = HERE;
    let mut walled_to = HERE;

    // The same stream in both, so the only difference is the mask.
    for _ in 0..200 {
        open_mind.wander(&mut Draws::Own(11), 3, HERE, &mut open_to, &open);
        walled_mind.wander(&mut Draws::Own(11), 3, HERE, &mut walled_to, &wall);
        if open_to != HERE {
            break;
        }
    }

    assert_ne!(open_to, HERE);
    assert_eq!(
        walled_to,
        Point::new(2 * HERE.x - open_to.x, 2 * HERE.y - open_to.y),
        "the same offset, both axes reversed"
    );
}

/// Resting is counted down and nothing is decided while it runs.
#[test]
fn a_resting_mind_decides_nothing() {
    let mut mind = Mind { rest: 5, ..Mind::default() };
    let mut move_to = HERE;

    for _ in 0..5 {
        mind.wander(&mut Draws::Own(1), 1, HERE, &mut move_to, &open);
        assert_eq!(move_to, HERE, "still resting");
    }
    assert_eq!(mind.rest, 0);

    mind.wander(&mut Draws::Own(1), 1, HERE, &mut move_to, &open);
    assert_ne!(move_to, HERE, "and then it decides");
}

/// **The pause after arriving is the short one the first time.** `first` is
/// set by every walk and spent by the arrival, so a character alternates
/// between half a second and a decision rather than resting twice over.
#[test]
fn the_first_pause_after_a_walk_is_half_a_second() {
    // Walk until one is actually chosen: the first decision may be the
    // one-in-nine rest instead.
    let mut mind = Mind::default();
    let mut move_to = wander_until_moving(&mut mind, 5, 5);
    assert!(mind.first, "a walk was chosen");

    // Arrived: the mind is asked again from the place it was walking to.
    let there = move_to;
    mind.wander(&mut Draws::Own(5), 5, there, &mut move_to, &open);
    assert_eq!(mind.rest, SECOND / 2, "half a second");
    assert!(!mind.first);
}

// -- creatures --------------------------------------------------------------

/// A creature with nobody in the location does what a person does: wanders.
#[test]
fn a_creature_alone_just_wanders() {
    let mut mind = Mind::default();
    let mut move_to = HERE;
    for _ in 0..200 {
        assert!(!mind.prowl(&mut Draws::Own(3), 3, HERE, &mut move_to, None, &open), "nothing to bite");
        if move_to != HERE {
            return;
        }
    }
    panic!("it never went anywhere");
}

/// **It only looks at a player within two hundred.** Further off and it has
/// not noticed you at all.
#[test]
fn a_player_too_far_away_is_not_noticed() {
    let mut mind = Mind::default();
    let mut move_to = HERE;
    let far = Point::new(HERE.x + SIGHT + 1, HERE.y);
    for _ in 0..300 {
        mind.prowl(&mut Draws::Own(3), 3, HERE, &mut move_to, Some(far), &open);
        assert!(!mind.charging, "it should not have chosen anybody");
    }
}

/// Within two hundred it comes for you — not at once, and not every time:
/// a one-in-three chance drawn afresh at each decision — not every third decision —
/// and three to seven seconds between decisions.
#[test]
fn it_comes_for_you_one_time_in_three() {
    let mut charged = 0;
    for record in 0..60 {
        let mut mind = Mind::default();
        let mut move_to = HERE;
        let near = Point::new(HERE.x + 100, HERE.y);
        for _ in 0..40 {
            mind.prowl(&mut Draws::Own(9), record, HERE, &mut move_to, Some(near), &open);
        }
        if mind.charging || mind.wait_after_attack > 0 {
            // It got as far as deciding something either way.
            charged += mind.charging as i32;
        }
    }
    assert!((5..=45).contains(&charged), "about a third of sixty, got {charged}");
}

/// **The wait between decisions is three to seven seconds.**
#[test]
fn it_waits_three_to_seven_seconds_between_decisions() {
    for record in 0..40 {
        let mut mind = Mind::default();
        let mut move_to = HERE;
        let near = Point::new(HERE.x + 50, HERE.y);
        // The first call spends the wait that starts at zero; a later one
        // decides and sets a new one — later rather than next, because a
        // mind that has just chosen to rest decides nothing while it does.
        let mut set = None;
        for _ in 0..200 {
            mind.prowl(&mut Draws::Own(4), record, HERE, &mut move_to, Some(near), &open);
            if mind.wait_after_attack > 0 {
                set = Some(mind.wait_after_attack);
                break;
            }
        }
        let wait = set.expect("it never decided anything");
        assert!(
            (3 * SECOND as i32..=7 * SECOND as i32).contains(&wait),
            "waited {wait} frames"
        );
    }
}

/// **It bites when it gets within twenty, and then backs off.** The same
/// reach `CreatureMind` uses, and the same "forget the target, go
/// somewhere" that follows.
#[test]
fn it_bites_inside_twenty_and_backs_off() {
    let mut mind = Mind { charging: true, wait_after_attack: 100, ..Mind::default() };
    let mut move_to = HERE;
    let close = Point::new(HERE.x + 19, HERE.y);

    let bit = mind.prowl(&mut Draws::Own(2), 2, HERE, &mut move_to, Some(close), &open);

    assert!(bit, "inside twenty");
    assert!(!mind.charging, "and it has let go of the target");
    assert_ne!(move_to, HERE, "and gone somewhere else in the same breath");
}

/// One step further and it does not: nineteen bites, twenty does not.
#[test]
fn twenty_is_too_far() {
    let mut mind = Mind { charging: true, wait_after_attack: 100, ..Mind::default() };
    let mut move_to = HERE;
    let just_out = Point::new(HERE.x + 20, HERE.y);

    assert!(!mind.prowl(&mut Draws::Own(2), 2, HERE, &mut move_to, Some(just_out), &open));
    assert!(mind.charging, "still coming");
}

/// **A chase it cannot finish is given up after three seconds**, which is
/// what stops a creature following somebody for ever.
#[test]
fn a_chase_times_out() {
    let mut mind = Mind { charging: true, wait_after_attack: 10_000, ..Mind::default() };
    let mut move_to = HERE;
    let away = Point::new(HERE.x + 150, HERE.y);

    for _ in 0..=CHASE {
        mind.prowl(&mut Draws::Own(2), 2, HERE, &mut move_to, Some(away), &open);
    }
    assert!(!mind.charging, "it gave up after {CHASE} frames");
}

// -- being spoken to --------------------------------------------------------

/// `RequestToTalk`: stop where you are, and wait four seconds. Otherwise
/// the person being asked for work walks off mid-question.
#[test]
fn somebody_spoken_to_stands_still() {
    let mut mind = Mind::default();
    let mut move_to = Point::new(400, 400);

    mind.asked_to_talk(HERE, &mut move_to);

    assert_eq!(move_to, HERE, "the walk is cancelled");
    assert_eq!(mind.rest, 4 * SECOND);
}

// -- the same twice ---------------------------------------------------------

/// **A mind is a function of the seed, the record and how many numbers it
/// has drawn** — which is why a save can carry a counter instead of a
/// generator, and why a resumed walk is the same walk.
#[test]
fn the_same_counter_gives_the_same_decisions() {
    let walk = |from: Mind| {
        let mut mind = from;
        let mut move_to = HERE;
        let mut steps = Vec::new();
        for _ in 0..400 {
            mind.wander(&mut Draws::Own(1993), 42, HERE, &mut move_to, &open);
            steps.push(move_to);
        }
        (mind, steps)
    };

    let (ended, first) = walk(Mind::default());
    let (again, second) = walk(Mind::default());
    assert_eq!(first, second, "the same from the start");

    // And picking up from a saved counter continues the same stream.
    let (_, from_save) = walk(Mind { draws: ended.draws, ..Mind::default() });
    let (_, carried_on) = walk(Mind { draws: again.draws, ..Mind::default() });
    assert_eq!(from_save, carried_on);
}


// -- the stream is part of the save format ------------------------------------

/// **These numbers are pinned on purpose.** A save carries each character's
/// draw counter and nothing else about their randomness, so the counter only
/// means something while the stream it indexes is unchanged. Change
/// `Mind::draw` — its mixing constants, its finalizer, the order of draws in a
/// decision — and every saved game resumes into a different world without a
/// word. If this test has to change, the save version has to change with it.
#[test]
fn the_stream_behind_a_saved_counter_does_not_move() {
    let open = |_: Point| true;
    let mut mind = Mind::default();
    let mut to = Point::new(200, 200);
    let mut seen: Vec<(Point, u32)> = Vec::new();
    for _ in 0..400 {
        mind.wander(&mut Draws::Own(1993), 42, Point::new(200, 200), &mut to, &open);
        if seen.last() != Some(&(to, mind.draws)) {
            seen.push((to, mind.draws));
        }
        if !seen.is_empty() {
            break;
        }
    }
    assert_eq!(
        seen,
        vec![
        (Point::new(291, 265), 5)
        ],
        "SimpleMind's stream for seed 1993, record 42"
    );

    let mut draws = 0;
    let mut move_to = Point::new(200, 200);
    burngfx::mind::follow(&mut Draws::Own(1993), 7, &mut draws, Point::new(205, 200), &mut move_to, Point::new(200, 200));
    assert_eq!((move_to, draws), (Point::new(211, 207), 1), "FellowerMind's step aside for id 7");
}

// -- a v1.2 game: the game's generator ---------------------------------------

use burngfx::spawn::Rng;

/// **v1.2's creature returns on the frame it bites** (`AI/CreatureMind.cs:56-58`:
/// `AttackWithoutRetaliation(attack); attack = null; return;`): no wait, no
/// rest roll, no wander — the blow's own flight moves it. The port's mind
/// went on to pick somewhere to wander, four draws for a destination the
/// flight then overwrote; a legacy game still does.
#[test]
fn a_v12_creature_does_nothing_else_on_the_frame_it_bites() {
    let mut mind = Mind { charging: true, wait_after_attack: -1, ..Mind::default() };
    let mut move_to = HERE;
    let close = Point::new(HERE.x + 5, HERE.y);
    let mut rng = Rng::game(1993);
    let before = rng.clone();
    assert!(mind.prowl(&mut Draws::Game(&mut rng), 2, HERE, &mut move_to, Some(close), &open));
    assert!(!mind.charging);
    assert_eq!(move_to, HERE, "no wander chosen");
    assert_eq!(mind.wait_after_attack, -1, "no wait counted or drawn");
    assert_eq!(rng, before, "not a number drawn");

    let mut legacy = Mind { charging: true, wait_after_attack: -1, ..Mind::default() };
    let mut legacy_to = HERE;
    assert!(legacy.prowl(&mut Draws::Own(2), 2, HERE, &mut legacy_to, Some(close), &open));
    assert_ne!(legacy_to, HERE, "a legacy creature goes somewhere in the same breath");
}

/// The wander step in upstream's order — both spans, then both signs:
/// `go.x = Next() % 75 + 25; go.y = …; go.x *= (Next() % 2) * 2 - 1; go.y *= …`
/// (`AI/SimpleMind.cs:43-47`, `AI/CreatureMind.cs:118-122`).
#[test]
fn a_v12_wander_draws_spans_then_signs() {
    let mut rng = Rng::game(42);
    for _ in 0..200 {
        let mut mind = Mind::default();
        let mut copy = rng.clone();
        // A fresh mind's first decision is the rest roll, then the step.
        let rest = copy.next_int() % 9 == 0;
        let (sx, sy) = (copy.next_int() % 75 + 25, copy.next_int() % 75 + 25);
        let (gx, gy) = (copy.next_int() % 2 * 2 - 1, copy.next_int() % 2 * 2 - 1);
        let mut move_to = HERE;
        mind.wander(&mut Draws::Game(&mut rng), 1, HERE, &mut move_to, &open);
        if rest {
            assert_eq!(move_to, HERE);
            assert_eq!(mind.rest, 2 * SECOND);
            continue;
        }
        assert_eq!(move_to, Point::new(HERE.x + sx * gx, HERE.y + sy * gy));
        assert_eq!(rng, copy, "four draws and the rest roll, nothing more");
    }
}

/// `CreatureMind`'s odds on the game's generator: the attack one time in
/// three (`Next() % 3 == 0`), the wait after it 3 to 7 seconds evenly
/// (`3 + Next() % 5`).
#[test]
fn a_v12_creature_attacks_one_time_in_three_and_waits_three_to_seven_seconds() {
    let mut rng = Rng::game(7);
    let (mut attacks, mut waits) = (0u32, [0u32; 5]);
    const N: u32 = 30_000;
    for _ in 0..N {
        let mut mind = Mind { wait_after_attack: -1, rest: 0, first: false, ..Mind::default() };
        // Not arrived, so no wander decision follows.
        let mut move_to = Point::new(HERE.x + 50, HERE.y);
        mind.prowl(&mut Draws::Game(&mut rng), 3, HERE, &mut move_to, Some(Point::new(HERE.x + 100, HERE.y)), &open);
        attacks += mind.charging as u32;
        waits[(mind.wait_after_attack as u32 / SECOND - 3) as usize] += 1;
    }
    let share = attacks as f64 / N as f64;
    assert!((share - 1.0 / 3.0).abs() < 0.015, "{share}");
    for w in waits {
        assert!((w as f64 / N as f64 - 0.2).abs() < 0.015, "{waits:?}");
    }
}

/// **A respawned or dismissed person does not replay old decisions.**
/// `People::returned` and `People::dismissed` give the person a new
/// `Mind::default()`, which in a legacy game restarts their own stream at
/// draw nought — the same decisions as the first time. In a v1.2 game the
/// numbers come from the game's generator, which has moved on.
#[test]
fn a_v12_mind_made_again_does_not_replay_its_first_life() {
    let life = |rng: &mut Rng| -> Vec<Point> {
        let mut mind = Mind::default();
        let mut at = HERE;
        let mut path = Vec::new();
        for _ in 0..400 {
            let mut to = at;
            mind.wander(&mut Draws::Game(rng), 42, at, &mut to, &open);
            if to != at {
                path.push(to);
                at = to;
            }
        }
        path
    };
    let mut rng = Rng::game(1993);
    let first = life(&mut rng);
    let second = life(&mut rng);
    assert!(first.len() > 5);
    assert_ne!(first, second);
}

/// **v1.2's creature and the game-wide cooldown** (`CreatureMind.cs:47-60`,
/// `States/ClassicGame.cs:106-119`): in reach with the cooldown running it
/// does not bite, does not count the chase, and keeps its target; at nought
/// it bites the target it holds, sets a second's cooldown and draws nothing.
#[test]
fn a_v12_creature_waits_for_the_cooldown_and_bites_whom_it_holds() {
    use burngfx::mind::{Held, CREATURE_COOLDOWN};
    let mut mind = Mind { charging: true, target: 7, wait_after_attack: 1000, ..Mind::default() };
    let mut move_to = Point::new(HERE.x + 50, HERE.y);
    let held = Some(Held { at: Point::new(HERE.x + 5, HERE.y), dead: false, deterred: false });
    let elsewhere = Some((Point::new(HERE.x + 1, HERE.y), 3));
    let mut rng = Rng::game(1);
    let mut cooldown = 4;
    for _ in 0..3 {
        let bit = mind.prowl_v12(&mut Draws::Game(&mut rng), 2, HERE, &mut move_to, false, held, elsewhere, &mut cooldown, &open);
        assert_eq!(bit, None);
        cooldown -= 1;
    }
    assert!(mind.charging && mind.chasing == 0 && mind.target == 7);
    let before = rng.clone();
    cooldown = 0;
    let bit = mind.prowl_v12(&mut Draws::Game(&mut rng), 2, HERE, &mut move_to, false, held, elsewhere, &mut cooldown, &open);
    assert_eq!(bit, Some(7), "the one it holds, not the nearer one");
    assert_eq!(cooldown, CREATURE_COOLDOWN);
    assert!(!mind.charging);
    assert_eq!(rng, before, "not a number drawn");
}

/// A deterrent on the held target drops it, and in a city a creature
/// neither keeps a charge nor rolls a new one (`CreatureMind.cs:31-44,70`).
#[test]
fn a_v12_creature_lets_go_for_a_deterrent_and_in_a_city() {
    use burngfx::mind::Held;
    let mut rng = Rng::game(1);
    let mut cooldown = 0;
    let near = Some(Held { at: Point::new(HERE.x + 5, HERE.y), dead: false, deterred: true });
    let mut mind = Mind { charging: true, wait_after_attack: 1000, ..Mind::default() };
    let mut move_to = Point::new(HERE.x + 50, HERE.y);
    assert_eq!(mind.prowl_v12(&mut Draws::Game(&mut rng), 2, HERE, &mut move_to, false, near, None, &mut cooldown, &open), None);
    assert!(!mind.charging);

    let mut mind = Mind { charging: true, wait_after_attack: -1, ..Mind::default() };
    let mut move_to = Point::new(HERE.x + 50, HERE.y);
    let held = Some(Held { at: Point::new(HERE.x + 5, HERE.y), dead: false, deterred: false });
    let before = rng.clone();
    assert_eq!(mind.prowl_v12(&mut Draws::Game(&mut rng), 2, HERE, &mut move_to, true, held, Some((HERE, 0)), &mut cooldown, &open), None);
    assert!(!mind.charging);
    assert_eq!(move_to, HERE, "the approach is cancelled");
    // Standing where it walks to, it decides: the one roll for a rest
    // (`Next() % 9`), and no attack roll before it.
    let mut one = before.clone();
    one.next_int();
    assert_eq!(rng, one, "no attack is rolled");
}
