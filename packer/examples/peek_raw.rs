//! Decode frames of a `.raw` or `.ani` sheet from `burntime.pak` to PNG, with
//! the default colour table, to look at them. A look, not a build step.
//!
//! `cargo run -p packer --example peek_raw -- munt.raw 0 30 out_dir`
use burngfx::pak::ClassicPak;
use burngfx::Palette;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (name, from, to, out) = (&args[1], args[2].parse::<usize>().unwrap(),
                                 args[3].parse::<usize>().unwrap(), &args[4]);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata");
    let bytes = std::fs::read(dir.join("burntime.pak")).expect("burntime.pak");
    let pak = ClassicPak::open(&bytes).expect("a pak");
    // `GetRawColorTable`'s default: map 0's table.
    let palette = Palette::from_mat_raw(pak.get("burn_gfx/mat_000.raw").expect("mat_000")).expect("palette");
    let raw = pak.get(&format!("burn_gfx/{name}")).expect("no such file in the pak");
    let sprites = burngfx::SpriteFile::parse(raw);
    println!("{name}: {} frame(s)", sprites.frame_count());
    std::fs::create_dir_all(out).unwrap();
    for f in from..to.min(sprites.frame_count()) {
        let Some(img) = sprites.decode_frame(f) else { println!("  {f}: none"); continue };
        let rgba = img.to_rgba(&palette);
        println!("  {f}: {}x{}", rgba.width, rgba.height);
        let file = std::fs::File::create(format!("{out}/{name}_{f:02}.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), rgba.width as u32, rgba.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba.rgba).unwrap();
    }
}
