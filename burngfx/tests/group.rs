//! A group with positions: following, joining a fight by reach, and a world
//! that steps at its own rate rather than the renderer's.

use burngfx::clock::{Clock, Pace};
use burngfx::combat::joins_group_attack;
use burngfx::game::Point;
use burngfx::mind::{follow, FOLLOW_MARGIN};

fn dist(a: Point, b: Point) -> f32 {
    (((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)) as f32).sqrt()
}

// -- following ------------------------------------------------------------

/// **Far behind, they head for the leader** — once their destination has
/// drifted more than 140 from where the leader is.
#[test]
fn a_companion_far_behind_heads_for_the_leader() {
    let leader = Point::new(400, 400);
    let at = Point::new(100, 400);
    let mut move_to = at;
    let mut draws = 0;

    follow(&mut burngfx::mind::Draws::Own(1), 1, &mut draws, at, &mut move_to, leader);

    assert_eq!(move_to, leader);
    assert_eq!(draws, 1, "the angle is drawn even though nothing uses it");
}

/// And if they are already heading somewhere within 140 of the leader, they
/// leave it alone rather than re-aim every frame.
#[test]
fn a_destination_close_enough_is_not_changed() {
    let leader = Point::new(400, 400);
    let at = Point::new(100, 400);
    let mut move_to = Point::new(300, 400); // 100 from the leader
    let mut draws = 0;

    follow(&mut burngfx::mind::Draws::Own(1), 1, &mut draws, at, &mut move_to, leader);

    assert_eq!(move_to, Point::new(300, 400));
    assert_eq!(draws, 0, "nothing drawn when nothing is decided");
}

/// **Between 31 and 150 the leash is shorter**: re-aim once the destination
/// is more than 20 from the leader.
#[test]
fn the_middle_band_has_a_shorter_leash() {
    let leader = Point::new(200, 200);
    let at = Point::new(260, 200); // 60 away
    let mut stays = Point::new(215, 200); // 15 from leader: close enough
    let mut moves = Point::new(250, 200); // 50 from leader: too far
    let (mut a, mut b) = (0, 0);

    follow(&mut burngfx::mind::Draws::Own(3), 3, &mut a, at, &mut stays, leader);
    follow(&mut burngfx::mind::Draws::Own(3), 3, &mut b, at, &mut moves, leader);

    assert_eq!(stays, Point::new(215, 200));
    assert_eq!(moves, leader);
}

/// **Between 15 and 30 they simply stand.** Nothing is decided at all.
#[test]
fn within_thirty_they_stand() {
    let leader = Point::new(200, 200);
    let at = Point::new(222, 200);
    let mut move_to = at;
    let mut draws = 0;

    follow(&mut burngfx::mind::Draws::Own(5), 5, &mut draws, at, &mut move_to, leader);

    assert_eq!(move_to, at);
    assert_eq!(draws, 0);
}

/// **Too close, they step aside** — to a point fourteen from the leader, at
/// a random angle, but only once their own destination is on top of the
/// leader.
#[test]
fn too_close_they_step_fourteen_aside() {
    let leader = Point::new(200, 200);
    for id in 0..30 {
        let at = Point::new(205, 200);
        let mut move_to = leader;
        let mut draws = 0;

        follow(&mut burngfx::mind::Draws::Own(9), id, &mut draws, at, &mut move_to, leader);

        let d = dist(move_to, leader);
        assert!(
            (d - FOLLOW_MARGIN as f32).abs() <= 1.5,
            "stepped {d} aside, not {FOLLOW_MARGIN}"
        );
        assert_eq!(draws, 1);
    }
}

// -- joining a fight --------------------------------------------------------

/// **Only those within 25 join.** Strictly less than: twenty-five exactly
/// stays out.
#[test]
fn a_companion_joins_the_attack_only_within_twenty_five() {
    let leader = Point::new(100, 100);
    assert!(joins_group_attack(leader, Point::new(124, 100)));
    assert!(!joins_group_attack(leader, Point::new(125, 100)));
    assert!(!joins_group_attack(leader, Point::new(120, 120)), "28 on the diagonal");
    assert!(joins_group_attack(leader, leader), "the leader joins their own attack");
}

// -- the clock ---------------------------------------------------------------

/// **A scripted run is one step per frame**, whatever the machine does, so
/// the same words give the same file everywhere.
#[test]
fn a_script_steps_once_a_frame_whatever_the_clock() {
    let mut clock = Clock::new(Pace::Scripted);
    for now in [0, 5, 5000, 5001, 999_999] {
        assert_eq!(clock.due(now), 1);
    }
}

/// **Played, the world steps with time rather than with frames.** A slow
/// machine draws fewer pictures of the same walk; it does not walk slower.
#[test]
fn played_the_world_keeps_time() {
    let mut fast = Clock::new(Pace::Played);
    let mut slow = Clock::new(Pace::Played);

    // One second, drawn every 10 ms on one machine and every 200 ms on the
    // other.
    let (mut fast_steps, mut slow_steps) = (fast.due(0), slow.due(0));
    for t in (10..=1000).step_by(10) {
        fast_steps += fast.due(t);
    }
    for t in (200..=1000).step_by(200) {
        slow_steps += slow.due(t);
    }

    assert_eq!(fast_steps, slow_steps, "the same second of world on both");
    assert!((17..=19).contains(&fast_steps), "about eighteen steps, got {fast_steps}");
}

/// **A step is exactly an eighteenth of a second**, so an hour played is
/// 64 800 steps and every timer counted in steps keeps real time. v1.2's
/// timers are float seconds of the measured frame time
/// (`Burntime.MonoGame/GameThread.cs:44-62`); a step of `1000 / 18 = 55` ms
/// by integer division made the port's world 1 % fast (the register's
/// RULE-44).
#[test]
fn an_hour_played_is_an_hour_of_steps() {
    let mut clock = Clock::new(Pace::Played);
    let mut steps = clock.due(0) as u64 - 1;
    for t in (16..=3_600_000u32).step_by(16) {
        steps += clock.due(t) as u64;
    }
    assert_eq!(steps, 18 * 3600);
}

/// **A stall is caught up in full.** v1.2 hands the whole measured frame
/// time to the world's update, unclamped (`GameThread.cs:44-62`), so ten
/// seconds without a frame are ten seconds of world; the port's cap of five
/// steps had no measured reason behind it (the register's RULE-45).
#[test]
fn a_long_stall_is_caught_up() {
    let mut clock = Clock::new(Pace::Played);
    clock.due(0);
    assert_eq!(clock.due(10_000), 180);
    assert_eq!(clock.due(10_056), 1, "and then back to normal time");
}

// -- eating near the leader ----------------------------------------------------

/// **A companion fifty or more away goes without.** `Group.Eat(leader, …)`
/// skips them, and the one beside the leader is fed a level at a time as
/// usual.
#[test]
fn a_far_companion_is_not_fed_from_the_leaders_hand() {
    use burngfx::game::{group_eat_near, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = |food| CharacterNeeds { food, max_food: MAX_FOOD, water: 2, max_water: MAX_WATER, health: 100 };
    let leader_at = Point::new(100, 100);
    let mut leader = needs(8);
    let mut group = vec![
        Companion { needs: needs(2), at: Point::new(130, 100), ..Companion::new(1) }, // 30 away
        Companion { needs: needs(2), at: Point::new(150, 100), ..Companion::new(2) }, // 50 away
    ];

    let left = group_eat_near(&mut leader, leader_at, &mut group, 4);

    assert_eq!(left, 0, "all four points went to somebody");
    assert_eq!(group[0].needs.food, 6, "the near one, from 2 to 6");
    assert_eq!(group[1].needs.food, 2, "the one at fifty, not a point");
    assert_eq!(leader.food, 8, "the leader was fuller than anyone and got nothing");
}

/// And the day's feeding does not filter: that is `group_eat`, with no leader.
#[test]
fn the_days_feeding_reaches_everybody() {
    use burngfx::game::{group_eat, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = |food| CharacterNeeds { food, max_food: MAX_FOOD, water: 2, max_water: MAX_WATER, health: 100 };
    let mut leader = needs(8);
    let mut group = vec![Companion { needs: needs(2), at: Point::new(900, 900), ..Companion::new(2) }];
    group_eat(&mut leader, &mut group, 3);
    assert_eq!(group[0].needs.food, 5, "however far off they are");
}

/// **A spring with more than the group can drink keeps the rest.** Entering
/// the room is `Source.Reserve = group.Drink(leader, Source.Reserve)`: ten in
/// the spring, a leader at 4 and a near companion at 3, a far one at 0.
/// Level 3 is the companion's, level 4 both; the far one is skipped, and
/// seven stay in the spring rather than being drunk by nobody.
#[test]
fn the_spring_keeps_what_the_near_group_did_not_drink() {
    use burngfx::game::{group_drink_near, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = |water| CharacterNeeds { food: 5, max_food: MAX_FOOD, water, max_water: MAX_WATER, health: 100 };
    let leader_at = Point::new(100, 100);
    let mut leader = needs(4);
    let mut group = vec![
        Companion { needs: needs(3), at: Point::new(110, 100), ..Companion::new(1) },
        Companion { needs: needs(0), at: Point::new(100, 160), ..Companion::new(2) },
    ];
    let reserve = group_drink_near(&mut leader, leader_at, &mut group, 10);
    assert_eq!(reserve, 7);
    assert_eq!((leader.water, group[0].needs.water, group[1].needs.water), (5, 5, 0));
}

/// **A spring with too little runs dry in group order.** Two in it, three
/// thirsty near the leader: level 0 is companion 1's and companion 2's, and
/// the leader at 1 gets none.
#[test]
fn a_short_spring_runs_dry_in_group_order() {
    use burngfx::game::{group_drink_near, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = |water| CharacterNeeds { food: 5, max_food: MAX_FOOD, water, max_water: MAX_WATER, health: 100 };
    let leader_at = Point::new(100, 100);
    let mut leader = needs(1);
    let mut group = vec![
        Companion { needs: needs(0), at: Point::new(110, 100), ..Companion::new(1) },
        Companion { needs: needs(0), at: Point::new(90, 100), ..Companion::new(2) },
        Companion { needs: needs(0), at: Point::new(100, 120), ..Companion::new(3) },
    ];
    let reserve = group_drink_near(&mut leader, leader_at, &mut group, 2);
    assert_eq!(reserve, 0);
    assert_eq!(
        (leader.water, group[0].needs.water, group[1].needs.water, group[2].needs.water),
        (1, 1, 1, 0),
        "the third near companion was next in line when it ran out"
    );
}

/// **An item eaten by hand goes whole, however little was eaten.** A snake
/// worth seven to a group with two points of room: two are eaten, five are
/// not kept, and `eaten` says the snake is used up.
#[test]
fn a_snake_is_gone_after_two_bites() {
    use burngfx::game::{eaten, group_eat_near, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = |food| CharacterNeeds { food, max_food: MAX_FOOD, water: 2, max_water: MAX_WATER, health: 100 };
    let leader_at = Point::new(100, 100);
    let mut leader = needs(8);
    let mut group = vec![
        Companion { needs: needs(8), at: Point::new(120, 100), ..Companion::new(1) },
        Companion { needs: needs(0), at: Point::new(300, 100), ..Companion::new(2) },
    ];
    let left = group_eat_near(&mut leader, leader_at, &mut group, 7);
    assert_eq!(left, 5);
    assert!(eaten(7, left));
    assert_eq!(group[1].needs.food, 0, "the far one, starving, is not reached");
}

// -- v1.2: the range filter switched off -------------------------------------

/// **In group mode a v1.2 room reaches everybody.** v1.2's
/// `Group.IgnoreRangeFilter` (commit `6d5f752`, `Logic/Character/Group.cs:32-45`)
/// is set to `!SingleMode` on entering a room (`Scenes/LocationScene.cs:1329`);
/// `IsRangeFiltered` is then false and `Drink`/`Eat` skip nobody
/// (`Group.cs:92-94`, `:120-122`). `None` is that: the companion eighty away
/// drinks and eats like the rest. With the filter on (`Some`), measured from
/// the selected character, he is passed over.
#[test]
fn with_the_range_filter_off_nobody_is_passed_over() {
    use burngfx::game::{group_drink_from, group_eat_from, CharacterNeeds, Companion, MAX_FOOD, MAX_WATER};
    let needs = || CharacterNeeds { food: 2, max_food: MAX_FOOD, water: 2, max_water: MAX_WATER, health: 100 };
    let leader_at = Point::new(100, 100);
    let group = || vec![Companion { needs: needs(), at: Point::new(180, 100), ..Companion::new(1) }];

    let (mut leader, mut far) = (needs(), group());
    assert_eq!(group_drink_from(None, &mut leader, leader_at, &mut far, 2), 0);
    assert_eq!((leader.water, far[0].needs.water), (3, 3), "both drank");
    assert_eq!(group_eat_from(None, &mut leader, leader_at, &mut far, 2), 0);
    assert_eq!((leader.food, far[0].needs.food), (3, 3), "both ate");

    let (mut leader, mut far) = (needs(), group());
    assert_eq!(group_drink_from(Some(leader_at), &mut leader, leader_at, &mut far, 2), 0);
    assert_eq!((leader.water, far[0].needs.water), (4, 2), "the far one passed over");

    // Measured from the selected companion, it is the leader who is too far.
    let (mut leader, mut far) = (needs(), group());
    let selected = far[0].at;
    assert_eq!(group_drink_from(Some(selected), &mut leader, leader_at, &mut far, 2), 0);
    assert_eq!((leader.water, far[0].needs.water), (2, 4), "the leader passed over");
}

/// **`Group.MoveItems`: the group's packs in order, the leader's first**
/// (`Logic/Character/Group.cs:178-182`, `ItemList.Move`): each takes from the
/// front of the heap until it is full, whoever the things came from. What
/// nobody has room for stays behind.
#[test]
fn moving_things_to_the_group_fills_the_leader_first() {
    use burngfx::game::{move_to_group, ItemBag};
    let mut table = ItemBag::with(4, &[7, 8, 9, 10]);
    table.set_rounds_left(1, Some(1));
    let mut leader = ItemBag::with(3, &[1, 2]);
    let mut companion = ItemBag::with(2, &[]);
    let mut other = ItemBag::with(1, &[3]);

    move_to_group(&mut table, &mut [&mut leader, &mut other, &mut companion]);

    assert_eq!(leader.iter().collect::<Vec<_>>(), vec![1, 2, 7], "the leader's one free slot, first");
    assert_eq!(other.iter().collect::<Vec<_>>(), vec![3], "full: passed over");
    assert_eq!(companion.iter().collect::<Vec<_>>(), vec![8, 9], "then the next with room");
    assert_eq!(companion.rounds_left(0), Some(1), "the rounds go with the gun");
    assert_eq!(table.iter().collect::<Vec<_>>(), vec![10], "no room anywhere: left on the table");
}
