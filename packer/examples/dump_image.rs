//! Write one packed image out of `burntime.pack` as a PNG, to look at it.
//! `cargo run -p packer --example dump_image -- scene/sta_en out.png`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata");
    let bytes = std::fs::read(dir.join("burntime.pack")).unwrap();
    let pack = burngfx::pack::Pack::open(&bytes).unwrap();
    let img = burngfx::pack::PackedImage::parse(pack.get(&a[1]).expect("no such entry")).unwrap();
    let f = std::fs::File::create(&a[2]).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&img.rgba[..(img.width * img.height * 4) as usize]).unwrap();
}
