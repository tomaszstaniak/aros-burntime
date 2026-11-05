//! The keyboard as v1.2's bindings name it: SDL scancodes to
//! [`burngfx::input::Key`] and back.
//!
//! Only keys from the window go through this. A script's own steps (`day`,
//! `take`, `pack`, …) are events of their own and never meet the bindings;
//! a script that means a real key says `press <key>`.

use burngfx::input::Key;

/// SDL's scancodes for the keys the bindings can name. Letters run from 4
/// (`a`) to 29 (`z`), digits from 30 (`1`) to 39 (`0`).
pub fn key_of(scancode: u32) -> Option<Key> {
    Some(match scancode {
        4..=29 => Key::Char((b'a' + (scancode - 4) as u8) as char),
        30..=38 => Key::Char((b'1' + (scancode - 30) as u8) as char),
        39 => Key::Char('0'),
        40 | 88 => Key::Enter,
        41 => Key::Escape,
        42 => Key::Char('\x08'),
        43 => Key::Tab,
        44 => Key::Char(' '),
        58 => Key::F1,
        79 => Key::Right,
        80 => Key::Left,
        81 => Key::Down,
        82 => Key::Up,
        224 | 228 => Key::Ctrl,
        226 | 230 => Key::Alt,
        _ => return None,
    })
}

/// The scancode a `press` step sends for a key: the left Ctrl and Alt, the
/// main Enter.
pub fn scancode_of(key: Key) -> Option<u32> {
    Some(match key {
        Key::Char(c @ 'a'..='z') => 4 + (c as u32 - 'a' as u32),
        Key::Char(c @ '1'..='9') => 30 + (c as u32 - '1' as u32),
        Key::Char('0') => 39,
        Key::Char(' ') => 44,
        Key::Char('\x08') => 42,
        Key::Char(_) => return None,
        Key::Enter => 40,
        Key::Escape => 41,
        Key::Tab => 43,
        Key::F1 => 58,
        Key::Right => 79,
        Key::Left => 80,
        Key::Down => 81,
        Key::Up => 82,
        Key::Ctrl => 224,
        Key::Alt => 226,
    })
}

/// A controller's buttons and triggers stand above every scancode: button
/// `b` is `PAD + b`, the triggers `PAD + 20` (left) and `PAD + 21` (right),
/// so the keys' gate, the keys held and the next-turn hold take them alike.
pub const PAD: u32 = 0x1000;

/// `SDL_GameControllerButton` (and the two triggers) as `[gamepad]` names
/// them.
pub fn pad_of(code: u32) -> Option<burngfx::input::Pad> {
    use burngfx::input::Pad;
    Some(match code.checked_sub(PAD)? {
        0 => Pad::A,
        1 => Pad::B,
        2 => Pad::X,
        3 => Pad::Y,
        4 => Pad::View,
        6 => Pad::Menu,
        7 => Pad::LeftStick,
        8 => Pad::RightStick,
        9 => Pad::LeftShoulder,
        10 => Pad::RightShoulder,
        11 => Pad::DPadUp,
        12 => Pad::DPadDown,
        13 => Pad::DPadLeft,
        14 => Pad::DPadRight,
        20 => Pad::LeftTrigger,
        21 => Pad::RightTrigger,
        _ => return None,
    })
}

/// The left stick's navigation press (`InputManager.Press(navigationAction)`):
/// a code of its own above the pad's, one for each of the eight directions,
/// that means its move whatever the bindings say.
pub const STICK: u32 = 0x1100;

/// The press's code for a direction.
pub fn stick_code(d: (i32, i32)) -> u32 {
    STICK + ((d.0 + 1) * 3 + (d.1 + 1)) as u32
}

/// The move a stick code means.
pub fn stick_action(code: u32) -> Option<burngfx::input::Action> {
    let n = code.checked_sub(STICK).filter(|&n| n < 9)? as i32;
    burngfx::input::Action::moving((n / 3 - 1, n % 3 - 1))
}

/// SDL's `KMOD_SHIFT`: either shift key.
pub const SHIFT: u16 = 0x0003;

/// Whether a key press is v1.2's F11 (`BurntimeGame.cs:582-589`), by SDL
/// scancode. `aros` is `cfg!(target_os = "aros")`, a parameter so that both
/// answers are tested on the host.
///
/// AROS-specific input adaptation (UI-20): on AROS One (ABIv11) a PC's F11
/// never reaches SDL as `SDL_SCANCODE_F11`. The i8042 driver's table
/// (deadwood2/AROS `e33ca77d`, `arch/all-pc/hidds/i8042/stdkeytable.h:96`)
/// turns PC scancode 0x57 into `RAWKEY_HELP` (0x5F), and the SDK's SDL2
/// (`libSDL2.a` 0779ea7b…) turns 0x5F into `SDL_SCANCODE_SCROLLLOCK`.
/// Measured on AROS One x86_64 with a probe linked against that `libSDL2.a`
/// (2026-10-25).
/// So on AROS, ScrollLock's scancode is accepted as F11 as well. A key the
/// keyboard driver calls Help arrives the same way and cannot be told apart;
/// elsewhere ScrollLock is not F11.
pub fn is_f11(scancode: u32, aros: bool) -> bool {
    scancode == sdlmini::scancode::F11 || (aros && scancode == sdlmini::scancode::SCROLLLOCK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burngfx::input::key_named;

    #[test]
    fn every_named_key_goes_there_and_back() {
        let names = ["f1", "ctrl", "space", "backspace", "enter", "escape", "tab", "alt", "up", "down", "left", "right",
                     "a", "q", "z", "0", "1", "9"];
        for name in names {
            let key = key_named(name).unwrap();
            let sc = scancode_of(key).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(key_of(sc), Some(key), "{name}");
        }
        assert_eq!(key_of(23), Some(Key::Char('t')));
        assert_eq!(key_of(88), Some(Key::Enter), "the keypad's Enter is Enter too");
        assert_eq!(key_of(72), None, "Pause is not a binding");
    }

    #[test]
    fn f11_is_scroll_lock_only_on_aros() {
        use sdlmini::scancode::{F11, F12, SCROLLLOCK};
        assert!(is_f11(F11, false));
        assert!(is_f11(F11, true));
        assert!(!is_f11(SCROLLLOCK, false), "ScrollLock is not F11 off AROS");
        assert!(is_f11(SCROLLLOCK, true), "AROS One delivers F11 as ScrollLock");
        assert!(!is_f11(F12, true));
        assert!(!is_f11(40, true), "Enter alone is not F11");
    }

    #[test]
    fn stick_codes_mean_their_moves() {
        use burngfx::input::Action as A;
        assert_eq!(stick_action(stick_code((1, 0))), Some(A::MoveRight));
        assert_eq!(stick_action(stick_code((-1, -1))), Some(A::MoveUpLeft));
        assert_eq!(stick_action(stick_code((0, 0))), None);
        assert_eq!(stick_action(PAD), None);
        assert_eq!(pad_of(stick_code((0, 1))), None, "a stick press is no button");
    }

    #[test]
    fn pad_codes_are_the_controllers_buttons() {
        use burngfx::input::Pad;
        assert_eq!(pad_of(PAD), Some(Pad::A));
        assert_eq!(pad_of(PAD + 12), Some(Pad::DPadDown));
        assert_eq!(pad_of(PAD + 21), Some(Pad::RightTrigger));
        assert_eq!(pad_of(PAD + 5), None, "the guide button is nobody's");
        assert_eq!(pad_of(23), None, "a scancode is not a pad's");
    }
}
