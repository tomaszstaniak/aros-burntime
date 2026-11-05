//! Whose turn it is: v0.9.1's `GameServer` loop, `GameClient.Finish` and
//! `WaitScene`, as state.
//!
//! Each day the server queues, in player order, every player who is not
//! dead, has not finished, and is not travelling. It activates them one at a
//! time and waits for each to finish; a machine player's turn resolves at
//! once. When the queue is empty the day ends (`World.Turn`), travellers move
//! a day on, and every `IsFinished` is cleared. `WaitScene` hands control to
//! whichever human the server has made ready.
//!
//! So a human on the road is never asked: their days pass without them, and
//! a day in which no human can act ends on its own. That is the whole reason
//! a journey needs no clicks.

use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Human,
    Machine,
}

/// What the scheduler wants next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// A human's turn: `WaitScene` sets `ActivePlayer` and opens their scene.
    Human(usize),
    /// A machine's turn, which the caller resolves and then finishes.
    Machine(usize),
    /// Nobody is left to act today: run `World.Turn`, then `begin_day`.
    DayEnds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turns {
    pub seats: Vec<Seat>,
    finished: Vec<bool>,
    queue: Vec<usize>,
    /// The player acting now, if any.
    pub active: Option<usize>,
}

impl Turns {
    pub fn new(seats: Vec<Seat>) -> Turns {
        let n = seats.len();
        Turns { seats, finished: alloc::vec![false; n], queue: Vec::new(), active: None }
    }

    /// `PlayerTurnLogic.Process`: queue, in order, everybody alive, not
    /// finished and not on the road. `out(i)` is `(dead, travelling)`.
    pub fn begin_day(&mut self, out: impl Fn(usize) -> (bool, bool)) {
        self.queue = (0..self.seats.len())
            .filter(|&i| {
                let (dead, travelling) = out(i);
                !dead && !travelling && !self.finished[i]
            })
            .collect();
        self.queue.reverse();
        self.active = None;
    }

    /// Hand over to the next in the queue.
    pub fn next(&mut self) -> Next {
        match self.queue.pop() {
            Some(i) => {
                self.active = Some(i);
                match self.seats[i] {
                    Seat::Human => Next::Human(i),
                    Seat::Machine => Next::Machine(i),
                }
            }
            None => {
                self.active = None;
                Next::DayEnds
            }
        }
    }

    /// `GameClient.Finish`: the active player is done for today.
    pub fn finish(&mut self) {
        if let Some(i) = self.active.take() {
            self.finished[i] = true;
        }
    }

    /// After `World.Turn`: `IsFinished = false` for everybody.
    pub fn day_ended(&mut self) {
        self.finished.iter_mut().for_each(|f| *f = false);
    }

    pub fn is_finished(&self, i: usize) -> bool {
        self.finished.get(i).copied().unwrap_or(false)
    }

    /// Who has finished today, in seat order — what a save writes down.
    pub fn finished_seats(&self) -> Vec<usize> {
        (0..self.finished.len()).filter(|&i| self.finished[i]).collect()
    }

    /// A save's `finished` put back, before `begin_day` queues the rest.
    pub fn restore_finished(&mut self, seats: &[usize]) {
        for &i in seats {
            if let Some(f) = self.finished.get_mut(i) {
                *f = true;
            }
        }
    }
}

/// **v1.2's end of a game every human has left** (`WaitScene.OnUpdate`):
/// `gameOver` holds when every one of `app.Clients` is game-over or dead,
/// and `app.Clients` holds **the humans only** — a human seat is added with
/// `GameServer.AddClient` and `app.Clients.Add`, a machine seat with
/// `Server.AddAI` alone (`GameCreation.cs:211-254`, and `LoadGame`). So the
/// machine seats being alive does not keep the game going: once the news is
/// shown, `WaitScene` stops the server and goes back to `MenuScene`.
///
/// `gone` is, for each human, whether the server has given his
/// `DeathNews` (`GameClient.Die`, after `World.Turn`).
pub fn every_human_gone(gone: impl IntoIterator<Item = bool>) -> bool {
    gone.into_iter().all(|g| g)
}
