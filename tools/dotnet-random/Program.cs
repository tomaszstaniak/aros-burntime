// What .NET's seeded `System.Random` returns, for burngfx's port of it.
//
// Upstream builds its one generator as `new Random(int seed)`
// (`Burntime.Platform/Math/Math.cs`), which in .NET 10 is the legacy
// subtractive generator (`Net5CompatSeedImpl`). burngfx::net_random ports it;
// this prints, for a few seeds, a fixed mix of calls and what each returned,
// so the port can be compared call by call:
//
//   seed SEED
//   next VALUE                     Next()
//   max N VALUE                    Next(N)
//   range A B VALUE                Next(A, B), the large-range path included
//   double BITS                    NextDouble(), the double's bits in hex
//   single BITS                    NextSingle(), the float's bits in hex
//
//   dotnet run -c Release > ../../burngfx/tests/data/dotnet_random.txt
//
// burngfx/tests/net_random.rs reads the file.
using System;
using System.Globalization;

CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
int[] seeds = { 0, 1, 7, 42, 1993, -7, 123456789, int.MaxValue, int.MinValue, -1 };
int[] maxes = { 0, 1, 2, 3, 4, 5, 9, 75, 360, 1000, 16777216, int.MaxValue };
(int, int)[] ranges =
{
    (0, 4), (-1, 2), (0, 2), (14, 21), (-20, 21), (3, 6), (1, 3), (2, 6), (7, 7), (0, 5),
    (int.MinValue, int.MaxValue), (-2000000000, 2000000000), (int.MinValue, 0), (0, int.MaxValue),
};
foreach (int seed in seeds)
{
    var r = new Random(seed);
    Console.WriteLine($"seed {seed}");
    for (int i = 0; i < 300; i++)
    {
        switch (i % 5)
        {
            case 0:
                Console.WriteLine($"next {r.Next()}");
                break;
            case 1:
            {
                int n = maxes[(i / 5) % maxes.Length];
                Console.WriteLine($"max {n} {r.Next(n)}");
                break;
            }
            case 2:
            {
                var (a, b) = ranges[(i / 5) % ranges.Length];
                Console.WriteLine($"range {a} {b} {r.Next(a, b)}");
                break;
            }
            case 3:
                Console.WriteLine($"double {BitConverter.DoubleToInt64Bits(r.NextDouble()):x16}");
                break;
            default:
                Console.WriteLine($"single {BitConverter.SingleToInt32Bits(r.NextSingle()):x8}");
                break;
        }
    }
}
