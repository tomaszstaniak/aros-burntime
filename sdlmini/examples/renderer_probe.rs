//! **A bounded test of the render path, before the game is trusted to it.**
//!
//! Opens a window, presents a fixed number of frames of a changing colour,
//! says which renderer SDL granted, and exits. Nothing else: no assets, no
//! event loop worth the name, no game. A hang here is a hang in the render
//! path and nowhere else, and it is run under a hard time limit so that it
//! cannot become the thing it is testing for.
fn main() {
    let (w, h) = (320usize, 200usize);
    let mut screen = match sdlmini::Screen::open(b"renderer probe\0", w, h, 640, 400) {
        Ok(s) => s,
        Err(e) => {
            println!("renderer probe: could not open: {e}");
            std::process::exit(2);
        }
    };
    println!("renderer probe: renderer = {}", screen.renderer_name());
    let mut px = vec![0u32; w * h];
    for frame in 0..120u32 {
        let c = 0xFF00_0000 | ((frame * 2) & 0xFF) << 16 | ((255 - frame * 2) & 0xFF) << 8 | 0x40;
        px.iter_mut().for_each(|p| *p = c);
        screen.present(&px);
        let _ = screen.poll();
        sdlmini::delay(8);
    }
    println!("renderer probe: 120 frames presented, exiting cleanly");
}
