//! `sdlmini::decode` on hand-made `SDL_Event` bytes, and `Keys` as presses
//! and releases come.

use sdlmini::{decode, Event, Keys, EVENT_KEYDOWN, EVENT_KEYUP};

fn key(kind: u32, scancode: u32, repeat: bool) -> [u8; 64] {
    let mut ev = [0u8; 64];
    ev[0..4].copy_from_slice(&kind.to_ne_bytes());
    ev[13] = repeat as u8;
    ev[16..20].copy_from_slice(&scancode.to_ne_bytes());
    ev
}

#[test]
fn a_key_is_pressed_and_let_go() {
    assert_eq!(decode(&key(EVENT_KEYDOWN, 23, false)), Event::KeyDown(23));
    assert_eq!(decode(&key(EVENT_KEYUP, 23, false)), Event::KeyUp(23));
}

#[test]
fn held_keys_follow_the_events() {
    let mut keys = Keys::default();
    // T pressed, repeated by the keyboard, then W pressed: both held, T once.
    for e in [Event::KeyDown(23), Event::KeyDown(23), Event::KeyDown(26)] {
        keys.apply(&e);
    }
    assert!(keys.is_held(23) && keys.is_held(26));
    keys.apply(&Event::KeyUp(23));
    assert!(!keys.is_held(23) && keys.is_held(26));
    // A release of a key not held changes nothing.
    keys.apply(&Event::KeyUp(4));
    assert!(keys.is_held(26));
    keys.clear();
    assert!(!keys.is_held(26));
}

#[test]
fn a_controller_button_and_axis() {
    use sdlmini::{EVENT_CONTROLLERAXISMOTION, EVENT_CONTROLLERBUTTONDOWN, EVENT_CONTROLLERBUTTONUP};
    let mut ev = [0u8; 64];
    ev[0..4].copy_from_slice(&EVENT_CONTROLLERBUTTONDOWN.to_ne_bytes());
    ev[12] = 11; // the d-pad's up
    assert_eq!(decode(&ev), Event::PadDown(11));
    ev[0..4].copy_from_slice(&EVENT_CONTROLLERBUTTONUP.to_ne_bytes());
    assert_eq!(decode(&ev), Event::PadUp(11));
    ev[0..4].copy_from_slice(&EVENT_CONTROLLERAXISMOTION.to_ne_bytes());
    ev[12] = 5; // the right trigger
    ev[16..18].copy_from_slice(&20000i16.to_ne_bytes());
    assert_eq!(decode(&ev), Event::PadAxis { axis: 5, value: 20000 });
}

/// `SDL_MouseWheelEvent`: type, timestamp, window, which, then `x` (16),
/// `y` (20) and `direction` (24). v1.2 scrolls its Saves list and its
/// manual by the wheel (`OptionsSavesPage.cs:225`, `ManualWindow.cs:840`);
/// AROS's SDL2 sends this event for `RAWKEY_NM_WHEEL_*`.
#[test]
fn the_wheel_turned_and_its_direction() {
    use sdlmini::EVENT_MOUSEWHEEL;
    let wheel = |y: i32, flipped: u32| {
        let mut ev = [0u8; 64];
        ev[0..4].copy_from_slice(&EVENT_MOUSEWHEEL.to_ne_bytes());
        ev[20..24].copy_from_slice(&y.to_ne_bytes());
        ev[24..28].copy_from_slice(&flipped.to_ne_bytes());
        ev
    };
    // Away from the user is up, as MonoGame's positive delta.
    assert_eq!(decode(&wheel(1, 0)), Event::Wheel(1));
    assert_eq!(decode(&wheel(-2, 0)), Event::Wheel(-2));
    // `SDL_MOUSEWHEEL_FLIPPED`: the system reverses it; the event says so.
    assert_eq!(decode(&wheel(1, 1)), Event::Wheel(-1));
}

#[test]
fn alt_is_read_from_the_key_modifiers() {
    // `KMOD_LALT` is 0x0100: v1.2's fullscreen chord is Left Alt + Enter.
    assert!(sdlmini::left_alt(0x0100));
    assert!(!sdlmini::left_alt(0x0200));
    assert!(!sdlmini::left_alt(0x0001));
}
