//! v1.2's scene blend: every `SceneManager.SetScene` fades the picture to
//! black, waits, changes the scene and fades back in
//! (`Burntime.Framework/SceneManager.cs:56-108`, `BlendOverlayBase.cs`,
//! `FadingHelper.cs`). `scene_blend` in `settings.txt [engine]` is the
//! fade's speed per second (4 as shipped: 250 ms).
//!
//! `FadeOut(wait: true)` sets the overlay rising and keeps the game's thread
//! asleep for `500 + (1 - state) * 1000 / speed` ms: the old picture goes
//! black over what is left of the fade and stays black for the rest. Then
//! the new scene is activated and `FadeIn` lets the overlay fall at the same
//! speed while the game runs on.

/// What `FadeOut(wait: true)` adds to the fade: half a second.
pub const HOLD_MS: u32 = 500;

/// One fade's length in ms at `scene_blend` speed `speed` (per second).
pub fn fade_ms(speed: u32) -> u32 {
    1000 / speed.max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blend {
    began: u32,
    /// The overlay when the change began, 0–255 (0 unless a fade-in was
    /// still under way).
    from: u32,
    fade: u32,
}

impl Blend {
    /// A scene change at `now`, the overlay at `from`.
    pub fn begin(now: u32, from: u32, fade: u32) -> Blend {
        Blend { began: now, from: from.min(255), fade: fade.max(1) }
    }

    /// A scene that opens from black: only the fade back in.
    pub fn fading_in(now: u32, fade: u32) -> Blend {
        let b = Blend::begin(now, 255, fade);
        Blend { began: now.wrapping_sub(b.wait_ms()), ..b }
    }

    /// How long the game waits: `500 + (1 - state) * 1000 / speed`.
    pub fn wait_ms(&self) -> u32 {
        HOLD_MS + (255 - self.from) * self.fade / 255
    }

    /// Still waiting at `now`: the old picture, going or gone to black.
    pub fn waiting(&self, now: u32) -> bool {
        now.wrapping_sub(self.began) < self.wait_ms()
    }

    /// The black over everything at `now`, 0–255.
    pub fn alpha(&self, now: u32) -> u32 {
        let t = now.wrapping_sub(self.began);
        let w = self.wait_ms();
        if t < w {
            (self.from + 255 * t / self.fade).min(255)
        } else {
            255 - (255 * (t - w) / self.fade).min(255)
        }
    }

    /// The fade back in is over.
    pub fn done(&self, now: u32) -> bool {
        now.wrapping_sub(self.began) >= self.wait_ms() + self.fade
    }
}
