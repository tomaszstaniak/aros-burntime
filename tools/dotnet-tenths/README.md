# How .NET writes `{x:0.0}`

Upstream's Modern AI writes gains into its decision reasons with
`{gain:0.0}` on a `float`. Its reasons are compared as strings (deferrals,
the stalled-wait watchdog), so the port must write the same text.

`Program.cs` prints, for 28,252 floats, the bits and .NET's string: ordinary
values, every hundredth from -40.99 to 40.99, rounding boundaries, negatives,
values where `float` precision shows (16777216, 3.4e38, 0.3499999), NaN and
the infinities, and 20,000 values from a fixed generator.

Run (here with the SDK installed into a scratch directory by
`dotnet-install.sh --version 10.0.400`, the version `global.json` pins at
`v1.2`):

    cd tools/dotnet-tenths
    dotnet run -c Release > ../../burngfx/tests/data/dotnet_tenths.txt

Each line has the three formats the reports use: `{x:0}`, `{x:0.0}`, `{x:0.00}`.

Result, 2026-10-08: .NET SDK 10.0.400, runtime 10.0.11, osx-arm64.
`burngfx/tests/modern_tenths.rs` compares every string (84,756) with
`reason::format_fixed`, and all match. What the runtime does:

- the value is taken to 7 significant digits first (`16777216` → `16777220.0`,
  `3.3999998e38` → `340000000000000000000000000000000000000.0`);
- then rounded half away from zero on those digits (`0.25` → `0.3`,
  `0.35f` = 0.3499999940… → `0.3500000` → `0.4`);
- the sign stays when the value rounds to zero (`-0.04` → `-0.0`, `-0` →
  `-0.0`);
- `NaN`, `Infinity`, `-Infinity` as words.

The port's earlier formatter agreed on ordinary values but overflowed on very
large ones and could not write `-0.0`; it was replaced by `format_tenths`.
