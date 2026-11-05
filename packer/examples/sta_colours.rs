//! Look at the colours of `sta.pac`'s two "SPIELER" captions, to tell the
//! letters from the stone. A look, not a build step.
use burngfx::pak::ClassicPak;
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata");
    let bytes = std::fs::read(dir.join("burntime.pak")).unwrap();
    let pak = ClassicPak::open(&bytes).unwrap();
    let raw = pak.get("burn_gfx/sta.pac").unwrap();
    let pac = burngfx::pac::decode(raw).unwrap();
    let (pal, _) = burngfx::pac::colour_table(raw, &pac).unwrap();
    let mut hist = std::collections::BTreeMap::new();
    for y in 8..24 { for x in 30..102 {
        let i = pac.indices[y * burngfx::pac::WIDTH as usize + x];
        *hist.entry(i).or_insert(0) += 1;
    }}
    for (i, n) in hist { let c = pal.color(i); println!("{i:3} {n:4}  {:3} {:3} {:3}", c.r, c.g, c.b); }
    for y in 6..26 {
        let mut line = String::new();
        for x in 28..104 {
            let i = pac.indices[y * burngfx::pac::WIDTH as usize + x];
            let c = pal.color(i);
            line.push(if c.r > c.g + 25 { '#' } else if (c.r as u32 + c.g as u32 + c.b as u32) < 150 { '.' } else { ' ' });
        }
        println!("{y:2} {line}");
    }
}
