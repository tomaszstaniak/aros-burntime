fn main() {
    // "aros" is a custom target, so rustc does not know the name.
    println!("cargo::rustc-check-cfg=cfg(target_os, values(\"aros\"))");
    println!("cargo::rustc-check-cfg=cfg(sdlmini_mixer)");
    println!("cargo::rustc-check-cfg=cfg(sdlmini_no_gl)");
    println!("cargo::rustc-check-cfg=cfg(sdlmini_gl_standins)");
    println!("cargo:rerun-if-env-changed=TARGET");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("aros") {
        aros_libraries();
        return;
    }
    if let Ok(out) = std::process::Command::new("sdl2-config").arg("--libs").output() {
        for arg in String::from_utf8_lossy(&out.stdout).split_whitespace() {
            if let Some(dir) = arg.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={dir}");
            }
        }
    }
    println!("cargo:rustc-link-lib=dylib=SDL2");

    // SDL2_mixer is optional on the host: with it the game has sound, without
    // it the game says so and runs silent. The AROS SDK always has it.
    if let Ok(out) = std::process::Command::new("pkg-config").args(["--libs", "SDL2_mixer"]).output() {
        if out.status.success() {
            for arg in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                if let Some(dir) = arg.strip_prefix("-L") {
                    println!("cargo:rustc-link-search=native={dir}");
                }
            }
            println!("cargo:rustc-cfg=sdlmini_mixer");
        }
    }
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
}

/// AROS links SDL2 and SDL2_mixer statically, the mixer first because it
/// calls into SDL2. SDL2 reaches gl.library for its renderer, so GL (the
/// SDK's stubs) comes along, and the AROS SDL2 is built with
/// `SDL_USE_LIBICONV`, so iconv too.
///
/// ABIv11 (`aros-x86_64-abiv11-pc`, or the older target name `x86_64-aros`):
/// the SDK's own static `libSDL2.a` and `libSDL2_mixer.a`.
///
/// ABIv1 (`aros-x86_64-abiv1-pc`, `aros-aarch64-abiv1-raspi`): mainline's
/// SDK has `libSDL2.a` too, but as link stubs to the shared SDL2.library,
/// and the Pi's none at all, so `scripts/deps.py` builds contrib's static
/// SDL2 under contrib's own name for it, `libSDL2_static.a`, beside
/// SDL2_mixer and libiconv; the target's link arguments name that directory.
///
/// Every AROS target leaves gl.library out (`sdlmini_no_gl`) and asks SDL
/// for its software renderer by name, the renderer it had chosen anyway.
/// libGL's link stubs carry an auto-open that opens gl.library when the
/// program starts; gl.library then keeps the drawer of the first program
/// that opened it in use until a restart, and on mainline that program's
/// output stream is left to the later programs of its Shell (AROS issue
/// 2100). The ABIv1 targets' SDL2 carries this repository's patch that
/// calls gl.library through the base SDL opens itself, so it needs no
/// stubs. The ABIv11 SDK's SDL2 still names the six glA* functions; there
/// they resolve to stand-ins that fail (`gl_standins`, `sdlmini_gl_standins`),
/// which SDL never calls while it uses the software renderer.
fn aros_libraries() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let abiv1 = target.contains("-abiv1-");
    let sdl2 = if abiv1 { "SDL2_static" } else { "SDL2" };
    let no_gl = true;
    println!("cargo:rustc-cfg=sdlmini_no_gl");
    if !abiv1 {
        println!("cargo:rustc-cfg=sdlmini_gl_standins");
    }
    for lib in ["SDL2_mixer", sdl2, "GL", "iconv"] {
        if lib == "GL" && no_gl {
            continue;
        }
        println!("cargo:rustc-link-lib={lib}");
    }
}
