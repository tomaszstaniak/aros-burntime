//! .NET's seeded `System.Random`, the generator v1.2 plays by.
//!
//! Upstream's one generator is `new Random((int)DateTime.Now.Ticks)`
//! (`Burntime.Platform/Math/Math.cs:9-13`). A `Random` built with a seed is,
//! in every .NET from Framework 1.0 to 10, Knuth's subtractive generator
//! (`Net5CompatSeedImpl` over `CompatPrng`): 55 numbers below
//! `int.MaxValue`, each new one the difference of two older ones. This is
//! that algorithm, call for call — `Next()`, `Next(max)`, `Next(min, max)`
//! with its large-range path, `NextDouble()` and `NextSingle()` — so a draw
//! made here has the range, the edges and the rounding v1.2's has.
//!
//! **Why this one and not a better generator**: the rules are written in its
//! calls. `Next(n)` is `(int)(Sample() * n)`, not a rejection-sampled
//! integer, and `Next(0)` is 0 but still spends a number. Porting the calls
//! onto another generator would keep the distribution only nearly; porting
//! the generator keeps it exactly, and the comparison with .NET itself is a
//! file (`tests/data/dotnet_random.txt`, made by `tools/dotnet-random`).
//!
//! **One per process, and never saved — as upstream's.** v1.2's generator
//! is a static field, seeded from the clock when the process starts and
//! shared by everything; no save carries it, so a load in the same process
//! continues it and a new process starts a fresh one. The port does the
//! same (`aros-game`'s process generator; save version 25 dropped the state
//! that 22 to 24 wrote). The `seed` argument seeds it, as upstream's tests
//! call `SetRandomSeed`: that is the controlled randomness two runs are
//! compared under. [`NetRandom::state`] and [`NetRandom::from_state`] stay
//! for reading those older files and for tests: the state is 56 numbers and
//! two indices.
//!
//! The arithmetic is C#'s `unchecked` int arithmetic: the seeding of a seed
//! near `int.MaxValue` passes through values that wrap, and the wrapping is
//! part of the sequence.

/// `int.MaxValue`, which the reference source calls `MBIG`.
const MBIG: i32 = i32::MAX;
/// `MSEED`: the golden ratio's digits.
const MSEED: i32 = 161_803_398;

/// .NET's `new Random(int seed)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl NetRandom {
    /// `CompatPrng(int seed)`.
    pub fn new(seed: i32) -> NetRandom {
        let mut seed_array = [0i32; 56];
        let subtraction = if seed == i32::MIN { i32::MAX } else { seed.wrapping_abs() };
        let mut mj = MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        let mut mk: i32 = 1;
        let mut ii = 0usize;
        for _ in 1..55 {
            ii += 21;
            if ii >= 55 {
                ii -= 55;
            }
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                let mut n = i + 30;
                if n >= 55 {
                    n -= 55;
                }
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[1 + n]);
                if seed_array[i] < 0 {
                    seed_array[i] = seed_array[i].wrapping_add(MBIG);
                }
            }
        }
        NetRandom { seed_array, inext: 0, inextp: 21 }
    }

    /// `InternalSample`: the next number, in `[0, int.MaxValue)`.
    fn internal_sample(&mut self) -> i32 {
        let mut inext = self.inext + 1;
        if inext >= 56 {
            inext = 1;
        }
        let mut inextp = self.inextp + 1;
        if inextp >= 56 {
            inextp = 1;
        }
        let mut ret = self.seed_array[inext].wrapping_sub(self.seed_array[inextp]);
        if ret == MBIG {
            ret -= 1;
        }
        if ret < 0 {
            ret = ret.wrapping_add(MBIG);
        }
        self.seed_array[inext] = ret;
        self.inext = inext;
        self.inextp = inextp;
        ret
    }

    /// `Sample`: `[0, 1)`.
    fn sample(&mut self) -> f64 {
        self.internal_sample() as f64 * (1.0 / MBIG as f64)
    }

    /// `GetSampleForLargeRange`: two numbers, the second's parity a sign.
    fn sample_for_large_range(&mut self) -> f64 {
        let mut result = self.internal_sample();
        if self.internal_sample() % 2 == 0 {
            result = -result;
        }
        let mut d = result as f64;
        d += (i32::MAX - 1) as f64;
        d /= 2.0 * i32::MAX as f64 - 1.0;
        d
    }

    /// `Next()`: `[0, int.MaxValue)`. Named for .NET's call, not an
    /// iterator's.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> i32 {
        self.internal_sample()
    }

    /// `Next(maxValue)`: `[0, maxValue)`, and 0 for 0 — after spending a
    /// number all the same. .NET throws for a negative bound; this treats it
    /// as nought, which no rule asks for.
    pub fn next_max(&mut self, max: i32) -> i32 {
        (self.sample() * max.max(0) as f64) as i32
    }

    /// `Next(minValue, maxValue)`: `[min, max)`, `min` when they are equal.
    /// A range wider than `int.MaxValue` takes two numbers. .NET throws when
    /// `min > max`; this returns `min`, which no rule asks for either.
    pub fn next_range(&mut self, min: i32, max: i32) -> i32 {
        if min > max {
            return min;
        }
        let range = max as i64 - min as i64;
        if range <= i32::MAX as i64 {
            (self.sample() * range as f64) as i32 + min
        } else {
            ((self.sample_for_large_range() * range as f64) as i64 + min as i64) as i32
        }
    }

    /// `NextDouble()`: `[0, 1)`.
    pub fn next_double(&mut self) -> f64 {
        self.sample()
    }

    /// `NextSingle()`, which the seeded generator computes as `(float)Sample()`.
    pub fn next_single(&mut self) -> f32 {
        self.sample() as f32
    }

    /// The whole state: the 56 numbers and the two indices. Saves before
    /// version 25 carried it; reading one checks it with [`NetRandom::from_state`].
    pub fn state(&self) -> ([i32; 56], usize, usize) {
        (self.seed_array, self.inext, self.inextp)
    }

    /// A state written by [`NetRandom::state`]. `None` when an index is past
    /// the table, which no state of the generator has.
    pub fn from_state(seed_array: [i32; 56], inext: usize, inextp: usize) -> Option<NetRandom> {
        if inext >= 56 || inextp >= 56 {
            return None;
        }
        Some(NetRandom { seed_array, inext, inextp })
    }
}
