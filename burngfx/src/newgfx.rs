//! v1.2's NewGfx substitution: a classic resource id in, the remaster's
//! replacement and its scale out — or the classic resource, if the
//! replacement's file is not there.
//!
//! The table is `newgfx.txt` (and, with the xBR shader, `xbr2.txt` over it).
//! Every section maps `classic-id = new-id` and carries a `sprite_scale`, the
//! replacement's pixels per game pixel. The lookup is
//! `ResourceManagerBase.GetReplacement` / `GetReplacementID`
//! (`ResourceManagerBase.cs:308-381`) and the use of it is
//! `ResourceManager.GetSprite` (`Burntime.MonoGame/Resource/ResourceManager.cs:142-161`):
//!
//! 1. the tables last first, the sections in file order, the first match wins;
//! 2. a section matches the whole id, else `format@file`, else `file`;
//! 3. a whole-id match is taken as written; otherwise an index is put into
//!    `{0}` after the first `?`, or appended as `?index[-end][?custom]`;
//! 4. the new id is used only if its format has a sprite reader and its file
//!    exists (`CheckReplacementID`, `:383-399`) — with `{0}` in the *file*
//!    filled from the index, as `syst.raw?100` reaches
//!    `gfx/ways/syst_100.png`;
//! 5. otherwise the classic id stands, at scale 1.
//!
//! The packer runs this on the host, so the game itself needs none of it;
//! it is `no_std` all the same, like the rest of the crate.

use crate::config::Config;
use crate::ratio::Pair;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// `Burntime.Platform.Resource.ResourceID`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceId {
    /// The whole id, in lower case, as `ToString()` gives it.
    pub id: String,
    /// Before `@`, else the file's extension; empty if neither.
    pub format: String,
    pub file: String,
    pub index: i32,
    /// `-1` when no range was given.
    pub end_index: i32,
    pub custom: String,
    pub index_provided: bool,
}

impl ResourceId {
    /// `new ResourceID(path)`. An index that is not a number — which upstream
    /// would throw on — reads as no index.
    pub fn parse(text: &str) -> ResourceId {
        let id = text.to_lowercase();
        let mut path: &str = &id;
        let mut format = String::new();
        let at: Vec<&str> = path.split('@').collect();
        if at.len() == 2 {
            format = at[0].to_string();
            path = at[1];
        }
        let mut index = 0;
        let mut end_index = -1;
        let mut custom = String::new();
        let mut index_provided = false;
        let q: Vec<&str> = path.split('?').collect();
        if q.len() >= 2 {
            path = q[0];
            if q.len() == 3 {
                custom = q[2].to_string();
            }
            let range: Vec<&str> = q[1].split('-').collect();
            if !range[0].is_empty() {
                match range[0].parse::<i32>() {
                    Ok(v) => {
                        index = v;
                        index_provided = true;
                        if range.len() == 2 {
                            end_index = range[1].parse().unwrap_or(-1);
                        }
                    }
                    Err(_) => {}
                }
            } else {
                end_index = 0;
            }
        }
        let file = path.to_string();
        if format.is_empty() {
            if let Some(dot) = file.rfind('.') {
                format = file[dot + 1..].to_string();
            }
        }
        ResourceId { id, format, file, index, end_index, custom, index_provided }
    }

    /// `string.Format(File, Index)`: the file a sprite reader opens.
    pub fn file_for_index(&self) -> String {
        let mut n = String::new();
        push_int(&mut n, self.index);
        self.file.replace("{0}", &n)
    }

    /// The frames the id covers: `index..=end_index`, or one.
    pub fn frames(&self) -> core::ops::RangeInclusive<i32> {
        if self.end_index == -1 {
            self.index..=self.index
        } else {
            self.index..=self.end_index
        }
    }

    /// `pngsheet`'s `?WxH`.
    pub fn cell(&self) -> Option<(u32, u32)> {
        let (w, h) = self.custom.split_once('x')?;
        Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
    }
}

fn push_int(s: &mut String, v: i32) {
    use core::fmt::Write;
    let _ = write!(s, "{v}");
}

/// One replacement file, parsed: its sections in order, each with its
/// `sprite_scale`.
pub struct Table {
    config: Config,
}

impl Table {
    pub fn parse(text: &str) -> Option<Table> {
        Some(Table { config: Config::parse(text)? })
    }
}

/// A replacement: the new id and its `sprite_scale` — replacement pixels per
/// game pixel. The sprite's `Resolution` (its factor) is the reciprocal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub id: ResourceId,
    pub scale: Pair,
}

/// `GetReplacementID` for one section.
fn replacement_id(id: &ResourceId, section: &crate::config::Section) -> Option<String> {
    if let Some(v) = section.get(&id.id) {
        return Some(v.to_string());
    }
    let mut format_file = id.format.clone();
    format_file.push('@');
    format_file.push_str(&id.file);
    let mut s = section.get(&format_file).or_else(|| section.get(&id.file))?.to_string();
    if id.index_provided {
        let after_q = s.split('?').nth(1);
        if s.contains('?') && after_q.is_some_and(|p| p.contains("{0}")) {
            let mut n = String::new();
            push_int(&mut n, id.index);
            if id.end_index != -1 {
                n.push('-');
                push_int(&mut n, id.end_index);
            }
            s = s.replace("{0}", &n);
        } else {
            s.push('?');
            push_int(&mut s, id.index);
            if id.end_index != -1 {
                s.push('-');
                push_int(&mut s, id.end_index);
            }
            if !id.custom.is_empty() {
                s.push('?');
                s.push_str(&id.custom);
            }
        }
    } else if !id.custom.is_empty() {
        s.push_str("??");
        s.push_str(&id.custom);
    }
    Some(s)
}

/// `GetReplacement`: the tables are in mount order, so the last is asked
/// first. No existence check here.
pub fn replacement(tables: &[Table], id: &ResourceId) -> Option<Replacement> {
    for table in tables.iter().rev() {
        for section in &table.config.sections {
            // v1.3's new character artwork is a session's opt-in
            // (`--experimental-characters`, `ResourceManagerBase.cs:336-338`),
            // off unless asked for; this port does not offer it.
            if section.get("experimental_characters").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
                continue;
            }
            if let Some(new) = replacement_id(id, section) {
                // `GetVector2f("sprite_scale", One)`; a zero scale is a
                // factor of one.
                let scale = section.get("sprite_scale").and_then(Pair::parse).unwrap_or(Pair::ONE);
                let scale = if scale.x.num() == 0 || scale.y.num() == 0 { Pair::ONE } else { scale };
                return Some(Replacement { id: ResourceId::parse(&new), scale });
            }
        }
    }
    None
}

/// The formats a sprite reader exists for: the platform's three
/// (`ResourceManagerBase.cs:35-40`) and `Burntime.BurnGfx`'s.
pub const SPRITE_FORMATS: &[&str] = &["png", "pngani", "pngsheet", "raw", "ani", "pac", "burngfxani", "burngfxtile"];

/// `CheckReplacementID`: a reader for the format, and the file.
pub fn available(id: &ResourceId, exists: &dyn Fn(&str) -> bool) -> bool {
    SPRITE_FORMATS.contains(&id.format.as_str()) && exists(&id.file_for_index())
}

/// What a sprite id draws as: `GetSprite`'s choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The replacement, at its scale.
    New(Replacement),
    /// The classic resource, at one pixel per game pixel.
    Classic(ResourceId),
}

pub fn resolve(tables: &[Table], id: &str, exists: &dyn Fn(&str) -> bool) -> Resolved {
    let id = ResourceId::parse(id);
    match replacement(tables, &id) {
        Some(r) if available(&r.id, exists) => Resolved::New(r),
        _ => Resolved::Classic(id),
    }
}

/// `ResolveFileReplacement`: the new file name if it exists. `LoadFont` takes
/// a font's replacement without asking (`ResourceManager.cs:56-58`): pass an
/// `exists` that always says yes for that.
pub fn resolve_file(tables: &[Table], file: &str, exists: &dyn Fn(&str) -> bool) -> String {
    let id = ResourceId::parse(file);
    match replacement(tables, &id) {
        Some(r) if exists(&r.id.file) => r.id.file,
        _ => id.file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratio::Ratio;

    /// The sections that matter, cut from v1.2's `classic_newgfx/newgfx.txt`
    /// (lines 5-205), in their order.
    const TABLE: &str = "[compatibility]\nitems.txt=rules/extended/items.txt\n\n\
[replacement_60x72]\n' 32x32 -> 60x72\nsprite_scale=1.875x2.25\n\
zei_004.raw=pngsheet@maps/shared/set_4.png?{0}?60x72\n\
maps/mat_007_tiles.png=pngsheet@maps/mat_007_tiles_2x.png?{0}?60x72\n\
syssze.raw=pngsheet@gfx/syssze.png?{0}?30x36\n\
munt.raw?0=gfx/munt_0.png\nmunt.raw?3=pngsheet@gfx/objects/munt.png?0?15x18\n\
syst.raw?16=pngsheet@gfx/objects/player_icon_colors.png?0?36x38\n\
syst.raw=gfx/ways/syst_{0}.png\n\
burngfxani@syst.raw?0-3=pngsheet@gfx/objects/flags.png?0-11?30x36\n\
raum_0.pac=gfx/backgrounds/room0_large.png\nsta.pac=gfx/start.png\n\
pngsheet@gfx/ui/start_buttons.png?0?112x24=pngsheet@gfx/start_buttons.png?0?192x54\n\
ges_00.ani=pngani@gfx/faces/geometry_true/ges_00.png\n\
gst.raw=pngsheet@gfx/items/gst2.png?{0}?60x72\n\
pngsheet@font-indicators.png=pngsheet@font-indicators.png?{0}?15x27\n\
[newgfx_native]\nsprite_scale=1.875x2.25\ngfx/char_shadow.png=gfx/char_shadow.png\n\
[replacement]\nsprite_scale=2x2\nburngfxani@munt.raw=pngani@gfx/munt_{0}.png\nmunt.raw=gfx/munt_{0}.png\nsta.pac=gfx/sta.png\n\
[replacement_stretched]\nsprite_scale=2x2.375\nraum_0.pac=gfx/backgrounds/room0.png\n\
gfx/advice.png=pngsheet@gfx/items/gst.png?58?64x76\n\
zei_001.raw=pngsheet@maps/shared/set_1.png?{0}?64x76\n\
[noscale]\nsprite_scale=1x1\nfont.txt=font-highres.txt\nfont-small.txt=font-small-highres.txt\n";

    const S60: Pair = Pair { x: Ratio::new_const(15, 8), y: Ratio::new_const(9, 4) };

    fn tables() -> Vec<Table> {
        alloc::vec![Table::parse(TABLE).unwrap()]
    }

    fn everything(_: &str) -> bool {
        true
    }

    fn new(id: &str) -> (String, Pair) {
        match resolve(&tables(), id, &everything) {
            Resolved::New(r) => (r.id.id, r.scale),
            Resolved::Classic(c) => panic!("{id} stayed classic as {}", c.id),
        }
    }

    #[test]
    fn ids_parse_as_resource_id_does() {
        let id = ResourceId::parse("BurnGfxTile@zei_004.raw?12?5");
        assert_eq!((id.format.as_str(), id.file.as_str(), id.index, id.end_index, id.custom.as_str(), id.index_provided),
                   ("burngfxtile", "zei_004.raw", 12, -1, "5", true));
        let id = ResourceId::parse("burngfxani@syssze.raw?16-31");
        assert_eq!((id.index, id.end_index), (16, 31));
        let id = ResourceId::parse("gfx/up.png");
        assert_eq!((id.format.as_str(), id.index_provided), ("png", false));
    }

    /// A map tile: `BurnmapProcessor.cs:263` asks for
    /// `burngfxtile@zei_NNN.raw?item?map`; the file key matches, `{0}`
    /// takes the item and the map (`custom`) is dropped.
    #[test]
    fn a_tile_takes_its_cell_of_the_shared_sheet() {
        assert_eq!(new("burngfxtile@zei_004.raw?12?5"), ("pngsheet@maps/shared/set_4.png?12?60x72".into(), S60));
        // A map's own sheet (`BurnmapProcessor.cs:254`).
        assert_eq!(new("pngsheet@maps/mat_007_tiles.png?3?32x32"), ("pngsheet@maps/mat_007_tiles_2x.png?3?60x72".into(), S60));
        // Section 1 is in the stretched section: 64 × 76 cells at 2 × 2.375.
        assert_eq!(new("burngfxtile@zei_001.raw?7?0").1, Pair::new(Ratio::int(2), Ratio::new(19, 8)));
    }

    /// A body is sixteen frames (`GameCreation.cs:465`): the range goes into
    /// `{0}` whole.
    #[test]
    fn a_range_fills_the_placeholder_whole() {
        assert_eq!(new("burngfxani@syssze.raw?16-31").0, "pngsheet@gfx/syssze.png?16-31?30x36");
    }

    /// A whole-id match is taken as written, before the file key is tried:
    /// `munt.raw?3` is its own entry, `munt.raw?1` falls to `[replacement]`'s
    /// `munt.raw`, whose `{0}` is in the file, not after a `?`.
    #[test]
    fn a_whole_id_wins_and_an_index_is_appended_otherwise() {
        assert_eq!(new("munt.raw?3"), ("pngsheet@gfx/objects/munt.png?0?15x18".into(), S60));
        assert_eq!(new("munt.raw?0").0, "gfx/munt_0.png");
        let r = match resolve(&tables(), "munt.raw?11", &|f| f == "gfx/munt_11.png") {
            Resolved::New(r) => r,
            other => panic!("{other:?}"),
        };
        assert_eq!((r.id.id.as_str(), r.id.file_for_index().as_str()), ("gfx/munt_{0}.png?11", "gfx/munt_11.png"));
        assert_eq!(r.scale, Pair::new(Ratio::int(2), Ratio::int(2)));
        // `burngfxani@munt.raw?10-13` matches `format@file` in `[replacement]`.
        assert_eq!(new("burngfxani@munt.raw?10-13").0, "pngani@gfx/munt_{0}.png?10-13");
        // The flags' four classic frames become twelve.
        assert_eq!(new("burngfxani@syst.raw?0-3").0, "pngsheet@gfx/objects/flags.png?0-11?30x36");
        // A road: `syst.raw?100` → `gfx/ways/syst_100.png`.
        let r = match resolve(&tables(), "syst.raw?100", &everything) { Resolved::New(r) => r, o => panic!("{o:?}") };
        assert_eq!(r.id.file_for_index(), "gfx/ways/syst_100.png");
    }

    /// The first section that matches wins, even if a later one would give a
    /// different file: `raum_0.pac` is in `[replacement_60x72]` (the large
    /// room) before `[replacement_stretched]`, and `sta.pac` likewise.
    #[test]
    fn the_first_section_wins() {
        assert_eq!(new("raum_0.pac"), ("gfx/backgrounds/room0_large.png".into(), S60));
        assert_eq!(new("sta.pac").0, "gfx/start.png");
        assert_eq!(new("ges_00.ani").0, "pngani@gfx/faces/geometry_true/ges_00.png");
        assert_eq!(new("gfx/advice.png").0, "pngsheet@gfx/items/gst.png?58?64x76");
        assert_eq!(new("gfx/char_shadow.png"), ("gfx/char_shadow.png".into(), S60));
    }

    /// No file, no replacement: the classic resource at scale 1
    /// (`ResourceManager.cs:153-158`).
    #[test]
    fn a_missing_file_falls_back_to_the_classic_resource() {
        let r = resolve(&tables(), "syst.raw?28", &|f| f != "gfx/ways/syst_28.png");
        assert_eq!(r, Resolved::Classic(ResourceId::parse("syst.raw?28")));
        // A format nobody reads is no replacement either.
        let r = resolve(&tables(), "items.txt", &everything);
        assert_eq!(r, Resolved::Classic(ResourceId::parse("items.txt")));
        // An id nothing names.
        assert!(matches!(resolve(&tables(), "inv.raw?3", &everything), Resolved::Classic(_)));
    }

    /// The later table is asked first: xBR's `xbr2.txt` over `newgfx.txt`.
    #[test]
    fn the_last_table_mounted_is_asked_first() {
        let xbr = Table::parse("[fonts]\nsprite_scale=1x1\nfont.txt=font-highres-xbr2.txt\n").unwrap();
        let ts = alloc::vec![Table::parse(TABLE).unwrap(), xbr];
        assert_eq!(resolve_file(&ts, "font.txt", &everything), "font-highres-xbr2.txt");
        assert_eq!(resolve_file(&tables(), "font-small.txt", &everything), "font-small-highres.txt");
        assert_eq!(resolve_file(&tables(), "font-small.txt", &|_| false), "font-small.txt");
    }
}
