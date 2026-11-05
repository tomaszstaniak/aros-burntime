//! Exact fractions, for the remaster's scale factors.
//!
//! v1.2 computes its sizes in `float`: `sprite_scale=1.875x2.25`, a font's
//! `scale=3.75x4.5`, `width1=4.2666667`. Every one of those is a short
//! decimal, so it is held here as the fraction it spells, and the arithmetic
//! that follows (`Math.Round(sourcePos * scale.x)`, a sprite edge at
//! `position * 1.875`) is exact. Where upstream's `float` would round a tie
//! differently from the exact value, the callers say so. `no_std`: AROS has
//! no `f32` maths in `core`.

use core::cmp::Ordering;
use core::ops::{Add, Div, Mul, Neg, Sub};

/// `num / den`, always reduced, `den > 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: i64,
    den: i64,
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Ratio {
    pub const ZERO: Ratio = Ratio { num: 0, den: 1 };
    pub const ONE: Ratio = Ratio { num: 1, den: 1 };

    /// For constants already in lowest terms, `den > 0`.
    pub const fn new_const(num: i64, den: i64) -> Ratio {
        Ratio { num, den }
    }

    pub fn new(num: i64, den: i64) -> Ratio {
        assert!(den != 0, "a fraction over zero");
        let g = gcd(num, den);
        let s = if den < 0 { -1 } else { 1 };
        Ratio { num: s * num / g, den: s * den / g }
    }

    /// From a 128-bit intermediate, reduced before it is narrowed.
    fn wide(num: i128, den: i128) -> Ratio {
        let (mut a, mut b) = (num.abs(), den.abs());
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let g = a.max(1);
        let s = if den < 0 { -1 } else { 1 };
        Ratio { num: (s * num / g) as i64, den: (s * den / g) as i64 }
    }

    pub fn int(v: i64) -> Ratio {
        Ratio { num: v, den: 1 }
    }

    pub fn num(self) -> i64 {
        self.num
    }

    pub fn den(self) -> i64 {
        self.den
    }

    /// A decimal as the file writes it — `1.875`, `-2`, `9.6666667`, `.5` —
    /// exactly. `None` for anything else, as `float.TryParse` fails.
    pub fn parse(text: &str) -> Option<Ratio> {
        let t = text.trim();
        let (neg, t) = match t.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
        if whole.is_empty() && frac.is_empty() {
            return None;
        }
        if !whole.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) || frac.len() > 12 {
            return None;
        }
        let mut num: i64 = 0;
        for b in whole.bytes().chain(frac.bytes()) {
            num = num.checked_mul(10)?.checked_add((b - b'0') as i64)?;
        }
        let den = 10i64.pow(frac.len() as u32);
        Some(Ratio::new(if neg { -num } else { num }, den))
    }

    /// The largest whole number not above it.
    pub fn floor(self) -> i64 {
        self.num.div_euclid(self.den)
    }

    pub fn ceil(self) -> i64 {
        -(-self).floor()
    }

    /// `Math.Round`: to the nearest, a tie to the even neighbour (.NET's
    /// default, `MidpointRounding.ToEven`).
    pub fn round_even(self) -> i64 {
        let f = self.floor();
        let rest = self - Ratio::int(f);
        match rest.cmp(&Ratio::new(1, 2)) {
            Ordering::Less => f,
            Ordering::Greater => f + 1,
            Ordering::Equal => f + (f & 1),
        }
    }

    /// `(int)x`: toward zero.
    pub fn trunc(self) -> i64 {
        self.num / self.den
    }

    pub fn is_integer(self) -> bool {
        self.den == 1
    }

    pub fn recip(self) -> Ratio {
        Ratio::new(self.den, self.num)
    }
}

impl From<i64> for Ratio {
    fn from(v: i64) -> Ratio {
        Ratio::int(v)
    }
}

impl From<i32> for Ratio {
    fn from(v: i32) -> Ratio {
        Ratio::int(v as i64)
    }
}

impl Add for Ratio {
    type Output = Ratio;
    fn add(self, o: Ratio) -> Ratio {
        Ratio::wide(self.num as i128 * o.den as i128 + o.num as i128 * self.den as i128,
                    self.den as i128 * o.den as i128)
    }
}

impl Sub for Ratio {
    type Output = Ratio;
    fn sub(self, o: Ratio) -> Ratio {
        self + -o
    }
}

impl Neg for Ratio {
    type Output = Ratio;
    fn neg(self) -> Ratio {
        Ratio { num: -self.num, den: self.den }
    }
}

impl Mul for Ratio {
    type Output = Ratio;
    fn mul(self, o: Ratio) -> Ratio {
        Ratio::wide(self.num as i128 * o.num as i128, self.den as i128 * o.den as i128)
    }
}

impl Div for Ratio {
    type Output = Ratio;
    fn div(self, o: Ratio) -> Ratio {
        Ratio::wide(self.num as i128 * o.den as i128, self.den as i128 * o.num as i128)
    }
}

impl Ord for Ratio {
    fn cmp(&self, o: &Ratio) -> Ordering {
        (self.num as i128 * o.den as i128).cmp(&(o.num as i128 * self.den as i128))
    }
}

impl PartialOrd for Ratio {
    fn partial_cmp(&self, o: &Ratio) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// A per-axis pair: `sprite_scale=1.875x2.25` and the like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pair {
    pub x: Ratio,
    pub y: Ratio,
}

impl Pair {
    pub const ONE: Pair = Pair { x: Ratio::ONE, y: Ratio::ONE };

    pub fn new(x: Ratio, y: Ratio) -> Pair {
        Pair { x, y }
    }

    /// `ConfigSection.GetVector2f`'s `AxB`. A single number is refused: the
    /// reader would give it a zero second axis, and no shipped file has one.
    pub fn parse(text: &str) -> Option<Pair> {
        let (a, b) = text.trim().split_once('x')?;
        Some(Pair { x: Ratio::parse(a)?, y: Ratio::parse(b)? })
    }

    pub fn recip(self) -> Pair {
        Pair { x: self.x.recip(), y: self.y.recip() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_exact() {
        assert_eq!(Ratio::parse("1.875"), Some(Ratio::new(15, 8)));
        assert_eq!(Ratio::parse("2.375"), Some(Ratio::new(19, 8)));
        assert_eq!(Ratio::parse("-2"), Some(Ratio::int(-2)));
        assert_eq!(Ratio::parse("9.6666667"), Some(Ratio::new(96_666_667, 10_000_000)));
        assert_eq!(Ratio::parse(".5"), Some(Ratio::new(1, 2)));
        assert_eq!(Ratio::parse("x"), None);
        assert_eq!(Ratio::parse(""), None);
        assert_eq!(Pair::parse("1.875x2.25"), Some(Pair::new(Ratio::new(15, 8), Ratio::new(9, 4))));
    }

    /// `Math.Round` sends a tie to the even neighbour: 2.5 → 2, 3.5 → 4.
    #[test]
    fn rounding_is_bankers() {
        assert_eq!(Ratio::new(5, 2).round_even(), 2);
        assert_eq!(Ratio::new(7, 2).round_even(), 4);
        assert_eq!(Ratio::new(-5, 2).round_even(), -2);
        assert_eq!(Ratio::new(26, 10).round_even(), 3);
        assert_eq!(Ratio::new(-7, 4).floor(), -2);
        assert_eq!(Ratio::new(-7, 4).trunc(), -1);
        assert_eq!(Ratio::new(-7, 4).ceil(), -1);
    }
}
