//! v1.2's input prompts on the screen: the rows `InputPromptOverlay` draws
//! along the bottom and beside buttons, and the key and button glyphs the
//! other windows put beside their lines (`InputControlLabelRenderer`).
//!
//! `burngfx::prompts` decides what goes where, in game pixels;
//! `burngfx::prompt_sets` is each scene's set. Here are the pictures (the
//! six `ui/glyphs_*` sheets), the words (`prompts.txt`, `controls.txt`,
//! `setupnotes.txt`, German under German), and the player's state that the
//! drawing needs every frame: the last input mode (`LastInputMode`), the
//! hints setting (`prompts` in `user.txt`) and the controller family
//! (`controller_glyphs`). A legacy game sets no mode, so nothing is drawn.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use burngfx::input::Bindings;
use burngfx::pack::{Pack, PackedImage};
use burngfx::prompt_sets::Set;
use burngfx::prompts::{self as pr, Chord, Control, Family, Hints, Label, Mode, Part, Piece, Prompt, Texts};
use burngfx::text::TextDb;

use crate::gfx::{Face, Target};

/// The words a prompt is drawn with.
#[derive(Default)]
pub struct Words {
    prompts: Vec<String>,
    controls: Vec<String>,
    notes: Vec<String>,
    tooltip: Vec<String>,
}

impl Words {
    /// In the language chosen now (`localized`).
    fn load(pack: &Pack) -> Words {
        let notes = crate::localized(pack, "v12/setupnotes").and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
        let tooltip = crate::localized(pack, "v12/tooltip").and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
        Words {
            prompts: lines(pack, "v12/prompts"),
            controls: lines(pack, "v12/controls"),
            notes: burngfx::manual_window::sections(notes).into_iter().next().unwrap_or_default(),
            // `tooltip.txt` ends each entry with `}` on its line.
            tooltip: tooltip
                .trim_start_matches('\u{feff}')
                .lines()
                .map(|l| {
                    let l = l.trim_end();
                    String::from(l.strip_suffix('}').unwrap_or(l))
                })
                .collect(),
        }
    }
}

impl Texts for Words {
    fn label(&self, label: Label) -> &str {
        match label {
            Label::Prompts(n) => self.prompts.get(n as usize).map_or("", String::as_str),
            Label::SetupNotes(n) => self.notes.get(n as usize).map_or("", String::as_str),
            Label::Tooltip(n) => self.tooltip.get(n as usize).map_or("", String::as_str),
            Label::Text(s) => s,
        }
    }

    fn control(&self, index: usize) -> &str {
        self.controls.get(index).map_or("", String::as_str)
    }
}

fn lines(pack: &Pack, key: &str) -> Vec<String> {
    let text = crate::localized(pack, key).and_then(|b| core::str::from_utf8(b).ok()).unwrap_or("");
    let db = TextDb::parse(text.trim_start_matches('\u{feff}'));
    (0..).map_while(|i| db.get(i).map(String::from)).collect()
}

const SHEETS: [&str; 6] =
    [pr::SHEET_KEYBOARD, pr::SHEET_XBOX, pr::SHEET_PLAYSTATION, pr::SHEET_STEAM, pr::SHEET_SWITCH, pr::SHEET_HOLD];

pub struct Prompts<'a> {
    sheets: [Option<PackedImage<'a>>; 6],
    /// A `RefCell`: v1.2 resolves `@prompts?N` as it draws, so a language
    /// chosen mid-run shows at once ([`Prompts::reload_words`]).
    words: RefCell<Words>,
    bindings: RefCell<Bindings>,
    /// `LastInputMode`; `None` draws nothing (a legacy game, or no input yet).
    pub mode: Cell<Option<Mode>>,
    pub hints: Cell<Hints>,
    pub family: Cell<Family>,
}

impl<'a> Prompts<'a> {
    pub fn load(pack: &Pack<'a>) -> Prompts<'a> {
        Prompts {
            sheets: SHEETS.map(|k| crate::start_view::image(pack, k)),
            words: RefCell::new(Words::load(pack)),
            bindings: RefCell::new(Bindings::defaults()),
            mode: Cell::new(None),
            hints: Cell::new(Hints::Full),
            family: Cell::new(Family::Xbox),
        }
    }

    /// The words again, after the language changed.
    pub fn reload_words(&self, pack: &Pack<'a>) {
        *self.words.borrow_mut() = Words::load(pack);
    }

    /// A label's words (a tooltip's prompt).
    pub fn label(&self, label: Label) -> String {
        String::from(self.words.borrow().label(label))
    }

    pub fn set_bindings(&self, b: &Bindings) {
        *self.bindings.borrow_mut() = b.clone();
    }

    /// A v1.2 scene with the player's hints hidden (`!ShowUIHints`): the
    /// tooltips, the label bars and the item focus go too.
    pub fn hides_hints(&self) -> bool {
        self.mode.get().is_some() && !self.hints.get().shows()
    }

    /// The mode prompts are drawn for, while hints are shown.
    pub fn shown_mode(&self) -> Option<Mode> {
        self.mode.get().filter(|_| self.hints.get().shows())
    }

    /// `InputControlDisplay.Resolve(app, mode, prompt)` with this player's
    /// keys and controller.
    pub fn control(&self, mode: Mode, prompt: &Prompt, preferred: Chord) -> Control {
        let (b, w) = (self.bindings.borrow(), self.words.borrow());
        let c = pr::Controls { bindings: &b, family: self.family.get(), texts: &*w, preferred_primary: preferred };
        c.of(mode, prompt)
    }

    /// `Resolve(app, mode, action, key, pad)`, for a menu's shortcut.
    pub fn action_control(&self, mode: Mode, action: burngfx::input::Action, key: Option<Chord>, pad: Option<burngfx::input::Pad>) -> Control {
        let (b, w) = (self.bindings.borrow(), self.words.borrow());
        let c = pr::Controls { bindings: &b, family: self.family.get(), texts: &*w, preferred_primary: pr::PREFER_SPACE };
        c.action(mode, action, key, pad, None)
    }

    /// A scene's overlay (`InputPromptOverlay.Refresh` and the rows'
    /// `OnRender`): the bottom rows in `small`, the inline ones in `font`.
    /// The scene's (0, 0) is the target's origin (`PositionOnScreen`).
    pub fn draw_set(&self, t: &mut Target, set: &Set, small: &Face, font: &Face) {
        let scene_at = t.origin;
        let Some(mode) = self.mode.get() else { return };
        let (b, w) = (self.bindings.borrow(), self.words.borrow());
        let controls = pr::Controls { bindings: &b, family: self.family.get(), texts: &*w, preferred_primary: set.preferred };
        let frame = pr::Frame {
            scene: &set.scene,
            root: pr::Root::Scene,
            mode: Some(mode),
            hints: self.hints.get(),
            new_gfx: crate::gfx::newgfx(),
            controls: &controls,
            small,
            font,
            game: t.game_size(),
            scene_at,
        };
        let (text, tint) = pr::colours(set.scheme);
        let (text, tint) = (rgb(text), rgb(tint));
        let origin = t.origin;
        t.origin = (0, 0);
        for placed in pr::overlay(&frame) {
            let (x, y) = placed.at;
            if let Some((bx, bw)) = placed.row.background {
                t.fill(x + bx, y, bw, placed.row.h, 0, pr::BACKGROUND_ALPHA as u32);
            }
            let face = if placed.small { small } else { font };
            // `glyphVerticalOffset: smallFont ? 0 : 0.5f` (`Row.cs:75`).
            self.draw_pieces(t, face, &placed.row.pieces, (x, y), text, tint, !placed.small);
        }
        t.origin = origin;
    }

    /// Laid-out text and glyphs, offset by `at`.
    /// `half`: the glyphs half a game pixel lower in NewGfx.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_pieces(&self, t: &mut Target, face: &Face, pieces: &[Piece], at: (i32, i32), text: u32, tint: u32,
                       half: bool) {
        let family = self.family.get();
        for piece in pieces {
            match piece {
                Piece::Text { x, y, text: s } => face.draw_at(t, s, at.0 + x, at.1 + y, Some(text)),
                Piece::Glyph { x, y, part } => {
                    let tinted = (!pr::untinted(part, family)).then_some(tint);
                    self.draw_glyph(t, part, at.0 + x, at.1 + y, half, tinted);
                }
            }
        }
    }

    fn draw_glyph(&self, t: &mut Target, part: &Part, x: i32, y: i32, half: bool, tint: Option<u32>) {
        let Some(cell) = pr::sheet_cell(part, self.family.get()) else { return };
        let Some(i) = SHEETS.iter().position(|&k| k == cell.sheet) else { return };
        if let Some(img) = &self.sheets[i] {
            let s = pr::GLYPH_SOURCE as u32;
            t.blit_glyph(img, (cell.x as u32, cell.y as u32, s, s), x, y, half, tint.filter(|&c| c & 0xFF_FFFF != 0xFF_FFFF));
        }
    }

    /// `DrawHoldGlyph`: the hold mark, its top left at `(x, y)` (already
    /// centred on the text line).
    pub fn draw_hold(&self, t: &mut Target, x: i32, y: i32, tint: u32) {
        if let Some(img) = &self.sheets[5] {
            let s = pr::GLYPH_SOURCE as u32;
            t.blit_glyph(img, (0, 0, s, s), x, y, true, Some(tint));
        }
    }

    /// `InputControlLabelRenderer.Draw(…, alignment: Right, labelFirst:
    /// true)` with the given brackets: the label then the control, ending at
    /// `right`, the text's top at `y`.
    #[allow(clippy::too_many_arguments)]
    /// `half`: the renderer's default glyph offset (a menu, the manual);
    /// a tooltip's is 0.
    pub fn draw_label_first(&self, t: &mut Target, face: &Face, brackets: pr::Brackets, control: &[Part], label: &str,
                            right: i32, y: i32, text: u32, tint: u32, half: bool) {
        let mut pieces = Vec::new();
        pr::draw_label_first(face, brackets, control, label, right, y, &mut pieces);
        self.draw_pieces(t, face, &pieces, (0, 0), text, tint, half);
    }
}

/// An `(r, g, b)` as the `0x00RRGGBB` the drawing takes.
pub fn rgb(c: pr::Rgb) -> u32 {
    (c.0 as u32) << 16 | (c.1 as u32) << 8 | c.2 as u32
}
