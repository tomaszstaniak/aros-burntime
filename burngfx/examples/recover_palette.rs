//! Rebuild the "map0" palette that STA.ANI expects, from decoded indices plus
//! the reference PNGs, and write it in `MAT_xxx.RAW` layout so that the viewer
//! loads it through exactly the same code path as a real map file.
//!
//! This exists because we don't have `MAT_000.RAW`. It is a development aid, not
//! part of the engine: a real installation supplies the palette itself.
//!
//!     cargo run -p burngfx --example recover_palette

use burngfx::SpriteFile;

fn main() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");
    let sta = std::fs::read(format!("{dir}/STA.ANI")).unwrap();
    let ani = SpriteFile::parse(&sta);

    // 16 bytes of header we never read, then 256 RGB triples in 6-bit VGA range.
    let mut out = vec![0u8; 16 + 256 * 3];
    let mut known = [false; 256];
    let mut recovered = 0;

    for i in 0..ani.frame_count() {
        let frame = ani.decode_frame(i).unwrap();

        let path = format!("{dir}/ref-STA/{i}.png");
        let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut want = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut want).unwrap();

        for (&index, px) in frame.indices.iter().zip(want.chunks(4)) {
            if index == 0 || known[index as usize] {
                continue;
            }
            known[index as usize] = true;
            recovered += 1;
            let at = 16 + index as usize * 3;
            out[at] = px[0] / 4;
            out[at + 1] = px[1] / 4;
            out[at + 2] = px[2] / 4;
        }
    }

    let path = format!("{dir}/map0-recovered.pal");
    std::fs::write(&path, &out).unwrap();
    println!("recovered {recovered} of 255 palette entries -> {path}");
}
