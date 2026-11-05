//! The four player colours of v1.3 (`13e12ef` "add gray and blue as
//! playable colors", `bae043c` "add character colorization"): which colour
//! each seat flies, and what follows from it — the ink of its names, its
//! map marker, its flag, and the clothes its boss and its recruits wear.
//!
//! Sources at v1.3 (`550a1194`): `Logic/Generation/PlayerColorSetup.cs`,
//! `GameCreation.cs:174-185,243-258`, `Helper.cs:8-34`, `Player.cs:139-147`,
//! `Platform/Graphics/GreenClothingRecolor.cs`.

/// `BurntimePlayerColor`, in its order (`GameCreation.cs:27-33`): the
/// start screen's toggle states and the colour tables are indexed by it.
/// v1.3 names the fourth `Black`; it is drawn gray (`(208, 208, 208)`, the
/// changelog's "gray").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerColor {
    Green = 0,
    Red = 1,
    Blue = 2,
    Black = 3,
}

pub const ALL: [PlayerColor; 4] = [PlayerColor::Green, PlayerColor::Red, PlayerColor::Blue, PlayerColor::Black];

impl PlayerColor {
    /// The start screen's toggle state, 0..4.
    pub fn from_index(i: usize) -> Option<PlayerColor> {
        ALL.get(i).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The word a save writes. Green and red are the words saves had before
    /// v1.3's colours, so those files read unchanged.
    pub fn word(self) -> &'static str {
        match self {
            PlayerColor::Green => "green",
            PlayerColor::Red => "red",
            PlayerColor::Blue => "blue",
            PlayerColor::Black => "black",
        }
    }

    pub fn from_word(w: &str) -> Option<PlayerColor> {
        ALL.into_iter().find(|c| c.word() == w)
    }

    /// `Toggle.NextState`: the next of the four, round.
    pub fn next(self) -> PlayerColor {
        ALL[(self.index() + 1) % 4]
    }

    /// `colors[(int)color]` (`GameCreation.cs:174-178`): `Player.Color`, the
    /// ink of the player's own people and of the cities he controls.
    pub fn ink(self) -> u32 {
        match self {
            PlayerColor::Green => 0x0000_D000,
            PlayerColor::Red => 0x00F0_4038,
            PlayerColor::Blue => 0x0080_80D0,
            PlayerColor::Black => 0x00D0_D0D0,
        }
    }

    /// `colorsdark[(int)color]` (`GameCreation.cs:180-184`): `ColorDark`,
    /// his campers' ink.
    pub fn ink_dark(self) -> u32 {
        match self {
            PlayerColor::Green => 0x0000_8400,
            PlayerColor::Red => 0x00C0_0000,
            PlayerColor::Blue => 0x0050_5084,
            PlayerColor::Black => 0x009C_9C9C,
        }
    }

    /// `PlayerColorSetup.IconId`: the world map marker.
    pub fn icon_id(self) -> usize {
        match self {
            PlayerColor::Red => 1,
            PlayerColor::Blue => 3,
            PlayerColor::Black => 2,
            PlayerColor::Green => 0,
        }
    }

    /// `PlayerColorSetup.FlagId`: the first of the flag's four `syst.raw`
    /// frames.
    pub fn flag_id(self) -> u8 {
        match self {
            PlayerColor::Red => 8,
            PlayerColor::Blue => 12,
            PlayerColor::Black => 4,
            PlayerColor::Green => 0,
        }
    }

    /// `PlayerColorSetup.BodyColorSet` at v1.3 (after `bae043c`).
    pub fn body_color_set(self) -> BodySet {
        match self {
            PlayerColor::Red => BodySet::Original(0),
            PlayerColor::Blue => BodySet::Original(1),
            PlayerColor::Black => BodySet::Gray,
            PlayerColor::Green => BodySet::Original(2),
        }
    }
}

/// `PlayerColorSetup.Assign`: the two choices first (the second moved to the
/// first free colour when it repeats the first), then the colours left, in
/// their order, for seats 2 and 3.
pub fn assign(first: PlayerColor, second: PlayerColor) -> [PlayerColor; 4] {
    let second = if second == first { ALL.into_iter().find(|&c| c != first).unwrap_or(PlayerColor::Red) } else { second };
    let mut out = [first, second, first, first];
    let mut n = 2;
    for c in ALL {
        if c != first && c != second {
            out[n] = c;
            n += 1;
        }
    }
    out
}

/// `PlayerColorSetup.OtherSelection`: the other player's colour after this
/// one chose `selected` — taking his colour hands him this one's previous.
pub fn other_selection(previous: PlayerColor, selected: PlayerColor, other: PlayerColor) -> PlayerColor {
    if selected == other { previous } else { other }
}

/// A body's clothes: one of the three original colour sets, or v1.3's gray
/// (`Helper.GrayBodyColorSet`, 3): the green set recoloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodySet {
    Original(u8),
    Gray,
    /// v0.9.1's colour set, by v0.9.1's own rules ([`v091_body`]): the
    /// legacy game.
    V091(u8),
}

/// v0.9.1's `Helper.GetSetBodyId` and `GetCharacterBody` (`Helper.cs` at
/// `v0.9.1`): a mercenary 1, a technician or a doctor 0, anybody else 2,
/// then `(body + 1) × 16 + 48 × colorSet` — so a hired dog takes the
/// set's third body, as v0.9.1 dresses him.
pub fn v091_body(class: crate::gamdat::CharClass, set: u8) -> u16 {
    use crate::gamdat::CharClass::*;
    let body: u16 = match class {
        Mercenary => 1,
        Technician | Doctor => 0,
        _ => 2,
    };
    (body + 1) * 16 + 48 * set as u16
}

/// `Helper.GrayClothingRgb`.
pub const GRAY_CLOTHING: u32 = 0x00D4_D4D4;

/// `Player.CharacterBodyColorSet` (`Player.cs:139-147`): a player's
/// recruits' clothes by his `IconID`, so that a save from before v1.3,
/// which kept the old `BodyColorSet`, still dresses them by colour.
pub fn body_set_of_icon(icon_id: usize) -> Option<BodySet> {
    match icon_id {
        0 => Some(BodySet::Original(2)),
        1 => Some(BodySet::Original(0)),
        2 => Some(BodySet::Gray),
        3 => Some(BodySet::Original(1)),
        _ => None,
    }
}

/// `Helper.GetBodyId`: the first frame of a body in `syssze.raw`'s
/// numbering. `body` is `GetSetBodyId`'s: 0 doctor, 1 mercenary, 2
/// technician, 3 boss.
pub fn body_id(body: u8, color: u8) -> Option<u16> {
    if color >= 3 {
        return None;
    }
    match body {
        3 => Some((14 + color as u16) * 16),
        0..=2 => Some((body as u16 + 1) * 16 + 48 * color as u16),
        _ => None,
    }
}

/// A body the way a person carries it: the first of its sixteen frames in
/// `syssze.raw`'s numbering, with [`GRAY`] set for v1.3's gray clothes.
/// Bosses are `224-271`, frames the classic sheet does not have:
/// [`classic_frame`] maps them as `classic/newgfx.txt` does; the NewGfx
/// sheet has its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub sprite: u16,
    pub gray: bool,
}

/// The flag a body number carries for gray clothes (`;recolor=d4d4d4`).
pub const GRAY: u16 = 0x1000;

impl Body {
    pub fn id(self) -> u16 {
        self.sprite | if self.gray { GRAY } else { 0 }
    }

    pub fn from_id(id: u16) -> Body {
        Body { sprite: id & !GRAY, gray: id & GRAY != 0 }
    }
}

/// `Helper.GetCharacterBodyResource`: gray takes the green set's frames,
/// recoloured.
pub fn character_body(body: u8, set: BodySet) -> Option<Body> {
    let (color, gray) = match set {
        BodySet::Original(c) => (c, false),
        BodySet::Gray => (2, true),
        BodySet::V091(c) => return Some(Body { sprite: (body as u16 + 1) * 16 + 48 * c as u16, gray: false }),
    };
    Some(Body { sprite: body_id(body, color)?, gray })
}

/// A frame of `syssze.raw` as the classic look has it: the bosses' `224-271`
/// are `classic/newgfx.txt`'s `48-63`, `96-111` and `144-159`.
pub fn classic_frame(frame: u16) -> u16 {
    match frame {
        224..=239 => frame - 224 + 48,
        240..=255 => frame - 240 + 96,
        256..=271 => frame - 256 + 144,
        n => n,
    }
}

/// `Helper.GetSetBodyId(CharClass)`.
pub fn set_body_id(class: crate::gamdat::CharClass) -> Option<u8> {
    use crate::gamdat::CharClass::*;
    match class {
        Mercenary => Some(1),
        Doctor => Some(0),
        Technician => Some(2),
        Boss => Some(3),
        _ => None,
    }
}

/// `GreenClothingRecolor.Apply`: every visible pixel whose green beats red
/// and blue by more than 30 % takes the target colour at the green's
/// intensity; alpha, skin and trim stay.
pub fn recolor_green(rgba: &mut [u8], target: u32) {
    let (tr, tg, tb) = ((target >> 16) & 255, (target >> 8) & 255, target & 255);
    for px in rgba.chunks_exact_mut(4) {
        let green = px[1] as u32;
        if px[3] == 0 || green * 10 <= px[0] as u32 * 13 || green * 10 <= px[2] as u32 * 13 {
            continue;
        }
        px[0] = ((green * tr + 127) / 255) as u8;
        px[1] = ((green * tg + 127) / 255) as u8;
        px[2] = ((green * tb + 127) / 255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PlayerColor::*;

    /// `GameDefinitionsTests.PlayerColorCases` at v1.3.
    #[test]
    fn every_pair_of_choices() {
        for first in ALL {
            for second in ALL {
                let a = assign(first, second);
                assert_eq!(a[0], first);
                if first != second {
                    assert_eq!(a[1], second);
                }
                for (i, c) in a.iter().enumerate() {
                    assert!(!a[..i].contains(c), "{first:?}/{second:?}: distinct colours");
                }
                let flags: alloc::vec::Vec<u8> = a.iter().map(|c| c.flag_id()).collect();
                let icons: alloc::vec::Vec<usize> = a.iter().map(|c| c.icon_id()).collect();
                for i in 0..4 {
                    assert!(!flags[..i].contains(&flags[i]) && !icons[..i].contains(&icons[i]));
                }
                assert_eq!(other_selection(first, second, second), first, "choosing the other's colour swaps it");
            }
        }
    }

    #[test]
    fn default_colours_keep_the_original_assets() {
        let a = assign(Green, Red);
        assert_eq!(a, [Green, Red, Blue, Black]);
        assert_eq!(a.map(|c| c.flag_id()), [0, 8, 12, 4], "gam.dat's flags");
        assert_eq!(a.map(|c| c.icon_id()), [0, 1, 3, 2], "the original icon order");
        assert_eq!(other_selection(Green, Black, Red), Red, "an unoccupied choice keeps the other");
    }

    /// `ClothingRecolorTests`: neutral gray keeps shadows, highlights, skin
    /// and white trim; only strongly green visible pixels change.
    #[test]
    fn recolouring_is_upstreams() {
        let mut px = [0u8, 60, 0, 255, 0, 132, 0, 128, 0, 208, 0, 255, 220, 180, 150, 255, 255, 255, 255, 255];
        recolor_green(&mut px, GRAY_CLOTHING);
        assert_eq!(px, [50, 50, 50, 255, 110, 110, 110, 128, 173, 173, 173, 255, 220, 180, 150, 255, 255, 255, 255, 255]);
        let mut px = [7u8, 227, 5, 255, 7, 87, 6, 128, 220, 180, 150, 255, 255, 255, 255, 255, 7, 227, 5, 0, 100, 130, 0, 255];
        recolor_green(&mut px, 0x8C_A6C7);
        assert_eq!(px, [125, 148, 177, 255, 48, 57, 68, 128, 220, 180, 150, 255, 255, 255, 255, 255, 7, 227, 5, 0, 100, 130, 0, 255]);
    }

    #[test]
    fn bodies() {
        assert_eq!(Black.body_color_set(), BodySet::Gray);
        assert_eq!(Blue.body_color_set(), BodySet::Original(1));
        // Bosses: red 48, blue 96, green 144; gray is green's, recoloured.
        assert_eq!(character_body(3, Red.body_color_set()), Some(Body { sprite: 224, gray: false }));
        assert_eq!(character_body(3, Blue.body_color_set()), Some(Body { sprite: 240, gray: false }));
        assert_eq!(character_body(3, Green.body_color_set()), Some(Body { sprite: 256, gray: false }));
        assert_eq!(character_body(3, BodySet::Gray), Some(Body { sprite: 256, gray: true }));
        assert_eq!([224, 240, 256 + 15, 32].map(classic_frame), [48, 96, 159, 32], "classic/newgfx.txt");
        assert_eq!(Body::from_id(Body { sprite: 128, gray: true }.id()), Body { sprite: 128, gray: true });
        // A mercenary: `(1 + 1) * 16 + 48 * colour`.
        assert_eq!(character_body(1, BodySet::Original(0)), Some(Body { sprite: 32, gray: false }));
        assert_eq!(character_body(1, BodySet::Gray), Some(Body { sprite: 128, gray: true }));
        // `ClothingRecolorTests` "saved player icons correct legacy bodyset assignments".
        assert_eq!(body_set_of_icon(3), Some(BodySet::Original(1)));
        assert_eq!(body_set_of_icon(2), Some(BodySet::Gray));
        // v0.9.1: a technician wears the doctor's body, a dog the set's third.
        use crate::gamdat::CharClass;
        assert_eq!(v091_body(CharClass::Mercenary, 2), 128);
        assert_eq!(v091_body(CharClass::Technician, 0), 16);
        assert_eq!(v091_body(CharClass::Dog, 1), 96);
        // The v0.9.1 boss: `GetCharacterBody(2, set)`, green 144, red 48.
        assert_eq!(character_body(2, BodySet::V091(2)), Some(Body { sprite: 144, gray: false }));
    }
}
