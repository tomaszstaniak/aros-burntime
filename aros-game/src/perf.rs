//! `perf <file>`: what a played game actually does each second, written to a
//! file rather than guessed at from how it feels.
//!
//! Per second: frames, the median and the longest frame, how long drawing and
//! presenting took, how many world steps ran, and how far the player's
//! character and the map view moved, in game pixels. Movement is summed frame
//! by frame, so a walk that stops and starts is counted as walked.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

#[derive(Default)]
struct Second {
    /// Milliseconds between one present and the next.
    frames: Vec<u32>,
    /// Milliseconds between presents that followed at least one world step —
    /// the pictures that differ. A present with no step in front of it shows
    /// the world as it was.
    updates: Vec<u32>,
    draw_ms: u32,
    present_ms: u32,
    steps: u32,
    /// Hundredths of a pixel.
    walked: u64,
    scrolled: u64,
    /// Frames in which the character's drawn body was the standing frame
    /// while it had moved in the last half second — the flicker to look for.
    idle_while_walking: u32,
    walking_frames: u32,
    /// Milliseconds between presents in which what is on screen moved — the
    /// drawn character or the camera — and the largest single move, px.
    visible: Vec<u32>,
    max_jump: i32,
    /// Microseconds per stage, summed; and the slowest frame's stages.
    stages: [u64; STAGES],
    slowest: (u64, [u64; STAGES], u32),
    /// Frames by how many world steps they ran: 0, 1, 2, 3+.
    by_steps: [u32; 4],
}

/// input+events, world steps, canvas, compose, present, sleep.
pub const STAGES: usize = 6;
pub const STAGE_NAMES: [&str; STAGES] = ["input", "world", "canvas", "compose", "present", "sleep"];

/// One frame's stage times, in microseconds.
pub type Stages = [u64; STAGES];

pub struct Perf {
    pub text: String,
    second: Second,
    /// When the last present that followed a step happened.
    last_update: Option<u32>,
    /// What was on screen last: the drawn character and the camera.
    last_drawn: Option<((i32, i32), (i32, i32))>,
    last_visible: Option<u32>,
    /// The stages of the frame being finished, set before `frame`.
    pub pending_stages: Stages,
    /// Stages thrown away as clock jumps, over the whole run.
    pub clock_jumps: u32,
    since: u32,
    written: u32,
}

fn hypot100(dx: i32, dy: i32) -> u64 {
    // sqrt((dx² + dy²) · 10⁴), by Newton, in integers.
    let n = ((dx as i64 * dx as i64 + dy as i64 * dy as i64) * 10_000) as u64;
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

impl Perf {
    pub fn new(header: &str, now: u32) -> Perf {
        let mut text = String::from(header);
        text.push_str("\n# presents: SDL presents this second; present_p50/max: ms between them\n");
        text.push_str("# updates: presents after >= 1 world step; update_p50/max: ms between those\n");
        text.push_str("# draw_ms / present_ms: time spent drawing / in SDL present, summed over the second\n");
        text.push_str("# visible: presents in which the drawn character or the camera moved; vis_p50/max: ms between them; jump: largest move in one present, px\n");
        text.push_str("# stage columns: microseconds a second spent in each, then the slowest frame's total and its stages\n");
        text.push_str("# steps0..3+: frames that ran 0, 1, 2, 3 or more world steps\n");
        text.push_str("second presents present_p50 present_max updates update_p50 update_max visible vis_p50 vis_max jump draw_ms present_ms steps walked_px scrolled_px walking_frames idle_while_walking input_us world_us canvas_us compose_us present_us sleep_us slowest_us slowest_stages slowest_steps steps0 steps1 steps2 steps3+\n");
        Perf { text, second: Second::default(), last_update: None, last_drawn: None, last_visible: None, pending_stages: [0; STAGES], clock_jumps: 0, since: now, written: 0 }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, now: u32, frame_ms: u32, draw_ms: u32, present_ms: u32, steps: u32,
                 walked: (i32, i32), scrolled: (i32, i32), walking: bool, drawn_idle: bool,
                 drawn: (i32, i32), camera: (i32, i32)) {
        if let Some((d, c)) = self.last_drawn {
            let jump = (drawn.0 - d.0).abs().max((drawn.1 - d.1).abs())
                .max((camera.0 - c.0).abs()).max((camera.1 - c.1).abs());
            if jump > 0 {
                if let Some(t) = self.last_visible {
                    self.second.visible.push(now.wrapping_sub(t));
                }
                self.last_visible = Some(now);
                self.second.max_jump = self.second.max_jump.max(jump);
            }
        }
        self.last_drawn = Some((drawn, camera));
        // `gettimeofday` on AROS is wall time and may step: a stage that
        // came out negative (wrapped) or over ten seconds is a clock jump, not
        // a cost, and is counted as a jump instead.
        let mut st = self.pending_stages;
        for v in st.iter_mut() {
            if *v > 10_000_000 {
                *v = 0;
                self.clock_jumps += 1;
            }
        }
        let total: u64 = st.iter().sum();
        for i in 0..STAGES {
            self.second.stages[i] += st[i];
        }
        if total > self.second.slowest.0 {
            self.second.slowest = (total, st, steps);
        }
        self.second.by_steps[(steps as usize).min(3)] += 1;
        let s = &mut self.second;
        s.frames.push(frame_ms);
        if steps > 0 {
            if let Some(last) = self.last_update {
                s.updates.push(now.wrapping_sub(last));
            }
            self.last_update = Some(now);
        }
        s.draw_ms += draw_ms;
        s.present_ms += present_ms;
        s.steps += steps;
        s.walked += hypot100(walked.0, walked.1);
        s.scrolled += hypot100(scrolled.0, scrolled.1);
        if walking {
            s.walking_frames += 1;
            if drawn_idle {
                s.idle_while_walking += 1;
            }
        }
        if now.wrapping_sub(self.since) >= 1000 {
            let mut f = core::mem::take(&mut s.frames);
            f.sort_unstable();
            let median = f.get(f.len() / 2).copied().unwrap_or(0);
            let max = f.last().copied().unwrap_or(0);
            let mut u = core::mem::take(&mut s.updates);
            u.sort_unstable();
            let u_median = u.get(u.len() / 2).copied().unwrap_or(0);
            let u_max = u.last().copied().unwrap_or(0);
            let mut v = core::mem::take(&mut s.visible);
            v.sort_unstable();
            let _ = writeln!(
                self.text,
                "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}.{:02} {}.{:02} {} {}",
                self.written,
                f.len(),
                median,
                max,
                u.len(),
                u_median,
                u_max,
                v.len(),
                v.get(v.len() / 2).copied().unwrap_or(0),
                v.last().copied().unwrap_or(0),
                s.max_jump,
                s.draw_ms,
                s.present_ms,
                s.steps,
                s.walked / 100,
                s.walked % 100,
                s.scrolled / 100,
                s.scrolled % 100,
                s.walking_frames,
                s.idle_while_walking
            );
            // The same line, carried on: stages and the slowest frame.
            if self.text.ends_with('\n') {
                self.text.pop();
            }
            let (slow, sst, ssteps) = s.slowest;
            let _ = write!(self.text, " {} {} {} {} {} {} {} {}/{}/{}/{}/{}/{} {} {} {} {} {}\n",
                s.stages[0], s.stages[1], s.stages[2], s.stages[3], s.stages[4], s.stages[5],
                slow, sst[0], sst[1], sst[2], sst[3], sst[4], sst[5], ssteps,
                s.by_steps[0], s.by_steps[1], s.by_steps[2], s.by_steps[3]);
            *s = Second::default();
            self.since = now;
            self.written += 1;
        }
    }

    /// Whether a second's worth has been added since `last`.
    pub fn seconds(&self) -> u32 {
        self.written
    }
}
