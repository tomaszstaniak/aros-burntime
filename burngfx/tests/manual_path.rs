//! `burngfx::manual_path`: v1.2's `ManualPath` on small walk grids drawn as
//! text (`#` a closed cell, `.` an open one; a cell is 8 pixels).

use burngfx::game::Point;
use burngfx::manual_path::ManualPath;
use burngfx::zei::WalkGrid;

fn grid(rows: &[&str]) -> WalkGrid {
    let (w, h) = (rows[0].len(), rows.len());
    let mut bytes = vec![w as u8, 0, h as u8, 0];
    for r in rows {
        bytes.extend(r.bytes().map(|b| (b == b'.') as u8));
    }
    WalkGrid::from_packed(&bytes).unwrap()
}

fn walk(g: &WalkGrid, from: Point, dir: (i32, i32), steps: usize) -> Point {
    let mut m = ManualPath::default();
    m.set_direction(dir);
    let mut at = from;
    for _ in 0..steps {
        at = m.step(g, at, 2.0);
    }
    at
}

#[test]
fn on_open_ground_it_goes_the_held_way() {
    let g = grid(&["..........", "..........", "..........", ".........."]);
    assert_eq!(walk(&g, Point::new(10, 12), (1, 0), 10), Point::new(30, 12));
    // Diagonally, a step of 2 is 1.41 each way: the fraction is kept between
    // steps, so ten steps make 14 and not 10.
    assert_eq!(walk(&g, Point::new(10, 4), (1, 1), 10), Point::new(24, 18));
}

#[test]
fn a_wall_met_at_an_angle_is_slid_along() {
    // Row 2 is a wall: going down-right, the character keeps going right.
    let g = grid(&["..........", "..........", "##########", ".........."]);
    let end = walk(&g, Point::new(10, 8), (1, 1), 20);
    assert!(end.x > 30, "{end:?}");
    assert!(end.y < 16, "kept above the wall: {end:?}");
}

#[test]
fn a_wall_met_head_on_slides_to_the_nearer_way_round() {
    // Going down at x 12 into a wall with a gap at cells 4–5: it slides
    // sideways to the gap (`GetSlideSide`) and goes through.
    let g = grid(&["..........", "..........", "####..####", "..........", ".........."]);
    let end = walk(&g, Point::new(20, 4), (0, 1), 30);
    assert!(end.y > 24, "through the gap: {end:?}");
}

#[test]
fn a_dead_end_stops_it() {
    // Walled in below with no way round within reach: it stops at the wall.
    let g = grid(&["....", "....", "####"]);
    let end = walk(&g, Point::new(12, 4), (0, 1), 20);
    assert!(end.y < 16, "{end:?}");
    assert_eq!(end.x, 12);
}

#[test]
fn out_of_a_small_pocket_by_a_detour() {
    // Going up from inside a pocket (a ceiling above, walls both sides): no
    // slide is possible, so it finds the nearest cell with two clear cells
    // above it within four (`TryBeginDetour`) — down, round the left wall —
    // and comes out above. An exit needs two clear cells beyond it the held
    // way, so the grid has room above.
    let g = grid(&["........", "........", "........", "....##..", "...#..#.", "........"]);
    let end = walk(&g, Point::new(36, 36), (0, -1), 40);
    assert!(end.y < 20, "out above: {end:?}");
}
