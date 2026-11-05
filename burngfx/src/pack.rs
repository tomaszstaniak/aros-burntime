//! A flat archive of everything the game needs at runtime.
//!
//! AROS has no PNG decoder and, in the `no_std` build, no file system to speak
//! of either. So a host tool decodes the artwork once and writes it here, and
//! the AROS build compiles the result in with `include_bytes!`. The reader is
//! zero-copy: entries are slices of the archive, not allocations.
//!
//! ```text
//! magic   "BURNPACK"      8 bytes
//! version u16             currently 1
//! (pad)   u16
//! count   u32
//! entries count * 32:     name [u8; 24], offset u32, len u32
//! blobs
//! ```
//!
//! Names are NUL-padded and byte-compared, so no allocation is needed to look
//! one up. They read like paths — `map/MAT_000.burnmap`, `tile/02_48`,
//! `text/BURN.TXT` — but nothing parses them; the archive is flat.

pub const MAGIC: &[u8; 8] = b"BURNPACK";
pub const VERSION: u16 = 1;
/// Entry names are fixed width so the table can be scanned without allocating.
pub const NAME_LEN: usize = 24;
const ENTRY_LEN: usize = NAME_LEN + 8;
const HEADER_LEN: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum PackError {
    /// Too short to hold what it claims, or an entry points outside it.
    Malformed,
    NotAPack,
    /// A version this build does not know how to read.
    UnsupportedVersion(u16),
}

/// `Debug` prints the shape, not the megabytes of pixels it points at.
impl core::fmt::Debug for Pack<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Pack {{ entries: {}, bytes: {} }}", self.count, self.bytes.len())
    }
}

pub struct Pack<'a> {
    bytes: &'a [u8],
    count: usize,
    /// The table is in name order (the packer writes it so): a lookup is a
    /// binary search. Older packs are searched one by one.
    sorted: bool,
}

impl<'a> Pack<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Pack<'a>, PackError> {
        if bytes.len() < HEADER_LEN {
            return Err(PackError::Malformed);
        }
        if &bytes[..8] != MAGIC {
            return Err(PackError::NotAPack);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != VERSION {
            return Err(PackError::UnsupportedVersion(version));
        }
        let count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;

        // The table has to fit before any blob does. `count` comes straight
        // out of the file, so the multiply is checked too — a hostile or
        // truncated header must not wrap around into a small number.
        let table_end = count
            .checked_mul(ENTRY_LEN)
            .and_then(|t| t.checked_add(HEADER_LEN))
            .ok_or(PackError::Malformed)?;
        if table_end > bytes.len() {
            return Err(PackError::Malformed);
        }

        let mut pack = Pack { bytes, count, sorted: false };
        // Check every entry once here rather than on each lookup.
        for i in 0..count {
            let (offset, len) = pack.extent(i);
            let end = offset.checked_add(len).ok_or(PackError::Malformed)?;
            if offset < table_end || end > bytes.len() {
                return Err(PackError::Malformed);
            }
        }
        pack.sorted = (1..count).all(|i| pack.name(i - 1) <= pack.name(i));
        Ok(pack)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn entry(&self, i: usize) -> &'a [u8] {
        let at = HEADER_LEN + i * ENTRY_LEN;
        &self.bytes[at..at + ENTRY_LEN]
    }

    fn extent(&self, i: usize) -> (usize, usize) {
        let e = self.entry(i);
        let n = |at: usize| {
            u32::from_le_bytes([e[at], e[at + 1], e[at + 2], e[at + 3]]) as usize
        };
        (n(NAME_LEN), n(NAME_LEN + 4))
    }

    /// The name of entry `i`, without its padding.
    pub fn name(&self, i: usize) -> &'a [u8] {
        let raw = &self.entry(i)[..NAME_LEN];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        &raw[..end]
    }

    pub fn data(&self, i: usize) -> &'a [u8] {
        let (offset, len) = self.extent(i);
        &self.bytes[offset..offset + len]
    }

    /// Find an entry by name: the first of that name. A binary search in a
    /// table in name order — a pack with NewGfx holds thousands of entries,
    /// and F8 reads every picture again — else one by one.
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        let want = name.as_bytes();
        if self.sorted {
            let (mut lo, mut hi) = (0, self.count);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if self.name(mid) < want {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            return (lo < self.count && self.name(lo) == want).then(|| self.data(lo));
        }
        (0..self.count).find(|&i| self.name(i) == want).map(|i| self.data(i))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// What a save is bound to: the entries the world and its rules are made
    /// from — `gam.dat`, the maps' entrances, walk grids, rooms and settings,
    /// the item and rule tables, v1.2's presets and `ai.txt` — by name and
    /// content, in the order of their names. Pictures, fonts, sounds and
    /// texts are not in it, so a pack made again with one more picture or
    /// line of text loads the same saves; a different `gam.dat` or rule
    /// table does not.
    pub fn rules_fingerprint(&self) -> u64 {
        let mut chosen: alloc::vec::Vec<usize> = (0..self.count).filter(|&i| shapes_the_world(self.name(i))).collect();
        chosen.sort_by(|&a, &b| self.name(a).cmp(self.name(b)));
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x1000_0000_01b3);
            }
        };
        for i in chosen {
            let data = self.data(i);
            feed(self.name(i));
            feed(&[0]);
            feed(&(data.len() as u32).to_le_bytes());
            feed(data);
        }
        hash
    }
}

/// [`Pack::rules_fingerprint`]'s entries: v1.2's are its four presets'
/// tables (`v12/<preset>/<table>`), `v12/ai`, its recipes
/// (`v12/constructions`) and its `settings.txt` (`v12/system`, as
/// `rules/system` is v0.9.1's), not its texts — not its conversations
/// (`v12/men/NNN`) either.
fn shapes_the_world(name: &[u8]) -> bool {
    const PREFIXES: [&[u8]; 14] = [
        b"classic/gam.dat", b"map/", b"walk/", b"rooms/", b"set/", b"items/", b"rules/", b"v12/ai",
        b"v12/dos/", b"v12/amiga/", b"v12/extended/", b"v12/classic/", b"v12/constructions", b"v12/system",
    ];
    PREFIXES.iter().any(|p| name.starts_with(p))
}

/// A decoded image inside the archive: `u16` width, `u16` height, then RGBA.
///
/// `scale` is its own resolution, image pixels per game pixel: one for every
/// 1993 picture, a NewGfx replacement's `sprite_scale` for one read through
/// [`ScaledImage`]. What a scene lays out is [`PackedImage::game_w`] and
/// [`PackedImage::game_h`], v1.2's `Sprite.Size`.
#[derive(Clone, Copy)]
pub struct PackedImage<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
    pub scale: crate::ratio::Pair,
}

impl<'a> PackedImage<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<PackedImage<'a>> {
        if bytes.len() < 4 {
            return None;
        }
        let width = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
        let height = u16::from_le_bytes([bytes[2], bytes[3]]) as u32;
        let rgba = &bytes[4..];
        // u16 * u16 * 4 exceeds u32 at the top of the range, so widen first.
        let needed = width as u64 * height as u64 * 4;
        if (rgba.len() as u64) < needed {
            return None;
        }
        Some(PackedImage { width, height, rgba, scale: crate::ratio::Pair::ONE })
    }

    /// `Sprite.Size`: `(Vector2)(frame size * Resolution)`, each axis
    /// `(int)(v + 0.5f)` — the size in game pixels.
    pub fn game_w(&self) -> i32 {
        Self::game(self.width, self.scale.x)
    }

    pub fn game_h(&self) -> i32 {
        Self::game(self.height, self.scale.y)
    }

    fn game(n: u32, scale: crate::ratio::Ratio) -> i32 {
        use crate::ratio::Ratio;
        (Ratio::int(n as i64) / scale + Ratio::new(1, 2)).floor() as i32
    }

    /// The pixel at (x, y) as RGBA, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<&'a [u8]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = ((y * self.width + x) * 4) as usize;
        self.rgba.get(at..at + 4)
    }
}

/// A NewGfx replacement in the archive: its `sprite_scale` — image pixels
/// per game pixel, as four bytes `x num, x den, y num, y den` — then a
/// [`PackedImage`]. Every scale v1.2 uses fits: 15/8 × 9/4, 2 × 2,
/// 2 × 19/8, 1 × 1.
pub struct ScaledImage<'a> {
    pub scale: crate::ratio::Pair,
    pub image: PackedImage<'a>,
}

impl<'a> ScaledImage<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<ScaledImage<'a>> {
        use crate::ratio::{Pair, Ratio};
        let h = bytes.get(..4)?;
        if h[1] == 0 || h[3] == 0 || h[0] == 0 || h[2] == 0 {
            return None;
        }
        let scale = Pair::new(Ratio::new(h[0] as i64, h[1] as i64), Ratio::new(h[2] as i64, h[3] as i64));
        let mut image = PackedImage::parse(&bytes[4..])?;
        image.scale = scale;
        Some(ScaledImage { scale, image })
    }

    /// The header for a scale, or `None` if a part does not fit in a byte.
    pub fn header(scale: crate::ratio::Pair) -> Option<[u8; 4]> {
        let b = |v: i64| u8::try_from(v).ok().filter(|&v| v > 0);
        Some([b(scale.x.num())?, b(scale.x.den())?, b(scale.y.num())?, b(scale.y.den())?])
    }
}

/// The names the packer writes, so both ends agree in one place.
///
/// Each returns `None` when the name will not fit in [`NAME_LEN`], rather than
/// writing a truncated one. A silently shortened name would collide with
/// another and be looked up successfully — the worst kind of failure — and the
/// packer cannot notice after the fact, because what it receives is already
/// short enough.
pub mod names {
    use super::NAME_LEN;

    /// `map/MAT_000.burnmap`
    pub fn map(buf: &mut [u8; NAME_LEN], name: &str) -> Option<usize> {
        write(buf, &[b"map/", name.as_bytes(), b".burnmap"])
    }

    /// `set/MAT_000.txt`
    pub fn settings(buf: &mut [u8; NAME_LEN], name: &str) -> Option<usize> {
        write(buf, &[b"set/", name.as_bytes(), b".txt"])
    }

    /// `rooms/MAT_000` — one byte per door, the classic room id.
    ///
    /// Everything a room is follows from that byte, so the archive carries the
    /// byte rather than a decoded form of it: see [`crate::matmap::RoomType`].
    pub fn rooms(buf: &mut [u8; NAME_LEN], name: &str) -> Option<usize> {
        write(buf, &[b"rooms/", name.as_bytes()])
    }

    /// `room/4` — a room interior, decoded to RGBA.
    ///
    /// Keyed by the `raum_N.pac` number from [`crate::matmap::Door::interior`]
    /// rather than by the room id, because several room ids share one picture
    /// and the ids that have no picture at all must not name an entry.
    pub fn room_image(buf: &mut [u8; NAME_LEN], interior: u8) -> Option<usize> {
        let mut n = [0u8; 3];
        let len = number(&mut n, interior);
        write(buf, &[b"room/", &n[..len]])
    }

    /// `scene/koch` — the picture behind a scene of its own, decoded to RGBA.
    ///
    /// Keyed by the `.pac` file's own stem, because these are not numbered:
    /// the pub is `bar` or `pub1`, the restaurant `wirt` or `koch`, the
    /// doctor `arzt`.
    pub fn scene_image(buf: &mut [u8; NAME_LEN], stem: &str) -> Option<usize> {
        write(buf, &[b"scene/", stem.as_bytes()])
    }

    /// `item/2` — one frame of `gst.raw`, decoded to RGBA.
    ///
    /// Keyed by the frame number, which is what an item type's `burngfx` line
    /// gives; see [`crate::items::ItemType::sprite`].
    pub fn item_sprite(buf: &mut [u8; NAME_LEN], frame: u8) -> Option<usize> {
        let mut n = [0u8; 3];
        let len = number(&mut n, frame);
        write(buf, &[b"item/", &n[..len]])
    }

    /// `body/NNN` — one frame of `syssze.raw`, the character sheet.
    ///
    /// A character's body is sixteen of these in a row, starting at the
    /// record's `SpriteId`: `syssze.raw?<id>-<id+15>` is how `GameCreation`
    /// asks for it. 214 frames in the classic file, so a byte is enough.
    pub fn body_sprite(buf: &mut [u8; NAME_LEN], frame: u8) -> Option<usize> {
        let mut n = [0u8; 3];
        let len = number(&mut n, frame);
        write(buf, &[b"body/", &n[..len]])
    }

    /// `rules/v091/original` — one release's settings for one mode.
    ///
    /// Both axes are in the key because both matter: v0.9.1 has one settings
    /// file for both modes and v1.1 has two, so "the original
    /// settings" is not a thing that exists without saying which release.
    pub fn rules_settings(buf: &mut [u8; NAME_LEN], engine: &str, mode: &str) -> Option<usize> {
        write(buf, &[b"rules/", engine.as_bytes(), b"/", mode.as_bytes()])
    }

    /// `v12/dos/items` — one of v1.2's preset files, by the directory it
    /// lives in (`rules/<dir>/<kind>.txt` at the tag). Classic borrows Dos's
    /// items and trader lists and Amiga's productions, so those are packed
    /// once, under their own directory. See [`crate::rules::RuleSet::files`].
    pub fn v12(buf: &mut [u8; NAME_LEN], dir: &str, kind: &str) -> Option<usize> {
        write(buf, &[b"v12/", dir.as_bytes(), b"/", kind.as_bytes()])
    }

    /// `items/original` — one mode's item table. The same file in both
    /// releases, so the release does not enter into it.
    pub fn rules_items(buf: &mut [u8; NAME_LEN], mode: &str) -> Option<usize> {
        write(buf, &[b"items/", mode.as_bytes()])
    }

    /// `walk/MAT_000` — the map's walkability grid.
    pub fn walk(buf: &mut [u8; NAME_LEN], name: &str) -> Option<usize> {
        write(buf, &[b"walk/", name.as_bytes()])
    }

    /// `tile/5/02_48` — a tile as one particular map colours it.
    ///
    /// The same tile looks different on different maps, because the palette
    /// comes from the map rather than from the tile set.
    pub fn map_tile(buf: &mut [u8; NAME_LEN], map: u8, section: u8, item: u8) -> Option<usize> {
        let (mut m, mut s, mut i) = ([0u8; 3], [0u8; 2], [0u8; 2]);
        let m_len = number(&mut m, map);
        two_digits(&mut s, section);
        two_digits(&mut i, item);
        write(buf, &[b"tile/", &m[..m_len], b"/", &s, b"_", &i])
    }

    fn number(out: &mut [u8; 3], mut v: u8) -> usize {
        if v == 0 {
            out[0] = b'0';
            return 1;
        }
        let mut digits = [0u8; 3];
        let mut n = 0;
        while v > 0 {
            digits[n] = b'0' + v % 10;
            v /= 10;
            n += 1;
        }
        for k in 0..n {
            out[k] = digits[n - 1 - k];
        }
        n
    }

    /// `tile/02_48`
    pub fn tile(buf: &mut [u8; NAME_LEN], section: u8, item: u8) -> Option<usize> {
        let (mut s, mut i) = ([0u8; 2], [0u8; 2]);
        two_digits(&mut s, section);
        two_digits(&mut i, item);
        write(buf, &[b"tile/", &s, b"_", &i])
    }

    /// `f/sta/8` — frame 8 of `sta.ani`, decoded to RGBA. For the `.raw` and
    /// `.ani` sheets the interface draws from: `sta`, `munt`, `syst`,
    /// `ges_00` … `ges_05`. The stem is the file's without its extension,
    /// because the `burngfx` resource names never need both.
    pub fn frame(buf: &mut [u8; NAME_LEN], stem: &str, frame: u8) -> Option<usize> {
        let mut n = [0u8; 3];
        let len = number(&mut n, frame);
        write(buf, &[b"f/", stem.as_bytes(), b"/", &n[..len]])
    }

    /// The remaster's own interface pictures, under names short enough for
    /// the table — `(key, path under resources/game/classic)`. One list, so
    /// the packer and the game cannot disagree about a name.
    /// v1.2's start screen pictures beyond v0.9.1's, from the v1.2
    /// resources (`testdata/v1.2/gfx/ui`): the Amiga opponent's button.
    /// v1.3's two new start-screen flags (`13e12ef`), from the remaster's
    /// classic package; NewGfx draws them from `start_flags.png` (`ng/` the
    /// same keys).
    pub const V13_FLAGS: &[(&str, &str)] = &[
        ("f/flag_blue", "gfx/flag_blue.png"),
        ("f/flag_black", "gfx/flag_black.png"),
    ];

    pub const V12_UI_IMAGES: &[(&str, &str)] = &[
        ("ui/amiga", "gfx/ui/start_button_amiga.png"),
        ("ui/amiga_down", "gfx/ui/start_button_amiga_down.png"),
        // `ItemGridWindow`'s focus frame.
        ("ui/item_focus", "gfx/ui/item_focus.png"),
        // The font's inline marks (`font.txt`'s `indicator_image`, 8 × 12
        // cells: blue bars, red bars, dots, camp people) and icons
        // (`icon_image`, 12 × 12: warning, radiation, gas).
        ("ui/indicators", "font-indicators.png"),
        ("ui/font_icons", "font-icons.png"),
        // The manual's GOAL picture (`ManualWindow`, `@city`).
        ("ui/manual_city", "gfx/ui/manual_city.png"),
        // v1.2's 36-pixel start-screen button, behind NOTES.
        ("ui/notes", "gfx/ui/menu_exit.png"),
        ("ui/notes_hover", "gfx/ui/menu_exit_hover.png"),
        // `ItemGridWindow`'s mark on what is in use (`IsItemInUse`).
        ("ui/item_equipped", "gfx/ui/item_equipped.png"),
        // v1.2's German package's own INFO buttons (`classic_de`,
        // `lang/de/gfx`), read for `ui/mapbutton` under German.
        ("ui/de/mapbutton", "classic_de/lang/de/gfx/mapbutton.png"),
        ("ui/de/mapbuttonh", "classic_de/lang/de/gfx/mapbuttonh.png"),
    ];

    /// v1.2's own texts that the packer copies from the player's v1.2
    /// files, `(key, path under v1.2/)`: the in-game manual (`manual.txt`,
    /// `manualui.txt`), the start screen's notes (`setupnotes.txt`) and the
    /// jukebox's song lists (`OptionsJukeboxPage`).
    pub const V12_TEXTS: &[(&str, &str)] = &[
        ("v12/manual", "lang/en/manual.txt"),
        ("v12/manualui", "lang/en/manualui.txt"),
        ("v12/setupnotes", "lang/en/setupnotes.txt"),
        // `InventoryScene.ShowTrapSelectedDialog`'s line (`@dialogs?0`).
        ("v12/dialogs", "lang/en/dialogs.txt"),
        ("v12/jukebox_dos", "music/jukebox_dos.txt"),
        ("v12/jukebox_amiga", "amiga/jukebox_amiga.txt"),
        // The Settings page's lines (`OptionsSettingsPage`).
        ("v12/controller", "lang/en/controller.txt"),
        ("v12/prompt_mode", "lang/en/prompt_mode.txt"),
        ("v12/language_mode", "lang/en/language_mode.txt"),
    ];

    /// v1.2's German texts (`resources/game/classic_de/lang/de`, the
    /// `classic_de` package that overlays `classic` when the language is
    /// German), `(key, file under v1.2/classic_de/lang/de/)`. The
    /// conversations (`MEN_NNN.TXT`) go to `v12/de/men/NNN` beside these.
    /// The game reads `v12/de/x` for `v12/x` while German is chosen.
    pub const V12_GERMAN_DIR: &str = "classic_de/lang/de";
    pub const V12_GERMAN: &[(&str, &str)] = &[
        ("v12/de/burn", "BURN.TXT"),
        ("v12/de/newburn", "newburn.txt"),
        ("v12/de/tooltip", "tooltip.txt"),
        ("v12/de/cityinfo", "cityinfo.txt"),
        ("v12/de/radio", "radio.txt"),
        ("v12/de/last_rival", "last_rival.txt"),
        ("v12/de/names", "names.txt"),
        ("v12/de/manual", "manual.txt"),
        ("v12/de/manualui", "manualui.txt"),
        ("v12/de/setupnotes", "setupnotes.txt"),
        ("v12/de/dialogs", "dialogs.txt"),
        ("v12/de/controller", "controller.txt"),
        ("v12/de/prompt_mode", "prompt_mode.txt"),
        ("v12/de/language_mode", "language_mode.txt"),
    ];

    /// v1.2's `font-small.txt` and the `font.png` it names (v1.2's, with the
    /// umlauts): the manual's text and the Options pages.
    pub const V12_SMALL_FONT: (&str, &str) = ("font/small", "font-small.txt");

    /// v1.2's input prompts (`InputControlLabelRenderer`, `InputPromptOverlay`),
    /// from the player's v1.2 files, `(key, path under v1.2/)`: the six
    /// glyph sheets of 22-pixel cells — keyboard and mouse (10 × 5), the
    /// four controller families (4 × 4), the hold mark — under the names
    /// `prompts` draws them by. The `*2x.png` sheets are xBR's only.
    pub const V12_PROMPT_GLYPHS: &[(&str, &str)] = &[
        (crate::prompts::SHEET_KEYBOARD, "gfx/ui/input_glyphs_keyboard.png"),
        (crate::prompts::SHEET_XBOX, "gfx/ui/input_glyphs_xbox.png"),
        (crate::prompts::SHEET_PLAYSTATION, "gfx/ui/input_glyphs_playstation.png"),
        (crate::prompts::SHEET_STEAM, "gfx/ui/input_glyphs_steam.png"),
        (crate::prompts::SHEET_SWITCH, "gfx/ui/input_glyphs_switch.png"),
        (crate::prompts::SHEET_HOLD, "gfx/ui/input_glyphs_hold.png"),
    ];

    /// The prompts' words: their labels (`prompts.txt`, `@prompts?N`) and
    /// the names of controls without a glyph (`controls.txt`), English and
    /// the `classic_de` package's German. The Settings page's two lines
    /// (`controller.txt`, `prompt_mode.txt`) are packed with that page.
    pub const V12_PROMPT_TEXTS: &[(&str, &str)] = &[
        ("v12/prompts", "lang/en/prompts.txt"),
        ("v12/controls", "lang/en/controls.txt"),
        ("v12/de/prompts", "classic_de/lang/de/prompts.txt"),
        ("v12/de/controls", "classic_de/lang/de/controls.txt"),
    ];

    pub const UI_IMAGES: &[(&str, &str)] = &[
        ("ui/start_buttons", "gfx/ui/start_buttons.png"),
        ("ui/level1", "gfx/ui/start_button_level1.png"),
        ("ui/level1_down", "gfx/ui/start_button_level1_down.png"),
        ("ui/level2", "gfx/ui/start_button_level2.png"),
        ("ui/level2_down", "gfx/ui/start_button_level2_down.png"),
        ("ui/level3", "gfx/ui/start_button_level3.png"),
        ("ui/level3_down", "gfx/ui/start_button_level3_down.png"),
        ("ui/remake", "gfx/ui/start_button_remake.png"),
        ("ui/remake_down", "gfx/ui/start_button_remake_down.png"),
        ("ui/original", "gfx/ui/start_button_original.png"),
        ("ui/original_down", "gfx/ui/start_button_original_down.png"),
        ("ui/ai", "gfx/ui/start_button_ai.png"),
        ("ui/ai_down", "gfx/ui/start_button_ai_down.png"),
        ("ui/noai", "gfx/ui/start_button_noai.png"),
        ("ui/noai_down", "gfx/ui/start_button_noai_down.png"),
        ("ui/menu_exit", "gfx/menu_exit.png"),
        ("ui/menu_exit_hover", "gfx/menu_exit_hover.png"),
        ("ui/start_maxdesign", "gfx/start_maxdesign.png"),
        ("ui/inventory_left", "gfx/inventory_left.png"),
        ("ui/grid", "gfx/grid.png"),
        ("ui/trader_bg", "gfx/trader_background.png"),
        ("ui/mapbutton", "gfx/mapbutton.png"),
        ("ui/mapbuttonh", "gfx/mapbuttonh.png"),
        ("ui/up", "gfx/up.png"),
        ("ui/down", "gfx/down.png"),
        ("ui/options_bulb", "gfx/ui/options_bulb.png"),
    ];

    /// The bitmap fonts: `(key, description under resources/game/classic)`.
    /// The strip is packed under the key with `.strip` appended.
    pub const FONTS: &[(&str, &str)] = &[
        ("font/plain", "font.txt"),
        ("font/hires", "highres-font.txt"),
        ("font/start", "gfx/ui/start_font.txt"),
        ("font/player", "gfx/ui/start_font_player.txt"),
    ];

    /// `ng/<classic key>` — the NewGfx replacement of what `<classic key>`
    /// names, as a [`super::ScaledImage`]; an animation whose replacement
    /// has another number of frames is `ng/<key>/<k>`, `k` from 0. Absent
    /// where v1.2 would fall back to the classic picture too.
    pub fn newgfx(buf: &mut [u8; NAME_LEN], classic_key: &str) -> Option<usize> {
        write(buf, &[NEWGFX_PREFIX.as_bytes(), classic_key.as_bytes()])
    }

    pub const NEWGFX_PREFIX: &str = "ng/";

    /// The NewGfx table: `(classic key, v1.2 resource id)` for what the game
    /// draws one by one. The packer resolves each id through the player's
    /// `newgfx.txt` and packs what it reaches under `ng/<key>`. The ranges
    /// — every frame of `munt`, `syst`, `inv`, `sta`, the bodies, the items,
    /// the faces, the packed maps' tiles, the rooms — are generated by the
    /// packer from the same key scheme as the classic entries
    /// (`f/munt/22`, `body/16`, `item/2`, `tile/04_12`, `room/0`).
    pub const NEWGFX: &[(&str, &str)] = &[
        ("ui/up", "gfx/up.png"),
        ("ui/down", "gfx/down.png"),
        ("ui/inventory_left", "gfx/inventory_left.png"),
        ("ui/trader_bg", "gfx/trader_background.png"),
        ("ui/start_maxdesign", "gfx/start_maxdesign.png"),
        // `MenuScene`'s buttons (`pngsheet@gfx/ui/start_buttons.png?N?WxH`).
        ("ui/start_buttons/0", "pngsheet@gfx/ui/start_buttons.png?0?112x24"),
        ("ui/start_buttons/1", "pngsheet@gfx/ui/start_buttons.png?1?112x24"),
        ("ui/start_buttons/2", "pngsheet@gfx/ui/start_buttons.png?2?64x24"),
        ("ui/start_buttons/3", "pngsheet@gfx/ui/start_buttons.png?3?64x24"),
        ("scene/sta", "sta.pac"),
        ("scene/info", "info.pac"),
        ("scene/opti", "opti.pac"),
        ("scene/hint2", "hint2.pac"),
        // `[newgfx_native]`: pictures only the NewGfx look has.
        ("n/char_shadow", "gfx/char_shadow.png"),
        ("n/stats_lb", "gfx/backgrounds/stats_leftbottom.png"),
        ("n/stats_lt", "gfx/backgrounds/stats_lefttop.png"),
        ("n/stats_rb", "gfx/backgrounds/stats_rightbottom.png"),
        ("n/stats_rt", "gfx/backgrounds/stats_righttop.png"),
        ("n/statistics", "gfx/backgrounds/statistics.png"),
        ("n/statistics2", "gfx/backgrounds/statistics2.png"),
        ("n/crack1", "gfx/start_crack1.png"),
        ("n/crack2", "gfx/start_crack2.png"),
        ("n/crack3", "gfx/start_crack3.png"),
        ("n/crack4", "gfx/start_crack4.png"),
        ("n/crack5", "gfx/start_crack5.png"),
        ("n/border_tl", "gfx/start_border_tl.png"),
        ("n/border_tr", "gfx/start_border_tr.png"),
        // `MenuScene.cs:110-113`.
        ("n/borders/0", "pngsheet@gfx/start_borders.png?0?512x64"),
        ("n/borders/1", "pngsheet@gfx/start_borders.png?1?512x64"),
        ("n/borders/2", "pngsheet@gfx/start_borders.png?2?512x64"),
        ("n/borders/3", "pngsheet@gfx/start_borders.png?3?512x64"),
        // v1.2's named item pictures (`items.txt`'s `image=`).
        ("gi/advice", "gfx/advice.png"),
        ("gi/dogshit", "gfx/dogshit.png"),
        ("gi/paper_helmet", "gfx/paper_helmet.png"),
        ("gi/pistol_loaded", "gfx/pistol_loaded.png"),
        ("gi/pistol_unloaded", "gfx/pistol_unloaded.png"),
        ("gi/rifle", "gfx/rifle.png"),
        ("gi/dish", "gfx/dish.png"),
        // Animations, by the id their scene asks for: the location's cursor
        // and fight cursor (`LocationScene.cs:89,500`), the flags, the map's
        // mode cursors, a death (`LocationScene.cs:1394`).
        ("a/cursor", "burngfxani@munt.raw?10-13"),
        ("a/fight", "burngfxani@munt.raw?14-17"),
        ("a/flag0", "burngfxani@syst.raw?0-3"),
        ("a/flag1", "burngfxani@syst.raw?4-7"),
        ("a/flag2", "burngfxani@syst.raw?8-11"),
        ("a/flag3", "burngfxani@syst.raw?12-15"),
        ("a/syst20", "burngfxani@syst.raw?20-23"),
        ("a/syst24", "burngfxani@syst.raw?24-27"),
        ("a/dead", "burngfxani@syssze.raw?208-213"),
    ];

    /// v1.2's two fonts as NewGfx swaps them (`newgfx.txt:202-205`), `(key,
    /// classic description)`: the description the table names is packed
    /// under `ng/<key>`, its atlas under `ng/<key>.strip`.
    pub const NEWGFX_FONTS: &[(&str, &str)] = &[("font/plain", "font.txt"), ("font/small", "font-small.txt")];

    fn two_digits(out: &mut [u8; 2], v: u8) {
        out[0] = b'0' + (v / 10) % 10;
        out[1] = b'0' + v % 10;
    }

    fn write(buf: &mut [u8; NAME_LEN], parts: &[&[u8]]) -> Option<usize> {
        let total: usize = parts.iter().map(|p| p.len()).sum();
        if total > NAME_LEN {
            return None;
        }
        let mut at = 0;
        for part in parts {
            buf[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        }
        Some(at)
    }
}
