//! Which scene is showing, and what of the world runs under it.
//!
//! v0.9.1 has no global game loop: each scene's `OnUpdate` decides. Read off
//! the tag, there are three answers:
//!
//! | scene | `World.Update` (the turn's clock) | people move (`ActiveLocationObj.Update`, `ActivePlayerObj.Update`) |
//! |---|---|---|
//! | `MapScene` | yes | no |
//! | `LocationScene`, its dialog included | yes | yes |
//! | `InventoryScene` — a room, a pile | yes | no |
//! | `TraderScene`, `DoctorScene`, `PubScene`, `RestaurantScene` | no | no |
//! | `InfoScene`, `StatisticsScene`, `OptionsScene` | no | no |
//! | `PauseScene`, `WaitScene` | no | no |
//! | `ImageScene` — a door's film (v1.2) | no | no |
//!
//! `World.Update` itself is only `Time -= TimePerDay / RoundTime * elapsed`;
//! everybody's walking, minds and blows are the location's and the player's
//! own `Update`, which only `LocationScene` calls. So a room lets the day run
//! out while nobody in the place takes a step.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    Map,
    Location,
    Inventory,
    Trader,
    Doctor,
    /// `PubScene` or `RestaurantScene`.
    Service,
    Info,
    Statistics,
    Options,
    Pause,
    Wait,
    /// `ImageScene`: the film a door shows before what it opens. Its
    /// `OnUpdate` moves only its subtitles (`ImageScene.cs:72-82`).
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runs {
    /// The turn's clock runs down.
    pub clock: bool,
    /// People in the place walk, think and strike.
    pub people: bool,
    /// Running out of time ends the turn from here (`Time <= 0` checked in
    /// `OnUpdate`): the map and the place. A room lets the clock reach zero
    /// and the turn ends on the way back out.
    pub ends_turn: bool,
}

impl Scene {
    pub fn runs(self) -> Runs {
        match self {
            Scene::Map => Runs { clock: true, people: false, ends_turn: true },
            Scene::Location => Runs { clock: true, people: true, ends_turn: true },
            Scene::Inventory => Runs { clock: true, people: false, ends_turn: false },
            _ => Runs { clock: false, people: false, ends_turn: false },
        }
    }
}
