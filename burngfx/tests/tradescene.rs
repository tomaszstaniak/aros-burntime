use burngfx::tradescene::*;

#[test]
fn a_narrow_screen_shows_one_side_and_switches_with_hysteresis() {
    assert_eq!(side_for(Side::None, 100), Side::Player);
    assert_eq!(side_for(Side::None, 130), Side::Trader);
    assert_eq!(side_for(Side::Player, 150), Side::Player, "left until 200");
    assert_eq!(side_for(Side::Player, 200), Side::Trader);
    assert_eq!(side_for(Side::Trader, 125), Side::Trader, "right until back past 120");
    assert_eq!(side_for(Side::Trader, 119), Side::Player);
    let l = Layout::new(341, Side::Trader);
    assert!(l.trader_shown && !l.player_shown);
    assert_eq!((l.top, l.bottom), ((2, 1), (2, 101)));
}

#[test]
fn a_wide_screen_shows_everything_in_470() {
    let l = Layout::new(682, Side::None);
    assert_eq!(l.width, 470);
    assert!(l.trader_shown && l.player_shown);
    assert_eq!(l.trader_window, (304, 5));
    assert_eq!(l.trader_grid().slot_at(3).map(|p| (p.x, p.y)), Some((328, 92 + 48)));
    assert_eq!(Layout::exchange_grid(l.top).slot_at(0).map(|p| (p.x, p.y)), Some((203, 20)));
}
