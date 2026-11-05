//! `DialogWindow`, drawn and handled: the speaker's face, a half-dark frame,
//! two lines of what they say at a time with "…" to go on, and three
//! choices. The conversation itself is `burngfx::dialog`'s.
//!
//! Positions are the window's own, from its top left, which `LocationScene`
//! puts at the middle of the map view, ten pixels up.

use alloc::string::String;
use alloc::vec::Vec;
use burngfx::dialog::{Action, Conversation, Pages};

use crate::gfx::{Face, Target};
use crate::hud::Hud;

/// `FrameSize` and `FramePos`.
const FRAME: (i32, i32, i32, i32) = (43, 10, 208, 90);
/// `Size = FramePos + FrameSize`.
pub const SIZE: (i32, i32) = (251, 100);
/// `new PixelColor(240, 164, 56)`, `(108, 116, 168)`, `(240, 64, 56)`.
const TEXT_INK: u32 = 0x00F0_A438;
const OPTION_INK: u32 = 0x006C_74A8;
const CHOICE_INK: u32 = 0x00F0_4038;
/// `burn?499`, the line that turns the page.
pub const MORE: u32 = 499;

pub struct Talk {
    /// Index into `People`.
    pub who: usize,
    pub face: u8,
    pub conv: Conversation,
    /// What is said, substitutions made.
    pub lines: Vec<String>,
    /// The three choices' text, substitutions made.
    pub choice_text: [String; 3],
    pub more_text: String,
    pub pages: Pages,
    /// `hover`: a choice 0–2, or 0 for the page line while paging.
    pub hover: Option<usize>,
    /// Where the window's top left is, in game coordinates.
    pub at: (i32, i32),
    /// Lines of text shown at once: two, or three when the window is opened
    /// with `textLinesPerPage: 3` (`LastRivalDialog`).
    pub per_page: usize,
    /// `face.IsVisible`: `SetCharacter(character, conversation)` opens the
    /// window without the face unless asked (`showFace`), as the `/petko`
    /// dialog is.
    pub show_face: bool,
}

impl Talk {
    /// `OnMouseMove`: a line is under the pointer when the pointer is over
    /// its text — measured with the text font, as the original measures
    /// options too.
    pub fn point(&mut self, x: i32, y: i32, font: &Face) {
        let (px, py) = (x - self.at.0, y - self.at.1);
        let textx = 55;
        self.hover = None;
        if !self.pages.choosing {
            let texty = 85;
            if px >= textx && py >= texty && px < textx + font.width(&self.more_text) && py < texty + 10 {
                self.hover = Some(0);
            }
        } else {
            let mut texty = 63;
            for i in 0..3 {
                let w = font.width(&self.choice_text[i]);
                if px >= textx && py >= texty && px < textx + w && py < texty + 10 {
                    self.hover = Some(i);
                }
                texty += 11;
            }
        }
    }

    /// `OnMouseClick`: turn the page, or answer with a choice's action.
    /// Clicking anywhere is taken; only a hovered line does anything.
    pub fn click(&mut self) -> Option<Action> {
        match self.hover {
            Some(0) if !self.pages.choosing => {
                self.pages.more(self.lines.len());
                None
            }
            Some(i) if self.pages.choosing => Some(self.conv.choices[i].action),
            _ => None,
        }
    }

    /// The choices that are there: a line with no text is not offered.
    fn offered(&self) -> Vec<usize> {
        (0..3).filter(|&i| !self.choice_text[i].is_empty()).collect()
    }

    /// `MoveFocus`, Up or Down: the lit choice, round the ones offered.
    pub fn key_focus(&mut self, down: bool) {
        if !self.pages.choosing {
            return;
        }
        let offered = self.offered();
        if offered.is_empty() {
            return;
        }
        let at = self.hover.and_then(|h| offered.iter().position(|&i| i == h));
        let n = offered.len();
        let next = match at {
            None => if down { 0 } else { n - 1 },
            Some(a) => if down { (a + 1) % n } else { (a + n - 1) % n },
        };
        self.hover = Some(offered[next]);
    }

    /// `OnInputAction` Primary: while there is more text, the next page;
    /// else the first press lights the first choice offered
    /// (`FirstVisibleChoice`) and the next answers with the lit one — the
    /// line [`Talk::click`] then takes, as a click on it would.
    pub fn key_primary(&mut self) -> bool {
        if !self.pages.choosing {
            self.pages.more(self.lines.len());
            return false;
        }
        match self.hover {
            Some(_) => true,
            None => {
                self.hover = self.offered().first().copied();
                false
            }
        }
    }

    /// `OnInputAction` Back: the choice that leaves (`Exit`), lit for the
    /// click that takes it.
    pub fn key_back(&mut self) -> bool {
        match (0..3).find(|&i| self.conv.choices[i].action == Action::Exit) {
            Some(i) => {
                self.pages.choosing = true;
                self.hover = Some(i);
                true
            }
            None => false,
        }
    }

    /// Drawn against whatever origin the target has: `(0, 0)` over the map,
    /// the room's corner over a room. `at` is in the same coordinates the
    /// clicks are read in, so the two cannot disagree about where it is.
    pub fn draw(&self, t: &mut Target, hud: &Hud, ms: u32) {
        let (ox, oy) = self.at;
        t.fill(ox + FRAME.0, oy + FRAME.1, FRAME.2, FRAME.3, 0, 128);
        // The face, at the window's corner.
        if self.show_face {
            hud.draw_face(t, self.face as usize, ox, oy, ms);
        }
        let Some(f) = &hud.font else { return };
        let mut y = oy + 20;
        for i in 0..self.per_page {
            if let Some(line) = self.lines.get(self.pages.offset + i) {
                let x = ox + 43 + FRAME.2 / 2 - (f.width(line) / 2 - 1);
                f.draw_at(t, line, x, y, Some(TEXT_INK));
                y += 11;
            }
        }
        if !self.pages.choosing {
            let ink = if self.hover == Some(0) { CHOICE_INK } else { OPTION_INK };
            f.draw_at(t, &self.more_text, ox + 55, oy + 85, Some(ink));
        } else {
            for i in 0..3 {
                let ink = if self.hover == Some(i) { CHOICE_INK } else { OPTION_INK };
                f.draw_at(t, &self.choice_text[i], ox + 55, oy + 63 + 11 * i as i32, Some(ink));
            }
        }
    }
}
