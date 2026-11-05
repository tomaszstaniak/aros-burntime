//! The viewport: a fixed window onto maps of every size.
//!
//! The first test here is the regression test for a bug that shipped: a map
//! wider than the window was copied into it as one flat run of pixels, so each
//! row landed further left than the last and the map arrived as diagonal
//! streaks. It was invisible while every packed map was 480 wide or less.

use burngfx::game::Point;
use burngfx::view::{Camera, Viewport};

/// A map whose pixels encode their own coordinates, so a misplaced row is
/// obvious rather than merely wrong-looking.
fn map(w: usize, h: usize) -> Vec<u32> {
    (0..w * h).map(|i| (((i / w) as u32) << 16) | ((i % w) as u32)).collect()
}

fn at(x: usize, y: usize) -> u32 {
    ((y as u32) << 16) | x as u32
}

/// The bug, pinned: a map wider than the window must be cropped, not sheared.
#[test]
fn a_wider_map_is_cropped_rather_than_sheared() {
    let view = Viewport::new(480, 384);
    let (mw, mh) = (576, 480);
    let src = map(mw, mh);

    // The camera 96 pixels in, which is as far right as this map goes.
    let camera = view.follow((mw, mh), Point::new(mw as i32, 0));
    assert_eq!(camera.x, 96, "clamped to the right edge");
    assert_eq!(camera.y, 0, "clamped to the top");

    let mut dst = view.canvas(0xDEAD_BEEF);
    view.blit(&mut dst, &src, mw, mh, camera);

    for y in 0..view.h {
        for x in 0..view.w {
            assert_eq!(
                dst[y * view.w + x],
                at(x + 96, y),
                "window ({x},{y}) should show map ({}, {y})",
                x + 96
            );
        }
    }

    // The old flat copy would have put map row 1 column 0 here.
    assert_ne!(dst[view.w], at(0, 1), "each row must start at the camera, not drift");
}

/// The case that used to work by accident, and must keep working.
#[test]
fn a_map_the_size_of_the_window_is_copied_unchanged() {
    let view = Viewport::new(480, 384);
    let src = map(480, 384);
    let camera = view.follow((480, 384), Point::new(240, 192));
    assert_eq!(camera, Camera { x: 0, y: 0 });

    let mut dst = view.canvas(0);
    view.blit(&mut dst, &src, 480, 384, camera);
    assert_eq!(dst, src);
}

/// A map smaller than the window sits in the middle of it, and the rest stays
/// background rather than showing a wrapped copy of the map.
#[test]
fn a_smaller_map_is_centred_on_the_background() {
    const BG: u32 = 0xDEAD_BEEF;
    let view = Viewport::new(480, 384);
    let (mw, mh) = (480, 320);
    let src = map(mw, mh);

    // Wherever the focus is, a map this size does not move.
    let a = view.follow((mw, mh), Point::new(0, 0));
    let b = view.follow((mw, mh), Point::new(479, 319));
    assert_eq!(a, b, "a map smaller than the window is not scrolled");
    assert_eq!(a, Camera { x: 0, y: -32 }, "(320 - 384) / 2");

    let mut dst = view.canvas(BG);
    view.blit(&mut dst, &src, mw, mh, a);

    for y in 0..view.h {
        for x in 0..view.w {
            let got = dst[y * view.w + x];
            if (32..352).contains(&y) {
                assert_eq!(got, at(x, y - 32), "({x},{y}) is on the map");
            } else {
                assert_eq!(got, BG, "({x},{y}) is above or below the map");
            }
        }
    }
}

/// Narrower than the window in one axis and larger in the other, which is the
/// case that catches an axis handled by copying the other one's code.
#[test]
fn the_two_axes_are_independent() {
    const BG: u32 = 7;
    let view = Viewport::new(100, 100);
    let (mw, mh) = (60, 400);
    let src = map(mw, mh);

    let camera = view.follow((mw, mh), Point::new(30, 200));
    assert_eq!(camera.x, -20, "narrow: centred");
    assert_eq!(camera.y, 150, "tall: following");

    let mut dst = view.canvas(BG);
    view.blit(&mut dst, &src, mw, mh, camera);

    assert_eq!(dst[0], BG, "left margin");
    assert_eq!(dst[20], at(0, 150), "the map starts 20 in");
    assert_eq!(dst[79], at(59, 150), "and ends 80 in");
    assert_eq!(dst[80], BG, "right margin");
    assert_eq!(dst[99 * 100 + 20], at(0, 249), "the last visible row");
}

#[test]
fn the_camera_never_shows_past_the_edge_of_a_large_map() {
    let view = Viewport::new(100, 100);
    let map_size = (400usize, 300usize);

    assert_eq!(view.follow(map_size, Point::new(0, 0)), Camera { x: 0, y: 0 });
    assert_eq!(view.follow(map_size, Point::new(200, 150)), Camera { x: 150, y: 100 });
    assert_eq!(view.follow(map_size, Point::new(1000, 1000)), Camera { x: 300, y: 200 });
    // Negative focus clamps too, rather than scrolling off the top left.
    assert_eq!(view.follow(map_size, Point::new(-50, -50)), Camera { x: 0, y: 0 });
}

/// A click is converted once, here, and the rules never see a window pixel.
#[test]
fn screen_and_map_coordinates_round_trip() {
    let view = Viewport::new(480, 384);
    for camera in [Camera { x: 0, y: 0 }, Camera { x: 96, y: 40 }, Camera { x: 0, y: -32 }] {
        for p in [Point::new(0, 0), Point::new(479, 383), Point::new(213, 77)] {
            let there = view.to_map(camera, p);
            assert_eq!(view.to_screen(camera, there), p);
        }
    }

    // And it is an offset, not a scale.
    assert_eq!(view.to_map(Camera { x: 96, y: 40 }, Point::new(10, 10)), Point::new(106, 50));
    assert_eq!(view.to_map(Camera { x: 0, y: -32 }, Point::new(10, 40)), Point::new(10, 8));
}

#[test]
fn what_is_off_the_window_is_known_to_be_off_it() {
    let view = Viewport::new(100, 100);
    let camera = Camera { x: 50, y: 50 };
    assert!(view.shows(camera, Point::new(50, 50)));
    assert!(view.shows(camera, Point::new(149, 149)));
    assert!(!view.shows(camera, Point::new(49, 50)));
    assert!(!view.shows(camera, Point::new(150, 50)));
}

/// Nothing here may write outside the buffer it was given, whatever the camera.
#[test]
fn a_camera_right_off_the_map_draws_nothing_and_panics_at_nothing() {
    const BG: u32 = 0x1234_5678;
    let view = Viewport::new(64, 64);
    let src = map(32, 32);

    for camera in [
        Camera { x: 1000, y: 0 },
        Camera { x: -1000, y: 0 },
        Camera { x: 0, y: 1000 },
        Camera { x: 0, y: -1000 },
        Camera { x: -64, y: 0 },
        Camera { x: 0, y: -64 },
    ] {
        let mut dst = view.canvas(BG);
        view.blit(&mut dst, &src, 32, 32, camera);
        assert!(dst.iter().all(|&p| p == BG), "camera {camera:?} drew something");
    }

    // A camera that puts the map only partly inside the window draws that
    // part and clips the rest, rather than refusing or running over the edge.
    let mut dst = view.canvas(BG);
    view.blit(&mut dst, &src, 32, 32, Camera { x: -48, y: -48 });
    assert_eq!(dst[48 * 64 + 48], at(0, 0), "the map's corner lands at (48,48)");
    assert_eq!(dst[63 * 64 + 63], at(15, 15), "and is cut off at the window's");
    assert_eq!(dst[47 * 64 + 48], BG, "nothing above it");

    // A source that does not hold what it claims is refused rather than read.
    let mut dst = view.canvas(BG);
    view.blit(&mut dst, &[0u32; 4], 32, 32, Camera::default());
    assert!(dst.iter().all(|&p| p == BG));

    // As is a destination too small to be the window.
    let mut small = alloc_small();
    view.blit(&mut small, &src, 32, 32, Camera::default());
    assert!(small.iter().all(|&p| p == 0));
}

fn alloc_small() -> Vec<u32> {
    vec![0u32; 8]
}

// -- item slots -------------------------------------------------------------

use burngfx::view::ItemGrid;

fn grid() -> ItemGrid {
    ItemGrid::plain(160, 20, 4, 5, 4)
}

/// Where a slot is drawn and where a click on it lands are the same
/// rectangle, and the test walks every slot to say so.
#[test]
fn every_slot_is_hit_by_a_click_on_the_slot_it_is_drawn_at() {
    let g = grid();
    assert_eq!(g.capacity(), 20);
    assert_eq!(g.slot_at(20), None, "one past the end");

    for i in 0..g.capacity() {
        let at = g.slot_at(i).unwrap();
        assert_eq!(g.hit(at), Some(i), "slot {i} at its corner");
        assert_eq!(g.hit(Point::new(at.x + 31, at.y + 31)), Some(i), "and at its far corner");
        assert_eq!(g.hit(Point::new(at.x + 16, at.y + 16)), Some(i), "and in the middle");
    }

    // The rows and columns are the way round they look.
    assert_eq!(g.slot_at(0), Some(Point::new(160, 20)));
    assert_eq!(g.slot_at(3), Some(Point::new(268, 20)), "end of the first row");
    assert_eq!(g.slot_at(4), Some(Point::new(160, 56)), "start of the second");
}

/// A click in the gap is a miss. Rounding it into the nearest slot would move
/// an item the player did not point at, which is worse than doing nothing.
#[test]
fn the_gaps_between_slots_belong_to_no_slot() {
    let g = grid();
    assert_eq!(g.hit(Point::new(192, 20)), None, "the four pixels after slot 0");
    assert_eq!(g.hit(Point::new(195, 20)), None);
    assert_eq!(g.hit(Point::new(196, 20)), Some(1), "and then slot 1 begins");
    assert_eq!(g.hit(Point::new(160, 52)), None, "the gap below the first row");
    assert_eq!(g.hit(Point::new(160, 56)), Some(4));

    // Outside on every side.
    assert_eq!(g.hit(Point::new(159, 20)), None);
    assert_eq!(g.hit(Point::new(160, 19)), None);
    assert_eq!(g.hit(Point::new(160 + 4 * 36, 20)), None, "past the last column");
    assert_eq!(g.hit(Point::new(160, 20 + 5 * 36)), None, "past the last row");
    assert_eq!(g.hit(Point::new(-1000, -1000)), None);
}

/// A water source shows eight, not thirty two, and the original moves the
/// grid down rather than shrinking the slots.
#[test]
fn a_water_source_grid_is_two_rows_lower_down() {
    let g = ItemGrid::plain(160, 128, 4, 2, 4);
    assert_eq!(g.capacity(), 8);
    assert_eq!(g.slot_at(7), Some(Point::new(268, 164)));
    assert_eq!(g.hit(Point::new(268, 164)), Some(7));
    assert_eq!(g.slot_at(8), None);
}


#[test]
fn a_double_layered_room_holds_thirty_two_and_the_second_layer_is_on_top() {
    let g = ItemGrid { double: true, ..ItemGrid::plain(160, 20, 4, 5, 4) };
    assert_eq!(g.capacity(), 32);
    assert_eq!(g.slot_at(20), Some(Point::new(176, 36)));
    assert_eq!(g.slot_at(31), Some(Point::new(160 + 2 * 36 + 16, 20 + 3 * 36 + 16)));
    // A point inside both slot 0 and slot 20: the second layer wins if filled.
    let p = Point::new(180, 40);
    assert_eq!(g.hit_filled(p, |_| true), Some(20));
    assert_eq!(g.hit_filled(p, |i| i == 0), Some(0));
    assert_eq!(g.hit_filled(p, |_| false), None);
}

#[test]
fn rows_and_columns_may_be_spaced_differently() {
    let g = ItemGrid { gap_y: 5, ..ItemGrid::plain(36, 103, 3, 2, 4) };
    assert_eq!(g.slot_at(3), Some(Point::new(36, 103 + 37)));
    assert_eq!(g.hit(Point::new(37, 141)), Some(3));
    assert_eq!(g.hit(Point::new(37, 137)), None, "in the five-pixel gap");
}
