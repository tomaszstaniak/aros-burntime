//! Just enough SDL2 to open a window and push pixels at it.
//!
//! Only opaque handles are used, so no SDL struct layout is duplicated here —
//! the one exception is `SDL_Event`, which is read as raw bytes and never
//! interpreted beyond its leading event type.
//!
//! AROS renders SDL in software, so the texture path (upload a whole frame,
//! let SDL scale it) is both the simplest and the fastest option.
//!
//! Lifted from the `aros` runtime crate in tomaszstaniak/rust-aros, where it was
//! AROS-only; nothing in it actually was, so it lives here now and serves the
//! host build too.

#![no_std]

use core::ffi::c_void;

// First, so that on AROS the static SDL2_mixer is linked ahead of the SDL2
// it calls into.
pub mod audio;
#[cfg(sdlmini_gl_standins)]
mod gl_standins;

// Which SDL2, SDL2_mixer, GL and iconv libraries are linked, and in what
// order, is build.rs's choice: it differs between the AROS targets.
extern "C" {
    fn SDL_Init(flags: u32) -> i32;
    fn SDL_Quit();
    fn SDL_GetError() -> *const u8;
    fn SDL_CreateWindow(title: *const u8, x: i32, y: i32, w: i32, h: i32, flags: u32) -> *mut c_void;
    fn SDL_DestroyWindow(w: *mut c_void);
    fn SDL_CreateRenderer(window: *mut c_void, index: i32, flags: u32) -> *mut c_void;
    fn SDL_DestroyRenderer(r: *mut c_void);
    fn SDL_CreateTexture(r: *mut c_void, format: u32, access: i32, w: i32, h: i32) -> *mut c_void;
    fn SDL_DestroyTexture(t: *mut c_void);
    fn SDL_UpdateTexture(t: *mut c_void, rect: *const c_void, pixels: *const c_void, pitch: i32) -> i32;
    fn SDL_RenderCopy(r: *mut c_void, t: *mut c_void, src: *const c_void, dst: *const c_void) -> i32;
    fn SDL_RenderPresent(r: *mut c_void);
    fn SDL_PollEvent(event: *mut u8) -> i32;
    fn SDL_GetTicks() -> u32;
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetPerformanceFrequency() -> u64;
    fn SDL_Delay(ms: u32);
    fn SDL_SetHint(name: *const u8, value: *const u8) -> i32;
    fn SDL_GetRendererInfo(renderer: *mut c_void, info: *mut u8) -> i32;
    fn SDL_ShowCursor(toggle: i32) -> i32;
    fn SDL_StartTextInput();
    fn SDL_InitSubSystem(flags: u32) -> i32;
    fn SDL_NumJoysticks() -> i32;
    fn SDL_IsGameController(index: i32) -> i32;
    fn SDL_GameControllerOpen(index: i32) -> *mut c_void;
    fn SDL_SetWindowFullscreen(window: *mut c_void, flags: u32) -> i32;
    fn SDL_GetWindowFlags(window: *mut c_void) -> u32;
    fn SDL_GetNumDisplayModes(display: i32) -> i32;
    fn SDL_GetWindowSize(window: *mut c_void, w: *mut i32, h: *mut i32);
    fn SDL_SetWindowSize(window: *mut c_void, w: i32, h: i32);
    fn SDL_GetWindowDisplayIndex(window: *mut c_void) -> i32;
    fn SDL_GetDesktopDisplayMode(display: i32, mode: *mut u8) -> i32;
    fn SDL_GetDisplayUsableBounds(display: i32, rect: *mut [i32; 4]) -> i32;
    fn SDL_GetWindowBordersSize(window: *mut c_void, top: *mut i32, left: *mut i32, bottom: *mut i32, right: *mut i32) -> i32;
    fn SDL_SetWindowPosition(window: *mut c_void, x: i32, y: i32);
    fn SDL_GetPreferredLocales() -> *mut [*const u8; 2];
    fn SDL_free(p: *mut c_void);
}

/// The system's first preferred language, as its two letters
/// (`SDL_GetPreferredLocales`, SDL 2.0.14): what v1.2's `Auto` language
/// asks the system (`DetectAutomaticLanguage`, the UI culture). `None`
/// where SDL has no answer — an SDL whose platform reports no locale.
pub fn preferred_language() -> Option<[u8; 2]> {
    unsafe {
        let list = SDL_GetPreferredLocales();
        if list.is_null() {
            return None;
        }
        let first = (*list)[0];
        let out = if first.is_null() || *first == 0 || *first.add(1) == 0 {
            None
        } else {
            Some([(*first).to_ascii_lowercase(), (*first.add(1)).to_ascii_lowercase()])
        };
        SDL_free(list as *mut c_void);
        out
    }
}

/// **Which renderer to ask SDL for, said out loud rather than left to it.**
///
/// On the Mac the SDL2 that Homebrew installs is `sdl2-compat`, a shim over
/// SDL3, and asked for "any renderer" SDL3 picks Metal. A full run on that
/// path hung. So the host asks for the software renderer unless told
/// otherwise: the game already draws every pixel into its own buffer and
/// only needs it copied to the window, which is exactly what the software
/// renderer does and nothing more.
///
/// `BURNTIME_RENDERER` overrides it — `opengl`, `metal`, `software` — for a
/// deliberate test of another path. AROS names the software renderer as
/// well (below), the one its SDL chose by itself.
#[cfg(not(target_os = "aros"))]
extern "C" {
    fn getenv(name: *const u8) -> *const u8;
}

#[cfg(not(target_os = "aros"))]
fn chosen_renderer() -> Option<&'static [u8]> {
    // `no_std`, so the environment is asked through libc directly.
    let asked = unsafe {
        let v = getenv(b"BURNTIME_RENDERER\0".as_ptr());
        if v.is_null() {
            &[][..]
        } else {
            let mut len = 0;
            while *v.add(len) != 0 && len < 32 {
                len += 1;
            }
            core::slice::from_raw_parts(v, len)
        }
    };
    Some(match asked {
        b"opengl" => b"opengl\0",
        b"metal" => b"metal\0",
        // Anything else, including nothing at all, is the safe one.
        _ => b"software\0",
    })
}

/// AROS: the software renderer by name, so that SDL never tries its
/// OpenGL one and never opens gl.library (`build.rs`, `sdlmini_no_gl`).
/// It is the renderer SDL ended up with there anyway.
#[cfg(target_os = "aros")]
fn chosen_renderer() -> Option<&'static [u8]> {
    Some(b"software\0")
}

pub const INIT_VIDEO: u32 = 0x0000_0020;
pub const WINDOWPOS_CENTERED: i32 = 0x2FFF_0000u32 as i32;
pub const WINDOW_SHOWN: u32 = 0x0000_0004;
/// The player may resize the window: v1.2's `Window.AllowUserResizing`
/// (`BurntimeGame.cs:296`).
pub const WINDOW_RESIZABLE: u32 = 0x0000_0020;
/// `SDL_WINDOW_FULLSCREEN_DESKTOP`: fullscreen at the desktop's own mode,
/// as v1.2's MonoGame build asks (`HardwareModeSwitch = false`).
pub const WINDOW_FULLSCREEN_DESKTOP: u32 = 0x0000_1001;
/// The bit both fullscreen kinds share.
pub const WINDOW_FULLSCREEN: u32 = 0x0000_0001;
pub const PIXELFORMAT_ARGB8888: u32 = 0x1636_2004;
pub const TEXTUREACCESS_STREAMING: i32 = 1;

pub const EVENT_QUIT: u32 = 0x100;
pub const EVENT_WINDOWEVENT: u32 = 0x200;
/// `SDL_WindowEventID`s that carry the client area's new size.
pub const WINDOWEVENT_RESIZED: u8 = 5;
pub const WINDOWEVENT_SIZE_CHANGED: u8 = 6;
pub const EVENT_KEYDOWN: u32 = 0x300;
pub const EVENT_KEYUP: u32 = 0x301;
pub const EVENT_TEXTINPUT: u32 = 0x303;
pub const EVENT_MOUSEMOTION: u32 = 0x400;
pub const EVENT_MOUSEBUTTONDOWN: u32 = 0x401;
pub const EVENT_MOUSEBUTTONUP: u32 = 0x402;
pub const EVENT_MOUSEWHEEL: u32 = 0x403;
pub const EVENT_CONTROLLERAXISMOTION: u32 = 0x650;
pub const EVENT_CONTROLLERBUTTONDOWN: u32 = 0x651;
pub const EVENT_CONTROLLERBUTTONUP: u32 = 0x652;
pub const EVENT_CONTROLLERDEVICEADDED: u32 = 0x653;
pub const INIT_GAMECONTROLLER: u32 = 0x0000_2000;

/// SDL's mouse button numbers.
pub const BUTTON_LEFT: u8 = 1;
pub const BUTTON_RIGHT: u8 = 3;

/// SDL scancodes, as many as anything here needs.
pub mod scancode {
    pub const RIGHT: u32 = 79;
    pub const LEFT: u32 = 80;
    pub const SPACE: u32 = 44;
    pub const ESCAPE: u32 = 41;
    pub const A: u32 = 4;
    pub const F: u32 = 9;
    pub const G: u32 = 10;
    pub const Q: u32 = 20;
    pub const W: u32 = 26;
    pub const I: u32 = 12;
    pub const N: u32 = 17;
    pub const J: u32 = 13;
    pub const D: u32 = 7;
    pub const T: u32 = 23;
    pub const C: u32 = 6;
    pub const P: u32 = 19;
    pub const O: u32 = 18;
    pub const B: u32 = 5;
    pub const H: u32 = 11;
    pub const S: u32 = 22;
    pub const L: u32 = 15;
    pub const RETURN: u32 = 40;
    pub const DOWN: u32 = 81;
    pub const UP: u32 = 82;
    /// The standing: what the player holds and what winning would take.
    pub const K: u32 = 14;
    /// `/` — the help page. Chosen because it is one key, needs no shift, and
    /// is next to where a hand rests.
    pub const SLASH: u32 = 56;
    pub const BACKSPACE: u32 = 42;
    pub const F8: u32 = 65;
    /// v1.2's music set, the remaster's or the Amiga's (`ToggleMusicMode`).
    pub const F9: u32 = 66;
    pub const F12: u32 = 69;
    /// v1.2's fullscreen key (`BurntimeGame.cs:582-589`).
    pub const F11: u32 = 68;
    /// What AROS One's SDL2 reports for a PC's F11 (`aros-game`'s
    /// `play_keys::is_f11`).
    pub const SCROLLLOCK: u32 = 71;
}

/// `KMOD_LALT`, in a key event's modifiers ([`Screen::last_key`]): v1.2's
/// fullscreen chord is the **left** Alt and Enter (`BurntimeGame.cs:556,583`).
pub fn left_alt(modifiers: u16) -> bool {
    modifiers & 0x0100 != 0
}

/// What came out of the event queue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Quit,
    /// A key press, by SDL scancode — see [`scancode`]. A key held down
    /// repeats it ([`Screen::last_repeat`] says which was a repeat).
    KeyDown(u32),
    /// A key let go, by scancode.
    KeyUp(u32),
    /// A game controller's button pressed and let go, by SDL's
    /// `SDL_GameControllerButton` number (A is 0, the d-pad 11–14).
    PadDown(u8),
    PadUp(u8),
    /// A controller's axis moved: `SDL_GameControllerAxis` (the sticks 0–3,
    /// the triggers 4 and 5) and its value, −32768 to 32767.
    PadAxis { axis: u8, value: i16 },
    /// A controller plugged in, by device index; [`Screen::poll`] opens it.
    PadAdded(i32),
    /// A mouse press, in window pixels. `button` is SDL's: 1 left, 3 right.
    MouseDown { x: i32, y: i32, button: u8 },
    /// A mouse button let go, in window pixels. The original's click is a
    /// press and a release; a right-button drag scrolls the map between them.
    MouseUp { x: i32, y: i32, button: u8 },
    /// The pointer moved, in window pixels.
    MouseMoved { x: i32, y: i32 },
    /// The wheel turned: positive away from the user (up), as MonoGame's
    /// delta, with a system's reversed direction undone. Where the pointer
    /// is comes from the last [`Event::MouseMoved`].
    Wheel(i32),
    /// One typed character, as SDL's text input delivers it — what a name
    /// field wants, which a scancode is not: it knows the keyboard layout and
    /// the shift key.
    Text(char),
    /// The window's client area is now `w` x `h` (`SDL_WINDOWEVENT_RESIZED`
    /// or `_SIZE_CHANGED`), v1.2's `ClientSizeChanged`. Zero when the
    /// system reports a minimised window that way.
    WindowSize { w: i32, h: i32 },
    /// Anything else, kept as its raw SDL event type.
    Other(u32),
}

/// Microseconds, for measuring. SDL's high-resolution counter does not move
/// on AROS One's SDL2 (every stage measured zero), so there it is the C
/// runtime's `gettimeofday`.
#[cfg(target_os = "aros")]
pub fn micros() -> u64 {
    // AROS's `struct timeval` is two 32-bit fields
    // (`aros/types/timeval_s.h`), not the 64-bit pair of other systems.
    extern "C" {
        fn gettimeofday(tv: *mut [u32; 2], tz: *mut c_void) -> i32;
    }
    let mut tv = [0u32; 2];
    unsafe {
        gettimeofday(&mut tv, core::ptr::null_mut());
    }
    tv[0] as u64 * 1_000_000 + tv[1] as u64
}

/// Microseconds on SDL's high-resolution counter, for measuring.
#[cfg(not(target_os = "aros"))]
pub fn micros() -> u64 {
    unsafe {
        let f = SDL_GetPerformanceFrequency().max(1);
        (SDL_GetPerformanceCounter() as u128 * 1_000_000 / f as u128) as u64
    }
}

/// Milliseconds since SDL started.
pub fn ticks() -> u32 {
    unsafe { SDL_GetTicks() }
}

/// Give the processor back for a while. A game loop with nothing due should
/// not spin drawing the same picture as fast as the machine allows.
pub fn delay(ms: u32) {
    unsafe { SDL_Delay(ms) }
}

/// A copy of the last SDL error message.
///
/// SDL owns the buffer `SDL_GetError` returns and overwrites it on the next
/// call, so the text is copied out rather than borrowed. An earlier version of
/// this returned `&'static str` straight into that buffer, which promised a
/// lifetime SDL does not honour.
pub struct Error {
    buf: [u8; 192],
    len: usize,
}

impl Error {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("(bad utf8)")
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The last SDL error, copied into a buffer we own.
pub fn error() -> Error {
    let mut e = Error { buf: [0; 192], len: 0 };
    unsafe {
        let p = SDL_GetError();
        if p.is_null() {
            let msg = b"(no error)";
            e.buf[..msg.len()].copy_from_slice(msg);
            e.len = msg.len();
            return e;
        }
        while e.len < e.buf.len() {
            let c = *p.add(e.len);
            if c == 0 {
                break;
            }
            e.buf[e.len] = c;
            e.len += 1;
        }
    }
    e
}

/// A window with a streaming texture behind it: write pixels, call
/// [`Screen::present`], repeat. Everything is released on drop, in order.
pub struct Screen {
    window: *mut c_void,
    renderer: *mut c_void,
    texture: *mut c_void,
    pub width: usize,
    pub height: usize,
    /// The last key press's `keysym.sym` and `keysym.mod`: the character
    /// the keyboard layout puts on that key, and the modifiers held.
    pub last_key: (u32, u16),
    /// Whether the last key press was the keyboard's repeat of a key held.
    pub last_repeat: bool,
    /// The keys down now, as the events have said.
    pub keys: Keys,
    /// The window can be made fullscreen, as far as anything has shown:
    /// the display reports a mode to be fullscreen in, and no switch has
    /// failed. v1.2 hides its Settings line and ignores F11 when its engine
    /// cannot toggle (`SupportsFullscreenToggle`); this is that answer,
    /// found at run time.
    fullscreen_capable: bool,
    /// Fullscreen now.
    fullscreen: bool,
    /// The size leaving fullscreen gives the window back: v1.2's
    /// `resetWindowSize` (`BurntimeGame.cs:333-341`), the size override or
    /// half the desktop — not whatever the window was before.
    windowed_size: (i32, i32),
}

/// Which keys are down, kept from the events: a press adds its scancode, a
/// release takes it away. v1.2 acts on keys held (the next turn after 0.6 s,
/// panning, walking) where v0.9.1 had only presses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// Up to sixteen at once; a seventeenth is not kept (no keyboard
    /// reports that many).
    held: [u32; 16],
    count: usize,
}

impl Keys {
    pub fn apply(&mut self, event: &Event) {
        match *event {
            Event::KeyDown(k) if !self.is_held(k) && self.count < self.held.len() => {
                self.held[self.count] = k;
                self.count += 1;
            }
            Event::KeyUp(k) => {
                if let Some(i) = self.held[..self.count].iter().position(|&h| h == k) {
                    self.held.copy_within(i + 1..self.count, i);
                    self.count -= 1;
                }
            }
            _ => {}
        }
    }

    pub fn is_held(&self, scancode: u32) -> bool {
        self.held[..self.count].contains(&scancode)
    }

    /// Everything let go, as when the window loses the keyboard.
    pub fn clear(&mut self) {
        self.count = 0;
    }
}

/// An `SDL_Event`'s bytes as an [`Event`]. Only the fields used are read:
/// the leading type; a key event's `repeat` (byte 13) and keysym (scancode
/// at 16); a mouse event's button (16) and coordinates (20, 24); text input's
/// first character (from 12); the wheel's turn (20) and direction (24); a
/// window event's kind (12) and its two numbers (16, 20).
pub fn decode(ev: &[u8; 64]) -> Event {
    let u32_at = |i: usize| u32::from_ne_bytes([ev[i], ev[i + 1], ev[i + 2], ev[i + 3]]);
    let i32_at = |i: usize| i32::from_ne_bytes([ev[i], ev[i + 1], ev[i + 2], ev[i + 3]]);
    let kind = u32_at(0);
    match kind {
        EVENT_QUIT => Event::Quit,
        // `SDL_WindowEvent`: type, timestamp, windowID, then `event` and
        // three bytes of padding, `data1` and `data2`.
        EVENT_WINDOWEVENT if matches!(ev[12], WINDOWEVENT_RESIZED | WINDOWEVENT_SIZE_CHANGED) => {
            Event::WindowSize { w: i32_at(16), h: i32_at(20) }
        }
        EVENT_KEYDOWN => Event::KeyDown(u32_at(16)),
        EVENT_KEYUP => Event::KeyUp(u32_at(16)),
        EVENT_MOUSEMOTION => Event::MouseMoved { x: i32_at(20), y: i32_at(24) },
        EVENT_MOUSEBUTTONDOWN => Event::MouseDown { x: i32_at(20), y: i32_at(24), button: ev[16] },
        EVENT_MOUSEBUTTONUP => Event::MouseUp { x: i32_at(20), y: i32_at(24), button: ev[16] },
        // `SDL_MouseWheelEvent`: `y` at 20, `direction` at 24 (1 flipped).
        EVENT_MOUSEWHEEL => Event::Wheel(if u32_at(24) == 1 { -i32_at(20) } else { i32_at(20) }),
        // `SDL_ControllerButtonEvent`: type, timestamp, which, then the button.
        EVENT_CONTROLLERBUTTONDOWN => Event::PadDown(ev[12]),
        EVENT_CONTROLLERBUTTONUP => Event::PadUp(ev[12]),
        // `SDL_ControllerAxisEvent`: the axis at 12, its value at 16.
        EVENT_CONTROLLERAXISMOTION => Event::PadAxis { axis: ev[12], value: i16::from_ne_bytes([ev[16], ev[17]]) },
        // `SDL_ControllerDeviceEvent`: the device index at 8.
        EVENT_CONTROLLERDEVICEADDED => Event::PadAdded(i32_at(8)),
        EVENT_TEXTINPUT => {
            let text = &ev[12..44];
            let len = text.iter().position(|&b| b == 0).unwrap_or(text.len());
            match core::str::from_utf8(&text[..len]).ok().and_then(|s| s.chars().next()) {
                Some(c) => Event::Text(c),
                None => Event::Other(kind),
            }
        }
        other => Event::Other(other),
    }
}

impl Screen {
    /// Open a `win_w` x `win_h` window drawing a `w` x `h` pixel buffer,
    /// scaled by SDL. `title` must be NUL-terminated.
    pub fn open(title: &[u8], w: usize, h: usize, win_w: i32, win_h: i32) -> Result<Screen, &'static str> {
        unsafe {
            if SDL_Init(INIT_VIDEO) != 0 {
                return Err("SDL_Init failed");
            }
        }
        Screen::create(title, w, h, (win_w, win_h), WINDOW_SHOWN)
    }

    /// **The game's window**, as v1.2 opens its own
    /// (`ApplyGraphicsDeviceResolution(initialize: true)`,
    /// `BurntimeGame.cs:314-365`): resizable, `size` across (half the
    /// desktop when `None`), and given back `windowed` when fullscreen ends
    /// (half the desktop when `None`). The pixel buffer starts as one
    /// pixel; the caller sizes it ([`Screen::set_buffer_size`]) once it
    /// knows the backbuffer the window's actual size makes — never the
    /// window's own size, which can be far larger than any texture.
    ///
    /// `fit`, when given and no `size` is, replaces half the desktop: it is
    /// asked for the window's client size from the desktop, the part of it
    /// windows may use and the window's frame ([`Fit`]) — once before the
    /// window exists, when the frame is not known and is given as nothing,
    /// and once after, with the frame the system gave it; a different
    /// answer the second time resizes and centres the window before
    /// anything is drawn. Its answer is also the size leaving fullscreen
    /// gives back when `windowed` is `None`.
    pub fn open_game(
        title: &[u8],
        size: Option<(i32, i32)>,
        windowed: Option<(i32, i32)>,
        fit: Option<fn(&Fit) -> (i32, i32)>,
    ) -> Result<Screen, &'static str> {
        unsafe {
            if SDL_Init(INIT_VIDEO) != 0 {
                return Err("SDL_Init failed");
            }
        }
        // `displayResolution / 2`. Without a desktop mode there is nothing to
        // halve; 1280 x 720 is what this program opened before it asked.
        let desktop = desktop_mode(0);
        let half = desktop.map_or((1280, 720), |(w, h)| ((w / 2).max(1), (h / 2).max(1)));
        let fit = fit.filter(|_| size.is_none() && desktop.is_some());
        let before = fit.map(|f| f(&Fit::of_display(0, (0, 0, 0, 0))));
        let (win_w, win_h) = size.or(before).unwrap_or(half);
        let mut screen = Screen::create(title, 1, 1, (win_w, win_h), WINDOW_SHOWN | WINDOW_RESIZABLE)?;
        let mut opened = (win_w, win_h);
        if let Some(f) = fit {
            let after = f(&screen.fit());
            if after != opened && after.0 > 0 && after.1 > 0 {
                unsafe {
                    SDL_SetWindowSize(screen.window, after.0, after.1);
                    SDL_SetWindowPosition(screen.window, WINDOWPOS_CENTERED, WINDOWPOS_CENTERED);
                }
                opened = after;
            }
        }
        screen.windowed_size = windowed.unwrap_or(if fit.is_some() { opened } else { half });
        Ok(screen)
    }

    /// The desktop, its usable part and this window's frame, as
    /// [`Screen::open_game`]'s `fit` is given them.
    pub fn fit(&self) -> Fit {
        let display = unsafe { SDL_GetWindowDisplayIndex(self.window) }.max(0);
        let (mut top, mut left, mut bottom, mut right) = (0, 0, 0, 0);
        let known = unsafe { SDL_GetWindowBordersSize(self.window, &mut top, &mut left, &mut bottom, &mut right) } == 0;
        let borders = if known { (top.max(0), left.max(0), bottom.max(0), right.max(0)) } else { (0, 0, 0, 0) };
        let mut fit = Fit::of_display(display, borders);
        fit.borders_known = known;
        fit
    }

    /// The window, its renderer and texture, SDL's video already started;
    /// on failure SDL is shut down again.
    fn create(title: &[u8], w: usize, h: usize, win: (i32, i32), flags: u32) -> Result<Screen, &'static str> {
        unsafe {
            let window = SDL_CreateWindow(
                title.as_ptr(), WINDOWPOS_CENTERED, WINDOWPOS_CENTERED, win.0, win.1, flags);
            if window.is_null() {
                SDL_Quit();
                return Err("SDL_CreateWindow failed");
            }
            if let Some(driver) = chosen_renderer() {
                SDL_SetHint(b"SDL_RENDER_DRIVER\0".as_ptr(), driver.as_ptr());
            }
            let renderer = SDL_CreateRenderer(window, -1, 0);
            if renderer.is_null() {
                SDL_DestroyWindow(window);
                SDL_Quit();
                return Err("SDL_CreateRenderer failed");
            }
            let texture = SDL_CreateTexture(
                renderer, PIXELFORMAT_ARGB8888, TEXTUREACCESS_STREAMING, w as i32, h as i32);
            if texture.is_null() {
                SDL_DestroyRenderer(renderer);
                SDL_DestroyWindow(window);
                SDL_Quit();
                return Err("SDL_CreateTexture failed");
            }
            let fullscreen_capable = SDL_GetNumDisplayModes(0) > 0;
            Ok(Screen {
                window, renderer, texture, width: w, height: h, last_key: (0, 0), last_repeat: false, keys: Keys::default(),
                fullscreen_capable, fullscreen: false, windowed_size: win,
            })
        }
    }

    /// The window's client area in the pixels the mouse is reported in:
    /// v1.2's `Window.ClientBounds`. The window is not high-DPI, so this is
    /// also the renderer's output.
    pub fn client_size(&self) -> (i32, i32) {
        let (mut w, mut h) = (0, 0);
        unsafe {
            SDL_GetWindowSize(self.window, &mut w, &mut h);
        }
        (w, h)
    }

    /// The desktop mode of the display the window is on:
    /// `GraphicsAdapter.DefaultAdapter.CurrentDisplayMode`.
    pub fn desktop_size(&self) -> Option<(i32, i32)> {
        let display = unsafe { SDL_GetWindowDisplayIndex(self.window) };
        desktop_mode(display.max(0))
    }

    /// Ask for a client area of `w` x `h`, as a player dragging the frame
    /// would. The system may give another size; [`Screen::client_size`]
    /// says which. Nothing happens in fullscreen.
    pub fn set_window_size(&mut self, w: i32, h: i32) {
        if !self.fullscreen && w > 0 && h > 0 {
            unsafe {
                SDL_SetWindowSize(self.window, w, h);
            }
        }
    }

    /// The pixel buffer [`Screen::present`] takes is now `w` x `h`: a new
    /// texture, stretched over the whole window. False, and the old buffer
    /// kept, when SDL cannot make it.
    pub fn set_buffer_size(&mut self, w: usize, h: usize) -> bool {
        if w == 0 || h == 0 {
            return false;
        }
        if (w, h) == (self.width, self.height) {
            return true;
        }
        unsafe {
            let texture = SDL_CreateTexture(
                self.renderer, PIXELFORMAT_ARGB8888, TEXTUREACCESS_STREAMING, w as i32, h as i32);
            if texture.is_null() {
                return false;
            }
            SDL_DestroyTexture(self.texture);
            self.texture = texture;
        }
        self.width = w;
        self.height = h;
        true
    }

    /// **The renderer SDL actually gave us**, by its own name. Asking for one
    /// and assuming it was granted is what a hang on the wrong path looks
    /// like from the inside; this reads the answer back.
    pub fn renderer_name(&self) -> &'static str {
        // `SDL_RendererInfo` begins with `const char *name`; the rest of the
        // struct is not needed and the buffer is generously larger than it.
        let mut info = [0u8; 128];
        unsafe {
            if SDL_GetRendererInfo(self.renderer, info.as_mut_ptr()) != 0 {
                return "unknown";
            }
            let name = usize::from_ne_bytes(
                info[..core::mem::size_of::<usize>()].try_into().unwrap_or([0; core::mem::size_of::<usize>()]),
            ) as *const u8;
            if name.is_null() {
                return "unknown";
            }
            let mut len = 0;
            while *name.add(len) != 0 && len < 64 {
                len += 1;
            }
            let bytes = core::slice::from_raw_parts(name, len);
            core::str::from_utf8(bytes).unwrap_or("unknown")
        }
    }

    /// `SupportsFullscreenToggle`: whether a fullscreen switch is worth
    /// offering. False once a switch has failed.
    pub fn supports_fullscreen(&self) -> bool {
        self.fullscreen_capable
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    /// Fullscreen at the desktop's mode (`HardwareModeSwitch = false`), or
    /// back to the window at its windowed size (`resetWindowSize`). The
    /// picture is not adapted here: the window's size has changed, and the
    /// caller rebuilds what follows from it as after any resize, from
    /// [`Screen::client_size`]. True when the window is now as asked; a
    /// switch SDL refuses, or one it accepts and does not make, puts the
    /// window back and marks the toggle unsupported, so the caller can stop
    /// offering it.
    pub fn set_fullscreen(&mut self, on: bool) -> bool {
        if !self.fullscreen_capable {
            return !on;
        }
        unsafe {
            let flags = if on { WINDOW_FULLSCREEN_DESKTOP } else { 0 };
            let failed = SDL_SetWindowFullscreen(self.window, flags) != 0
                || (SDL_GetWindowFlags(self.window) & WINDOW_FULLSCREEN != 0) != on;
            if failed {
                SDL_SetWindowFullscreen(self.window, 0);
                self.fullscreen_capable = false;
                self.fullscreen = false;
                return !on;
            }
            self.fullscreen = on;
            if !on {
                SDL_SetWindowSize(self.window, self.windowed_size.0, self.windowed_size.1);
            }
            true
        }
    }

    /// Hide the system pointer, for a game that draws its own. The original
    /// draws `munt.raw` frame 0 under the mouse and nothing else.
    pub fn hide_pointer(&mut self) {
        unsafe {
            SDL_ShowCursor(0);
        }
    }

    /// The character the last key press types, from its keycode and Shift,
    /// for an SDL that sends no text events: printable ASCII only.
    pub fn typed_from_last_key(&self) -> Option<char> {
        let (sym, m) = self.last_key;
        if !(32..127).contains(&sym) {
            return None;
        }
        let shift = m & 0x0003 != 0; // KMOD_LSHIFT | KMOD_RSHIFT
        let caps = m & 0x2000 != 0; // KMOD_CAPS
        let c = sym as u8 as char;
        Some(if c.is_ascii_lowercase() && (shift ^ caps) { c.to_ascii_uppercase() } else { c })
    }

    /// Ask for typed characters ([`Event::Text`]). SDL on the desktop turns
    /// text input on by itself; an SDL that does not will deliver key
    /// presses and no text until this is called.
    pub fn start_text_input(&mut self) {
        unsafe {
            SDL_StartTextInput();
        }
    }

    /// Upload one frame of `width * height` ARGB pixels and show it.
    pub fn present(&mut self, pixels: &[u32]) {
        debug_assert_eq!(pixels.len(), self.width * self.height);
        unsafe {
            SDL_UpdateTexture(
                self.texture,
                core::ptr::null(),
                pixels.as_ptr() as *const c_void,
                (self.width * 4) as i32,
            );
            SDL_RenderCopy(self.renderer, self.texture, core::ptr::null(), core::ptr::null());
            SDL_RenderPresent(self.renderer);
        }
    }

    /// True once the user closes the window or presses a key.
    ///
    /// For anything that needs to tell keys apart, use [`Screen::poll`].
    pub fn should_quit(&mut self) -> bool {
        let mut quit = false;
        while let Some(event) = self.poll() {
            if matches!(event, Event::Quit | Event::KeyDown(_)) {
                quit = true;
            }
        }
        quit
    }

    /// The next pending event, or `None` when the queue is empty. A key
    /// press also notes its keysym and modifiers ([`Screen::last_key`]) and
    /// whether it was a repeat; presses and releases keep [`Screen::keys`].
    pub fn poll(&mut self) -> Option<Event> {
        // SDL_Event is a union of at most 56 bytes.
        let mut ev = [0u8; 64];
        unsafe {
            if SDL_PollEvent(ev.as_mut_ptr()) == 0 {
                return None;
            }
        }
        let event = decode(&ev);
        if let Event::KeyDown(_) = event {
            // keysym: scancode at 16, sym at 20, mod at 24.
            self.last_key = (
                u32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]),
                u16::from_ne_bytes([ev[24], ev[25]]),
            );
            self.last_repeat = ev[13] != 0;
        }
        self.keys.apply(&event);
        if let Event::PadAdded(index) = event {
            open_controller(index);
        }
        Some(event)
    }
}

/// The game-controller subsystem, and every controller already there.
/// Nothing is wrong when there is none, or no subsystem: no pad events come.
pub fn open_controllers() -> usize {
    unsafe {
        if SDL_InitSubSystem(INIT_GAMECONTROLLER) != 0 {
            return 0;
        }
        (0..SDL_NumJoysticks()).filter(|&i| open_controller(i)).count()
    }
}

/// What the first window's size is fitted to ([`Screen::open_game`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    /// The desktop mode, `SDL_GetDesktopDisplayMode`; (0, 0) when unknown.
    pub desktop: (i32, i32),
    /// `SDL_GetDisplayUsableBounds`: the part of the desktop windows may
    /// cover, frame included. SDL gives the whole display when the video
    /// driver has no answer of its own; the desktop when SDL has none.
    pub usable: (i32, i32, i32, i32),
    /// `SDL_GetWindowBordersSize`, top, left, bottom, right: the frame
    /// around the client area. (0, 0, 0, 0) before a window exists.
    pub borders: (i32, i32, i32, i32),
    /// SDL answered for the frame (rather than (0, 0, 0, 0) standing in).
    pub borders_known: bool,
}

impl Fit {
    fn of_display(display: i32, borders: (i32, i32, i32, i32)) -> Fit {
        let desktop = desktop_mode(display).unwrap_or((0, 0));
        let mut r = [0i32; 4];
        let usable = if unsafe { SDL_GetDisplayUsableBounds(display, &mut r) } == 0 && r[2] > 0 && r[3] > 0 {
            (r[0], r[1], r[2], r[3])
        } else {
            (0, 0, desktop.0, desktop.1)
        };
        Fit { desktop, usable, borders, borders_known: false }
    }
}

/// `SDL_GetDesktopDisplayMode`'s size. `SDL_DisplayMode` is a `Uint32`
/// format, then `w` and `h`; the rest is not read, and the buffer is larger
/// than the struct.
fn desktop_mode(display: i32) -> Option<(i32, i32)> {
    let mut mode = [0u8; 64];
    unsafe {
        if SDL_GetDesktopDisplayMode(display, mode.as_mut_ptr()) != 0 {
            return None;
        }
    }
    let at = |i: usize| i32::from_ne_bytes([mode[i], mode[i + 1], mode[i + 2], mode[i + 3]]);
    Some((at(4), at(8))).filter(|&(w, h)| w > 0 && h > 0)
}

fn open_controller(index: i32) -> bool {
    unsafe { SDL_IsGameController(index) != 0 && !SDL_GameControllerOpen(index).is_null() }
}

impl Drop for Screen {
    fn drop(&mut self) {
        unsafe {
            SDL_DestroyTexture(self.texture);
            SDL_DestroyRenderer(self.renderer);
            SDL_DestroyWindow(self.window);
            SDL_Quit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `SDL_WindowEvent` as SDL lays it out: type, timestamp, windowID,
    /// the event's kind and padding, `data1`, `data2`.
    fn window_event(kind: u8, w: i32, h: i32) -> [u8; 64] {
        let mut ev = [0u8; 64];
        ev[..4].copy_from_slice(&EVENT_WINDOWEVENT.to_ne_bytes());
        ev[8..12].copy_from_slice(&1u32.to_ne_bytes());
        ev[12] = kind;
        ev[16..20].copy_from_slice(&w.to_ne_bytes());
        ev[20..24].copy_from_slice(&h.to_ne_bytes());
        ev
    }

    #[test]
    fn a_window_resize_carries_the_client_size() {
        assert_eq!(decode(&window_event(WINDOWEVENT_RESIZED, 1920, 1080)), Event::WindowSize { w: 1920, h: 1080 });
        assert_eq!(decode(&window_event(WINDOWEVENT_SIZE_CHANGED, 640, 480)), Event::WindowSize { w: 640, h: 480 });
        // Minimised (7), moved (4): not a size, kept as the raw type.
        assert_eq!(decode(&window_event(7, 0, 0)), Event::Other(EVENT_WINDOWEVENT));
        assert_eq!(decode(&window_event(4, 10, 10)), Event::Other(EVENT_WINDOWEVENT));
    }
}
