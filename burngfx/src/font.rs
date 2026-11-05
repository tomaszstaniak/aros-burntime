//! Metrics for the game's bitmap fonts.
//!
//! A font is a PNG strip one line tall plus a `.txt` describing it: the
//! characters it covers, in order, and the pixel width of each. Glyphs sit
//! side by side with no padding, so a character's x position is the sum of the
//! widths before it.
//!
//! Only the metrics live here — the pixels are a PNG, which `no_std` cannot
//! decode. Callers hand [`Font::layout`] a position and get back where to copy
//! from and where to put it.

use alloc::string::String;
use alloc::vec::Vec;

pub struct Font {
    /// Name of the image file the metrics describe.
    pub image: String,
    /// Glyph height in **screen** pixels — what the description says.
    pub height: u32,
    /// Image pixels per screen pixel: 1, or 2 for a description with
    /// `factor=0.5` (`highres-font.txt`), whose strip is drawn at twice the
    /// resolution of the screen it lays text out on. `FontProcessorTxt` divides
    /// both the height and each glyph's image width by `factor` and advances by
    /// the declared width, so layout is the same as the plain font and only
    /// the pixels are finer.
    pub scale: u32,
    /// The `offset` the description declares, which is a **vertical** metric.
    ///
    /// Not inter-character spacing, though the name invites that reading and
    /// applying it sideways (-2 in the shipped fonts) makes the letters
    /// overlap. `Font.cs` uses it to place a line —
    /// `offset.y += GetHeight() - Resource.Offset` — while `DrawChar` advances
    /// horizontally by the glyph width alone, in both v0.9.1 and current
    /// upstream. Kept because the files carry it and a caller laying out lines
    /// will want it.
    pub declared_offset: i32,
    /// v1.2's `line_height`, 0 where the description has none.
    declared_line_height: i32,
    /// The characters covered, in strip order.
    chars: Vec<char>,
    /// Width of each, parallel to `chars`.
    widths: Vec<u32>,
    /// x of each glyph in the strip, parallel to `chars`.
    offsets: Vec<u32>,
}

/// Where one character lives in the strip, and where it goes on the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    /// x in the font strip, in image pixels.
    pub src_x: u32,
    /// Width in the strip, in image pixels: `width * scale`.
    pub src_width: u32,
    /// How far the pen moves, in screen pixels.
    pub width: u32,
    /// x on the target, relative to where the string starts.
    pub dst_x: i32,
}

impl Font {
    /// Parse a font description.
    ///
    /// The format is `key=value` lines. `char0` is a quoted list of characters
    /// in which `""` stands for one `"`, and `width0` is whitespace-separated
    /// widths in the same order. Anything else is ignored.
    pub fn parse(text: &str) -> Option<Font> {
        let mut image = String::new();
        let mut height = 0;
        let mut declared_offset = 0;
        let mut declared_line_height = 0;
        let mut scale = 1;
        let mut chars: Vec<char> = Vec::new();
        let mut widths: Vec<u32> = Vec::new();

        // A UTF-8 BOM in front of `image=` would otherwise hide the key.
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            match key.trim() {
                "image" => image = String::from(value.trim()),
                "height" => height = value.trim().parse().unwrap_or(0),
                "offset" => declared_offset = value.trim().parse().unwrap_or(0),
                "line_height" => declared_line_height = value.trim().parse().unwrap_or(0),
                // Only the two values the shipped fonts use. Anything else is
                // refused rather than rounded into a wrong strip.
                "factor" => {
                    scale = match value.trim() {
                        "1" | "1.0" => 1,
                        "0.5" => 2,
                        _ => return None,
                    }
                }
                "char0" => {
                    let inner = value.trim().trim_matches('"');
                    // `""` is an escaped quote, so fold each pair into one.
                    let mut it = inner.chars().peekable();
                    while let Some(c) = it.next() {
                        if c == '"' && it.peek() == Some(&'"') {
                            it.next();
                        }
                        chars.push(c);
                    }
                }
                "width0" => {
                    widths = value.split_whitespace().filter_map(|w| w.parse().ok()).collect()
                }
                _ => {}
            }
        }

        if height == 0 || chars.is_empty() || chars.len() != widths.len() {
            return None;
        }

        let mut offsets = Vec::with_capacity(widths.len());
        let mut x = 0;
        for w in &widths {
            offsets.push(x);
            x += w * scale;
        }
        Some(Font { image, height, scale, declared_offset, declared_line_height, chars, widths, offsets })
    }

    /// Total width of the strip the metrics describe.
    pub fn strip_width(&self) -> u32 {
        self.widths.iter().sum::<u32>() * self.scale
    }

    /// `Font.GetHeight()`: the glyph height plus the offset at both ends —
    /// 8 for both shipped fonts. Vertical centring halves this, and a line
    /// is this minus the offset.
    pub fn metric_height(&self) -> i32 {
        self.height as i32 + self.declared_offset * 2
    }

    /// v1.2's `Font.LineHeight` (`FontResource.Load`): the declared
    /// `line_height`, else `max(1, height + offset)` — 11 for
    /// `font-small.txt`, 10 for a plain font.
    pub fn line_height(&self) -> i32 {
        if self.declared_line_height > 0 {
            self.declared_line_height
        } else {
            (self.height as i32 + self.declared_offset).max(1)
        }
    }

    /// Where a glyph's top row goes relative to the text position:
    /// `DrawChar` draws at `pos + (0, offset)`.
    pub fn draw_offset(&self) -> i32 {
        self.declared_offset
    }

    /// Whether the font has this character. `NameWindow` accepts a typed
    /// key only if it does.
    pub fn supports(&self, c: char) -> bool {
        self.chars.contains(&c)
    }

    pub fn glyph(&self, c: char) -> Option<Glyph> {
        let i = self.chars.iter().position(|&k| k == c)?;
        Some(Glyph {
            src_x: self.offsets[i],
            src_width: self.widths[i] * self.scale,
            width: self.widths[i],
            dst_x: 0,
        })
    }

    /// Lay a string out left to right. A character the font lacks is drawn
    /// as `?`, as `translateChar` does, or skipped if there is no `?` either.
    pub fn layout(&self, s: &str) -> Vec<Glyph> {
        let mut out = Vec::new();
        let mut x = 0i32;
        for c in s.chars() {
            let Some(mut g) = self.glyph(c).or_else(|| self.glyph('?')) else { continue };
            g.dst_x = x;
            x += g.width as i32;
            out.push(g);
        }
        out
    }

    /// Width the string will occupy once laid out.
    pub fn measure(&self, s: &str) -> u32 {
        match self.layout(s).last() {
            Some(g) => (g.dst_x + g.width as i32).max(0) as u32,
            None => 0,
        }
    }
}

/// v1.2's font description, read as `FontProcessorTxt.Process` reads it
/// (`Burntime.Platform/Graphics/FontProcessorTxt.cs:39-127`), and laid out
/// as `Font.cs` lays text out.
///
/// What [`Font`] has not got: several atlas lines (`lines`, `charN`,
/// `widthN`, `atlas_yN`), a scale per axis that need not be whole
/// (`scale=1.875x2.25` for `font-highres.txt`, `3.75x4.5` for
/// `font-small-highres.txt`), fractional widths, the tight advance
/// `renderwidthN`, and kerning. Positions are fractions of a game pixel;
/// the draw puts each glyph where those fractions fall.
///
/// Not read: the inline indicators and icons (`indicator_*`, `icon_*`) and
/// the blink markup, which are their own sprites in v1.2.
pub struct ScaledFont {
    pub image: String,
    /// Atlas pixels per game pixel, per axis, before `multiplier`.
    pub scale: crate::ratio::Pair,
    /// `multiplier`: 2 for the xBR fonts, else 1.
    pub multiplier: i64,
    /// `GlyphHeight`: `Math.Round(height * scale.y) * multiplier`, atlas
    /// pixels.
    pub glyph_height: i64,
    pub offset: i64,
    declared_line_height: i64,
    pub post_filter: bool,
    chars: Vec<char>,
    glyphs: Vec<ScaledGlyph>,
    /// `(previous, current) → amount`, negative: what `kerningN` takes off.
    kerning: Vec<((char, char), crate::ratio::Ratio)>,
}

/// `CharInfo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaledGlyph {
    /// The glyph's rectangle in the atlas.
    pub src_x: i64,
    pub src_y: i64,
    pub src_w: i64,
    pub src_h: i64,
    /// How far the pen moves, in game pixels: `renderwidth * 2 / scale.x`,
    /// or the `width` itself where there is no `renderwidth`.
    pub advance: crate::ratio::Ratio,
}

/// One glyph placed: where to copy from, and the game x it goes at,
/// relative to the line's (snapped) start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub glyph: ScaledGlyph,
    pub x: crate::ratio::Ratio,
}

impl ScaledFont {
    /// Parse a description whose atlas is `atlas_height` pixels high: the
    /// last line's glyphs are cut to the atlas, as `Process` does once it
    /// has the picture.
    pub fn parse(text: &str, atlas_height: i64) -> Option<ScaledFont> {
        use crate::ratio::{Pair, Ratio};
        let config = crate::config::Config::parse(text)?;
        let s = config.section("")?;
        let int = |k: &str| s.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
        let lines = int("lines");
        let source_height = s.get("height").and_then(Ratio::parse).unwrap_or(Ratio::ZERO);
        let declared_line_height = int("line_height");
        let offset = int("offset");
        // `GetVector2f("scale", One)`; one number is both axes.
        let mut scale = match s.get("scale") {
            None => Pair::ONE,
            Some(v) => match v.split_once('x') {
                Some(_) => Pair::parse(v)?,
                None => {
                    let r = Ratio::parse(v)?;
                    Pair::new(r, r)
                }
            },
        };
        if scale.x.num() == 0 || scale.y.num() == 0 {
            scale = Pair::ONE;
        }
        let multiplier = int("multiplier").max(1);
        let post_filter = s.get("post_filter").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
        let m = Ratio::int(multiplier);
        let glyph_height = (source_height * scale.y).round_even() * multiplier;

        let mut kerning: Vec<((char, char), Ratio)> = Vec::new();
        for (key, value) in s.entries() {
            let Some(amount) = key.strip_prefix("kerning").and_then(Ratio::parse) else { continue };
            if amount <= Ratio::ZERO {
                continue;
            }
            for pair in value.split_whitespace() {
                let cs: Vec<char> = pair.chars().collect();
                // v1.2 throws on anything but two characters.
                let [a, b] = cs[..] else { return None };
                match kerning.iter_mut().find(|(p, _)| *p == (a, b)) {
                    Some(k) => k.1 = -amount,
                    None => kerning.push(((a, b), -amount)),
                }
            }
        }

        let floats = |k: &str| -> Option<Vec<Ratio>> {
            s.get(k).unwrap_or("").split_whitespace().map(Ratio::parse).collect()
        };
        let mut chars: Vec<char> = Vec::new();
        let mut glyphs: Vec<ScaledGlyph> = Vec::new();
        for line in 0..lines {
            let mut key = String::from("char");
            push_i64(&mut key, line);
            let sequence: Vec<char> = s.get(&key).unwrap_or("").chars().collect();
            key = String::from("width");
            push_i64(&mut key, line);
            let widths = floats(&key)?;
            key = String::from("renderwidth");
            push_i64(&mut key, line);
            let render = floats(&key)?;
            if !render.is_empty() && render.len() != widths.len() {
                return None;
            }
            key = String::from("atlas_y");
            push_i64(&mut key, line);
            let y = match s.get(&key) {
                Some(v) => v.trim().parse().ok()?,
                None => line * glyph_height,
            };
            let mut source = Ratio::ZERO;
            for (i, &c) in sequence.iter().enumerate() {
                let w = *widths.get(i)?;
                let pos = (source * scale.x).round_even() * multiplier;
                source = source + w;
                let end = (source * scale.x).round_even() * multiplier;
                let advance = if render.is_empty() { w } else { render[i] * Ratio::int(2) / scale.x };
                // `renderWidth / factor.x`: the factor is 1 / (scale × multiplier).
                let src_w = if render.is_empty() { end - pos } else { (advance * scale.x * m).round_even() };
                let src_h = glyph_height.min(atlas_height - y).max(0);
                let g = ScaledGlyph { src_x: pos, src_y: y, src_w, src_h, advance };
                match chars.iter().position(|&k| k == c) {
                    Some(at) => glyphs[at] = g,
                    None => {
                        chars.push(c);
                        glyphs.push(g);
                    }
                }
            }
        }
        if chars.is_empty() {
            return None;
        }
        let image = String::from(s.get("image").unwrap_or("").trim());
        Some(ScaledFont { image, scale, multiplier, glyph_height, offset, declared_line_height, post_filter, chars, glyphs, kerning })
    }

    /// `Sprite.Resolution`, the factor: game pixels per atlas pixel.
    pub fn factor(&self) -> crate::ratio::Pair {
        let m = crate::ratio::Ratio::int(self.multiplier);
        crate::ratio::Pair::new(self.scale.x * m, self.scale.y * m).recip()
    }

    /// `Font.GetHeight()`: `(int)(Height * Resolution.y + Offset * 2)`.
    pub fn metric_height(&self) -> i64 {
        use crate::ratio::Ratio;
        (Ratio::int(self.glyph_height) * self.factor().y + Ratio::int(self.offset * 2)).trunc()
    }

    /// `FontResource.LineHeight`: the declared `line_height`, else
    /// `max(1, (int)(height * Resolution.y + offset))`.
    pub fn line_height(&self) -> i64 {
        use crate::ratio::Ratio;
        if self.declared_line_height > 0 {
            return self.declared_line_height;
        }
        (Ratio::int(self.glyph_height) * self.factor().y + Ratio::int(self.offset)).trunc().max(1)
    }

    pub fn supports(&self, c: char) -> bool {
        self.chars.contains(&c)
    }

    /// `translateChar`: the character, else `?`.
    pub fn glyph(&self, c: char) -> Option<(char, ScaledGlyph)> {
        let i = self.chars.iter().position(|&k| k == c).or_else(|| self.chars.iter().position(|&k| k == '?'))?;
        Some((self.chars[i], self.glyphs[i]))
    }

    fn kern(&self, previous: Option<char>, current: char) -> crate::ratio::Ratio {
        previous
            .and_then(|p| self.kerning.iter().find(|(k, _)| *k == (p, current)))
            .map_or(crate::ratio::Ratio::ZERO, |(_, a)| *a)
    }

    /// One line laid out from x = 0: each glyph's game x, kerning applied
    /// before it. `previous` is the last character that was not white space,
    /// so a pair kerns across a space, as `DrawText` and `GetWidthFPlain`
    /// have it.
    pub fn layout(&self, s: &str) -> Vec<Placed> {
        let mut out = Vec::new();
        let mut x = crate::ratio::Ratio::ZERO;
        let mut previous = None;
        for c in s.chars() {
            let Some((current, g)) = self.glyph(c) else { continue };
            x = x + self.kern(previous, current);
            out.push(Placed { glyph: g, x });
            x = x + g.advance;
            if !c.is_whitespace() {
                previous = Some(current);
            }
        }
        out
    }

    /// `GetWidthF`: the pen's travel, kerning included.
    pub fn width_f(&self, s: &str) -> crate::ratio::Ratio {
        let mut x = crate::ratio::Ratio::ZERO;
        let mut previous = None;
        for c in s.chars() {
            let Some((current, g)) = self.glyph(c) else { continue };
            x = x + self.kern(previous, current) + g.advance;
            if !c.is_whitespace() {
                previous = Some(current);
            }
        }
        x
    }

    /// `GetWidth`: `Math.Round(GetWidthF)`.
    pub fn width(&self, s: &str) -> i64 {
        self.width_f(s).round_even()
    }
}

fn push_i64(s: &mut String, v: i64) {
    use core::fmt::Write;
    let _ = write!(s, "{v}");
}

#[cfg(test)]
mod scaled_tests {
    use super::*;
    use crate::ratio::{Pair, Ratio};

    /// The head of v1.2's `classic/font-highres.txt` (512 × 87 atlas) with
    /// its first line and a little of the second and third, and two kerning
    /// lines.
    const HIGHRES: &str = "\u{feff}line_height=11\nimage=font-highres.png\nscale=1.875x2.25\noffset=-2\n\
kerning1=Fa Fc\n'kerning1=Sf St\nkerning2=Ta Tc\nkerning1=YA VA\n\
lines=3\nheight=13\n\
char0=\" !\"\"#\"\nwidth0      =7 3 5 5\nrenderwidth0=3 2 4 4\ny0=0\n\
char1=\"ABCDEFGHIJKLMNOPQRSTUVWXYZ\"\nwidth1      =6 6 6 6 6 6 6 6 3 6 6 5 7 6 6 6 6 6 6 6 6 6 7 6 6 6\n\
renderwidth1=5 5 5 5 5 5 5 5 2 5 5 4 6 5 5 5 5 5 5 5 5 5 6 5 5 5\n\
char2=\"ac\"\nwidth2=7 6\nrenderwidth2=5 5\n";

    /// The head of `classic_newgfx/font-small-highres.txt` (768 × 130).
    const SMALL: &str = "line_height=9\nimage=font-small-highres.png\nscale=3.75x4.5\npost_filter=true\noffset=-1\n\
lines=3\nheight=9.6666667\natlas_y0=0\natlas_y1=44\natlas_y2=87\n\
char0=\" !\"\nwidth0      =5.25 2.25\nrenderwidth0=4.5 3\n\
char1=\"AB\"\nwidth1=4.2666667 4.5333333\nrenderwidth1=7.5 8\n\
char2=\"ab\"\nwidth2=5.0666667 4.5333333\nrenderwidth2=7.5 8\n";

    /// `FontProcessorTxt.Process`: height `Math.Round(13 × 2.25)` = 29;
    /// glyph x `Math.Round(sourcePos × 1.875)`; render width `rw × 2 /
    /// 1.875`; atlas width `Math.Round(renderWidth / factor.x)`; lines
    /// `line × height` down (`y0=` is not a key it reads).
    #[test]
    fn the_highres_font_measures_as_v12_measures_it() {
        let f = ScaledFont::parse(HIGHRES, 87).unwrap();
        assert_eq!(f.scale, Pair::new(Ratio::new(15, 8), Ratio::new(9, 4)));
        assert_eq!(f.glyph_height, 29);
        // GetHeight: (int)(29 / 2.25 - 4) = (int)8.89 = 8; line_height 11.
        assert_eq!((f.metric_height(), f.line_height()), (8, 11));
        let (_, bang) = f.glyph('!').unwrap();
        // sourcePos 7 → round(13.125) = 13; renderwidth 2 → 2 × 2 / 1.875.
        assert_eq!((bang.src_x, bang.src_y, bang.src_h), (13, 0, 29));
        assert_eq!(bang.advance, Ratio::new(32, 15));
        assert_eq!(bang.src_w, 4);
        let (_, quote) = f.glyph('"').unwrap();
        // sourcePos 10 → round(18.75) = 19.
        assert_eq!(quote.src_x, 19);
        let (_, a) = f.glyph('A').unwrap();
        assert_eq!((a.src_x, a.src_y), (0, 29));
        let (_, i) = f.glyph('I').unwrap();
        // sourcePos 6 × 8 = 48 → 90; renderwidth 2 → atlas width 4.
        assert_eq!((i.src_x, i.src_w), (90, 4));
        let (_, c) = f.glyph('c').unwrap();
        assert_eq!((c.src_x, c.src_y), (13, 58)); // round(7 × 1.875) = round(13.125)
        // A character the font lacks is drawn as `?`; this cut has none.
        assert!(f.glyph('Z').is_some() && f.glyph('~').is_none());
    }

    /// `Math.Round` ties to even: `sourcePos × 1.875` is a tie at 4, 12, …
    /// (7.5, 22.5) and goes to 8 and 22.
    #[test]
    fn glyph_positions_round_ties_to_even() {
        let text = "lines=1\nheight=12\nscale=1.875x2.25\nchar0=\"abcd\"\nwidth0=4 8 4 1\n";
        let f = ScaledFont::parse(text, 100).unwrap();
        let xs: Vec<i64> = "abcd".chars().map(|c| f.glyph(c).unwrap().1.src_x).collect();
        assert_eq!(xs, [0, 8, 22, 30]);
        // Without renderwidth the atlas width is end - pos.
        assert_eq!(f.glyph('b').unwrap().1.src_w, 14);
    }

    /// The small font: height `Math.Round(9.6666667 × 4.5)` = 44, lines at
    /// the declared `atlas_y`, the last cut to the 130-pixel atlas;
    /// `GetHeight` (int)(44 / 4.5 - 2) = 7; advances `rw × 2 / 3.75`.
    #[test]
    fn the_small_highres_font_measures_as_v12_measures_it() {
        let f = ScaledFont::parse(SMALL, 130).unwrap();
        assert_eq!(f.glyph_height, 44);
        assert_eq!((f.metric_height(), f.line_height()), (7, 9));
        assert!(f.post_filter);
        assert_eq!(f.factor(), Pair::new(Ratio::new(4, 15), Ratio::new(2, 9)));
        let (_, b) = f.glyph('B').unwrap();
        // sourcePos 4.2666667 × 3.75 = 16.00000013 → 16; rw 8 → 16/3.75.
        assert_eq!((b.src_x, b.src_y, b.src_h), (16, 44, 44));
        assert_eq!(b.advance, Ratio::new(64, 15));
        assert_eq!(b.src_w, 16);
        let (_, a) = f.glyph('a').unwrap();
        assert_eq!((a.src_y, a.src_h), (87, 43));
        let (_, bang) = f.glyph('!').unwrap();
        // round(5.25 × 3.75 = 19.6875) = 20.
        assert_eq!(bang.src_x, 20);
    }

    /// `GetWidthFPlain`: the render widths, plus each kerning pair's
    /// (negative) amount; the previous character skips white space, so
    /// `F a` still kerns; `GetWidth` rounds.
    #[test]
    fn kerning_and_widths_follow_get_width() {
        let f = ScaledFont::parse(HIGHRES, 87).unwrap();
        let five = Ratio::new(16, 3); // 5 × 2 / 1.875
        assert_eq!(f.width_f("FA"), five + five);
        assert_eq!(f.width_f("Fa"), five + five - Ratio::int(1));
        assert_eq!(f.width_f("Ta"), five + five - Ratio::int(2));
        let space = Ratio::new(16, 5); // 3 × 2 / 1.875
        assert_eq!(f.width_f("F a"), five + space + five - Ratio::int(1));
        // "YA" kerns by 1, read from the second `kerning1=` line.
        assert_eq!(f.width_f("YA"), five + five - Ratio::int(1));
        assert_eq!(f.width("FA"), 11); // 10.67
        let placed = f.layout("Fa");
        assert_eq!(placed[1].x, five - Ratio::int(1));
    }
}
