//! Write a packed image out as a PPM, for looking at:
//! `cargo run -p burngfx --example dump_packed -- an/opta0/0 out.ppm`.
fn main() {
    let key = std::env::args().nth(1).expect("an entry name");
    let out = std::env::args().nth(2).expect("an output path");
    let bytes = std::fs::read("burngfx/testdata/burntime.pack").expect("the pack");
    let pack = burngfx::pack::Pack::open(&bytes).expect("a pack");
    let raw = pack.get(&key).expect("no such entry");
    let (w, h) = (u16::from_le_bytes([raw[0], raw[1]]) as usize, u16::from_le_bytes([raw[2], raw[3]]) as usize);
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    for i in 0..w * h {
        let p = &raw[4 + i * 4..8 + i * 4];
        ppm.extend_from_slice(if p[3] == 0 { &[255, 0, 255] } else { &p[..3] });
    }
    std::fs::write(&out, ppm).expect("written");
    println!("{key}: {w} x {h}");
}
