// v1.2's ComplexPath and .NET's List<T>.Sort, run for real, for
// burngfx::complex_path to be compared against.
//
// Compiles upstream's own `PathFinding/ComplexPath.cs` and `PathState.cs`
// (with `Platform/Math/Vector2.cs`, `Rect.cs`) from the v1.2 checkout, with
// stubs below for the engine around them, and prints:
//
//   sort asc|desc N  KEYS... = IDS...    List<(int,int)>.Sort by key (desc:
//                                        the reverse comparer's shape), the
//                                        ids in the order .NET leaves them
//   route MAP SX SY GX GY  X,Y X,Y ...   a walk at 50 px/s in steps of
//                                        1/18 s from cell centre (SX,SY)
//                                        to (GX,GY): the position each step
//                                        returns, until it stops moving
//
//   UPSTREAM=/path/to/up/source dotnet run -c Release -- PACK \
//       > ../../burngfx/tests/data/dotnet_complexpath.txt
//
// burngfx/tests/complex_path.rs reads the file.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using Burntime.Platform;
using Burntime.Data.BurnGfx;
using Burntime.Remaster.PathFinding;

CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
var out_ = new StringBuilder();

// Sorts: sizes across insertion sort, the partition and (by sheer length)
// deep recursion; few distinct keys, so many ties.
uint state = 12345;
int Next(int n) { state = state * 1103515245 + 12345; return (int)((state >> 16) % (uint)n); }
foreach (bool desc in new[] { false, true })
    foreach (int n in new[] { 2, 3, 5, 9, 16, 17, 18, 24, 33, 40, 64, 100, 200 })
        for (int rep = 0; rep < 3; rep++)
        {
            int distinct = 1 + Next(6);
            var list = new List<(int key, int id)>();
            for (int i = 0; i < n; i++) list.Add((Next(distinct), i));
            var keys = string.Join(" ", list.ConvertAll(e => e.key));
            if (desc) list.Sort(new Desc()); else list.Sort(new Asc());
            out_.Append($"sort {(desc ? "desc" : "asc")} {n} {keys} = {string.Join(" ", list.ConvertAll(e => e.id))}\n");
        }

// Routes on a real mask.
byte[] pack = File.ReadAllBytes(args[0]);
foreach (var (map, sx, sy, gx, gy) in new[] {
    ("MAT_001", 8, 40, 45, 8), ("MAT_001", 20, 5, 20, 45), ("MAT_001", 30, 50, 10, 12), ("MAT_001", 8, 40, 30, 30) })
{
    PathMask mask = Walk(pack, "walk/" + map);
    var path = new ComplexPath();
    path.Begin();
    path.Speed = 50;
    Vector2 at = new Vector2(sx * 8 + 4, sy * 8 + 4);
    path.Stop(at);
    path.MoveTo = new Vector2(gx * 8 + 4, gy * 8 + 4);
    var steps = new List<string>();
    Vector2 last = at; int still = 0;
    for (int i = 0; i < 20000 && still < 40; i++)
    {
        at = path.Process(mask, at, 1f / 18f);
        steps.Add($"{at.x},{at.y}");
        still = at == last ? still + 1 : 0;
        last = at;
    }
    steps.RemoveRange(steps.Count - still, still);
    out_.Append($"route {map} {sx} {sy} {gx} {gy} {string.Join(" ", steps)}\n");
}
Console.Out.Write(out_.ToString());

static PathMask Walk(byte[] pack, string name)
{
    int count = BitConverter.ToInt32(pack, 12);
    for (int i = 0; i < count; i++)
    {
        int e = 16 + i * 32;
        string n = Encoding.ASCII.GetString(pack, e, 24).TrimEnd('\0');
        if (n != name) continue;
        int off = BitConverter.ToInt32(pack, e + 24);
        int w = BitConverter.ToUInt16(pack, off), h = BitConverter.ToUInt16(pack, off + 2);
        var m = new PathMask(w, h, 8);
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
                m[x, y] = pack[off + 4 + y * w + x] != 0;
        return m;
    }
    throw new Exception("no " + name);
}

sealed class Asc : IComparer<(int key, int id)> { public int Compare((int key, int id) a, (int key, int id) b) => a.key.CompareTo(b.key); }
sealed class Desc : IComparer<(int key, int id)> { public int Compare((int key, int id) a, (int key, int id) b) => b.key.CompareTo(a.key); }

// The engine around the two files, as little as compiles.
namespace Burntime.Framework { public class Unused { } }
namespace Burntime.Framework.States
{
    public abstract class StateObject
    {
        protected virtual void InitInstance(object[] parameter) { }
        protected virtual void AfterDeserialization() { }
        public void Begin() => InitInstance(new object[0]);
    }
}
namespace Burntime.Platform { public struct PixelColor { public PixelColor(int r, int g, int b) { } } }
namespace Burntime.Platform.Graphics
{
    public class RenderTarget { public void RenderLine(Vector2f a, Vector2f b, PixelColor c) { } }
}
namespace Burntime.Data.BurnGfx
{
    public class PathMask
    {
        public int Resolution, Width, Height;
        bool[] mask;
        public PathMask(int w, int h, int r) { Width = w; Height = h; Resolution = r; mask = new bool[w * h]; }
        public bool this[Vector2 p] { get => this[p.x, p.y]; }
        public bool this[int x, int y]
        {
            get { if (x < 0 || y < 0 || x >= Width || y >= Height) return false; return mask[x + y * Width]; }
            set { mask[x + y * Width] = value; }
        }
        public bool IsWalkableMapPosition(Vector2 p) => this[p / Resolution];
    }
}
namespace Burntime.Remaster
{
    public class BurntimeClassic
    {
        public static BurntimeClassic Instance = new BurntimeClassic();
        public Settings Settings = new Settings();
    }
    public class Settings { public Section this[string s] => new Section(); }
    // `RES/game/classic/settings.txt:11`.
    public class Section { public int GetInt(string k) => k == "pathfinding_cycles" ? 50 : throw new Exception(k); }
}
