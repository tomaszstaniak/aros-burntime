//! `burngfx::view::grid_move_focus` and `grid_focus_edge`: `ItemGridWindow`'s
//! keyboard focus over filled slots.

use burngfx::game::Point;
use burngfx::view::{grid_focus_edge, grid_move_focus, ItemGrid};

#[test]
fn the_focus_moves_between_filled_slots() {
    // Three by two, slots 0, 2, 4 filled.
    let g = ItemGrid::plain(0, 0, 3, 2, 4);
    let filled = [true, false, true, false, true, false];
    assert_eq!(grid_move_focus(&g, &filled, None, (1, 0)), Some(0), "nothing focused: the first filled");
    assert_eq!(grid_move_focus(&g, &filled, Some(0), (1, 0)), Some(2), "right skips the empty slot");
    assert_eq!(grid_move_focus(&g, &filled, Some(2), (1, 0)), None, "nothing further right");
    // Down from 2: 4 is one row down and one column left, the only one below.
    assert_eq!(grid_move_focus(&g, &filled, Some(2), (0, 1)), Some(4));
    // Left from 4: slot 0 is left too, a row up — any filled slot ahead counts.
    assert_eq!(grid_move_focus(&g, &filled, Some(4), (-1, 0)), Some(0));
}

#[test]
fn in_a_double_layered_grid_left_and_right_keep_to_the_layer() {
    // Four by five with the offset layer (the room's floor): slot 20 is the
    // first of the layer, between 0, 1, 4 and 5.
    let g = ItemGrid { double: true, ..ItemGrid::plain(0, 0, 4, 5, 4) };
    let mut filled = vec![false; g.capacity()];
    for i in [0, 1, 20, 21] {
        filled[i] = true;
    }
    assert_eq!(grid_move_focus(&g, &filled, Some(0), (1, 0)), Some(1), "right stays on the first layer");
    assert_eq!(grid_move_focus(&g, &filled, Some(20), (1, 0)), Some(21), "and on the second");
    // Down from 0 goes to the nearer layer: 20, half a row down.
    assert_eq!(grid_move_focus(&g, &filled, Some(0), (0, 1)), Some(20));
}

#[test]
fn entering_a_grid_takes_the_slot_at_its_edge_on_the_nearest_row() {
    let g = ItemGrid::plain(100, 0, 3, 2, 4);
    let filled = [false, true, true, true, true, true];
    // From the left at the second row's height: the leftmost filled slot of
    // that row (3).
    assert_eq!(grid_focus_edge(&g, &filled, Point::new(50, 52), (1, 0)), Some(3));
    // From the right at the first row's height: its rightmost (2).
    assert_eq!(grid_focus_edge(&g, &filled, Point::new(300, 16), (-1, 0)), Some(2));
}
