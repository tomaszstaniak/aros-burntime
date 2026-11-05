//! How many steps of the world are due, separate from how often it is drawn.
//!
//! Until this existed the game advanced one step per frame drawn, so a
//! machine that drew slowly also walked slowly and reacted slowly — the
//! original's three seconds became however many frames AROS managed in
//! three seconds. Now the world steps at a fixed rate and the picture is
//! drawn as often as it can be.
//!
//! **A scripted run stays one step per frame.** Its whole value is that the
//! same words give the same file on every machine, and a wall clock would
//! make the number of steps between two script words depend on how fast the
//! machine drew them.

/// Steps of the world in a second: [`crate::mind::SECOND`], which is what
/// made the original's timers convertible in the first place.
///
/// **A step is exactly `1000 / 18` ms**, kept as a fraction rather than
/// rounded to 55: the clock counts the steps due since it started, so no
/// remainder is lost and an hour of play is 64 800 steps. (55 ms made every
/// timer in steps 1 % short of v1.2's seconds.)
pub const STEPS_PER_SECOND: u32 = crate::mind::SECOND;

// **No cap on catching up.** v1.2 hands the world the whole measured time
// of a frame, unclamped (`Burntime.MonoGame/GameThread.cs:44-62`): a
// machine that stalls for a second comes back to a world a second on. The
// port runs the steps that second holds. A cap of five steps a frame stood
// here, with no measurement of a frame that slow on AROS behind it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// One step for every frame, whatever the clock says.
    Scripted,
    /// As many steps as the clock has let pass.
    Played,
}

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pace: Pace,
    /// When the clock started, in the caller's milliseconds.
    origin: u32,
    /// Steps already run since then.
    done: u64,
    started: bool,
}

impl Clock {
    pub fn new(pace: Pace) -> Clock {
        Clock { pace, origin: 0, done: 0, started: false }
    }

    /// Milliseconds since the clock started, times the steps in a second:
    /// the time in thousandths of a step.
    fn permille_steps(&self, now_ms: u32) -> u64 {
        now_ms.wrapping_sub(self.origin) as u64 * STEPS_PER_SECOND as u64
    }

    /// How many steps this frame should run, given the clock now.
    pub fn due(&mut self, now_ms: u32) -> u32 {
        match self.pace {
            Pace::Scripted => 1,
            Pace::Played => {
                if !self.started {
                    self.started = true;
                    self.origin = now_ms;
                    return 1;
                }
                let total = self.permille_steps(now_ms) / 1000;
                let steps = total.saturating_sub(self.done);
                self.done = self.done.max(total);
                steps.min(u32::MAX as u64) as u32
            }
        }
    }

    /// How far into the current step the clock is, 0 to 1000 — for drawing
    /// between two world states. Always 1000 when scripted: a script's
    /// pictures are of the world as it is.
    pub fn phase_permille(&self, now_ms: u32) -> u32 {
        match self.pace {
            Pace::Scripted => 1000,
            Pace::Played => self.permille_steps(now_ms).saturating_sub(self.done * 1000).min(1000) as u32,
        }
    }

    /// How long the caller may sleep before a step is due, so that a fast
    /// machine does not spin drawing the same picture.
    pub fn idle_ms(&self, now_ms: u32) -> u32 {
        match self.pace {
            Pace::Scripted => 0,
            Pace::Played => {
                // The next step falls due at ⌈(done + 1) · 1000 / 18⌉ ms.
                let s = STEPS_PER_SECOND as u64;
                let next = ((self.done + 1) * 1000).div_ceil(s);
                next.saturating_sub(now_ms.wrapping_sub(self.origin) as u64) as u32
            }
        }
    }
}
