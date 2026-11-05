//! v1.2's walk: speeds in pixels a second, the fraction of a pixel carried
//! from step to step (`PathState.precisePosition`), `SimplePath` and
//! `ComplexPath`'s walk.

use burngfx::gait::{self, Stride, Walked};
use burngfx::game::Point;

/// Steps until `along` arrives, on open ground.
fn steps_along(from: Point, route: &[Point], goal: Point, speed: u32) -> u32 {
    let mut route: Vec<Point> = route.iter().rev().copied().collect();
    let (mut at, mut stride) = (from, Stride::default());
    for n in 1..1000 {
        let (now, how) = gait::along(at, &mut stride, &mut route, goal, gait::per_step(speed), &|_| true);
        at = now;
        if how == Walked::Arrived {
            assert_eq!(at, goal);
            return n;
        }
    }
    panic!("never arrived");
}

/// **The leader covers 100 px in 2.0 s** (`CONTROLLED_WALK_SPEED` = 50,
/// `Character.cs:76,975-976`): 36 steps of an eighteenth of a second, give
/// or take the one the fraction rounds into. Two pixels a step took 50.
#[test]
fn the_leader_walks_a_hundred_pixels_in_two_seconds() {
    let two_seconds = 36..=37;
    assert!(two_seconds.contains(&steps_along(Point::new(0, 0), &[], Point::new(100, 0), gait::CONTROLLED)));
    // On the diagonal too: a length, not a stride along the longer axis.
    assert!(two_seconds.contains(&steps_along(Point::new(0, 0), &[], Point::new(71, 71), gait::CONTROLLED)));
}

/// What a step left over past a waypoint is walked on toward the next
/// (`ComplexPath.cs:592-610`): a route with corners costs its length.
#[test]
fn a_corner_costs_no_time() {
    let route = [Point::new(50, 0)];
    assert!((36..=37).contains(&steps_along(Point::new(0, 0), &route, Point::new(50, 50), gait::CONTROLLED)));
}

/// Each speed, a step at a time, within a pixel a minute of v1.2's.
#[test]
fn the_speeds() {
    for (speed, px) in [(gait::CONTROLLED, 50.0), (gait::WALK, 35.0), (gait::WANDER, 23.1), (gait::SLOW_MUTANT, 17.5), (gait::FLEE, 70.0)] {
        let per_minute = gait::per_step(speed) as f64 * 18.0 * 60.0 / gait::SUB as f64;
        assert!((per_minute - px * 60.0).abs() < 1.0, "{px}: {per_minute}");
    }
}

/// **`SimplePath` gives up when blocked** (`SimplePath.cs:51-56`): the step
/// and the point half a mask cell beyond must both be walkable, else the
/// walk ends where he stands.
#[test]
fn a_simple_walk_stops_at_a_wall() {
    let wall = |p: Point| p.x < 40;
    let (mut at, mut stride, mut to) = (Point::new(20, 0), Stride::default(), Point::new(80, 0));
    let mut stopped = None;
    for _ in 0..100 {
        let (now, how) = gait::simple(at, &mut stride, &mut to, gait::per_step(gait::WANDER), &wall);
        at = now;
        if how == Walked::Blocked {
            stopped = Some(at);
            break;
        }
    }
    let at = stopped.expect("blocked");
    assert_eq!(to, at, "the walk is given up where he stands");
    assert!(at.x < 40 && at.x + 6 >= 33, "short of the wall by half a cell: {at:?}");
}

/// The fraction belongs to its pixel: moved by hand, a character starts
/// from the whole pixel (`PathState.BeginMovement`).
#[test]
fn a_fraction_is_forgotten_when_the_pixel_changes() {
    let mut stride = Stride::default();
    let mut to = Point::new(100, 0);
    let (at, _) = gait::simple(Point::new(0, 0), &mut stride, &mut to, gait::per_step(gait::WANDER), &|_| true);
    assert_ne!(stride.carry(at), (0, 0));
    assert_eq!(stride.carry(Point::new(at.x + 5, at.y)), (0, 0));
}
