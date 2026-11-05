//! How many pixels of packed images are transparent.
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata");
    let bytes = std::fs::read(dir.join("burntime.pack")).unwrap();
    let pack = burngfx::pack::Pack::open(&bytes).unwrap();
    for key in std::env::args().skip(1) {
        let img = burngfx::pack::PackedImage::parse(pack.get(&key).unwrap()).unwrap();
        let n = (img.width * img.height) as usize;
        let clear = (0..n).filter(|i| img.rgba[i * 4 + 3] == 0).count();
        let black = (0..n).filter(|i| img.rgba[i * 4 + 3] != 0 && img.rgba[i * 4..i * 4 + 3] == [0, 0, 0]).count();
        println!("{key}: {}x{} transparent {clear}, opaque black {black}", img.width, img.height);
    }
}
