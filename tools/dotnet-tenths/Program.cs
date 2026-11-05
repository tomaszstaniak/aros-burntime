// How .NET writes a float with `{x:0.0}`, as upstream's Modern AI does in its
// decision reasons (e.g. `{economicGain:0.0}`, `AiTurnController.cs`).
//
// Prints one line per value: the float's bits in hex, then the string. The
// values are a fixed list (ordinary values, every tenths boundary, negatives,
// values that expose float precision) and 20,000 pseudo-random floats from a
// fixed linear congruential generator, so the output is reproducible.
//
//   dotnet run -c Release > burngfx/tests/data/dotnet_tenths.txt
//
// burngfx/tests/modern_tenths.rs compares `reason::tenths` with the file.
using System;
using System.Collections.Generic;
using System.Globalization;

CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
var values = new List<float>();
float[] fixedValues =
{
    0f, -0f, 0.4f, 1f, 3.14159f, 12.96f, 99.94f, 100f, 0.04f, 0.0499999f,
    0.05f, 0.15f, 0.25f, 0.35f, 0.45f, 0.55f, 0.65f, 0.75f, 0.85f, 0.95f,
    1.05f, 1.25f, 2.45f, 2.5f, 9.95f, 99.95f, 999.95f, 1234.55f, 12345.65f,
    123456.75f, 1234567.5f, 16777216f, 16777217f, 1e7f, 1e7f + 0.05f,
    3.4e38f, 1e-7f, 1e-45f, 0.3499999f, 0.3500001f, 0.0999999f,
    -0.05f, -0.15f, -0.25f, -0.35f, -0.04f, -1.25f, -9.95f, -12345.65f,
    float.NaN, float.PositiveInfinity, float.NegativeInfinity,
};
values.AddRange(fixedValues);
for (int whole = 0; whole <= 40; whole++)
    for (int hundredths = 0; hundredths < 100; hundredths++)
    {
        values.Add(whole + hundredths / 100f);
        values.Add(-(whole + hundredths / 100f));
    }
uint state = 12345;
uint Next() { state = state * 1664525u + 1013904223u; return state; }
for (int i = 0; i < 10_000; i++)
    values.Add((Next() >> 8) / (float)(1 << 24) * 2000f - 1000f);
for (int i = 0; i < 10_000; i++)
    values.Add(BitConverter.UInt32BitsToSingle(Next()));
foreach (float v in values)
    Console.WriteLine($"{BitConverter.SingleToUInt32Bits(v):x8} {v:0} {v:0.0} {v:0.00}");
