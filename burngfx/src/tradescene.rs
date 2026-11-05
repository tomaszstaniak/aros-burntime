//! `TraderScene`'s layout: where the two inventories, the two exchange
//! frames and the buttons are, and which of them a 320-wide screen shows.
//!
//! At a game width of 450 or more the scene is 470 × 200 and everything is
//! on it. Narrower, it is 320 × 200 and shows one side at a time, chosen by
//! where the pointer is: over to the right past 120 shows the trader, back
//! left of 200 shows the player (`PositionElements`).

use crate::view::ItemGrid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Before the pointer has moved: `InventorySide.None`.
    None,
    Player,
    Trader,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: i32,
    pub player_shown: bool,
    pub trader_shown: bool,
    /// The trader's `InventoryWindow`.
    pub trader_window: (i32, i32),
    /// `exchangeTop` (the trader's offer) and `exchangeBottom` (the player's).
    pub top: (i32, i32),
    pub bottom: (i32, i32),
    pub accept: (i32, i32),
}

pub const WIDE: i32 = 450;

impl Layout {
    pub fn new(game_w: i32, side: Side) -> Layout {
        if game_w >= WIDE {
            return Layout {
                width: 470,
                player_shown: true,
                trader_shown: true,
                trader_window: (304, 5),
                top: (195, 1),
                bottom: (195, 101),
                accept: (320, 183),
            };
        }
        let trader = side == Side::Trader;
        Layout {
            width: 320,
            player_shown: !trader,
            trader_shown: trader,
            trader_window: (154, 5),
            top: if trader { (2, 1) } else { (195, 1) },
            bottom: if trader { (2, 101) } else { (195, 101) },
            accept: (170, 183),
        }
    }

    /// The trader's pack grid: `(9, 72) + basePos`, spacing (4, 16).
    pub fn trader_grid(&self) -> ItemGrid {
        ItemGrid { gap_y: 16, ..ItemGrid::plain(self.trader_window.0 + 24, self.trader_window.1 + 87, 3, 2, 4) }
    }

    /// An exchange frame's grid: `(8, 19)`, spacing (4, 7).
    pub fn exchange_grid(at: (i32, i32)) -> ItemGrid {
        ItemGrid { gap_y: 7, ..ItemGrid::plain(at.0 + 8, at.1 + 19, 3, 2, 4) }
    }
}

/// `PositionElements`' choice, from the pointer's x in scene coordinates.
pub fn side_for(side: Side, x: i32) -> Side {
    let threshold = if side != Side::Player { 120 } else { 200 };
    if x >= threshold { Side::Trader } else { Side::Player }
}

/// `CheckTrade`'s light, from `trade::check`'s answer and the two counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Light {
    Ok,
    Ng,
}

impl Light {
    /// `(0, 156, 0)` or `(208, 0, 0)`; the player's own frame is always
    /// `(72, 72, 116)`.
    pub fn rgb(self) -> u32 {
        match self {
            Light::Ok => 0x00_009C_00,
            Light::Ng => 0x00_D000_00,
        }
    }
}

pub const NO_LIGHT: u32 = 0x0048_4874;
