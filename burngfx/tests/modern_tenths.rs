//! `{x:0}`, `{x:0.0}` and `{x:0.00}` on a `float` as .NET 10 writes them
//! (runtime 10.0.11, the SDK `global.json` pins at `v1.2`: 10.0.400), against
//! `reason::format_fixed`. `data/dotnet_tenths.txt` is the output of
//! `tools/dotnet-tenths`: a float's bits and .NET's three strings per line.

use burngfx::modern_ai::reason::{format_fixed, format_tenths};

#[test]
fn every_value_is_written_as_dotnet_writes_it() {
    let text = include_str!("data/dotnet_tenths.txt");
    let mut wrong = Vec::new();
    let mut n = 0;
    for line in text.lines() {
        let mut w = line.split(' ');
        let bits = w.next().unwrap();
        let x = f32::from_bits(u32::from_str_radix(bits, 16).unwrap());
        for (decimals, want) in w.enumerate() {
            let got = format_fixed(x, decimals);
            n += 1;
            if got != want {
                wrong.push(format!("{bits} ({x:e}) with {decimals} decimals: .NET {want}, port {got}"));
            }
        }
    }
    assert!(n > 80_000, "{n} values");
    assert!(wrong.is_empty(), "{} of {n} differ, first: {:#?}", wrong.len(), &wrong[..wrong.len().min(15)]);
}

/// `Math.Max(0, x)` before the gain is written: never `-0.0`, NaN kept.
#[test]
fn the_gain_is_maxed_as_dotnet_does() {
    use burngfx::modern_ai::reason::dotnet_max;
    assert_eq!(format_tenths(dotnet_max(0.0, -0.0)), "0.0");
    assert_eq!(format_tenths(dotnet_max(-0.0, 0.0)), "0.0");
    assert_eq!(format_tenths(dotnet_max(0.0, -0.3)), "0.0");
    assert_eq!(format_tenths(dotnet_max(0.0, f32::NAN)), "NaN");
    assert_eq!(format_tenths(dotnet_max(0.0, 0.25)), "0.3");
}
