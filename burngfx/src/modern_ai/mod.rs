//! v1.2's Modern opponent (`ClassicAiState` and `AI/PlayerBehavior/`), at
//! tag `v1.2`. Built a part at a time; `AiProfile::Modern` stays unavailable
//! until the turn controller is wired to both presets. The audit and the
//! order of the parts: `docs/audit-modern-ai.md`.

pub mod attack;
pub mod camps;
pub mod claims;
pub mod controller;
pub mod decision;
pub mod deferrals;
pub mod encounter;
pub mod equipment;
pub mod expansion;
pub mod group;
pub mod hiring;
pub mod intel;
pub mod needs;
pub mod policy;
pub mod production;
pub mod reason;
pub mod recovery;
pub mod recruitment;
pub mod reinforcement;
pub mod resolve;
pub mod route;
pub mod seat;
pub mod state;
pub mod supplies;
pub mod support;
pub mod telemetry;
pub mod trade_plan;
pub mod trading;
pub mod upkeep;
