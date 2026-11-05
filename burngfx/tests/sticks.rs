//! The pad's sticks as v1.2 reads them (`HandleGamePadInput`): the axes as
//! they stand, read once a frame.

use burngfx::input::Sticks;

const FULL: i16 = 32767;

fn set(s: &mut Sticks, axes: &[(usize, i16)], diagonal: bool) -> burngfx::input::StickPresses {
    for &(a, v) in axes {
        s.axis(a, v);
    }
    s.frame(diagonal)
}

#[test]
fn the_middle_and_the_dead_zone_do_nothing() {
    let mut s = Sticks::default();
    assert_eq!(set(&mut s, &[(0, 7000)], false).navigate, None);
    assert_eq!(s.moving(), (0, 0));
    // Past the dead zone but under a quarter of what is left: still nothing.
    assert_eq!(set(&mut s, &[(0, 12000)], false).navigate, None);
}

#[test]
fn the_left_stick_presses_once_and_holds() {
    let mut s = Sticks::default();
    assert_eq!(set(&mut s, &[(0, FULL)], false).navigate, Some((1, 0)));
    assert_eq!(s.moving(), (1, 0));
    // Held: no second press; down as well makes a diagonal held.
    assert_eq!(set(&mut s, &[(1, FULL)], false).navigate, None);
    assert_eq!(s.moving(), (1, 1));
    set(&mut s, &[(0, 0), (1, 0)], false);
    assert_eq!(s.moving(), (0, 0));
    // Back in the middle: the next push presses again; SDL's y is down.
    assert_eq!(set(&mut s, &[(1, -FULL)], false).navigate, Some((0, -1)));
}

#[test]
fn a_diagonal_press_only_where_the_scene_takes_one() {
    let mut s = Sticks::default();
    assert_eq!(set(&mut s, &[(0, FULL), (1, FULL)], true).navigate, Some((1, 1)));
    let mut s = Sticks::default();
    // Not a diagonal scene: the stronger axis.
    assert_eq!(set(&mut s, &[(0, FULL), (1, 20000)], false).navigate, Some((1, 0)));
}

#[test]
fn the_right_stick_pans_and_presses_as_each_direction_starts() {
    let mut s = Sticks::default();
    assert_eq!(set(&mut s, &[(2, -FULL)], false).pan, vec![(-1, 0)]);
    assert_eq!(s.panning(), (-1, 0));
    assert_eq!(set(&mut s, &[(3, FULL)], false).pan, vec![(0, 1)]);
    assert_eq!(s.panning(), (-1, 1));
    assert!(set(&mut s, &[(3, FULL)], false).pan.is_empty());
    assert_eq!(s.moving(), (0, 0));
}
