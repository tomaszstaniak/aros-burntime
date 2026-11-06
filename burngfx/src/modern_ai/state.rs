//! The Modern opponent's state between turns (`ClassicAiState`'s fields).
//!
//! **A save keeps what v1.2's keeps, and a load does what v1.2's does**
//! At tag `v1.2` only the settings
//! (`AiState.cs:38`), the slump grants used (`:73-74`), the target
//! (`headedLocation`, `:37`), the reserve (`itemPool`, `:42`) and the
//! leader's weapon and protection (`Character.cs:40-41`) survive a save.
//! Everything else is `[NonSerialized]` (`AiState.cs:43-122,811-812`) or
//! lives in a static `ConditionalWeakTable` (`AttackRetryMemory`,
//! `DefenseIntelligence`, `TerritorialTargetDeferrals`, `EconomicSupport`,
//! `Recruitment.SurvivalReleaseByPlayer`) and is empty after a load:
//! [`ModernAi::forget_unserialized`]. `InitAfterLoad` (`AiState.cs:171-178`)
//! then recomputes the settlement intent and gives a hostile target a fresh
//! deadline: [`ModernAi::after_load`]. A resumed game therefore decides as
//! v1.2's resumed game does — "loading a game gives the plan a fresh
//! chance" (`AiState.cs:79-81`) — not as the uninterrupted one. The per-turn
//! context (`AiTurnContext`: needs, camp assessments, the best trade city)
//! is not state: it is computed again from the world each turn.
//!
//! What the opponent *knows* is kept as it learnt it while the game runs:
//! [`Observation`] is what it saw of a camp on the day it saw it.
//!
//! **A save never falls inside a Modern turn.** The turn runs from start to
//! end within one call (`controller`), so there is no half-done action to
//! keep. It draws from the process's one generator, which no save carries.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;


/// `AiItemPool.MaximumItemsPerType`.
pub const RESERVE_CAP: u8 = 5;

/// What a group or a camp's defence amounted to (`AttackRetryMemory.Force`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Force {
    pub count: i32,
    pub health: i32,
    pub strength: f32,
    pub boss_health: i32,
}

/// `AttackRetryMemory`'s record for one target.
#[derive(Clone, Debug, PartialEq)]
pub struct Retry {
    pub target: usize,
    pub owner: Option<usize>,
    pub attacker: Force,
    pub defender: Force,
    pub blocked: bool,
}

/// `DefenseIntelligence`'s observation of one camp, as it was seen.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub camp: usize,
    pub owner: Option<usize>,
    pub owner_since: i32,
    pub observed_day: i32,
    pub defenders: i32,
    pub strength: f32,
}

/// What stopped a target (`TerritorialTargetDeferrals.Blocker`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    None,
    RouteSupplies,
    Attackers,
    CombatReadiness,
    SettlementSupplies,
}

impl Blocker {
    fn word(self) -> &'static str {
        match self {
            Blocker::None => "none",
            Blocker::RouteSupplies => "route_supplies",
            Blocker::Attackers => "attackers",
            Blocker::CombatReadiness => "combat_readiness",
            Blocker::SettlementSupplies => "settlement_supplies",
        }
    }
    fn parse(w: &str) -> Option<Blocker> {
        [Blocker::None, Blocker::RouteSupplies, Blocker::Attackers, Blocker::CombatReadiness, Blocker::SettlementSupplies].into_iter().find(|b| b.word() == w)
    }
}

/// `TerritorialTargetDeferrals.Entry`: a target put aside — whose it was,
/// what stopped it, the day to try again, how far along the blocker was,
/// and the order it was put aside in.
#[derive(Clone, Debug, PartialEq)]
pub struct Deferral {
    pub target: usize,
    pub owner: Option<usize>,
    pub blocker: Blocker,
    pub retry_day: i32,
    pub progress: i32,
    pub sequence: u64,
}

/// A journey of several days the opponent committed to
/// (`committedJourneyDestination`, `…Reason`).
#[derive(Clone, Debug, PartialEq)]
pub struct Commitment {
    pub destination: usize,
    pub reason: String,
}

/// `failedAttack*`.
#[derive(Clone, Debug, PartialEq)]
pub struct FailedAttack {
    pub camp: usize,
    pub until_day: i32,
    pub group_size: i32,
    pub attacker_strength: f32,
    pub defender_strength: f32,
}

/// `attackRecruitmentDeferred*` (`AiState.cs:60-65`). The city attack
/// recruits are sought in is [`ModernAi::attack_recruitment_city`].
#[derive(Clone, Debug, PartialEq)]
pub struct AttackRecruitment {
    pub group_size: i32,
    pub until_day: i32,
}

/// The watchdog's counters (`consecutive*Waits`, `noProgressLocation`,
/// `stalledWait*`, `previousExploratoryCamp`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stall {
    pub stalled_target: Option<usize>,
    pub stalled_reason: String,
    pub stalled_waits: i32,
    pub no_progress_waits: i32,
    pub no_progress_location: Option<usize>,
    pub strategic_no_progress_waits: i32,
    pub previous_exploratory_camp: Option<usize>,
}

/// `EconomicSupport.ProgressState` (a weak table upstream; saved here).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Support {
    /// `Initialized`.
    pub initialized: bool,
    /// `LastAdvancedTrapDay`.
    pub last_trap_day: i32,
    /// `AdvancedTrapCount`.
    pub trap_count: i32,
    /// `SavingForSnakeTrap`.
    pub saving_for_snake_trap: bool,
    /// `StrategicProgressInitialized`, `LastStrategicLocation`,
    /// `LastCampCount`, `LastStrategicProgressDay`.
    pub strategic: Option<(usize, i32, i32)>,
    /// `TraderExposureDay`: `(trader, day)`, in the order first seen.
    pub traders_seen: Vec<(usize, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModernAi {
    pub difficulty: u8,
    /// `AiSettings.MaxAdvance` (`camp_max_advance`).
    pub max_advance: i32,
    /// `StrategicTarget`: settle a neutral camp or take a hostile one.
    pub target: Option<usize>,
    /// Whether the target had no owner when it was chosen.
    pub target_was_neutral: bool,
    /// The reserve, `(item kind, count)`, in the order kinds first came in.
    pub reserve: Vec<(usize, u8)>,
    pub slump_grants_used: i32,
    pub journey: Option<Commitment>,
    /// The last day of the attack plan on `target`.
    pub attack_plan_until: Option<i32>,
    pub attack_recruitment: Option<AttackRecruitment>,
    /// `(player, until day)`.
    pub retaliating: Option<(usize, i32)>,
    /// `(camp, until day)`.
    pub contested: Option<(usize, i32)>,
    pub post_capture_pause: i32,
    pub failed_attack: Option<FailedAttack>,
    pub last_chance_target: Option<usize>,
    pub retries: Vec<Retry>,
    pub knowledge: Vec<Observation>,
    pub deferrals: Vec<Deferral>,
    /// `Memory.NextSequence`.
    pub next_deferral: u64,
    pub stall: Stall,
    /// Followers sent home from a group too big (`demobilizedFollowers`).
    pub demobilized: Vec<usize>,
    pub support: Support,
    /// `AttackRecruitmentCity`: where attack recruits are sought, kept across
    /// stops and checked again at each decision.
    pub attack_recruitment_city: Option<usize>,
    /// Places where a follower was let go to survive
    /// (`Recruitment.SurvivalReleaseByPlayer`): nobody is hired there again.
    pub survival_releases: Vec<usize>,
    /// The leader's chosen weapon and protection (`Character.Weapon`,
    /// `.Protection`), as slots of his pack. A companion keeps his own.
    pub leader_kit: crate::combat::Kit,
    /// `recruitmentFailureTheaterAnchor`: the attack target given up for want
    /// of recruits in a city, consumed by the next choice.
    pub recruitment_failure_anchor: Option<usize>,
    /// `AiTelemetry`: off unless the game asks; never saved.
    pub telemetry: super::telemetry::Telemetry,
}

impl ModernAi {
    pub fn new(difficulty: u8, max_advance: i32) -> ModernAi {
        ModernAi {
            difficulty,
            max_advance,
            target: None,
            target_was_neutral: false,
            reserve: Vec::new(),
            slump_grants_used: 0,
            journey: None,
            attack_plan_until: None,
            attack_recruitment: None,
            retaliating: None,
            contested: None,
            post_capture_pause: 0,
            failed_attack: None,
            last_chance_target: None,
            recruitment_failure_anchor: None,
            telemetry: super::telemetry::Telemetry::default(),
            retries: Vec::new(),
            knowledge: Vec::new(),
            deferrals: Vec::new(),
            next_deferral: 0,
            stall: Stall::default(),
            demobilized: Vec::new(),
            support: Support::default(),
            leader_kit: crate::combat::Kit::default(),
            attack_recruitment_city: None,
            survival_releases: Vec::new(),
        }
    }

    pub fn reserve_count(&self, kind: usize) -> u8 {
        self.reserve.iter().find(|(k, _)| *k == kind).map_or(0, |(_, n)| *n)
    }

    /// `AiItemPool.InsertUnchecked`: false when the kind is already at the
    /// cap. A kind new to the reserve goes at the end — the reserve keeps the
    /// order kinds first came in, which is the order every "first of equals"
    /// over its contents follows.
    pub fn reserve_add(&mut self, kind: usize) -> bool {
        match self.reserve.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) if *n >= RESERVE_CAP => false,
            Some((_, n)) => {
                *n += 1;
                true
            }
            None => {
                self.reserve.push((kind, 1));
                true
            }
        }
    }

    /// `Take`: one fewer. An emptied kind keeps its place, as a `PoolItem`
    /// of count nought does.
    pub fn reserve_take(&mut self, kind: usize) -> bool {
        match self.reserve.iter_mut().find(|(k, n)| *k == kind && *n > 0) {
            Some((_, n)) => {
                *n -= 1;
                true
            }
            None => false,
        }
    }

    /// Whether the attack plan still runs on `day` (`IsAttackPlanExpired`'s
    /// opposite).
    pub fn attack_plan_running(&self, day: i32) -> bool {
        self.attack_plan_until.is_some_and(|u| day <= u)
    }

    pub fn retaliating_against(&self, player: usize, day: i32) -> bool {
        self.retaliating.is_some_and(|(p, u)| p == player && day <= u)
    }

    pub fn was_recently_contested(&self, camp: usize, day: i32) -> bool {
        self.contested.is_some_and(|(c, u)| c == camp && day <= u)
    }


    /// Memories whose time is over are dropped, once a turn. A memory with
    /// a day still to come is kept whatever the world now says.
    pub fn forget_expired(&mut self, day: i32) {
        if self.retaliating.is_some_and(|(_, u)| day > u) {
            self.retaliating = None;
        }
        if self.contested.is_some_and(|(_, u)| day > u) {
            self.contested = None;
        }
        if self.failed_attack.as_ref().is_some_and(|f| day > f.until_day) {
            self.failed_attack = None;
        }
        if self.attack_recruitment.as_ref().is_some_and(|a| day > a.until_day) {
            self.attack_recruitment = None;
        }
    }

    /// What `BinaryFormatter` leaves of a `ClassicAiState`: the serialized
    /// fields as they were, every other one at its CLR default, the weak
    /// tables empty. Kept: `settings` (difficulty, max advance;
    /// `AiState.cs:38`), `slumpMaterialGrantsUsed` (`:73-74`),
    /// `headedLocation` (`:37`), `itemPool` (`:42`) and the leader's
    /// `weapon`/`protection` (`Character.cs:40-41`). Reset, each
    /// `[NonSerialized]` unless named otherwise: `strategicTargetWasNeutral`
    /// (`:77-78`), `attackPlanUntilDay` (`:55-56`), `postCapturePauseTurns`
    /// (`:52-53`), `committedJourney*` (`:103-106`), `attackRecruitment*`
    /// (`:60-65`), `retaliat*` (`:43-48`), `recentlyContested*` (`:45-50`),
    /// `failedAttack*` (`:107-116`), `lastChanceAttackTarget` (`:117-118`),
    /// `recruitmentFailureTheaterAnchor` (`:69-70`), the watchdog
    /// (`:82-100`), `demobilizedFollowers` (`:121-122`),
    /// `AttackRecruitmentCity` (`:811-812`); the static weak tables
    /// `AttackRetryMemory` (`AttackRetryMemory.cs:12-13`),
    /// `DefenseIntelligence` (`DefenseIntelligence.cs:17`),
    /// `TerritorialTargetDeferrals` (`TerritorialTargetDeferrals.cs:22`),
    /// `EconomicSupport.ProgressState` (`EconomicSupport.cs:23`) and
    /// `Recruitment.SurvivalReleaseByPlayer` (`Recruitment.cs:17-23`).
    pub fn forget_unserialized(&mut self) {
        let mut fresh = ModernAi::new(self.difficulty, self.max_advance);
        fresh.target = self.target;
        fresh.reserve = core::mem::take(&mut self.reserve);
        fresh.slump_grants_used = self.slump_grants_used;
        fresh.leader_kit = self.leader_kit;
        // Not state: the sink stays with the game.
        fresh.telemetry = core::mem::take(&mut self.telemetry);
        *self = fresh;
    }

    /// **v1.2's load step for this seat**: [`ModernAi::forget_unserialized`],
    /// then `ClassicAiState.InitAfterLoad` (`AiState.cs:171-178`):
    ///
    /// * `strategicTargetWasNeutral = headedLocation != null &&
    ///   headedLocation.Player == null` — read from the world now, a city
    ///   included;
    /// * `HasAttackPlan` (`:334-336`: a target that is no city, owned, and
    ///   not by this player) gives the plan `Day + AttackPlanTurns`
    ///   (`attack_plan_turns`, 40 in every section of `ai.txt`); otherwise
    ///   the deadline stays at its default, none.
    ///
    /// `site(l)` is `(IsCity, Player)` of location `l` as the loaded world
    /// has it. The third call, `NormalizeLoadedGarrisonBelongings`, is the
    /// world's: [`super::seat::normalize_loaded_garrison_belongings`].
    pub fn after_load(&mut self, day: i32, me: usize, site: impl Fn(usize) -> (bool, Option<usize>), attack_plan_turns: i32) {
        self.forget_unserialized();
        let Some(target) = self.target else { return };
        let (is_city, owner) = site(target);
        self.target_was_neutral = owner.is_none();
        if !is_city && owner.is_some_and(|o| o != me) {
            self.attack_plan_until = Some(day + attack_plan_turns);
        }
    }
}

// --- the save's lines --------------------------------------------------
//
// `modern PLAYER WHAT …`, one record a line. A file from before version 25
// also holds the records v1.2 does not save; they are read, so that such a
// file is checked as before, and forgotten at load. Their floats were
// written as their bits.

fn opt(v: Option<usize>) -> String {
    v.map_or(String::from("none"), |v| alloc::format!("{v}"))
}

/// A reason as files before version 25 wrote it, read back: `-` for none;
/// as it is when it is one word; else `%` and the text with every blank and
/// `%` written `%XX` by its bytes. Only an older file still has one.
fn unreason(word: &str) -> Option<String> {
    let Some(rest) = word.strip_prefix('%') else {
        return Some(if word == "-" { String::new() } else { String::from(word) });
    };
    let mut bytes = Vec::new();
    let mut it = rest.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let hi = (it.next()? as char).to_digit(16)?;
            let lo = (it.next()? as char).to_digit(16)?;
            bytes.push((hi * 16 + lo) as u8);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

impl ModernAi {
    /// The records v1.2 serializes (see [`ModernAi::forget_unserialized`]):
    /// `settings DIFFICULTY MAX_ADVANCE GRANTS`, `target T`, the reserve
    /// and the leader's kit. Nothing else of the state goes into a save
    /// (save version 25); the settlement intent and the attack deadline are
    /// worked out again at load ([`ModernAi::after_load`]).
    pub fn write(&self, player: usize, out: &mut String) {
        let p = player;
        let _ = writeln!(out, "modern {p} settings {} {} {}", self.difficulty, self.max_advance, self.slump_grants_used);
        if let Some(t) = self.target {
            let _ = writeln!(out, "modern {p} target {t}");
        }
        for (k, n) in &self.reserve {
            let _ = writeln!(out, "modern {p} reserve {k} {n}");
        }
        if self.leader_kit != crate::combat::Kit::default() {
            let _ = writeln!(out, "modern {p} kit {} {}", opt(self.leader_kit.weapon), opt(self.leader_kit.protection));
        }
    }

    /// One `modern` line after `modern PLAYER`: `settings` makes the state,
    /// every other record adds to the one already made.
    pub fn read(state: &mut Option<ModernAi>, what: &str, w: &mut core::str::SplitWhitespace) -> Result<(), &'static str> {
        fn n<T: core::str::FromStr>(w: &mut core::str::SplitWhitespace) -> Result<T, &'static str> {
            w.next().and_then(|v| v.parse().ok()).ok_or("a number")
        }
        fn o(w: &mut core::str::SplitWhitespace) -> Result<Option<usize>, &'static str> {
            match w.next() {
                Some("none") => Ok(None),
                Some(v) => v.parse().map(Some).map_err(|_| "a location or none"),
                None => Err("a location or none"),
            }
        }
        fn f(w: &mut core::str::SplitWhitespace) -> Result<f32, &'static str> {
            w.next().and_then(|v| u32::from_str_radix(v, 16).ok()).map(f32::from_bits).ok_or("a float's bits")
        }
        fn r(w: &mut core::str::SplitWhitespace) -> Result<String, &'static str> {
            w.next().and_then(unreason).ok_or("a reason")
        }
        fn force(w: &mut core::str::SplitWhitespace) -> Result<Force, &'static str> {
            Ok(Force { count: n(w)?, health: n(w)?, strength: f(w)?, boss_health: n(w)? })
        }
        if what == "settings" {
            if state.is_some() {
                return Err("a second settings line");
            }
            let difficulty: u8 = n(w)?;
            if difficulty > 2 {
                return Err("difficulty");
            }
            let mut ai = ModernAi::new(difficulty, n(w)?);
            ai.slump_grants_used = n(w)?;
            // Before version 25 the record ended with the post-capture pause,
            // which v1.2 does not save: read, and forgotten at load.
            let mut rest = w.clone();
            if rest.next().is_some() {
                ai.post_capture_pause = n(w)?;
            }
            *state = Some(ai);
            return if w.next().is_some() { Err("more than the record holds") } else { Ok(()) };
        }
        let ai = state.as_mut().ok_or("a record before its settings")?;
        match what {
            "target" => {
                ai.target = Some(n(w)?);
                // Before version 25 the intent followed; v1.2 works it out
                // again at load (`AiState.cs:173-174`), and so does the port.
                ai.target_was_neutral = match w.next() {
                    None => false,
                    Some("neutral") => true,
                    Some("hostile") => false,
                    _ => return Err("neutral or hostile"),
                };
            }
            "reserve" => {
                let kind: usize = n(w)?;
                let count: u8 = n(w)?;
                if count > RESERVE_CAP || ai.reserve.iter().any(|(k, _)| *k == kind) {
                    return Err("reserve");
                }
                ai.reserve.push((kind, count));
            }
            "journey" => ai.journey = Some(Commitment { destination: n(w)?, reason: r(w)? }),
            "attack_plan" => ai.attack_plan_until = Some(n(w)?),
            "attack_recruitment" => {
                ai.attack_recruitment = Some(AttackRecruitment { group_size: n(w)?, until_day: n(w)? });
                // A city nobody ever read (LOAD-7), in files before version 25.
                o(w)?;
            }
            "retaliating" => ai.retaliating = Some((n(w)?, n(w)?)),
            "contested" => ai.contested = Some((n(w)?, n(w)?)),
            "failed" => {
                ai.failed_attack = Some(FailedAttack {
                    camp: n(w)?,
                    until_day: n(w)?,
                    group_size: n(w)?,
                    attacker_strength: f(w)?,
                    defender_strength: f(w)?,
                })
            }
            "last_chance" => ai.last_chance_target = Some(n(w)?),
            "recruitment_failure" => ai.recruitment_failure_anchor = Some(n(w)?),
            "retry" => {
                let target = n(w)?;
                let owner = o(w)?;
                let blocked = n::<u8>(w)? != 0;
                ai.retries.push(Retry { target, owner, blocked, attacker: force(w)?, defender: force(w)? });
            }
            "knows" => ai.knowledge.push(Observation {
                camp: n(w)?,
                owner: o(w)?,
                owner_since: n(w)?,
                observed_day: n(w)?,
                defenders: n(w)?,
                strength: f(w)?,
            }),
            "defer" => ai.deferrals.push(Deferral {
                target: n(w)?,
                owner: o(w)?,
                blocker: w.next().and_then(Blocker::parse).ok_or("a blocker")?,
                retry_day: n(w)?,
                progress: n(w)?,
                sequence: n(w)?,
            }),
            "next_deferral" => ai.next_deferral = n(w)?,
            "stall" => {
                ai.stall = Stall {
                    stalled_target: o(w)?,
                    stalled_reason: r(w)?,
                    stalled_waits: n(w)?,
                    no_progress_waits: n(w)?,
                    no_progress_location: o(w)?,
                    strategic_no_progress_waits: n(w)?,
                    previous_exploratory_camp: o(w)?,
                }
            }
            "demobilized" => ai.demobilized.push(n(w)?),
            "support" => {
                ai.support.initialized = n::<u8>(w)? != 0;
                ai.support.last_trap_day = n(w)?;
                ai.support.trap_count = n(w)?;
                ai.support.saving_for_snake_trap = n::<u8>(w)? != 0;
                let loc = o(w)?;
                let (camps, day): (i32, i32) = (n(w)?, n(w)?);
                ai.support.strategic = loc.map(|l| (l, camps, day));
            }
            "seen" => ai.support.traders_seen.push((n(w)?, n(w)?)),
            "recruitment_city" => ai.attack_recruitment_city = Some(n(w)?),
            "survival_release" => ai.survival_releases.push(n(w)?),
            "kit" => ai.leader_kit = crate::combat::Kit { weapon: o(w)?, protection: o(w)? },
            _ => return Err("an unknown modern record"),
        }
        if w.next().is_some() {
            return Err("more than the record holds");
        }
        Ok(())
    }
}
