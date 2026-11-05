//! Why a decision was taken, as upstream says it (`AiDecision.Reason`).
//!
//! Kept as identifiers — places by index, people by record — so that the
//! planner needs no names; [`Reason::text`] writes upstream's sentence with
//! the names the world gives it. The text is the same as at tag `v1.2`, word
//! for word, so runs can be compared with upstream's telemetry.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Names for the reasons' places and people.
/// `System.Math.Max(float, float)` as .NET Core 3.0 and later define it:
/// NaN if either is NaN, and +0 above -0. `f32::max` drops a NaN and may
/// return either zero, which a formatted `-0.0` would show.
pub fn dotnet_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        f32::NAN
    } else if a == b {
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Enumerable.Sum` over floats: from +0. Rust's `Sum` for `f32` starts from
/// −0, so an empty sum would be written `-0`.
pub fn dotnet_sum(values: impl IntoIterator<Item = f32>) -> f32 {
    values.into_iter().fold(0.0, |a, b| a + b)
}

/// `{x:0.0}` on a `float`, as .NET 10 writes it: [`format_fixed`] with one
/// decimal.
pub fn format_tenths(x: f32) -> String {
    format_fixed(x, 1)
}

/// `{x:0}`, `{x:0.0}`, `{x:0.00}`… on a `float`, as .NET 10 writes them
/// (checked against the runtime, `tests/modern_tenths.rs`): the value taken
/// to 7 significant digits, then rounded half away from zero to `decimals`,
/// on the digits. The sign stays when the value rounds to zero (`-0.0`);
/// `NaN`, `Infinity` and `-Infinity` are words.
pub fn format_fixed(x: f32, decimals: usize) -> String {
    if x.is_nan() {
        return String::from("NaN");
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x.is_infinite() {
        return alloc::format!("{sign}Infinity");
    }
    let mut out: Vec<u8> = Vec::new();
    if x != 0.0 {
        // 7 significant digits: d1.d2..d7 × 10^e.
        let sci = alloc::format!("{:.6e}", x.abs());
        let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
        let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
        let e: i32 = exp.parse().unwrap_or(0);
        // Digits kept: those before the point (e + 1) and `decimals` after.
        let kept = e + 1 + decimals as i32;
        if kept > 0 {
            out = (0..kept as usize).map(|i| digits.get(i).copied().unwrap_or(0)).collect();
        }
        let next = if kept < 0 { 0 } else { digits.get(kept as usize).copied().unwrap_or(0) };
        if next >= 5 {
            let mut i = out.len();
            loop {
                if i == 0 {
                    out.insert(0, 1);
                    break;
                }
                i -= 1;
                if out[i] == 9 {
                    out[i] = 0;
                } else {
                    out[i] += 1;
                    break;
                }
            }
        }
    }
    while out.len() < decimals + 1 {
        out.insert(0, 0);
    }
    let (int, frac) = out.split_at(out.len() - decimals);
    let int: String = int.iter().map(|d| (b'0' + d) as char).collect();
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    if decimals == 0 {
        return alloc::format!("{sign}{int}");
    }
    let frac: String = frac.iter().map(|d| (b'0' + d) as char).collect();
    alloc::format!("{sign}{int}.{frac}")
}


pub trait Names {
    /// `Location.Title`.
    fn place(&self, location: usize) -> String;
    /// `Character.Name`.
    fn person(&self, record: usize) -> String;
    /// `Player.Name`: a human's as typed, a machine player's its
    /// character's (record `seat`).
    fn player(&self, seat: usize) -> String {
        self.person(seat)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    /// A reason this port has no source text for. Never produced by the
    /// planner; for tests and the controller's own notes.
    Note(String),

    // `ReinforcementPlanning.AddCandidates`
    DemobilizeIntoGarrison,
    StageFrontierGuard { camp: usize },
    RaiseCriticalGarrison { target: i32 },
    DemobilizeIntoGarrisonAt { camp: usize },
    DeliverFrontierGuard { camp: usize },
    ReinforceCriticalCamp { target: i32 },
    DismissSurplusFollower,

    // `Recruitment.AddCandidates` and its searches
    AdvanceToFirstCampWaypoint { waypoint: Option<usize> },
    RecallSettler { target: usize },
    RecallGuardForDelivery { camp: usize },
    RecallSurplusFollower { desired: i32 },
    ProvisionBeforeSettler { target: usize },
    DelayRecruitment { target: usize },
    RecruitSettler { role: &'static str, target: usize },
    CriticalCampNeedsGuard { camp: usize },
    CityRecruitOpportunity { desired: i32 },
    GroupNeedsRecruit,
    FindStagedSettler { target: usize },
    FindGuardForDelivery { camp: usize },
    LeaderNeedsRecruit,
    FillCaravanBeforeRecruiting,
    SoloApproachToRouteRecruit { recruit: usize, stop: usize },
    PickUpFreeFirstSettler { recruit: usize, target: usize },
    HireWithReservedItem { recruit: usize, target: usize },
    PickUpAtRouteStop { recruit: usize, stop: usize, target: usize },

    // `Recruitment.AddAttackPreparationCandidates` and its helpers
    RecallBeforeAttack { target: usize },
    RestoreTravelGroup { target: usize },
    MobilizeFrontierGuard { target: usize },
    AssembleAtFrontier { target: usize, camp: usize },
    RecallToAssemble { size: i32, target: usize },
    RecruitAttackers { size: i32, target: usize },
    DeferAttackNoAssembly { target: usize, size: i32 },
    ContinueAttackRecruitment { city: usize, here: usize },
    CancelAttackNoRecruit { target: usize, here: usize },
    /// `AddRecruitOrCityCandidate` with a camp to stock up at first.
    CollectRecruitmentSupplies(Box<Reason>),

    // `AttackPlanning`
    RetreatToPrevious { place: usize },
    RetreatToSafety,
    EmergencyRetreat,
    NoRetreatForUnsuitableAttack,
    HostileCampBlocksRoute,
    /// `AdvanceBlockingReason`.
    AttackWaiting { target: usize, why: Waiting },

    // `Trading.AddCandidates`, `RouteOpportunities`, `CampDelivery`
    ContinueTrading,
    LeaveFollowerForCaravan,
    WaitToFillCaravan,
    FillCaravanAtCamp,
    OfferToRoamingTrader,
    DeliverSurplusGoods,
    SecureWaterWaypoint,
    /// The gain in value a day, written `{gain:0.0}`.
    DeliverUpgrade { gain: f32 },

    // `ExpansionPlanning`
    ClaimConnecting,
    ClaimCurrent { role: &'static str },
    PostCapturePause,
    SettlementWaitingReserves { target: usize },
    AdvanceToward { role: &'static str },
    AttackPlanReady { target: usize },
}

/// Why an attack plan cannot advance (`AdvanceBlockingReason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    Attackers { size: i32 },
    Weapon,
    NotFrontier,
    NoRoute,
    CampWater,
    RouteSupplies,
    CombatReadiness,
    LegalAdvance,
}

impl Reason {
    /// Upstream's sentence.
    pub fn text(&self, n: &dyn Names) -> String {
        let p = |l: &usize| n.place(*l);
        let who = |r: &usize| n.person(*r);
        match self {
            Reason::Note(s) => s.clone(),
            Reason::DemobilizeIntoGarrison => String::from("demobilize a surplus attack follower into a strategically needed garrison"),
            Reason::StageFrontierGuard { camp } => format!("stage a recruited guard at frontier camp {}", p(camp)),
            Reason::RaiseCriticalGarrison { target } => format!("raise critical garrison toward {target} guards"),
            Reason::DemobilizeIntoGarrisonAt { camp } => format!("demobilize a surplus attack follower into the needed garrison at {}", p(camp)),
            Reason::DeliverFrontierGuard { camp } => format!("deliver a recruited guard to frontier camp {}", p(camp)),
            Reason::ReinforceCriticalCamp { target } => format!("reinforce critical camp toward {target} guards"),
            Reason::DismissSurplusFollower => String::from("dismiss a surplus attack follower because no strategic garrison needs another guard"),
            Reason::AdvanceToFirstCampWaypoint { waypoint: None } => String::from("advance to a viable first-camp waypoint"),
            Reason::AdvanceToFirstCampWaypoint { waypoint: Some(w) } => format!("advance to viable first-camp waypoint {}", p(w)),
            Reason::RecallSettler { target } => format!("recall a settler for {}", p(target)),
            Reason::RecallGuardForDelivery { camp } => format!("recall a guard for delivery to {}", p(camp)),
            Reason::RecallSurplusFollower { desired } => format!("recall a surplus camp follower toward {desired} people"),
            Reason::ProvisionBeforeSettler { target } => format!("provision at an owned camp before recruiting a settler for {}", p(target)),
            Reason::DelayRecruitment { target } => format!("delay recruitment until the projected two-person route to {} is supplied", p(target)),
            Reason::RecruitSettler { role, target } => format!("recruit a settler for {role} at {}", p(target)),
            Reason::CriticalCampNeedsGuard { camp } => format!("critical camp {} needs another guard", p(camp)),
            Reason::CityRecruitOpportunity { desired } => format!("city opportunity: recruit toward {desired} people"),
            Reason::GroupNeedsRecruit => String::from("group needs another recruit"),
            Reason::FindStagedSettler { target } => format!("find a safely staged settler for {}", p(target)),
            Reason::FindGuardForDelivery { camp } => format!("find a guard for delivery to {}", p(camp)),
            Reason::LeaderNeedsRecruit => String::from("leader needs a recruit before claiming camps"),
            Reason::FillCaravanBeforeRecruiting => String::from("fill the caravan before recruiting in a city"),
            Reason::SoloApproachToRouteRecruit { recruit, stop } => {
                format!("continue the survivable solo approach to route recruit {} at {}", who(recruit), p(stop))
            }
            Reason::PickUpFreeFirstSettler { recruit, target } => {
                format!("pick up free first settler {} at {}, then establish the camp", who(recruit), p(target))
            }
            Reason::HireWithReservedItem { recruit, target } => {
                format!("hire {} at {} with the reserved requested item, then establish the camp", who(recruit), p(target))
            }
            Reason::PickUpAtRouteStop { recruit, stop, target } => {
                format!("pick up {} at route stop {} before settling {}", who(recruit), p(stop), p(target))
            }
            Reason::RecallBeforeAttack { target } => format!("recall a surplus guard before attacking {}", p(target)),
            Reason::RestoreTravelGroup { target } => format!("restore the two-person travel group before attacking {}", p(target)),
            Reason::MobilizeFrontierGuard { target } => format!("temporarily mobilize another frontier guard for the attack on {}", p(target)),
            Reason::AssembleAtFrontier { target, camp } => format!("assemble the attack group for {} at frontier camp {}", p(target), p(camp)),
            Reason::RecallToAssemble { size, target } => format!("recall a surplus guard to assemble {size} attackers for {}", p(target)),
            Reason::RecruitAttackers { size, target } => format!("recruit toward {size} attackers for {}", p(target)),
            Reason::DeferAttackNoAssembly { target, size } => {
                format!("defer attack on {}: no safe recruitment or staging route can assemble {size} attackers", p(target))
            }
            Reason::ContinueAttackRecruitment { city, here } => {
                format!("continue attack recruitment at {} because {} has no safe recruit", p(city), p(here))
            }
            Reason::CancelAttackNoRecruit { target, here } => {
                format!("cancel attack on {}: no safe, affordable recruit is available at {}", p(target), p(here))
            }
            Reason::CollectRecruitmentSupplies(inner) => format!("collect recruitment-trip supplies before {}", inner.text(n)),
            Reason::RetreatToPrevious { place } => format!("retreat to the previous location {} because no owned logistics remain", p(place)),
            Reason::RetreatToSafety => String::from("retreat toward the nearest reachable safe location"),
            Reason::EmergencyRetreat => String::from("make an emergency retreat despite insufficient route supplies"),
            Reason::NoRetreatForUnsuitableAttack => String::from("will not initiate an unsuitable attack without a retreat route"),
            Reason::HostileCampBlocksRoute => String::from("hostile camp blocks the current route"),
            Reason::ContinueTrading => String::from("continue trading surplus goods for needed equipment"),
            Reason::LeaveFollowerForCaravan => String::from("leave an extra follower at camp for an efficient city caravan"),
            Reason::WaitToFillCaravan => String::from("wait for camp production to fill the city caravan"),
            Reason::FillCaravanAtCamp => String::from("fill the city caravan at an owned camp near the trader"),
            Reason::OfferToRoamingTrader => String::from("offer local exports to a nearby roaming trader before the regional market"),
            Reason::DeliverSurplusGoods => String::from("deliver surplus goods and trade for needed equipment"),
            Reason::SecureWaterWaypoint => String::from("secure a critical water waypoint"),
            Reason::DeliverUpgrade { gain } => format!("deliver an upgrade worth about {} sustainable value/day", format_tenths(*gain)),
            Reason::ClaimConnecting => String::from("claim neutral camp connecting owned territory"),
            Reason::ClaimCurrent { role } => format!("claim {role} at current location"),
            Reason::PostCapturePause => String::from("post-capture pause"),
            Reason::SettlementWaitingReserves { target } => format!("settlement route to {} is waiting for travel reserves", p(target)),
            Reason::AdvanceToward { role } => format!("advance toward {role}"),
            Reason::AttackPlanReady { target } => format!("attack plan ready: advance toward {}", p(target)),
            Reason::AttackWaiting { target, why } => {
                let t = p(target);
                match why {
                    Waiting::Attackers { size } => format!("attack plan for {t} is waiting for {size} attackers"),
                    Waiting::Weapon => format!("attack plan for {t} is waiting for a weapon"),
                    Waiting::NotFrontier => format!("attack plan for {t} is no longer on the territorial frontier"),
                    Waiting::NoRoute => format!("attack plan for {t} has no permitted route"),
                    Waiting::CampWater => format!("attack plan for {t} is waiting for camp water capacity"),
                    Waiting::RouteSupplies => format!("attack plan for {t} is waiting for route supplies"),
                    Waiting::CombatReadiness => format!("attack plan for {t} is waiting for safe combat readiness"),
                    Waiting::LegalAdvance => format!("attack plan for {t} is waiting for a legal advance"),
                }
            }
        }
    }
}
