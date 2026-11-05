//! `burngfx::net_random` against .NET 10 itself: `tests/data/dotnet_random.txt`
//! is what `tools/dotnet-random` printed — `new Random(seed)` for ten seeds,
//! 300 calls each of `Next()`, `Next(max)`, `Next(min, max)` (the large-range
//! path included), `NextDouble()` and `NextSingle()`.
use burngfx::net_random::NetRandom;

const VECTORS: &str = include_str!("data/dotnet_random.txt");

#[test]
fn every_call_returns_what_dotnet_returned() {
    let mut rng: Option<NetRandom> = None;
    let (mut seeds, mut calls) = (0, 0);
    for line in VECTORS.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        let int = |i: usize| w[i].parse::<i32>().unwrap();
        if w[0] == "seed" {
            rng = Some(NetRandom::new(int(1)));
            seeds += 1;
            continue;
        }
        let r = rng.as_mut().unwrap();
        match w[0] {
            "next" => assert_eq!(r.next(), int(1), "{line}"),
            "max" => assert_eq!(r.next_max(int(1)), int(2), "{line}"),
            "range" => assert_eq!(r.next_range(int(1), int(2)), int(3), "{line}"),
            "double" => assert_eq!(r.next_double().to_bits(), u64::from_str_radix(w[1], 16).unwrap(), "{line}"),
            "single" => assert_eq!(r.next_single().to_bits(), u32::from_str_radix(w[1], 16).unwrap(), "{line}"),
            other => panic!("unknown call {other}"),
        }
        calls += 1;
    }
    assert_eq!((seeds, calls), (10, 3000));
}

/// A generator taken from its state goes on exactly as the one it was taken
/// from — what a save and a load rely on.
#[test]
fn a_restored_state_continues_the_sequence() {
    let mut a = NetRandom::new(1993);
    for _ in 0..1000 {
        a.next();
    }
    let (table, inext, inextp) = a.state();
    let mut b = NetRandom::from_state(table, inext, inextp).unwrap();
    for _ in 0..1000 {
        assert_eq!(a.next_max(75), b.next_max(75));
    }
    assert!(NetRandom::from_state(table, 56, 0).is_none());
}
