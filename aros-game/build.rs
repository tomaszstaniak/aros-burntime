//! Decide what the game compiles its data in from: a developer's
//! `burntime.pack`, or nothing (`BURNTIME_EMBED=no`, or no pack).
//!
//! AROS gets the archive embedded rather than loaded, so the choice is a build
//! time one. If `scripts/pack.sh` has produced `burntime.pack`, use it.
//! Otherwise embed an empty archive, so that a fresh clone — which ships no
//! game data, and cannot legally ship any — still builds and still links. The
//! game then says what is missing instead of failing to compile.

use std::path::PathBuf;

fn main() {
    // "aros" is a custom target, so rustc does not know the name.
    println!("cargo::rustc-check-cfg=cfg(target_os, values(\"aros\"))");

    // `aros_keyed_text`: the SDL2 this target links sends key presses but
    // no SDL_TEXTINPUT, so typed characters are read off the key and Shift
    // (`Screen::typed_from_last_key`). True of the ABIv11 SDK's SDL2 (built
    // before contrib's ce77be61, "SDL2: fix text input ... MapRawKey-based
    // SDL_TEXTINPUT"). The ABIv1 targets link contrib's SDL2 from after that
    // fix (scripts/deps.py), which does send text: reading the key there
    // as well would type every character twice. Decided by the target's
    // name, as `sdlmini/build.rs` picks its SDL2.
    println!("cargo::rustc-check-cfg=cfg(aros_keyed_text)");
    println!("cargo:rerun-if-env-changed=TARGET");
    let target = std::env::var("TARGET").unwrap_or_default();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("aros") && !target.contains("-abiv1-") {
        println!("cargo:rustc-cfg=aros_keyed_text");
    }
    // `aros_contrib_sdl`: the ABIv1 targets, which link contrib's static
    // SDL2 with its AHI driver (main.rs, `run`, for what that changes).
    println!("cargo::rustc-check-cfg=cfg(aros_contrib_sdl)");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("aros") && target.contains("-abiv1-") {
        println!("cargo:rustc-cfg=aros_contrib_sdl");
    }

    let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../burngfx/testdata/burntime.pack");
    println!("cargo:rerun-if-changed={}", pack.display());
    // `BURNTIME_EMBED=no`: a binary to give away carries no game data, even
    // where a developer's pack is lying about; it reads the player's own
    // from their data drawer at run time.
    println!("cargo:rerun-if-env-changed=BURNTIME_EMBED");
    let embed = std::env::var("BURNTIME_EMBED").map_or(true, |v| v != "no");

    if embed && pack.is_file() {
        println!("cargo:rustc-env=BURNTIME_PACK={}", pack.display());
        return;
    }

    // The header of `burngfx::pack`, with a count of zero: magic, version,
    // padding, count. Written rather than hand-waved so `Pack::open` accepts
    // it and the game reaches its own "no data" message.
    let mut empty = Vec::from(*b"BURNPACK");
    empty.extend_from_slice(&1u16.to_le_bytes());
    empty.extend_from_slice(&0u16.to_le_bytes());
    empty.extend_from_slice(&0u32.to_le_bytes());

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("empty.pack");
    std::fs::write(&out, empty).unwrap();
    println!("cargo:rustc-env=BURNTIME_PACK={}", out.display());
}
