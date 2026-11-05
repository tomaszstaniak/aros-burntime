//! v1.2's scene blend (`SceneManager.SetScene`, `BlendOverlayBase.FadeOut`
//! and `FadeIn`, `FadingHelper.Update`), at `scene_blend = 4`.

use burngfx::scene_blend::{fade_ms, Blend, HOLD_MS};

#[test]
fn a_scene_change_goes_black_waits_and_comes_back() {
    assert_eq!(fade_ms(4), 250);
    assert_eq!(HOLD_MS, 500);
    let b = Blend::begin(1000, 0, fade_ms(4));
    // `FadeOut(wait: true)`: 500 + (1 - 0) * 1000 / 4 ms of waiting.
    assert_eq!(b.wait_ms(), 750);
    assert_eq!(b.alpha(1000), 0);
    assert_eq!(b.alpha(1125), 127);
    assert_eq!(b.alpha(1250), 255);
    assert_eq!(b.alpha(1700), 255);
    assert!(b.waiting(1749) && !b.waiting(1750));
    // `FadeIn`: down again over 250 ms while the new scene runs.
    assert_eq!(b.alpha(1750), 255);
    assert_eq!(b.alpha(1875), 128);
    assert_eq!(b.alpha(2000), 0);
    assert!(!b.done(1999) && b.done(2000));
}

#[test]
fn a_change_during_a_fade_in_waits_only_for_what_is_left() {
    // The overlay half up: half the fade, then the hold.
    let b = Blend::begin(0, 128, 250);
    assert_eq!(b.wait_ms(), 500 + 127 * 250 / 255);
    assert_eq!(b.alpha(0), 128);
}

#[test]
fn opening_from_black_only_fades_in() {
    let b = Blend::fading_in(5000, 250);
    assert!(!b.waiting(5000));
    assert_eq!(b.alpha(5000), 255);
    assert_eq!(b.alpha(5250), 0);
    assert!(b.done(5250));
}
