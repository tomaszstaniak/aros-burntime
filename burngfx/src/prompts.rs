//! v1.2's input prompts: the rows of "label + key" hints along the bottom of
//! a scene and next to some buttons.
//!
//! Ported from `Burntime.Framework/GUI/InputPromptController.cs` (the model
//! and `Resolve`), `Burntime.Remaster/GUI/InputControlDisplay.cs` (which
//! glyph or text stands for a control), `InputControlLabelRenderer.cs`
//! (measuring and placing one prompt) and `InputPromptOverlay.cs` /
//! `InputPromptOverlay.Row.cs` (the rows). Only the arithmetic lives here:
//! the game draws the [`Piece`]s a [`Row`] holds, from the glyph sheets
//! [`sheet_cell`] names.
//!
//! Upstream asks its window tree at render time: which windows are visible,
//! focused, which conditions hold. The port has no window tree, so a scene
//! describes the same thing each frame as a [`Scene`] of [`Win`]s whose
//! conditions it has already evaluated, and [`overlay`] turns that into rows.
//!
//! Coordinates are game pixels (`Resolution.Game`), the space upstream lays
//! out in. A glyph is [`GLYPH_W`] × [`GLYPH_H`] game pixels there and is drawn
//! from a 22 × 22 cell, which is 22 × 22 back-buffer pixels at the game's
//! 15/8 × 9/4 scale (`InputControlLabelRenderer.cs:15-20`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::input::{Action, Bindings, Key, Pad};

/// `InputMode`, less `None` (nothing is drawn then: `Row.cs:123-124`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Mouse,
    Keyboard,
    Gamepad,
}

/// `InputPattern`, less `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    HorizontalNavigation,
    HorizontalPaging,
    VerticalPaging,
}

/// `InputPromptPosition`: the slot a context prompt takes in the right row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Primary,
    Secondary,
    Extended,
}

/// `MouseButton`. `None` is a real value upstream: a prompt that sets it
/// says it has no mouse control at all, which hides it in mouse mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    None,
    Left,
    Right,
    Middle,
}

/// A prompt's `GuiString`, by what identifies it (`InputPrompt.Equals`
/// compares `Label.ID`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Label {
    /// `@prompts?N`: line N (from 0) of `prompts.txt`.
    Prompts(u16),
    /// `@setupnotes?N`: the start screen's NOTES button.
    SetupNotes(u16),
    /// `@tooltip?N`: an item tooltip's own prompt (Check materials).
    Tooltip(u16),
    /// A literal: `"..."`, `""`, the language scene's own words.
    Text(&'static str),
}

/// A keyboard `Key` as a prompt names it: the key and whether Shift is part
/// of it (`ModifierKeys.Shift`; v1.2's prompts use no other modifier).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub shift: bool,
}

impl Chord {
    pub const fn plain(key: Key) -> Chord {
        Chord { key, shift: false }
    }

    pub const fn shifted(key: Key) -> Chord {
        Chord { key, shift: true }
    }
}

/// `Scene.PreferredPrimaryKeyboardControl`'s default, `new Key(' ')`
/// (`Scene.cs:23`). `MenuScene` and `OptionsScene` prefer Enter.
pub const PREFER_SPACE: Chord = Chord::plain(Key::Char(' '));
pub const PREFER_ENTER: Chord = Chord::plain(Key::Enter);

/// `InputPrompt` (`InputPromptController.cs:23-70`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prompt {
    /// `None` for `InputAction.None`.
    pub action: Option<Action>,
    pub pattern: Option<Pattern>,
    pub label: Label,
    /// Overrides `pattern` in mouse and keyboard mode.
    pub keyboard_pattern: Option<Pattern>,
    pub key: Option<Chord>,
    pub pad: Option<Pad>,
    pub mouse: Option<Mouse>,
    pub show_in_mouse_mode: bool,
}

impl Prompt {
    pub const fn action(action: Action, label: Label) -> Prompt {
        Prompt {
            action: Some(action),
            pattern: None,
            label,
            keyboard_pattern: None,
            key: None,
            pad: None,
            mouse: None,
            show_in_mouse_mode: false,
        }
    }

    pub const fn pattern(pattern: Pattern, label: Label) -> Prompt {
        Prompt {
            action: None,
            pattern: Some(pattern),
            label,
            keyboard_pattern: None,
            key: None,
            pad: None,
            mouse: None,
            show_in_mouse_mode: false,
        }
    }

    pub const fn with_mouse(mut self, mouse: Mouse) -> Prompt {
        self.mouse = Some(mouse);
        self
    }

    pub const fn with_key(mut self, key: Chord) -> Prompt {
        self.key = Some(key);
        self
    }

    pub const fn with_keyboard_pattern(mut self, pattern: Pattern) -> Prompt {
        self.keyboard_pattern = Some(pattern);
        self
    }

    pub const fn with_pad(mut self, pad: Pad) -> Prompt {
        self.pad = Some(pad);
        self
    }

    pub const fn shown_with_mouse(mut self) -> Prompt {
        self.show_in_mouse_mode = true;
        self
    }

    /// `IsEmpty`: neither an action nor a pattern.
    pub fn is_empty(&self) -> bool {
        self.action.is_none() && self.pattern.is_none()
    }

    /// `EffectiveMouseControl`.
    pub fn effective_mouse(&self) -> Option<Mouse> {
        self.mouse.or_else(|| self.action.and_then(default_mouse))
    }
}

/// `InputPrompt.DefaultMouseControl` (`:64-69`): Primary is the left
/// button, Secondary and Action the right.
pub fn default_mouse(action: Action) -> Option<Mouse> {
    match action {
        Action::Primary => Some(Mouse::Left),
        Action::Secondary | Action::Action => Some(Mouse::Right),
        _ => None,
    }
}

/// `PositionAlignment`. For a vertical alignment `Right` means bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// Where an inline prompt goes (`InputPromptCollection.Add(…, position, …)`,
/// `:187-209`), with upstream's defaults from [`Inline::at`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inline {
    /// Relative to the window that owns the prompt.
    pub at: (i32, i32),
    pub h_align: Align,
    pub v_align: Align,
    pub background: bool,
    pub padding: i32,
    pub separator: &'static str,
}

impl Inline {
    pub const fn at(x: i32, y: i32) -> Inline {
        Inline { at: (x, y), h_align: Align::Left, v_align: Align::Left, background: true, padding: 4, separator: "   " }
    }

    pub const fn aligned(mut self, h: Align, v: Align) -> Inline {
        self.h_align = h;
        self.v_align = v;
        self
    }

    pub const fn separated(mut self, separator: &'static str) -> Inline {
        self.separator = separator;
        self
    }

    pub const fn bare(mut self, padding: i32) -> Inline {
        self.background = false;
        self.padding = padding;
        self
    }
}

/// `InputPromptEntry`, with its condition or selector already evaluated:
/// `active` is what `Resolve()` returned this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub candidates: Vec<Prompt>,
    pub active: Option<Prompt>,
    /// `ContextPosition`, from `AddDynamic(position, …)`.
    pub role: Option<Role>,
    /// Set for an inline prompt.
    pub inline: Option<Inline>,
}

impl Entry {
    /// `Add(prompt, condition)`: `shown` is the condition (true when none).
    pub fn add(prompt: Prompt, shown: bool) -> Entry {
        Entry { candidates: alloc::vec![prompt], active: shown.then_some(prompt), role: None, inline: None }
    }

    /// `AddDynamic(selector, candidates)`; `active` is the selector's answer.
    pub fn dynamic(candidates: &[Prompt], active: Option<Prompt>) -> Entry {
        // Upstream throws on an empty list (`:166-168`).
        assert!(!candidates.is_empty(), "a dynamic prompt needs candidates");
        Entry { candidates: candidates.to_vec(), active, role: None, inline: None }
    }

    /// `AddDynamic(position, selector, candidates)`.
    pub fn dynamic_at(role: Role, candidates: &[Prompt], active: Option<Prompt>) -> Entry {
        Entry { role: Some(role), ..Entry::dynamic(candidates, active) }
    }

    /// `AddDynamic(action, labelSelector, labels)`: one action, the label
    /// chosen.
    pub fn dynamic_label(action: Action, labels: &[Label], active: Option<Label>) -> Entry {
        let candidates: Vec<Prompt> = labels.iter().map(|&l| Prompt::action(action, l)).collect();
        Entry::dynamic(&candidates, active.map(|l| Prompt::action(action, l)))
    }

    /// `Add(prompt, position, condition, …)`.
    pub fn inline(prompt: Prompt, shown: bool, place: Inline) -> Entry {
        Entry { inline: Some(place), ..Entry::add(prompt, shown) }
    }
}

/// `InputPromptCollection`, with `SuppressWhen` evaluated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collection {
    pub entries: Vec<Entry>,
    pub suppressed: bool,
    pub hide_in_new_gfx: bool,
}

impl Collection {
    pub fn of(entries: Vec<Entry>) -> Collection {
        Collection { entries, ..Collection::default() }
    }

    /// `InputPromptController.IsSuppressed` (`:255-256`).
    fn hidden(&self, new_gfx: bool) -> bool {
        self.suppressed || (self.hide_in_new_gfx && new_gfx)
    }
}

/// A window below the scene that owns prompts, or whose children do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Win {
    pub prompts: Collection,
    pub visible: bool,
    /// `IsPromptActive(LastInputMode)`: `HasFocus` by default; a button's
    /// hover (mouse) or key selection; `ItemGridWindow` while it has a
    /// focused item; always for `MapView`.
    pub active: bool,
    /// `PositionOnScreen - scene.PositionOnScreen`, for its inline prompts.
    pub at: (i32, i32),
    pub children: Vec<Win>,
}

/// The scene: its own prompts and its windows, in `Windows` order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    pub prompts: Collection,
    pub children: Vec<Win>,
}

/// `SceneManager.InputWindow`: the scene itself, a window under it (a path
/// of child indices from the scene), or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Root<'a> {
    None,
    Scene,
    Window(&'a [usize]),
}

/// `InputPromptContextGroup`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub candidates: Vec<Prompt>,
    pub active: Option<Prompt>,
}

/// `InputPromptInlineEntry`; `place.at` is relative to the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub prompt: Prompt,
    pub place: Inline,
}

/// `InputPromptLayout`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    /// The bottom-left row.
    pub scene: Vec<Prompt>,
    /// The bottom-right row, one slot per role.
    pub context: Vec<Group>,
    pub inline: Vec<Placed>,
}

#[derive(Clone, Copy)]
struct Candidate {
    prompt: Prompt,
    available: bool,
    projected_scene: bool,
}

/// `InputPromptController.Resolve()` (`:258-325`).
pub fn resolve(scene: &Scene, root: Root, mode: Mode, new_gfx: bool) -> Resolved {
    let mouse = mode == Mode::Mouse;
    let mut out = Resolved::default();
    // Indexed by `Role as usize`; `Dictionary` order does not matter, the
    // role order below decides.
    let mut slots: [Vec<Candidate>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    if !scene.prompts.hidden(new_gfx) {
        for entry in scene.prompts.entries.iter().filter(|e| e.inline.is_none()) {
            // Outside mouse mode, or shown with the mouse anyway, a scene
            // prompt goes in the left row. Otherwise it competes for the
            // right one, as if a window had it.
            if !mouse || entry.active.is_some_and(|p| p.show_in_mouse_mode) {
                out.scene.extend(entry.active);
                continue;
            }
            add_candidates(entry, true, mouse, true, &mut slots);
        }
    }

    match root {
        Root::None => {}
        // The root's own prompts are the scene's, already taken; when they
        // are suppressed, so is everything under the scene.
        Root::Scene if scene.prompts.hidden(new_gfx) => {}
        Root::Scene => {
            for w in &scene.children {
                window_candidates(w, mouse, new_gfx, true, &mut slots);
            }
        }
        Root::Window(path) => {
            let mut w = path.first().and_then(|&i| scene.children.get(i));
            for &i in path.iter().skip(1) {
                w = w.and_then(|w| w.children.get(i));
            }
            if let Some(w) = w {
                window_candidates(w, mouse, new_gfx, true, &mut slots);
            }
        }
    }

    // Navigation extends the secondary side of the row; ordinary context
    // rows read primary then secondary from left to right.
    let order: &[Role] = if slots[Role::Extended as usize].is_empty() {
        &[Role::Primary, Role::Secondary]
    } else {
        &[Role::Extended, Role::Secondary, Role::Primary]
    };
    for &role in order {
        let slot = &slots[role as usize];
        if slot.is_empty() {
            continue;
        }
        let mut active: Option<Candidate> = None;
        for c in slot.iter().filter(|c| c.available) {
            // A window's prompt wins over the scene's; among equals the last.
            match active {
                Some(a) if c.projected_scene != a.projected_scene && c.projected_scene => {}
                _ => active = Some(*c),
            }
        }
        out.context.push(Group { candidates: slot.iter().map(|c| c.prompt).collect(), active: active.map(|c| c.prompt) });
    }

    inline_prompts(&scene.prompts, (0, 0), mouse, new_gfx, &mut out.inline);
    for w in &scene.children {
        window_inline(w, mouse, new_gfx, &mut out.inline);
    }
    out
}

/// `AddWindowContextCandidates` (`:330-352`).
fn window_candidates(w: &Win, mouse: bool, new_gfx: bool, own: bool, slots: &mut [Vec<Candidate>; 3]) {
    if !w.visible || w.prompts.hidden(new_gfx) {
        return;
    }
    if own {
        for entry in w.prompts.entries.iter().filter(|e| e.inline.is_none()) {
            add_candidates(entry, w.active, mouse, false, slots);
        }
    }
    for child in &w.children {
        window_candidates(child, mouse, new_gfx, true, slots);
    }
}

/// `AddEntryContextCandidates` (`:385-420`).
fn add_candidates(entry: &Entry, owner_active: bool, mouse: bool, projected_scene: bool, slots: &mut [Vec<Candidate>; 3]) {
    let mut active_is_candidate = false;
    for &prompt in &entry.candidates {
        let selected = entry.active == Some(prompt);
        active_is_candidate |= selected;
        let Some(role) = role_of(entry, &prompt, mouse) else { continue };
        slots[role as usize].push(Candidate { prompt, available: owner_active && selected, projected_scene });
    }
    // A selector may answer with a prompt it did not list.
    let Some(active) = entry.active else { return };
    if !owner_active || active_is_candidate {
        return;
    }
    if let Some(role) = role_of(entry, &active, mouse) {
        slots[role as usize].push(Candidate { prompt: active, available: true, projected_scene });
    }
}

/// The slot a candidate takes: by its mouse button in mouse mode
/// (`RoleForMouse`; none without one), else `ContextRole` (`:434-453`).
fn role_of(entry: &Entry, prompt: &Prompt, mouse: bool) -> Option<Role> {
    if mouse {
        return match prompt.effective_mouse()? {
            Mouse::None => None,
            Mouse::Left => Some(Role::Primary),
            Mouse::Right => Some(Role::Secondary),
            Mouse::Middle => Some(Role::Extended),
        };
    }
    Some(entry.role.unwrap_or(if prompt.pattern.is_some() {
        Role::Extended
    } else if prompt.action == Some(Action::Primary) {
        Role::Primary
    } else {
        Role::Secondary
    }))
}

/// `AddInlinePrompts` (`:354-383`), for one collection.
fn inline_prompts(c: &Collection, owner: (i32, i32), mouse: bool, new_gfx: bool, out: &mut Vec<Placed>) {
    if c.hidden(new_gfx) {
        return;
    }
    for entry in &c.entries {
        let (Some(place), Some(prompt)) = (entry.inline, entry.active) else { continue };
        // With the mouse only a prompt that names a button of its own.
        if mouse && matches!(prompt.mouse, None | Some(Mouse::None)) {
            continue;
        }
        let at = (owner.0 + place.at.0, owner.1 + place.at.1);
        out.push(Placed { prompt, place: Inline { at, ..place } });
    }
}

fn window_inline(w: &Win, mouse: bool, new_gfx: bool, out: &mut Vec<Placed>) {
    if !w.visible {
        return;
    }
    inline_prompts(&w.prompts, w.at, mouse, new_gfx, out);
    for child in &w.children {
        window_inline(child, mouse, new_gfx, out);
    }
}

// ---------------------------------------------------------------------------
// What stands for a control (`InputControlDisplay.cs`).

/// `KeyboardGlyph` (`InputControlDisplay.cs:8-18`), a cell of
/// `input_glyphs_keyboard.png`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyGlyph {
    /// `a`…`z`, lower case.
    Letter(char),
    Digit(u8),
    Escape,
    Enter,
    Tab,
    Space,
    Ctrl,
    Alt,
    Shift,
    Up,
    Down,
    Left,
    Right,
    MouseLeft,
    MouseRight,
}

impl KeyGlyph {
    /// The enum's value upstream: `A` is 1, `MouseRight` 49.
    pub fn ordinal(self) -> u8 {
        match self {
            KeyGlyph::Letter(c) => 1 + (c as u8 - b'a'),
            KeyGlyph::Digit(d) => 27 + d,
            KeyGlyph::Escape => 37,
            KeyGlyph::Enter => 38,
            KeyGlyph::Tab => 39,
            KeyGlyph::Space => 40,
            KeyGlyph::Ctrl => 41,
            KeyGlyph::Alt => 42,
            KeyGlyph::Shift => 43,
            KeyGlyph::Up => 44,
            KeyGlyph::Down => 45,
            KeyGlyph::Left => 46,
            KeyGlyph::Right => 47,
            KeyGlyph::MouseLeft => 48,
            KeyGlyph::MouseRight => 49,
        }
    }

    /// `GetKeyboardAtlasIndex` (`InputControlLabelRenderer.cs:157-176`):
    /// letters, digits, Escape, Enter, Tab and Space fill the first eight
    /// columns of the 10 × 5 sheet row by row; the rest sit in the last two.
    pub fn atlas_index(self) -> u8 {
        let seq = self.ordinal() - 1;
        if self.ordinal() <= KeyGlyph::Space.ordinal() {
            return seq / 8 * 10 + seq % 8;
        }
        match self {
            KeyGlyph::Ctrl => 8,
            KeyGlyph::Left => 9,
            KeyGlyph::Shift => 18,
            KeyGlyph::Right => 19,
            KeyGlyph::Alt => 28,
            KeyGlyph::MouseLeft => 29,
            KeyGlyph::Up => 38,
            KeyGlyph::MouseRight => 39,
            KeyGlyph::Down => 48,
            _ => 0,
        }
    }
}

/// `InputGlyph` (`Burntime.Platform/InputGlyph.cs:4-25`), a cell of a
/// controller family's sheet. `Plus` and `Minus` are `Menu` and `View`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PadGlyph {
    FaceSouth = 1,
    FaceEast,
    FaceWest,
    FaceNorth,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Menu,
    View,
    DPadHorizontal,
    RightStick,
    LeftTrigger,
    RightTrigger,
    LeftShoulder,
    RightShoulder,
}

/// `GamepadLabelStyle`: which controller's pictures and names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Xbox,
    PlayStation,
    Steam,
    Switch,
}

/// One part of a control's picture (`InputControlPart`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Text(String),
    Key(KeyGlyph),
    Pad(PadGlyph),
}

impl Part {
    pub fn has_glyph(&self) -> bool {
        !matches!(self, Part::Text(_))
    }
}

/// `InputControlLabel`: empty means the prompt is not shown.
pub type Control = Vec<Part>;

/// Where the words come from: prompt labels and `controls.txt`.
pub trait Texts {
    fn label(&self, label: Label) -> &str;
    /// `controls.txt` line `index` from 0 (`InputControlDisplay.cs:39-62`).
    fn control(&self, index: usize) -> &str;
}

/// `controls.txt` lines (`InputControlDisplay.cs:39-62`).
pub mod line {
    pub const ESCAPE: usize = 0;
    pub const ENTER: usize = 1;
    pub const TAB: usize = 2;
    pub const UP: usize = 3;
    pub const DOWN: usize = 4;
    pub const LEFT: usize = 5;
    pub const RIGHT: usize = 6;
    pub const SPACE: usize = 7;
    pub const BACKSPACE: usize = 8;
    pub const SHIFT: usize = 9;
    pub const ALT: usize = 10;
    pub const HOLD: usize = 11;
    pub const LEFT_BUMPER: usize = 12;
    pub const RIGHT_BUMPER: usize = 13;
    pub const LEFT_STICK: usize = 14;
    pub const RIGHT_STICK: usize = 15;
    pub const LEFT_TRIGGER: usize = 16;
    pub const RIGHT_TRIGGER: usize = 17;
    pub const DPAD_UP: usize = 18;
    pub const DPAD_DOWN: usize = 19;
    pub const DPAD_LEFT: usize = 20;
    pub const DPAD_RIGHT: usize = 21;
    pub const MENU: usize = 22;
    pub const VIEW: usize = 23;
}

/// What resolving a control needs besides the prompt.
pub struct Controls<'a> {
    pub bindings: &'a Bindings,
    pub family: Family,
    pub texts: &'a dyn Texts,
    /// The scene's `PreferredPrimaryKeyboardControl`.
    pub preferred_primary: Chord,
}

impl Controls<'_> {
    /// `InputControlDisplay.Resolve(app, mode, prompt, preferred)` (`:98-116`).
    pub fn of(&self, mode: Mode, prompt: &Prompt) -> Control {
        let pattern = match mode {
            Mode::Keyboard | Mode::Mouse => prompt.keyboard_pattern.or(prompt.pattern),
            Mode::Gamepad => prompt.pattern,
        };
        if let Some(p) = pattern {
            return self.pattern(mode, p);
        }
        let Some(action) = prompt.action else { return Control::new() };
        let key = prompt.key.or((action == Action::Primary).then_some(self.preferred_primary));
        self.action(mode, action, key, prompt.pad, prompt.effective_mouse())
    }

    /// `ResolvePattern` (`:66-96`).
    pub fn pattern(&self, mode: Mode, pattern: Pattern) -> Control {
        use KeyGlyph as K;
        match (mode, pattern) {
            (Mode::Mouse | Mode::Keyboard, Pattern::HorizontalNavigation) => alloc::vec![Part::Key(K::Left), Part::Key(K::Right)],
            (Mode::Mouse | Mode::Keyboard, Pattern::HorizontalPaging) => {
                alloc::vec![Part::Key(K::Shift), Part::Key(K::Left), Part::Key(K::Right)]
            }
            (Mode::Mouse | Mode::Keyboard, Pattern::VerticalPaging) => {
                alloc::vec![Part::Key(K::Shift), Part::Key(K::Up), Part::Key(K::Down)]
            }
            (Mode::Gamepad, Pattern::HorizontalNavigation) => alloc::vec![Part::Pad(PadGlyph::DPadHorizontal)],
            (Mode::Gamepad, _) => self.pair(mode, Action::LeftArea, Action::RightArea),
        }
    }

    /// `ResolvePair` (`:129-152`), as the paging patterns use it.
    fn pair(&self, mode: Mode, first: Action, second: Action) -> Control {
        let a = self.action(mode, first, None, None, None);
        let b = self.action(mode, second, None, None, None);
        if a.is_empty() || b.is_empty() {
            return Control::new();
        }
        if let ([Part::Pad(x)], [Part::Pad(y)]) = (a.as_slice(), b.as_slice()) {
            if matches!((x, y), (PadGlyph::DPadLeft, PadGlyph::DPadRight) | (PadGlyph::DPadRight, PadGlyph::DPadLeft)) {
                return alloc::vec![Part::Pad(PadGlyph::DPadHorizontal)];
            }
        }
        if let ([Part::Text(x)], [Part::Text(y)]) = (a.as_slice(), b.as_slice()) {
            let mut s = x.clone();
            s.push_str(" / ");
            s.push_str(y);
            return alloc::vec![Part::Text(s)];
        }
        if a.iter().any(Part::has_glyph) && b.iter().any(Part::has_glyph) {
            // `CombineGlyphs`: the modifiers both start with, once.
            let shared = a
                .iter()
                .zip(&b)
                .take_while(|(x, y)| matches!(x, Part::Key(KeyGlyph::Ctrl | KeyGlyph::Alt | KeyGlyph::Shift)) && x == y)
                .count();
            let mut out = a;
            out.extend(b.into_iter().skip(shared));
            return out;
        }
        let mut out = a;
        out.push(Part::Text(String::from(" / ")));
        out.extend(b);
        out
    }

    /// `Resolve(app, mode, action, key, pad, mouse)` (`:175-226`).
    pub fn action(&self, mode: Mode, action: Action, key: Option<Chord>, pad: Option<Pad>, mouse: Option<Mouse>) -> Control {
        let mut mode = mode;
        if mode == Mode::Mouse {
            if let Some(button) = mouse.or_else(|| default_mouse(action)) {
                return match button {
                    Mouse::Left => alloc::vec![Part::Key(KeyGlyph::MouseLeft)],
                    Mouse::Right => alloc::vec![Part::Key(KeyGlyph::MouseRight)],
                    _ => Control::new(),
                };
            }
            // Mouse users can still use the regular keyboard shortcuts.
            mode = Mode::Keyboard;
        }
        if mode == Mode::Keyboard {
            let keys = keyboard_controls(self.bindings, action);
            let Some(&first) = keys.first() else { return Control::new() };
            let chosen = key.and_then(|k| keys.iter().copied().find(|&c| c == k)).unwrap_or(first);
            return self.keyboard(chosen);
        }
        let pads = self.bindings.pads_for(action);
        let control = match (pads.first(), pad) {
            (None, None) => return Control::new(),
            (None, Some(p)) => p,
            (Some(&first), preferred) => {
                let want = preferred.or_else(|| default_pad(action));
                want.and_then(|w| pads.iter().copied().find(|&p| p == w)).unwrap_or(first)
            }
        };
        match pad_glyph(control, self.family) {
            Some(g) => alloc::vec![Part::Pad(g)],
            None => text_part(pad_text(control, self.family, self.texts)),
        }
    }

    /// `FormatKeyboard` (`:294-307`).
    fn keyboard(&self, chord: Chord) -> Control {
        let Some(glyph) = key_glyph(chord.key) else { return text_part(self.key_text(chord)) };
        let mut out = Control::new();
        if chord.shift {
            out.push(Part::Key(KeyGlyph::Shift));
        }
        out.push(Part::Key(glyph));
        out
    }

    /// `Format(app, Key)` (`:340-366`), for a key with no glyph.
    fn key_text(&self, chord: Chord) -> String {
        let mut s = String::new();
        if chord.shift {
            s.push_str(self.texts.control(line::SHIFT));
            s.push('+');
        }
        match chord.key {
            Key::Escape => s.push_str(self.texts.control(line::ESCAPE)),
            Key::Enter => s.push_str(self.texts.control(line::ENTER)),
            Key::Tab => s.push_str(self.texts.control(line::TAB)),
            Key::Up => s.push_str(self.texts.control(line::UP)),
            Key::Down => s.push_str(self.texts.control(line::DOWN)),
            Key::Left => s.push_str(self.texts.control(line::LEFT)),
            Key::Right => s.push_str(self.texts.control(line::RIGHT)),
            // `VirtualKey.ToString()`.
            Key::F1 => s.push_str("F1"),
            Key::Ctrl => s.push_str("Ctrl"),
            Key::Alt => s.push_str("Alt"),
            Key::Char(' ') => s.push_str(self.texts.control(line::SPACE)),
            Key::Char('\x08') => s.push_str(self.texts.control(line::BACKSPACE)),
            Key::Char(c) => s.extend(c.to_uppercase()),
        }
        s
    }
}

fn text_part(s: String) -> Control {
    // `FromText`: no text, no control.
    if s.is_empty() { Control::new() } else { alloc::vec![Part::Text(s)] }
}

/// `KeyboardBindings.GetControls` (`KeyboardBindings.cs:113-143`): the
/// bound keys, then the chords that are always there.
pub fn keyboard_controls(bindings: &Bindings, action: Action) -> Vec<Chord> {
    let mut out: Vec<Chord> = bindings.keys_for(action).into_iter().map(Chord::plain).collect();
    match action {
        Action::LeftArea => out.push(Chord::shifted(Key::Left)),
        Action::RightArea => out.push(Chord::shifted(Key::Right)),
        Action::PreviousTarget => out.push(Chord::shifted(Key::Tab)),
        Action::NextTarget => out.push(Chord::plain(Key::Tab)),
        _ => {}
    }
    out
}

/// `GetKeyboardGlyph` (`InputControlDisplay.cs:309-338`).
pub fn key_glyph(key: Key) -> Option<KeyGlyph> {
    Some(match key {
        Key::Char(c) => match c.to_ascii_lowercase() {
            l @ 'a'..='z' => KeyGlyph::Letter(l),
            d @ '0'..='9' => KeyGlyph::Digit(d as u8 - b'0'),
            ' ' => KeyGlyph::Space,
            _ => return None,
        },
        Key::Escape => KeyGlyph::Escape,
        Key::Enter => KeyGlyph::Enter,
        Key::Tab => KeyGlyph::Tab,
        Key::Alt => KeyGlyph::Alt,
        Key::Ctrl => KeyGlyph::Ctrl,
        Key::Up => KeyGlyph::Up,
        Key::Down => KeyGlyph::Down,
        Key::Left => KeyGlyph::Left,
        Key::Right => KeyGlyph::Right,
        Key::F1 => return None,
    })
}

/// `DefaultGamepadControl` (`:262-267`): the pad control a prompt prefers
/// for these two actions when several are bound.
fn default_pad(action: Action) -> Option<Pad> {
    match action {
        Action::Statistics => Some(Pad::DPadLeft),
        Action::LocationInfo => Some(Pad::DPadRight),
        _ => None,
    }
}

/// `ForcedInputGlyphProvider.GetGlyph` (`Burntime.MonoGame/
/// ForcedInputGlyphProvider.cs:12-34`): every control has a picture except
/// the left stick, which is named in text.
pub fn pad_glyph(pad: Pad, _family: Family) -> Option<PadGlyph> {
    // Switch's `Plus`/`Minus` are the same cells as `Menu`/`View`.
    Some(match pad {
        Pad::A => PadGlyph::FaceSouth,
        Pad::B => PadGlyph::FaceEast,
        Pad::X => PadGlyph::FaceWest,
        Pad::Y => PadGlyph::FaceNorth,
        Pad::DPadUp => PadGlyph::DPadUp,
        Pad::DPadDown => PadGlyph::DPadDown,
        Pad::DPadLeft => PadGlyph::DPadLeft,
        Pad::DPadRight => PadGlyph::DPadRight,
        Pad::Menu => PadGlyph::Menu,
        Pad::View => PadGlyph::View,
        Pad::RightStick => PadGlyph::RightStick,
        Pad::LeftTrigger => PadGlyph::LeftTrigger,
        Pad::RightTrigger => PadGlyph::RightTrigger,
        Pad::LeftShoulder => PadGlyph::LeftShoulder,
        Pad::RightShoulder => PadGlyph::RightShoulder,
        Pad::LeftStick => return None,
    })
}

/// `Format(app, GamepadControl, style)` (`:368-410`): a pad control's name.
pub fn pad_text(pad: Pad, family: Family, texts: &dyn Texts) -> String {
    let named = match (family, pad) {
        (Family::PlayStation | Family::Steam, Pad::LeftShoulder) => Some("L1"),
        (Family::PlayStation | Family::Steam, Pad::RightShoulder) => Some("R1"),
        (Family::PlayStation | Family::Steam, Pad::LeftStick) => Some("L3"),
        (Family::PlayStation | Family::Steam, Pad::RightStick) => Some("R3"),
        (Family::PlayStation | Family::Steam, Pad::LeftTrigger) => Some("L2"),
        (Family::PlayStation | Family::Steam, Pad::RightTrigger) => Some("R2"),
        (Family::Switch, Pad::LeftShoulder) => Some("L"),
        (Family::Switch, Pad::RightShoulder) => Some("R"),
        (Family::Switch, Pad::LeftStick) => Some("LS"),
        (Family::Switch, Pad::RightStick) => Some("RS"),
        (Family::Switch, Pad::LeftTrigger) => Some("ZL"),
        (Family::Switch, Pad::RightTrigger) => Some("ZR"),
        _ => None,
    };
    if let Some(s) = named {
        return String::from(s);
    }
    let index = match pad {
        Pad::LeftShoulder => line::LEFT_BUMPER,
        Pad::RightShoulder => line::RIGHT_BUMPER,
        Pad::LeftStick => line::LEFT_STICK,
        Pad::RightStick => line::RIGHT_STICK,
        Pad::LeftTrigger => line::LEFT_TRIGGER,
        Pad::RightTrigger => line::RIGHT_TRIGGER,
        Pad::DPadUp => line::DPAD_UP,
        Pad::DPadDown => line::DPAD_DOWN,
        Pad::DPadLeft => line::DPAD_LEFT,
        Pad::DPadRight => line::DPAD_RIGHT,
        Pad::Menu => line::MENU,
        Pad::View => line::VIEW,
        // `control.ToString()`.
        Pad::A => return String::from("A"),
        Pad::B => return String::from("B"),
        Pad::X => return String::from("X"),
        Pad::Y => return String::from("Y"),
    };
    String::from(texts.control(index))
}

// ---------------------------------------------------------------------------
// Glyph sheets.

/// A glyph cell's side in the sheets, in sheet pixels.
pub const GLYPH_SOURCE: i32 = 22;
/// A glyph's advance and height in game pixels: `round(22 / 1.875)` and
/// `round(22 / 2.25)` (`InputControlLabelRenderer.cs:18-20`).
pub const GLYPH_W: i32 = 12;
pub const GLYPH_H: i32 = 10;

/// The pack's names for the six sheets; the packer reads the files of the
/// same names under `gfx/ui/` (see `pack::names::V12_UI_IMAGES`).
pub const SHEET_KEYBOARD: &str = "ui/glyphs_keyboard";
pub const SHEET_XBOX: &str = "ui/glyphs_xbox";
pub const SHEET_PLAYSTATION: &str = "ui/glyphs_playstation";
pub const SHEET_STEAM: &str = "ui/glyphs_steam";
pub const SHEET_SWITCH: &str = "ui/glyphs_switch";
/// One 22 × 22 picture (`DrawHoldGlyph`).
pub const SHEET_HOLD: &str = "ui/glyphs_hold";

/// A picture's cell: the sheet and the top-left corner in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub sheet: &'static str,
    pub x: i32,
    pub y: i32,
}

/// The glyph a part is, in the family's sheet. The keyboard sheet is
/// 10 × 5 cells, a controller sheet 4 × 4 indexed by `InputGlyph - 1`.
pub fn sheet_cell(part: &Part, family: Family) -> Option<Cell> {
    let (sheet, index, columns) = match part {
        Part::Text(_) => return None,
        Part::Key(k) => (SHEET_KEYBOARD, k.atlas_index() as i32, 10),
        Part::Pad(p) => {
            let sheet = match family {
                Family::Xbox => SHEET_XBOX,
                Family::PlayStation => SHEET_PLAYSTATION,
                Family::Steam => SHEET_STEAM,
                Family::Switch => SHEET_SWITCH,
            };
            (sheet, *p as i32 - 1, 4)
        }
    };
    Some(Cell { sheet, x: index % columns * GLYPH_SOURCE, y: index / columns * GLYPH_SOURCE })
}

// ---------------------------------------------------------------------------
// Colours (`InputPromptOverlay.Row.cs:57-69`, `ClassicColors.cs`).

/// `InputPromptColorScheme`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Default,
    Muted,
    Hud,
    Options,
}

pub type Rgb = (u8, u8, u8);

const LIGHT_GRAY: Rgb = (212, 212, 212);
const HUD_TEXT_HOVER: Rgb = (144, 160, 212);
const OPTIONS_RED_HOVER: Rgb = (190, 77, 12);
const WHITE: Rgb = (255, 255, 255);

/// The label colour and the glyph tint.
pub fn colours(scheme: Scheme) -> (Rgb, Rgb) {
    match scheme {
        Scheme::Hud => (HUD_TEXT_HOVER, HUD_TEXT_HOVER),
        Scheme::Options => (OPTIONS_RED_HOVER, OPTIONS_RED_HOVER),
        Scheme::Muted => (LIGHT_GRAY, LIGHT_GRAY),
        Scheme::Default => (LIGHT_GRAY, WHITE),
    }
}

/// A row's background, `PixelColor(128, 0, 0, 0)`: alpha 128, black.
pub const BACKGROUND_ALPHA: u8 = 128;

/// Whether a glyph keeps its own colours: the Xbox and PlayStation face
/// buttons are coloured pictures (`InputControlLabelRenderer.cs:118-120`).
pub fn untinted(part: &Part, family: Family) -> bool {
    matches!(family, Family::Xbox | Family::PlayStation)
        && matches!(part, Part::Pad(PadGlyph::FaceSouth | PadGlyph::FaceEast | PadGlyph::FaceWest | PadGlyph::FaceNorth))
}

// ---------------------------------------------------------------------------
// Measuring and placing (`InputControlLabelRenderer.cs`, `Row.cs`).

/// What the layout needs of a font, in game pixels.
pub trait Metrics {
    /// `GetWidth`.
    fn width(&self, s: &str) -> i32;
    /// `GetHeight()`: the glyph height plus the offset at both ends.
    fn height(&self) -> i32;
    /// `LineHeight`.
    fn line_height(&self) -> i32;
}

impl Metrics for crate::font::Font {
    fn width(&self, s: &str) -> i32 {
        self.measure(s) as i32
    }

    fn height(&self) -> i32 {
        self.metric_height()
    }

    fn line_height(&self) -> i32 {
        crate::font::Font::line_height(self)
    }
}

/// `InputControlLabelRenderer`'s bracket options. A row's is
/// `brackets: false` (`Row.cs:74`): brackets only round text-only controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brackets {
    pub always: bool,
    pub text_controls: bool,
}

pub const ROW_BRACKETS: Brackets = Brackets { always: false, text_controls: true };

impl Brackets {
    /// `UsesBrackets` (`:178-186`).
    fn used(self, control: &[Part]) -> bool {
        self.always || (!control.iter().any(Part::has_glyph) && self.text_controls)
    }
}

/// `GetLabelGap` (`:188-194`): half a space where the label touches a glyph,
/// else a space.
fn label_gap(m: &dyn Metrics, control: &[Part], brackets: bool, label_first: bool) -> i32 {
    let edge = if label_first { control.first() } else { control.last() };
    let space = m.width(" ");
    if !brackets && edge.is_some_and(Part::has_glyph) { (space / 2).max(1) } else { space }
}

/// `Measure(control, label, labelFirst)` (`:64-77`), with no prefix.
pub fn measure(m: &dyn Metrics, b: Brackets, control: &[Part], label: &str, label_first: bool) -> i32 {
    let brackets = b.used(control);
    let mut w = if brackets { m.width("[]") } else { 0 };
    for p in control {
        w += match p {
            Part::Text(s) => m.width(s),
            _ => GLYPH_W,
        };
    }
    if !label.is_empty() {
        w += label_gap(m, control, brackets, label_first) + m.width(label);
    }
    w
}

/// Something to draw, in the coordinates of whatever was laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text, its top-left at `(x, y)` as `DrawText` with `Top` alignment.
    Text { x: i32, y: i32, text: String },
    /// A glyph's cell, its top-left at `(x, y)`, [`GLYPH_W`] × [`GLYPH_H`].
    Glyph { x: i32, y: i32, part: Part },
}

/// `Draw(target, position, control, label, alignment: Right, labelFirst:
/// true)` (`:79-132`), as a row draws a prompt: the label, then the control,
/// ending at `right`. `y` is the row's text line.
pub fn draw_label_first(m: &dyn Metrics, b: Brackets, control: &[Part], label: &str, right: i32, y: i32, out: &mut Vec<Piece>) {
    let brackets = b.used(control);
    let mut x = right - measure(m, b, control, label, true);
    if !label.is_empty() {
        let gap = label_gap(m, control, brackets, true);
        out.push(Piece::Text { x, y, text: String::from(label) });
        x += m.width(label) + gap;
    }
    // `DrawGlyph`: centred on the text line by integer halves; NewGfx adds
    // half a pixel more, which is not modelled here.
    let glyph_y = y + (m.height() - GLYPH_H) / 2;
    if brackets {
        out.push(Piece::Text { x, y, text: String::from("[") });
        x += m.width("[");
    }
    for p in control {
        match p {
            Part::Text(s) => {
                out.push(Piece::Text { x, y, text: s.clone() });
                x += m.width(s);
            }
            _ => {
                out.push(Piece::Glyph { x, y: glyph_y, part: p.clone() });
                x += GLYPH_W;
            }
        }
    }
    if brackets {
        out.push(Piece::Text { x, y, text: String::from("]") });
    }
}

/// `RowPrompt`: a prompt in a row; an invisible one keeps its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowPrompt {
    pub prompt: Prompt,
    pub visible: bool,
    pub reserved: i32,
}

impl RowPrompt {
    pub fn shown(prompt: Prompt) -> RowPrompt {
        RowPrompt { prompt, visible: true, reserved: 0 }
    }
}

/// How a row is set: the inline options, or the bottom rows' defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowStyle {
    pub background: bool,
    pub padding: i32,
    pub separator: &'static str,
    /// `UseSmallFontLayout`: a bottom row in NewGfx (`Row.cs:20`).
    pub small_layout: bool,
}

impl RowStyle {
    /// The bottom rows: background, padding 4, three spaces.
    pub const fn bottom(new_gfx: bool) -> RowStyle {
        RowStyle { background: true, padding: 4, separator: "   ", small_layout: new_gfx }
    }

    pub const fn inline(place: &Inline) -> RowStyle {
        RowStyle { background: place.background, padding: place.padding, separator: place.separator, small_layout: false }
    }
}

const VERTICAL_PADDING: i32 = 2;

/// A laid-out row, in its own coordinates (0, 0 its top-left).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub w: i32,
    pub h: i32,
    /// The background's x and width, the row's full height; `None` without
    /// one.
    pub background: Option<(i32, i32)>,
    pub pieces: Vec<Piece>,
}

/// `RefreshText`, `UpdateBackgroundBounds` and `OnRender` of a row
/// (`Row.cs:118-230`): `None` when nothing in it is visible.
pub fn layout_row(m: &dyn Metrics, c: &Controls, mode: Mode, prompts: &[RowPrompt], style: RowStyle) -> Option<Row> {
    struct Shown {
        control: Control,
        label: String,
        content: i32,
        reserved: i32,
        visible: bool,
    }
    let mut shown: Vec<Shown> = Vec::new();
    for rp in prompts {
        let control = c.of(mode, &rp.prompt);
        if control.is_empty() {
            continue;
        }
        let label = String::from(c.texts.label(rp.prompt.label));
        let content = measure(m, ROW_BRACKETS, &control, &label, true);
        shown.push(Shown { control, label, content, reserved: rp.reserved.max(content), visible: rp.visible });
    }
    if !shown.iter().any(|s| s.visible) {
        return None;
    }

    let sep = m.width(style.separator);
    let mut width: i32 = shown.iter().map(|s| s.reserved).sum();
    width += sep * (shown.len() as i32 - 1);
    // Glyph artwork has an inset of its own; NewGfx's small rows do not
    // double it at the right edge.
    let ends_in_glyph = shown.last().and_then(|s| s.control.last()).is_some_and(Part::has_glyph);
    let right_padding =
        if style.small_layout && ends_in_glyph { (style.padding - 2).max(0) } else { style.padding };
    let line = m.line_height().max(GLYPH_H);
    let w = width + style.padding + right_padding;
    let h = if style.small_layout { line + VERTICAL_PADDING + 1 } else { m.height() + 2 * VERTICAL_PADDING };
    let text_y = VERTICAL_PADDING + if style.small_layout { (line - m.height()) / 2 } else { 0 };

    // Right to left, so the trailing prompts keep their pixels when context
    // prompts come and go before them.
    let mut pieces = Vec::new();
    let (mut left, mut right) = (w, 0);
    let mut x = w - right_padding;
    for s in shown.iter().rev() {
        if s.visible {
            draw_label_first(m, ROW_BRACKETS, &s.control, &s.label, x, text_y, &mut pieces);
            left = left.min(x - s.content);
            right = right.max(x);
        }
        x -= s.reserved + sep;
    }
    let background = style
        .background
        .then_some((left - style.padding, right - left + style.padding + right_padding));
    Some(Row { w, h, background, pieces })
}

/// `BuildContextPrompts` (`InputPromptOverlay.cs:79-104`): each slot keeps
/// the width of its widest candidate, shown or not, so prompts do not jump.
pub fn context_row(m: &dyn Metrics, c: &Controls, mode: Mode, groups: &[Group]) -> Vec<RowPrompt> {
    let mut out = Vec::new();
    for g in groups {
        let mut widest: Option<Prompt> = None;
        let mut max = 0;
        for p in &g.candidates {
            let control = c.of(mode, p);
            let w = if control.is_empty() { 0 } else { measure(m, ROW_BRACKETS, &control, c.texts.label(p.label), true) };
            if w > max {
                max = w;
                widest = Some(*p);
            }
        }
        // `default(InputPrompt)` is empty and is skipped.
        let Some(display) = g.active.or(widest) else { continue };
        if display.is_empty() {
            continue;
        }
        out.push(RowPrompt { prompt: display, visible: g.active.is_some(), reserved: max });
    }
    out
}

/// `Boundings` (`Window.cs:136-150`): the top-left of a `size` window
/// placed at `at` with these alignments.
pub fn top_left(at: (i32, i32), size: (i32, i32), h: Align, v: Align) -> (i32, i32) {
    let shift = |p: i32, s: i32, a: Align| match a {
        Align::Left => p,
        Align::Right => p - s,
        Align::Center => p - s / 2,
    };
    (shift(at.0, size.0, h), shift(at.1, size.1, v))
}

/// The bottom rows' margin from the screen's edges (`Row.cs:81-94`).
pub const MARGIN: i32 = 6;

/// A row and where its top-left goes on the screen, in game pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub at: (i32, i32),
    pub row: Row,
    /// A bottom row, set in `font-small.txt`; an inline row is in the main
    /// font.
    pub small: bool,
}

/// What a scene's `InputPromptOverlay` draws this frame (`Refresh`,
/// `InputPromptOverlay.cs:57-69`): the scene row at the bottom left, the
/// context row at the bottom right, the inline rows where their owners put
/// them. Nothing when the player hides hints (`prompts=2`) or before any
/// input (`mode` `None`).
pub struct Frame<'a> {
    pub scene: &'a Scene,
    pub root: Root<'a>,
    pub mode: Option<Mode>,
    pub hints: Hints,
    pub new_gfx: bool,
    pub controls: &'a Controls<'a>,
    /// `font-small.txt`, for the bottom rows.
    pub small: &'a dyn Metrics,
    /// The main font, for inline rows.
    pub font: &'a dyn Metrics,
    /// `Resolution.Game`.
    pub game: (i32, i32),
    /// The scene's `PositionOnScreen`: the start screen and the menus are
    /// centred.
    pub scene_at: (i32, i32),
}

pub fn overlay(f: &Frame) -> Vec<Placement> {
    let mut out = Vec::new();
    let Some(mode) = f.mode else { return out };
    if !f.hints.shows() {
        return out;
    }
    let r = resolve(f.scene, f.root, mode, f.new_gfx);
    let bottom = RowStyle::bottom(f.new_gfx);
    let (gw, gh) = f.game;

    let scene_row: Vec<RowPrompt> = r.scene.iter().map(|&p| RowPrompt::shown(p)).collect();
    if let Some(row) = layout_row(f.small, f.controls, mode, &scene_row, bottom) {
        let at = top_left((MARGIN, gh - MARGIN), (row.w, row.h), Align::Left, Align::Right);
        out.push(Placement { at, row, small: true });
    }
    let context = context_row(f.small, f.controls, mode, &r.context);
    if let Some(row) = layout_row(f.small, f.controls, mode, &context, bottom) {
        let at = top_left((gw - MARGIN, gh - MARGIN), (row.w, row.h), Align::Right, Align::Right);
        out.push(Placement { at, row, small: true });
    }

    // `RefreshInline`: prompts at the same place, set the same way, share a
    // row, in the order they first appear.
    let mut groups: Vec<(Inline, Vec<RowPrompt>)> = Vec::new();
    for p in &r.inline {
        match groups.iter_mut().find(|(k, _)| *k == p.place) {
            Some((_, v)) => v.push(RowPrompt::shown(p.prompt)),
            None => groups.push((p.place, alloc::vec![RowPrompt::shown(p.prompt)])),
        }
    }
    for (place, prompts) in groups {
        if let Some(row) = layout_row(f.font, f.controls, mode, &prompts, RowStyle::inline(&place)) {
            let at = (f.scene_at.0 + place.at.0, f.scene_at.1 + place.at.1);
            let at = top_left(at, (row.w, row.h), place.h_align, place.v_align);
            out.push(Placement { at, row, small: false });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The player's settings (`Application.cs`).

/// `ControllerGlyphMode`, `controller_glyphs` in `user.txt`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerGlyphs {
    #[default]
    Auto,
    Xbox,
    PlayStation,
    Steam,
    Switch,
}

impl ControllerGlyphs {
    /// `ParseControllerGlyphMode` (`Application.cs:278-286`): anything else
    /// is Auto.
    pub fn parse(value: &str) -> ControllerGlyphs {
        match value.trim().to_ascii_lowercase().as_str() {
            "xbox" => ControllerGlyphs::Xbox,
            "playstation" => ControllerGlyphs::PlayStation,
            "steam" => ControllerGlyphs::Steam,
            "switch" => ControllerGlyphs::Switch,
            _ => ControllerGlyphs::Auto,
        }
    }

    /// `FormatControllerGlyphMode` (`:288-295`).
    pub fn name(self) -> &'static str {
        match self {
            ControllerGlyphs::Auto => "auto",
            ControllerGlyphs::Xbox => "xbox",
            ControllerGlyphs::PlayStation => "playstation",
            ControllerGlyphs::Steam => "steam",
            ControllerGlyphs::Switch => "switch",
        }
    }

    /// `CycleControllerGlyphMode` (`:259-269`).
    pub fn next(self) -> ControllerGlyphs {
        match self {
            ControllerGlyphs::Auto => ControllerGlyphs::Xbox,
            ControllerGlyphs::Xbox => ControllerGlyphs::PlayStation,
            ControllerGlyphs::PlayStation => ControllerGlyphs::Steam,
            ControllerGlyphs::Steam => ControllerGlyphs::Switch,
            ControllerGlyphs::Switch => ControllerGlyphs::Auto,
        }
    }

    /// The pictures drawn. Auto asks Steam Input and falls back to Xbox
    /// when Steamworks is not there (`BurntimeGame.cs:160-197`); the port
    /// has no Steam, so Auto is Xbox.
    pub fn family(self) -> Family {
        match self {
            ControllerGlyphs::PlayStation => Family::PlayStation,
            ControllerGlyphs::Steam => Family::Steam,
            ControllerGlyphs::Switch => Family::Switch,
            ControllerGlyphs::Auto | ControllerGlyphs::Xbox => Family::Xbox,
        }
    }

    /// The Settings line, `controller.txt` line N (`OptionsSettingsPage.cs:196-202`).
    pub fn line(self) -> usize {
        self as usize
    }
}

/// `UIHintVisibilityMode`, `prompts` in `user.txt`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hints {
    #[default]
    Full,
    Less,
    Hide,
}

impl Hints {
    /// `Clamp(GetInt("prompts"), Full, Hide)` (`Application.cs:161-164`);
    /// what is not a number is 0.
    pub fn parse(value: &str) -> Hints {
        match value.trim().parse::<i32>().unwrap_or(0) {
            i32::MIN..=0 => Hints::Full,
            1 => Hints::Less,
            _ => Hints::Hide,
        }
    }

    /// What `user.txt` keeps: the number.
    pub fn value(self) -> u8 {
        self as u8
    }

    /// `ShowUIHints`: Less shows them as Full does.
    pub fn shows(self) -> bool {
        self != Hints::Hide
    }

    /// `CycleUIHintVisibilityMode` (`:271-276`): the Settings line goes
    /// between Full and Hide only.
    pub fn next(self) -> Hints {
        if self == Hints::Hide { Hints::Full } else { Hints::Hide }
    }

    /// The Settings line, `prompt_mode.txt` line N (`OptionsSettingsPage.cs:203-204`).
    pub fn line(self) -> usize {
        usize::from(self == Hints::Hide)
    }
}
