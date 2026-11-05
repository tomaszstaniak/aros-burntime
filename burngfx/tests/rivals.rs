//! Winning, losing, and holding ground against somebody else.

use burngfx::rivals::{
    can_travel, defeat, has_won, holdings_wanted, last_camper_leaves, winner, winner_within,
    Decision, Map,
};

/// A world written out by hand: which locations are cities, and which roads
/// run between them.
struct World {
    cities: Vec<usize>,
    roads: Vec<(usize, usize)>,
    count: usize,
}

impl Map for World {
    fn count(&self) -> usize {
        self.count
    }
    fn is_city(&self, location: usize) -> bool {
        self.cities.contains(&location)
    }
    fn neighbours(&self, location: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for &(a, b) in &self.roads {
            if a == location {
                out.push(b);
            }
            if b == location {
                out.push(a);
            }
        }
        out
    }
}

/// **The rule is the union of every city's neighbourhood.**
///
/// Two cities, each with a ring of its own, and one location out on its own
/// with no city next to it:
///
/// ```text
///   1 — 0(city) — 2 — 3(city) — 4        5 — 6
/// ```
///
/// Holding the ring around city 0 — locations 1 and 2 — is not winning,
/// because 2 and 4 touch city 3 as well. A reading that stopped at the first
/// city would call the first case a win, and that is the reading this test
/// exists to refuse.
#[test]
fn the_ring_is_around_every_city_not_one() {
    let world = World {
        count: 7,
        cities: vec![0, 3],
        roads: vec![(1, 0), (0, 2), (2, 3), (3, 4), (5, 6)],
    };

    // 1 and 2 touch city 0; 2 and 4 touch city 3. 0 itself touches no city
    // (its neighbours are 1 and 2), and 5 and 6 touch nothing.
    assert_eq!(holdings_wanted(&world), vec![1, 2, 4]);

    let ring_of_the_first_city = |l: usize| (l == 1 || l == 2).then_some(0);
    assert!(
        !has_won(&world, 0, ring_of_the_first_city),
        "one city's ring is not the condition"
    );

    let every_ring = |l: usize| (l == 1 || l == 2 || l == 4).then_some(0);
    assert!(has_won(&world, 0, every_ring));
}

/// **A city is a location like any other.** One that neighbours another city
/// has to be held too — and it can be, because nothing in the game refuses to
/// camp in a city; only the opponent does.
#[test]
fn a_city_beside_a_city_must_be_held_as_well() {
    let world = World { count: 3, cities: vec![0, 1], roads: vec![(0, 1), (1, 2)] };
    // 0 touches city 1; 1 touches city 0; 2 touches city 1.
    assert_eq!(holdings_wanted(&world), vec![0, 1, 2]);
}

/// A location with no city among its neighbours never counts, however many
/// roads it has.
#[test]
fn the_far_country_is_not_part_of_it() {
    let world = World {
        count: 5,
        cities: vec![0],
        roads: vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 2)],
    };
    assert_eq!(holdings_wanted(&world), vec![1], "only what touches the city");
    assert!(has_won(&world, 0, |l| (l == 1).then_some(0)));
}

/// Held by somebody else, or by nobody, is equally not a win.
#[test]
fn unheld_ground_is_not_won_ground() {
    let world = World { count: 3, cities: vec![0], roads: vec![(0, 1), (0, 2)] };
    assert!(!has_won(&world, 0, |_| None), "nobody's");
    assert!(!has_won(&world, 0, |l| (l == 1).then_some(0)), "half of it");
    assert!(!has_won(&world, 0, |l| Some(if l == 1 { 0 } else { 1 })), "the other player's");
    assert!(has_won(&world, 0, |_| Some(0)));
}

/// `CheckWinner` walks the players in order and returns the first that has
/// won, so a tie goes to the lower index. Nobody winning is `None`.
#[test]
fn the_first_player_in_order_takes_a_tie() {
    // A world with nothing to hold: everybody has won it, vacuously.
    let empty = World { count: 2, cities: vec![], roads: vec![(0, 1)] };
    assert_eq!(winner(&empty, &[3, 1, 0], |_| None), Some(3), "first asked, first served");

    let world = World { count: 2, cities: vec![0], roads: vec![(0, 1)] };
    assert_eq!(winner(&world, &[0, 1], |_| None), None);
    assert_eq!(winner(&world, &[0, 1], |_| Some(1)), Some(1));
}

/// **A world with no cities is won by everybody**, because the loop finds
/// nothing to fail on. Upstream does the same, and the game never reaches
/// it; it is pinned so that a later "tidying" cannot change the rule by
/// accident.
#[test]
fn a_world_with_no_cities_is_won_by_everybody() {
    let world = World { count: 3, cities: vec![], roads: vec![(0, 1), (1, 2)] };
    assert!(holdings_wanted(&world).is_empty());
    assert!(has_won(&world, 0, |_| None));
    assert!(has_won(&world, 7, |_| Some(1)));
}

// -- the road out of somebody else's ground -----------------------------------

/// `Player.CanTravel`: standing where somebody else holds, the only road is
/// back the way you came.
#[test]
fn enemy_ground_lets_you_go_back_and_nowhere_else() {
    // Their location, having come from 4.
    assert!(can_travel(Some(1), 0, Some(4), 4), "back the way we came");
    assert!(!can_travel(Some(1), 0, Some(4), 9), "not onwards");
    // With nowhere recorded to go back to, nothing is allowed.
    assert!(!can_travel(Some(1), 0, None, 4));
    // Your own ground, and nobody's, are both free.
    assert!(can_travel(Some(0), 0, None, 9));
    assert!(can_travel(None, 0, None, 9));
}

// -- taking and losing --------------------------------------------------------

/// **The emptying belongs to a takeover and to nothing else.** The last
/// camper leaving always loses the place; the rooms are cleared only when
/// the camp was not the leaving player's own.
#[test]
fn only_taking_somebody_elses_camp_empties_it() {
    // Player 0 takes player 1's camp.
    assert_eq!(last_camper_leaves(Some(1), 0), (true, true));
    // Player 0 gives up their own: lost, but the rooms keep what they hold.
    assert_eq!(last_camper_leaves(Some(0), 0), (true, false));
    // Nobody's already: nothing to lose and nothing to clear.
    assert_eq!(last_camper_leaves(None, 0), (false, false));
}

/// What a takeover puts back is the source's own list, and it is the same at
/// every difficulty the game ships.
// What a taken camp is restocked with is `takeover_restock`, tested in
// `dismiss.rs` with the source's own slips.

/// Losing is the leader dying, and it costs everything at once: every camp
/// emptied, every companion dismissed.
#[test]
fn a_dead_leader_loses_every_camp_and_every_companion() {
    let lost = defeat(&[3, 7, 11], 2);
    assert_eq!(lost.camps, vec![3, 7, 11]);
    assert_eq!(lost.companions, 2);

    // And it is a decision about that player, not about the world.
    assert_eq!(Decision::Lost(0), Decision::Lost(0));
    assert_ne!(Decision::Lost(0), Decision::Won(0));
}

/// **A held place is not taken by camping in it.** The menu that leaves
/// somebody behind refuses when the location is somebody else's — it offers a
/// conversation about capturing it instead — so the way to take a camp is to
/// remove the camper who holds it.
#[test]
fn you_cannot_camp_on_top_of_somebody_elses_camp() {
    assert!(burngfx::rivals::may_camp(None, 0), "nobody's");
    assert!(burngfx::rivals::may_camp(Some(0), 0), "your own grows");
    assert!(!burngfx::rivals::may_camp(Some(1), 0), "not theirs");
    assert!(!burngfx::rivals::may_camp(Some(0), 1), "and not the other way either");
}

// -- a scenario's shortened goal ----------------------------------------------

/// **A short game is a scenario, not a different rule.** The condition still
/// says "every location beside a city"; a scenario asks for the first few of
/// them, so that a demonstration can be played to the end.
#[test]
fn a_shortened_goal_asks_for_the_first_few_of_the_same_places() {
    let world = World {
        count: 7,
        cities: vec![0, 3],
        roads: vec![(1, 0), (0, 2), (2, 3), (3, 4), (5, 6)],
    };
    assert_eq!(holdings_wanted(&world), vec![1, 2, 4]);

    let holds_two = |l: usize| (l == 1 || l == 2).then_some(0);
    // The real game is not won on two of the three.
    assert_eq!(winner(&world, &[0, 1], holds_two), None);
    // A scenario that asks for two is.
    assert_eq!(winner_within(&world, Some(2), &[0, 1], holds_two), Some(0));
    // And asking for all three is the real game again.
    assert_eq!(winner_within(&world, Some(3), &[0, 1], holds_two), None);
    assert_eq!(winner_within(&world, None, &[0, 1], holds_two), None);
}

/// A goal beyond what the world has is the world's own list, not an error.
#[test]
fn a_goal_larger_than_the_world_is_the_whole_world() {
    let world = World { count: 3, cities: vec![0], roads: vec![(0, 1), (0, 2)] };
    let holds_all = |_: usize| Some(0);
    assert_eq!(winner_within(&world, Some(99), &[0], holds_all), Some(0));
}

/// **A scenario that asks for nothing announces nothing.** Zero goals is a
/// scenario that was not set up, and a victory there would be a victory
/// nobody played for.
#[test]
fn a_goal_of_none_is_not_a_victory() {
    let world = World { count: 3, cities: vec![0], roads: vec![(0, 1), (0, 2)] };
    assert_eq!(winner_within(&world, Some(0), &[0], |_| Some(0)), None);

    // And a world with nothing to hold is the same: the real rule calls it a
    // win, a scenario does not pretend to.
    let bare = World { count: 2, cities: vec![], roads: vec![(0, 1)] };
    assert!(has_won(&bare, 0, |_| None));
    assert_eq!(winner_within(&bare, Some(1), &[0], |_| None), None);
}

// -- what the player is shown ------------------------------------------------

/// **The screen does not count the way the condition does.** `StatisticsScene`
/// skips city neighbours when it asks whether a city is controlled;
/// `VictoryCondition` skips nothing. So a world where two cities are
/// neighbours can show every city controlled while the game is not won, and
/// this test exists so that nobody later "fixes" the display into agreeing
/// with the rule.
///
/// ```text
///   1 — 0(city) — 3(city) — 2
/// ```
///
/// Holding 1 and 2 controls both cities by the screen's rule — each city's
/// only non-city neighbour is held — while the condition also wants 0 and 3,
/// because each is a location with a city among its neighbours.
#[test]
fn the_screen_counts_differently_from_the_condition() {
    let world = World { count: 4, cities: vec![0, 3], roads: vec![(1, 0), (0, 3), (3, 2)] };
    assert_eq!(holdings_wanted(&world), vec![0, 1, 2, 3]);

    let ours = |l: usize| (l == 1 || l == 2).then_some(0);
    let s = burngfx::rivals::standing_of(&world, 0, ours);
    assert_eq!(s.cities, 2);
    assert_eq!(s.controlled, 2, "the screen says both");
    assert_eq!(s.held, 2);
    assert_eq!((s.wanted_held, s.wanted), (2, 4), "and the condition says half");
    assert!(!has_won(&world, 0, ours), "not won, whatever the screen says");
}

/// The ordinary case, where the two agree: one city, no city neighbours.
#[test]
fn the_plain_case_where_screen_and_condition_agree() {
    let world = World { count: 3, cities: vec![0], roads: vec![(0, 1), (0, 2)] };
    let none = burngfx::rivals::standing_of(&world, 0, |_| None);
    assert_eq!((none.cities, none.controlled, none.held), (1, 0, 0));
    assert_eq!((none.wanted_held, none.wanted), (0, 2));

    let all = burngfx::rivals::standing_of(&world, 0, |_| Some(0));
    assert_eq!((all.cities, all.controlled), (1, 1));
    assert_eq!(all.held, 3, "the city is held too, and counts as a holding");
    assert_eq!((all.wanted_held, all.wanted), (2, 2));
    assert!(has_won(&world, 0, |_| Some(0)));
}

/// **A standing is one player's own.** Somebody else's holdings are not
/// counted into it, and nothing here reports them: the original's statistics
/// screen has no line about another player, and its info mode refuses to open
/// a place that is not yours.
#[test]
fn a_standing_says_nothing_about_anybody_else() {
    let world = World { count: 4, cities: vec![0], roads: vec![(0, 1), (0, 2), (2, 3)] };
    let split = |l: usize| match l {
        1 => Some(0),
        2 => Some(1),
        _ => None,
    };
    let mine = burngfx::rivals::standing_of(&world, 0, split);
    assert_eq!(mine.held, 1);
    assert_eq!(mine.controlled, 0, "their holding is not mine, and is not named");
    assert_eq!((mine.wanted_held, mine.wanted), (1, 2));

    let theirs = burngfx::rivals::standing_of(&world, 1, split);
    assert_eq!(theirs.held, 1, "the same function, asked about them, is the same report");
}

/// **A scenario's progress is counted over the scenario's own places**, and
/// never mixed with the condition's count. Counting holdings over the whole
/// wanted list and printing it against the scenario's total gives "3 of 2",
/// which is not a state of any game.
#[test]
fn a_scenarios_progress_is_its_own_count() {
    let world = World {
        count: 7,
        cities: vec![0, 3],
        roads: vec![(1, 0), (0, 2), (2, 3), (3, 4), (5, 6)],
    };
    assert_eq!(holdings_wanted(&world), vec![1, 2, 4]);

    // Holding all three of them: the condition says three of three, and a
    // scenario that asked for the first two says two of two.
    let all = |_: usize| Some(0);
    let s = burngfx::rivals::standing_of(&world, 0, all);
    assert_eq!((s.wanted_held, s.wanted), (3, 3));
    assert_eq!(burngfx::rivals::scenario_progress(&world, 2, 0, all), (2, 2));

    // Holding only the third: the scenario's own two are not held at all,
    // even though the condition's count is one.
    let far = |l: usize| (l == 4).then_some(0);
    assert_eq!(burngfx::rivals::standing_of(&world, 0, far).wanted_held, 1);
    assert_eq!(burngfx::rivals::scenario_progress(&world, 2, 0, far), (0, 2));

    // A goal past the end of the list is the whole list.
    assert_eq!(burngfx::rivals::scenario_progress(&world, 99, 0, all), (3, 3));
}

/// `Location.ControllingPlayer` (v1.2 `Logic/Location.cs:93-110`): a city
/// whose every neighbour one player holds is that player's; any other place,
/// a city with a neighbour free or held by another, or no neighbours at all,
/// is nobody's. `DrawWorldLocationText` draws such a city's name in that
/// player's colour (`Maps/MapViewOverlayHoverText.cs:162-172`).
#[test]
fn a_city_is_controlled_when_one_player_holds_every_neighbour() {
    use burngfx::rivals::controlling_player;
    let owners = [None, Some(2), Some(2), Some(0), None];
    let owner = |n: usize| owners[n];
    assert_eq!(controlling_player(true, &[1, 2], owner), Some(2), "both held by seat 2");
    assert_eq!(controlling_player(true, &[1, 2, 3], owner), None, "split between two players");
    assert_eq!(controlling_player(true, &[1, 4], owner), None, "one neighbour free");
    assert_eq!(controlling_player(true, &[0, 4], owner), None, "nobody holds any");
    assert_eq!(controlling_player(true, &[], owner), None, "no neighbours");
    assert_eq!(controlling_player(false, &[1, 2], owner), None, "not a city");
}
