fn main() {
    let b = std::fs::read("burngfx/testdata/burntime.pak").unwrap();
    let pak = burngfx::pak::ClassicPak::open(&b).unwrap();
    let g = pak.get("burn_gfx/gam.dat").unwrap();
    let base = 22;
    for i in 0..37usize {
        let r = &g[base + i * 50..base + (i + 1) * 50];
        let u16le = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
        let products: Vec<i8> = (24..28).map(|o| r[o] as i8).collect();
        if products.iter().any(|&p| p >= 0) || u16le(28) != 0xFFFF {
            println!("loc {i:2}: owner {} products {:?} producing {} city {}",
                     u16le(2), products, u16le(28) as i16, u16le(22));
        }
    }
}
