//! `AiTelemetry.Report`: a line of what the opponent did, in upstream's
//! words and order. Upstream writes nowhere unless a sink is installed, and
//! only `HeadlessSimulation` installs one (`Turn N: <player> <message>.`);
//! the game itself never shows them. Here the sink is [`Telemetry`] on the
//! opponent's state: off unless the game turns it on, never saved, ignored
//! by equality, and shared by every copy of the state, so that a line
//! reported on a working copy keeps its place in the order.
//!
//! Places and people are written as tokens ([`place`], [`person`]) and given
//! their names by [`resolve`] when printed; [`Tokens`] lets a structured
//! [`super::reason::Reason`] be quoted inside a line the same way.

use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use super::reason::Names;

/// Upstream's `AiTelemetry.Sink`, per opponent.
#[derive(Clone, Default)]
pub struct Telemetry(Option<Rc<Sink>>);

struct Sink {
    lines: RefCell<Vec<String>>,
    /// Milliseconds, for the timings some reports carry (`Stopwatch`).
    clock: Option<fn() -> u64>,
    /// `Trading.LastReportedTradeFailure`: the trader whose failure was
    /// last reported, so that it is not reported again. Upstream keeps it
    /// only while a sink listens, and so does this.
    trade_failure: RefCell<Option<usize>>,
}

impl Telemetry {
    /// A sink that keeps what is reported.
    pub fn on() -> Telemetry {
        Telemetry::with_clock(None)
    }

    /// The same, with a clock for the timed reports.
    pub fn with_clock(clock: Option<fn() -> u64>) -> Telemetry {
        Telemetry(Some(Rc::new(Sink { lines: RefCell::new(Vec::new()), clock, trade_failure: RefCell::new(None) })))
    }

    /// Now, in milliseconds, when listening with a clock.
    pub fn now(&self) -> Option<u64> {
        self.0.as_ref().and_then(|s| s.clock).map(|c| c())
    }

    /// `LastReportedTradeFailure`: whether `trader`'s failure is new (and is
    /// now the one remembered).
    pub fn new_trade_failure(&self, trader: usize) -> bool {
        match &self.0 {
            Some(s) => s.trade_failure.borrow_mut().replace(trader) != Some(trader),
            None => false,
        }
    }

    /// `LastReportedTradeFailure.Remove`.
    pub fn trade_succeeded(&self) {
        if let Some(s) = &self.0 {
            *s.trade_failure.borrow_mut() = None;
        }
    }

    pub fn is_on(&self) -> bool {
        self.0.is_some()
    }

    /// `AiTelemetry.Report(player, message)`: the message is only built when
    /// somebody listens.
    pub fn report(&self, message: impl FnOnce() -> String) {
        if let Some(s) = &self.0 {
            s.lines.borrow_mut().push(message());
        }
    }

    /// A line upstream reports under another player (`AiTelemetry.Report(
    /// defenderOwner, …)`): kept here, in order, and printed under `player`.
    pub fn report_as(&self, player: usize, message: impl FnOnce() -> String) {
        if let Some(s) = &self.0 {
            s.lines.borrow_mut().push(alloc::format!("{MARK}@{player}{MARK}{}", message()));
        }
    }

    /// What has been reported, in order; the sink is left empty.
    pub fn take(&self) -> Vec<String> {
        self.0.as_ref().map_or(Vec::new(), |s| core::mem::take(&mut *s.lines.borrow_mut()))
    }
}

/// The sink is not part of the opponent's state.
impl PartialEq for Telemetry {
    fn eq(&self, _: &Telemetry) -> bool {
        true
    }
}

impl core::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str(if self.is_on() { "Telemetry(on)" } else { "Telemetry(off)" })
    }
}

const MARK: char = '\u{1}';

/// `string.Join(", ", items.GroupBy(item => item.ID).Select(group =>
/// group.Count() > 1 ? $"{group.Key} x{group.Count()}" : group.Key))`:
/// in order of first appearance.
pub fn grouped_ids(types: &crate::items::ItemTypes, kinds: &[usize]) -> String {
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for &k in kinds {
        match groups.iter_mut().find(|g| g.0 == k) {
            Some(g) => g.1 += 1,
            None => groups.push((k, 1)),
        }
    }
    let parts: Vec<String> = groups
        .iter()
        .map(|&(k, n)| {
            let id = types.name_of(k).unwrap_or("");
            if n > 1 { alloc::format!("{id} x{n}") } else { String::from(id) }
        })
        .collect();
    parts.join(", ")
}

/// `Location.Title`, resolved when printed.
pub fn place(location: usize) -> String {
    alloc::format!("{MARK}p{location}{MARK}")
}

/// `Character.Name` of a `gam.dat` record, resolved when printed. A player's
/// own character is record `player`.
pub fn person(record: usize) -> String {
    alloc::format!("{MARK}c{record}{MARK}")
}

/// `Player.Name`, resolved when printed.
pub fn player(seat: usize) -> String {
    alloc::format!("{MARK}h{seat}{MARK}")
}

/// Names written as tokens, for quoting a [`super::reason::Reason`].
pub struct Tokens;

impl Names for Tokens {
    fn place(&self, location: usize) -> String {
        place(location)
    }
    fn person(&self, record: usize) -> String {
        person(record)
    }
}

/// Whom a line is reported under, if not the opponent who made it, and the
/// line without that mark.
pub fn attribution(line: &str) -> (Option<usize>, &str) {
    let Some(rest) = line.strip_prefix(MARK).and_then(|r| r.strip_prefix('@')) else { return (None, line) };
    match rest.split_once(MARK) {
        Some((p, tail)) => (p.parse().ok(), tail),
        None => (None, line),
    }
}

/// The line with its tokens replaced by names.
pub fn resolve(line: &str, names: &dyn Names) -> String {
    let mut out = String::new();
    let mut parts = line.split(MARK);
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    let mut token = true;
    for part in parts {
        if token {
            let n = part.get(1..).and_then(|s| s.parse::<usize>().ok());
            match (part.as_bytes().first(), n) {
                (Some(b'p'), Some(l)) => out.push_str(&names.place(l)),
                (Some(b'c'), Some(r)) => out.push_str(&names.person(r)),
                (Some(b'h'), Some(s)) => out.push_str(&names.player(s)),
                _ => out.push_str(part),
            }
        } else {
            out.push_str(part);
        }
        token = !token;
    }
    out
}
